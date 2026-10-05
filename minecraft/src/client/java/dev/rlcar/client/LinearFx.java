package dev.rlcar.client;

import com.mojang.blaze3d.framegraph.FramePass;
import com.mojang.blaze3d.framegraph.FrameGraphBuilder;
import com.mojang.blaze3d.pipeline.RenderTarget;
import com.mojang.blaze3d.resource.RenderTargetDescriptor;
import com.mojang.blaze3d.resource.ResourceHandle;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.blaze3d.vertex.BufferBuilder;
import com.mojang.blaze3d.vertex.ByteBufferBuilder;
import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.blaze3d.vertex.MeshData;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import com.mojang.renderpearl.api.commands.CommandEncoder;
import com.mojang.renderpearl.api.commands.RenderPass;
import com.mojang.renderpearl.api.pipeline.BindGroupLayout;
import com.mojang.renderpearl.api.pipeline.BlendFunction;
import com.mojang.renderpearl.api.pipeline.ColorTargetState;
import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import com.mojang.renderpearl.api.pipeline.UniformType;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuSampler;
import dev.rlcar.RlCar;
import java.util.ArrayList;
import java.util.List;
import java.util.Optional;
import java.util.OptionalDouble;
import java.util.function.Supplier;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.LevelTargetBundle;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.texture.AbstractTexture;
import net.minecraft.resources.Identifier;
import org.joml.Matrix4f;
import org.joml.Vector4f;
import org.jspecify.annotations.Nullable;

/**
 * Composites the car effects the way Rocket League does: in linear light, over the scene.
 *
 * <p>The game (UE3) blends its particles into an HDR scene colour buffer in linear light and only
 * converts to the display at the end. Minecraft's main target holds 8-bit sRGB values and blends
 * them with fixed-function blending, so drawing an effect straight into it adds or blends sRGB
 * values: an additive glow of 0.4 (linear) over the sky adds about 0.66 instead of about 0.2. So
 * the effects ({@link RlFx}, {@link RlBoost}) are queued here instead of being submitted to the
 * level's features, drawn after the main pass into an RGBA16F target as linear, premultiplied
 * light (additive: rgb with alpha 0; translucent: rgb x alpha with alpha coverage; both blended
 * "over", One / OneMinusSrcAlpha), depth-tested against the main depth (so the world, the cars and
 * translucent blocks, which write depth, hide them), and composited once:
 * {@code srgb(linear(dst) * (1 - fx.a) + fx.rgb)} ({@code rlcar:core/fx_composite}). That is the
 * game's "add" for additive effects and its "over" for translucent ones; only the order between
 * interleaved additive and translucent particles is not kept.
 *
 * <p>The pass is added to the level's frame graph after the main pass by
 * {@code LevelRendererMixin}, both with the classic and the improved (OIT) transparency. When it is
 * not running (a shader pack, another mod replacing the level renderer, or a failure), the effects
 * are submitted as before, each converted to sRGB before blending.
 */
public final class LinearFx {
	/** The effects' colour target: linear premultiplied light, blended "over". */
	static final ColorTargetState TARGET = new ColorTargetState(Optional.of(BlendFunction.TRANSLUCENT_PREMULTIPLIED_ALPHA), GpuFormat.RGBA16_FLOAT, ColorTargetState.WRITE_ALL);
	private static final RenderTargetDescriptor.TextureProperties FX_COLOR = new RenderTargetDescriptor.TextureProperties(new Vector4f(0.0F), GpuFormat.RGBA16_FLOAT);
	private static final RenderTargetDescriptor.TextureProperties COPY_COLOR = new RenderTargetDescriptor.TextureProperties(null, GpuFormat.RGBA8_UNORM);
	/** How long without the frame graph hook before falling back (a frame or two at the lowest frame rates). */
	private static final long HOOK_TIMEOUT_NANOS = 500_000_000L;

	/** A texture of a queued draw: the sampler the render type would use, or the texture's own. */
	record Tex(String name, Identifier texture, @Nullable Supplier<GpuSampler> sampler) {
	}

	private record Draw(RenderPipeline pipeline, List<Tex> textures, MeshData mesh) {
	}

	private static @Nullable RenderPipeline composite;
	private static final List<Draw> QUEUE = new ArrayList<>();
	private static @Nullable ByteBufferBuilder bytes;
	private static @Nullable GpuBuffer vertices;
	private static RenderSystem.@Nullable AutoStorageIndexBuffer quadIndices;
	private static long lastHook;
	private static long queuedAt;
	private static boolean failed;
	/** {@code -Drlcar.srgbFx}: the old way (each effect converted to sRGB and blended into the main target), to compare. */
	/** {@code -Drlcar.srgbFx}: the old way (each effect converted to sRGB and blended into the main target), to compare. */
	private static final boolean DISABLED = System.getProperty("rlcar.srgbFx") != null;

	private LinearFx() {
	}

	/** Registers the composite pipeline (before the first resource load compiles the pipelines). */
	public static void registerPipelines() {
		composite = RenderPipelines.register(RenderPipeline.builder()
			.withLocation(RlCar.id("pipeline/fx_composite"))
			.withVertexShader(Identifier.withDefaultNamespace("core/screenquad"))
			.withFragmentShader(RlCar.id("core/fx_composite"))
			.withBindGroupLayout(BindGroupLayout.builder()
				.withUniform("SceneSampler", UniformType.COMBINED_IMAGE_SAMPLER)
				.withUniform("FxSampler", UniformType.COMBINED_IMAGE_SAMPLER)
				.build())
			.withPrimitiveTopology(PrimitiveTopology.TRIANGLES)
			.withColorTargetState(ColorTargetState.DEFAULT)
			.build());
	}

	/**
	 * Whether the effects go through here this frame: the frame graph pass ran recently, nothing
	 * failed, and no shader pack replaces the level rendering. Otherwise they are submitted as before.
	 */
	static boolean active() {
		if (DISABLED || failed || composite == null || shaderPackInUse()) {
			return false;
		}
		long now = System.nanoTime();
		if (!QUEUE.isEmpty() && now - queuedAt > HOOK_TIMEOUT_NANOS) {
			// Queued, but the pass never came to draw it.
			discard();
		}
		return lastHook != 0 && now - lastHook < HOOK_TIMEOUT_NANOS;
	}

	/**
	 * Queues geometry for this frame's effects pass: built now, at the pose stack's current pose
	 * (camera-relative, as the level's features are), drawn with {@code pipeline} (a variant of the
	 * effect's pipeline with {@code FX_LINEAR} and {@link #TARGET}).
	 */
	static void submit(RenderPipeline pipeline, List<Tex> textures, PoseStack poseStack, SubmitNodeCollector.CustomGeometryRenderer geometry) {
		if (bytes == null) {
			bytes = new ByteBufferBuilder(256 * 1024);
		}
		BufferBuilder builder = new BufferBuilder(bytes, PrimitiveTopology.QUADS, DefaultVertexFormat.ENTITY);
		geometry.render(poseStack.last(), builder);
		MeshData mesh = builder.build();
		if (mesh != null) {
			if (QUEUE.isEmpty()) {
				queuedAt = System.nanoTime();
			}
			QUEUE.add(new Draw(pipeline, textures, mesh));
		}
	}

	private static void discard() {
		for (Draw d : QUEUE) {
			d.mesh.close();
		}
		QUEUE.clear();
	}

	/**
	 * Called by {@code LevelRendererMixin} after the level's main pass is added to the frame graph:
	 * adds the effects pass when anything was queued. {@code fog} is the terrain fog the main pass
	 * draws with.
	 */
	public static void addPass(FrameGraphBuilder frame, LevelTargetBundle targets, GpuBufferSlice fog) {
		lastHook = System.nanoTime();
		if (QUEUE.isEmpty()) {
			return;
		}
		if (failed || composite == null) {
			discard();
			return;
		}
		RenderTarget main = Minecraft.getInstance().gameRenderer.mainRenderTarget();
		int w = main.width, h = main.height;
		Matrix4f modelView = RenderSystem.getModelViewMatrixCopy();
		List<Draw> draws = new ArrayList<>(QUEUE);
		QUEUE.clear();
		FramePass pass = frame.addPass("rlcar_fx");
		targets.main = pass.readsAndWrites(targets.main);
		ResourceHandle<RenderTarget> fx = pass.createsInternal("rlcar_fx_color", new RenderTargetDescriptor(w, h, FX_COLOR, null));
		ResourceHandle<RenderTarget> copy = pass.createsInternal("rlcar_fx_scene", new RenderTargetDescriptor(w, h, COPY_COLOR, null));
		ResourceHandle<RenderTarget> mainHandle = targets.main;
		pass.executes(() -> {
			try {
				render(draws, mainHandle.get(), fx.get(), copy.get(), fog, modelView);
			} catch (RuntimeException e) {
				failed = true;
				RlCar.LOG.error("RL Car: the linear effects pass failed; drawing the effects in sRGB from now on", e);
			} finally {
				for (Draw d : draws) {
					d.mesh.close();
				}
			}
		});
	}

	private static void render(List<Draw> draws, RenderTarget main, RenderTarget fx, RenderTarget copy, GpuBufferSlice fog, Matrix4f modelView) {
		CommandEncoder encoder = RenderSystem.getDevice().createCommandEncoder();
		// Upload every draw's vertices into one buffer.
		int stride = DefaultVertexFormat.ENTITY.getVertexSize();
		long size = 0;
		int maxIndices = 0;
		for (Draw d : draws) {
			size += d.mesh.vertexBuffer().remaining();
			maxIndices = Math.max(maxIndices, d.mesh.drawState().indexCount());
		}
		if (vertices == null || vertices.size() < size) {
			if (vertices != null) {
				vertices.close();
			}
			long capacity = Math.max(size * 3 / 2, 64L * 1024L);
			vertices = RenderSystem.getDevice().createBuffer(() -> "RL Car effects vertices", GpuBuffer.USAGE_VERTEX | GpuBuffer.USAGE_COPY_DST, capacity);
		}
		int[] baseVertex = new int[draws.size()];
		long offset = 0;
		for (int i = 0; i < draws.size(); i++) {
			java.nio.ByteBuffer data = draws.get(i).mesh.vertexBuffer();
			int n = data.remaining();
			encoder.writeToBuffer(vertices.slice(offset, n), data);
			baseVertex[i] = (int) (offset / stride);
			offset += n;
		}
		RenderSystem.AutoStorageIndexBuffer indices = quadIndices();
		indices.requestIndexCount(maxIndices);
		RenderSystem.resizeAllAutoStorageIndexBuffers();
		GpuBuffer indexBuffer = indices.getBuffer(maxIndices);
		GpuBufferSlice transforms = RenderSystem.getDynamicUniforms().writeTransform(modelView);
		RenderSystem.setShaderFog(fog);

		// The effects, as linear premultiplied light, over transparent black; occluded by the scene.
		try (RenderPass pass = encoder.createRenderPass(() -> "RL Car effects", fx.getColorTextureView(), Optional.of(new Vector4f(0.0F)),
			main.getDepthTextureView(), OptionalDouble.empty())) {
			RenderSystem.bindDefaultUniforms(pass);
			pass.setUniform("DynamicTransforms", transforms);
			pass.setVertexBuffer(0, vertices.slice());
			pass.setIndexBuffer(indexBuffer, indices.type());
			for (int i = 0; i < draws.size(); i++) {
				Draw d = draws.get(i);
				pass.setPipeline(RenderSystem.getCompiledPipeline(d.pipeline));
				for (Tex t : d.textures) {
					AbstractTexture texture = Minecraft.getInstance().getTextureManager().getTexture(t.texture);
					GpuSampler sampler = t.sampler == null ? null : t.sampler.get();
					pass.setUniform(t.name, texture.getTextureView(), sampler != null ? sampler : texture.getSampler());
				}
				pass.drawIndexed(d.mesh.drawState().indexCount(), 1, 0, baseVertex[i], 0);
			}
		}

		// The composite: the scene (sRGB) to linear, the effects "over" it, back to sRGB.
		copy.copyColorFrom(main);
		GpuSampler nearest = RenderSystem.getSamplerCache().getClampToEdge(FilterMode.NEAREST);
		try (RenderPass pass = encoder.createRenderPass(() -> "RL Car effects composite", main.getColorTextureView(), Optional.empty())) {
			pass.setPipeline(RenderSystem.getCompiledPipeline(composite));
			pass.setUniform("SceneSampler", copy.getColorTextureView(), nearest);
			pass.setUniform("FxSampler", fx.getColorTextureView(), nearest);
			pass.draw(3, 1, 0, 0);
		}
	}

	private static RenderSystem.AutoStorageIndexBuffer quadIndices() {
		if (quadIndices == null) {
			quadIndices = RenderSystem.getSequentialBuffer(PrimitiveTopology.QUADS);
		}
		return quadIndices;
	}

	// ----------------------------------------------------------------------------- shader packs

	private static boolean irisChecked;
	private static java.lang.reflect.@Nullable Method irisInUse;
	private static @Nullable Object iris;

	/** An Iris shader pack replaces the level rendering (and its targets): leave the effects to it. */
	private static boolean shaderPackInUse() {
		if (!irisChecked) {
			irisChecked = true;
			try {
				Class<?> api = Class.forName("net.irisshaders.iris.api.v0.IrisApi");
				iris = api.getMethod("getInstance").invoke(null);
				irisInUse = api.getMethod("isShaderPackInUse");
			} catch (ReflectiveOperationException | LinkageError e) {
				iris = null;
			}
		}
		if (iris == null || irisInUse == null) {
			return false;
		}
		try {
			return (Boolean) irisInUse.invoke(iris);
		} catch (ReflectiveOperationException | RuntimeException e) {
			return true;
		}
	}
}
