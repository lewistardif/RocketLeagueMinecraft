package dev.rlcar.client;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import com.mojang.renderpearl.api.pipeline.BlendFunction;
import com.mojang.renderpearl.api.pipeline.ColorTargetState;
import com.mojang.renderpearl.api.pipeline.CompareOp;
import com.mojang.renderpearl.api.pipeline.DepthStencilState;
import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import com.mojang.renderpearl.api.textures.FilterMode;
import dev.rlcar.RlCar;
import dev.rlcar.entity.BallEntity;
import dev.rlcar.physics.Space;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.BindGroupLayouts;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.oit.OitPipelineSet;
import net.minecraft.client.renderer.rendertype.RenderSetup;
import net.minecraft.client.renderer.rendertype.RenderType;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import net.minecraft.client.renderer.texture.OverlayTexture;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.resources.Identifier;
import net.minecraft.util.ARGB;
import net.minecraft.util.Mth;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.RenderShape;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;
import org.joml.Vector3f;
import org.jspecify.annotations.Nullable;

/**
 * The ball's markers, as Rocket League's ball FX actor ({@code FXActors.Ball.Ball_FXActor},
 * decompiled from {@code GameInfo_Soccar_SF}) attaches them:
 *
 * <ul>
 * <li>{@code GroundDecal}: a 192 x 192 uu decal projected straight down from 50 uu above the ball
 * to 3096 uu below ({@code Ball_GroundReticle_DMat}, additive): a ring the size of the ball on
 * the ground under it, and an inner ring that closes in as the ball climbs;</li>
 * <li>{@code GroundLinePSC} ({@code Ball_LocationBeam01_PS}), one second after the ball appears:
 * a faint dashed line 2500 uu down from the ball, a 192 uu ring around the ball drawn through
 * everything once the ball is far away (its outline), and a dark halo behind the ball (the
 * inside of a 128 uu "clarity sphere").</li>
 * </ul>
 *
 * The game hides both on its {@code HideWorldUI} event; here they hide with the HUD (F1). The
 * materials are ports of the game's shaders ({@code rl_marker.fsh}); the decal's cross cut needs
 * the extracted {@code Reticles_01_Pack} texture ({@code ball/}), the rest needs nothing extracted.
 *
 * <p>The decal lies on the top faces of the blocks under the ball (the first collision surface in
 * each block column of its box), the way Minecraft lays entity shadows.
 */
public final class BallMarker {
	private static final float UU = Space.UU_PER_BLOCK;
	// Ball_FXActor's DecalComponent.
	private static final float DECAL_SIZE = 192.0F / UU;
	private static final float DECAL_NEAR = 50.0F / UU;
	private static final float DECAL_FAR = 3096.0F / UU;
	// Ball_LocationBeam01_PS: the reticle sprite (192 uu), the beam (2 x 2500 uu, opacity 1/16,
	// colour ReticleColor's default 1 remapped to 0..8) and the ClaritySphere mesh (radius 128 uu).
	private static final float RETICLE_SIZE = 192.0F / UU;
	private static final float BEAM_WIDTH = 2.0F / UU;
	private static final float BEAM_LENGTH = 2500.0F / UU;
	private static final float BEAM_ALPHA = 0.0625F;
	private static final float SPHERE_RADIUS = 128.0F / UU;
	/** GroundLinePSC's AttachDelay (1 s). */
	private static final int LINE_DELAY_TICKS = 20;
	/** The decal floats this far above the surface it lies on (blocks), clear of z-fighting. */
	private static final float LIFT = 0.002F;
	private static final int SPHERE_RINGS = 16;
	private static final int SPHERE_SEGMENTS = 32;

	private static final int DECAL = 1;
	private static final int RETICLE = 2;
	private static final int BEAM = 3;
	private static final int SPHERE = 4;
	private static final DepthStencilState TEST_NO_WRITE = new DepthStencilState(CompareOp.GREATER_THAN_OR_EQUAL, false);
	private static final DepthStencilState NO_TEST = new DepthStencilState(CompareOp.ALWAYS_PASS, false);
	private static final Identifier WHITE = Identifier.withDefaultNamespace("textures/block/white_concrete.png");

	private static final @Nullable RenderPipeline[] PIPELINES = new RenderPipeline[5];
	private static final @Nullable OitPipelineSet[] OIT = new OitPipelineSet[5];
	private static final @Nullable RenderType[] TYPES = new RenderType[5];
	private static boolean typesMade;
	private static @Nullable Path reticleTexture;
	/** The clarity sphere: quads facing inwards (x, y, z, inward normal x, y, z per vertex). */
	private static final float[] SPHERE_MESH = sphere();

	private BallMarker() {
	}

	// ----------------------------------------------------------------------------- pipelines

	/** Registers the four marker pipelines (before the first resource load compiles them). */
	public static void registerPipelines() {
		reticleTexture = findReticle();
		for (int kind = DECAL; kind <= SPHERE; kind++) {
			RenderPipeline.Builder b = RenderPipeline.builder()
				.withVertexShader(RlCar.id("core/rl_marker"))
				.withFragmentShader(RlCar.id("core/rl_marker"))
				.withBindGroupLayout(BindGroupLayouts.SAMPLER0)
				.withVertexBinding(0, DefaultVertexFormat.ENTITY)
				.withPrimitiveTopology(PrimitiveTopology.QUADS)
				// Reticle_Mat has bDisableDepthTest: the ball's outline shows through everything.
				.withDepthStencilState(kind == RETICLE ? NO_TEST : TEST_NO_WRITE)
				// Only the far, inward-facing half of the clarity sphere is drawn.
				.withCull(kind == SPHERE)
				.withShaderDefine("MARKER_KIND", kind);
			if (kind == DECAL && reticleTexture != null) {
				b.withShaderDefine("RETICLE_TEXTURE");
			}
			RenderPipeline.Snippet snippet = b.buildSnippet();
			BlendFunction blend = kind == DECAL ? BlendFunction.ADDITIVE : kind == BEAM ? BlendFunction.TRANSLUCENT_PREMULTIPLIED_ALPHA : BlendFunction.TRANSLUCENT;
			PIPELINES[kind] = RenderPipelines.register(RenderPipeline.builder(RenderPipelines.MATRICES_FOG_SNIPPET, snippet)
				.withLocation(RlCar.id("pipeline/ball_marker_" + kind))
				.withColorTargetState(new ColorTargetState(blend))
				.build());
			if (kind != RETICLE) {
				// The reticle stays out of the order-independent transparency pass, which tests depth.
				RenderPipeline.Builder oit = RenderPipeline.builder(snippet)
					.withBindGroupLayout(BindGroupLayouts.DYNAMIC_TRANSFORMS)
					.withBindGroupLayout(BindGroupLayouts.FOG);
				if (kind == DECAL) {
					oit.withShaderDefine("OIT_ADDITIVE");
				}
				OIT[kind] = RenderPipelines.register(OitPipelineSet.builder("rlcar_ball_marker_" + kind, oit).build());
			}
		}
	}

	/** {@code ball/<reticle>} from the extraction's {@code ball/materials.json}, or null. */
	private static @Nullable Path findReticle() {
		Path root = RlModels.root();
		if (root == null) {
			return null;
		}
		Path json = root.resolve("ball/materials.json");
		try {
			if (Files.isRegularFile(json)) {
				JsonObject j = JsonParser.parseString(Files.readString(json)).getAsJsonObject();
				if (j.has("reticle")) {
					Path p = root.resolve("ball").resolve(j.get("reticle").getAsString());
					return Files.isRegularFile(p) ? p : null;
				}
			}
		} catch (IOException | RuntimeException e) {
			RlCar.LOG.error("RL Car: cannot read {}", json, e);
		}
		return null;
	}

	private static @Nullable RenderType type(int kind) {
		if (!typesMade) {
			typesMade = true;
			for (int k = DECAL; k <= SPHERE; k++) {
				RenderPipeline pipeline = PIPELINES[k];
				if (pipeline == null) {
					continue;
				}
				try {
					RenderSetup.RenderSetupBuilder setup = RenderSetup.builder(pipeline);
					OitPipelineSet oit = OIT[k];
					if (oit != null) {
						setup.setOitPipelines(oit);
					}
					if (k == DECAL && reticleTexture != null) {
						setup.withTexture("Sampler0", RlModels.texture(reticleTexture), () -> RenderSystem.getSamplerCache().getClampToEdge(FilterMode.LINEAR, true));
					} else {
						setup.withTexture("Sampler0", WHITE); // sampled by nothing, bound all the same
					}
					if (k == SPHERE) {
						setup.sortOnUpload();
					}
					TYPES[k] = RenderType.create("rlcar_ball_marker_" + k, setup.createRenderSetup());
				} catch (IOException | RuntimeException e) {
					RlCar.LOG.error("RL Car: cannot set up the ball marker {}", k, e);
				}
			}
		}
		return TYPES[kind];
	}

	// ----------------------------------------------------------------------------- extraction

	/**
	 * Fills the markers of {@code state} for a ball centred at {@code state.x, y, z}: whether they
	 * show, the decal's quads on the ground (relative to the centre) and the ball's altitude above
	 * the ground under it (the decal's {@code Altitude} parameter, 0..1024 uu as 0..1).
	 */
	static void extract(BallEntity ball, BallRenderState state) {
		state.markers = !Minecraft.getInstance().gui.hud.isHidden();
		state.line = state.markers && ball.tickCount >= LINE_DELAY_TICKS;
		state.decalQuads = 0;
		if (!state.markers) {
			return;
		}
		Level level = ball.level();
		double cx = state.x, cy = state.y, cz = state.z;
		double half = DECAL_SIZE / 2.0;
		double x0 = cx - half, x1 = cx + half, z0 = cz - half, z1 = cz + half;
		double top = cy + DECAL_NEAR, bottom = cy - DECAL_FAR;
		int centreX = Mth.floor(cx), centreZ = Mth.floor(cz);
		double altitude = Double.MAX_VALUE;
		BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
		for (int bx = Mth.floor(x0); bx <= Mth.floor(x1); bx++) {
			for (int bz = Mth.floor(z0); bz <= Mth.floor(z1); bz++) {
				for (int by = Mth.floor(top); by >= Mth.floor(bottom); by--) {
					pos.set(bx, by, bz);
					BlockState block = level.getBlockState(pos);
					// Decals land on what is drawn: not on barriers and the like.
					VoxelShape shape = block.getRenderShape() == RenderShape.INVISIBLE ? Shapes.empty() : block.getCollisionShape(level, pos);
					if (shape.isEmpty()) {
						continue;
					}
					double surface = by + shape.max(Direction.Axis.Y);
					if (surface > top || surface < bottom) {
						break; // a wall over the ball's height, or nothing in reach
					}
					for (AABB box : shape.toAabbs()) {
						double y = by + box.maxY;
						double ax0 = Math.max(x0, bx + box.minX), ax1 = Math.min(x1, bx + box.maxX);
						double az0 = Math.max(z0, bz + box.minZ), az1 = Math.min(z1, bz + box.maxZ);
						if (y <= top && ax1 > ax0 && az1 > az0) {
							decalQuad(state, (float) (ax0 - cx), (float) (ax1 - cx), (float) (y + LIFT - cy), (float) (az0 - cz), (float) (az1 - cz));
						}
					}
					if (bx == centreX && bz == centreZ) {
						altitude = cy - surface;
					}
					break;
				}
			}
		}
		state.altitude = altitude == Double.MAX_VALUE ? 1.0F : Mth.clamp((float) (altitude * UU / 1024.0), 0.0F, 1.0F);
	}

	/** A quad on the ground, relative to the ball centre; UVs across the decal's box. */
	private static void decalQuad(BallRenderState state, float x0, float x1, float y, float z0, float z1) {
		int n = state.decalQuads * 20;
		if (state.decal.length < n + 20) {
			state.decal = Arrays.copyOf(state.decal, Math.max(n + 20, state.decal.length * 2));
		}
		float[] q = {x0, z1, x1, z1, x1, z0, x0, z0}; // counter-clockwise seen from above
		for (int k = 0; k < 4; k++) {
			float x = q[k * 2], z = q[k * 2 + 1];
			state.decal[n++] = x;
			state.decal[n++] = y;
			state.decal[n++] = z;
			state.decal[n++] = x / DECAL_SIZE + 0.5F;
			state.decal[n++] = z / DECAL_SIZE + 0.5F;
		}
		state.decalQuads++;
	}

	// ----------------------------------------------------------------------------- drawing

	/** Draws the markers; the pose stack is at the ball's centre, world-aligned. */
	static void submit(SubmitNodeCollector collector, PoseStack poseStack, BallRenderState state, CameraRenderState camera) {
		if (!state.markers) {
			return;
		}
		RenderType decal = type(DECAL);
		if (state.decalQuads > 0 && decal != null) {
			int color = ARGB.colorFromFloat(1.0F, state.altitude, 1.0F, 1.0F);
			float[] d = state.decal;
			int quads = state.decalQuads;
			collector.submitCustomGeometry(poseStack, decal, (pose, buf) -> {
				for (int i = 0; i < quads * 4; i++) {
					vertex(buf, pose, d[i * 5], d[i * 5 + 1], d[i * 5 + 2], d[i * 5 + 3], d[i * 5 + 4], color, 0, 1, 0);
				}
			});
		}
		if (!state.line) {
			return;
		}
		Vector3f toCam = new Vector3f((float) (camera.pos.x - state.x), (float) (camera.pos.y - state.y), (float) (camera.pos.z - state.z));

		// The beam: a vertical strip turned towards the camera.
		RenderType beam = type(BEAM);
		float horizontal = Mth.sqrt(toCam.x * toCam.x + toCam.z * toCam.z);
		if (beam != null && horizontal > 1.0E-4F) {
			float rx = toCam.z / horizontal * BEAM_WIDTH * 0.5F, rz = -toCam.x / horizontal * BEAM_WIDTH * 0.5F;
			int color = ARGB.colorFromFloat(BEAM_ALPHA, 1.0F, 1.0F, 1.0F);
			collector.submitCustomGeometry(poseStack, beam, (pose, buf) -> {
				vertex(buf, pose, -rx, 0, -rz, 0, 0, color, 0, 1, 0);
				vertex(buf, pose, -rx, -BEAM_LENGTH, -rz, 0, 1, color, 0, 1, 0);
				vertex(buf, pose, rx, -BEAM_LENGTH, rz, 1, 1, color, 0, 1, 0);
				vertex(buf, pose, rx, 0, rz, 1, 0, color, 0, 1, 0);
			});
		}

		// The clarity sphere, then the reticle over everything.
		RenderType sphere = type(SPHERE);
		if (sphere != null) {
			collector.submitCustomGeometry(poseStack, sphere, (pose, buf) -> {
				float[] m = SPHERE_MESH;
				for (int i = 0; i < m.length; i += 6) {
					vertex(buf, pose, m[i], m[i + 1], m[i + 2], 0, 0, 0xFF000000, m[i + 3], m[i + 4], m[i + 5]);
				}
			});
		}
		RenderType reticle = type(RETICLE);
		if (reticle != null) {
			Vector3f right = camera.orientation.transform(new Vector3f(RETICLE_SIZE * 0.5F, 0, 0));
			Vector3f up = camera.orientation.transform(new Vector3f(0, RETICLE_SIZE * 0.5F, 0));
			collector.submitCustomGeometry(poseStack, reticle, (pose, buf) -> {
				vertex(buf, pose, -right.x + up.x, -right.y + up.y, -right.z + up.z, 0, 0, -1, 0, 1, 0);
				vertex(buf, pose, -right.x - up.x, -right.y - up.y, -right.z - up.z, 0, 1, -1, 0, 1, 0);
				vertex(buf, pose, right.x - up.x, right.y - up.y, right.z - up.z, 1, 1, -1, 0, 1, 0);
				vertex(buf, pose, right.x + up.x, right.y + up.y, right.z + up.z, 1, 0, -1, 0, 1, 0);
			});
		}
	}

	private static void vertex(VertexConsumer b, PoseStack.Pose p, float x, float y, float z, float u, float v, int argb, float nx, float ny, float nz) {
		b.addVertex(p, x, y, z).setColor(argb).setUv(u, v).setOverlay(OverlayTexture.NO_OVERLAY).setLight(0).setNormal(p, nx, ny, nz);
	}

	/** How far below the ball its markers reach (blocks), for culling. */
	static float reach() {
		return DECAL_FAR;
	}

	/** How far around the ball its markers reach (blocks), for culling. */
	static float radius() {
		return Math.max(SPHERE_RADIUS, DECAL_SIZE * 0.5F * Mth.SQRT_OF_TWO);
	}

	/** A sphere of the clarity sphere's radius as quads wound towards the inside, with inward normals. */
	private static float[] sphere() {
		float[] out = new float[SPHERE_RINGS * SPHERE_SEGMENTS * 4 * 6];
		int n = 0;
		for (int i = 0; i < SPHERE_RINGS; i++) {
			float t0 = Mth.PI * i / SPHERE_RINGS, t1 = Mth.PI * (i + 1) / SPHERE_RINGS;
			for (int j = 0; j < SPHERE_SEGMENTS; j++) {
				float p0 = Mth.TWO_PI * j / SPHERE_SEGMENTS, p1 = Mth.TWO_PI * (j + 1) / SPHERE_SEGMENTS;
				// The reverse of BallRenderer's outward (counter-clockwise from outside) order.
				float[][] corners = {{t0, p0}, {t1, p0}, {t1, p1}, {t0, p1}};
				for (float[] c : corners) {
					float nx = Mth.sin(c[0]) * Mth.cos(c[1]), ny = Mth.cos(c[0]), nz = Mth.sin(c[0]) * Mth.sin(c[1]);
					out[n++] = nx * SPHERE_RADIUS;
					out[n++] = ny * SPHERE_RADIUS;
					out[n++] = nz * SPHERE_RADIUS;
					out[n++] = -nx;
					out[n++] = -ny;
					out[n++] = -nz;
				}
			}
		}
		return out;
	}
}
