package dev.rlcar.client;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.renderpearl.api.pipeline.BindGroupLayout;
import com.mojang.renderpearl.api.pipeline.ColorTargetState;
import com.mojang.renderpearl.api.pipeline.DepthStencilState;
import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import com.mojang.renderpearl.api.pipeline.UniformType;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuSampler;
import dev.rlcar.RlCar;
import dev.rlcar.physics.RlCarNative;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.Locale;
import java.util.Map;
import java.util.function.Supplier;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.rendertype.RenderSetup;
import net.minecraft.client.renderer.rendertype.RenderType;
import net.minecraft.resources.Identifier;
import net.minecraft.util.Mth;
import net.minecraft.world.attribute.EnvironmentAttributeProbe;
import net.minecraft.world.attribute.EnvironmentAttributes;
import net.minecraft.world.level.dimension.DimensionType;
import org.joml.Vector3f;
import org.joml.Vector3fc;
import org.jspecify.annotations.Nullable;

/**
 * Draws the cars and the wheel with ports of Rocket League's own material shaders, decompiled from
 * the game's shader cache: the body paint ({@code Body_Paintable_Mat}, with its windows, trims
 * and team colours), the chassis ({@code MasterChassis_MAT}, with the head and tail lights) and
 * the wheels ({@code Wheel_Master_Mat}). The shaders are in
 * {@code assets/rlcar/shaders/core/rl_*.fsh}; their inputs (the unbaked textures and each
 * material instance's parameter values) are written next to the models by
 * {@code tools/rl_assets/extract.py} as {@code materials.json} and {@code shading/}.
 *
 * <p>The game's lights are replaced by Minecraft's: the sun or the moon is the directional light,
 * the lightmap at the car is the ambient light, and the sky and fog colours make up the reflected
 * environment. The frame's light direction and sky colour are packed into the vertices (the
 * vertex colour and the overlay coordinates, which these shaders do not otherwise use).
 *
 * <p>Material parameters are compiled into each pipeline as defines, so a re-extracted car needs
 * a restart. Without the files the renderer keeps the plain textured models.
 */
public final class RlShading {
	private enum Kind {
		BODY("core/rl_body", new String[] {"TertiaryNormalMap", "BodyMaskMap", "DiffuseMap", "SkinMap", "PartsNormalMap", "Detail1Map", "Detail2Map", "LightRampMap", "CurvatureMap", "EnvPackMap"}),
		CHASSIS("core/rl_chassis", new String[] {"LightRampMap", "DiffuseMap", "MasksMap"}),
		WHEEL("core/rl_wheel", new String[] {"RimNormalMap", "RimAddNormalMap", "TireNormalMap", "SwirlMap", "RimDiffuseMap", "TireDiffuseMap", "TireMaskMap"}),
		GLASS("core/rl_glass", new String[0]),
		BASIC("core/rl_basic", new String[] {"BaseMap"});

		final String shader;
		final String[] samplers;

		Kind(String shader, String[] samplers) {
			this.shader = shader;
			this.samplers = samplers;
		}
	}

	/** One material instance of a model, as {@code materials.json} describes it. */
	private record Material(Kind kind, Map<String, Path> textures, Map<String, float[]> params) {
	}

	/** The materials of one model folder ({@code cars/<preset>} or {@code wheel}) and the team colours. */
	private record Folder(Map<String, Material> materials, Map<String, float[]> teams) {
	}

	private static final String[] TEAMS = {"blue", "orange"};
	private static final Map<String, @Nullable Folder> FOLDERS = new HashMap<>();
	private static final Map<String, RenderPipeline> PIPELINES = new HashMap<>();
	private static final Map<String, @Nullable RenderType> TYPES = new HashMap<>();

	private RlShading() {
	}

	// ----------------------------------------------------------------------------- data

	private static @Nullable Folder folder(String name) {
		if (FOLDERS.containsKey(name)) {
			return FOLDERS.get(name);
		}
		Folder f = null;
		Path root = RlModels.root();
		Path file = root == null ? null : root.resolve(name).resolve("materials.json");
		if (file != null && Files.isRegularFile(file) && Files.isRegularFile(root.resolve("shading/LightFalloffArray.png"))) {
			try {
				f = parse(file, root.resolve("shading"));
			} catch (IOException | RuntimeException e) {
				RlCar.LOG.error("RL Car: cannot read {}; drawing {} with plain textures", file, name, e);
			}
		}
		FOLDERS.put(name, f);
		return f;
	}

	private static Folder parse(Path file, Path shared) throws IOException {
		JsonObject j = JsonParser.parseString(Files.readString(file)).getAsJsonObject();
		Path dir = file.getParent();
		Map<String, Material> materials = new LinkedHashMap<>();
		for (Map.Entry<String, JsonElement> e : j.getAsJsonObject("materials").entrySet()) {
			JsonObject m = e.getValue().getAsJsonObject();
			Kind kind = Kind.valueOf(m.get("kind").getAsString().toUpperCase(Locale.ROOT));
			Map<String, Path> textures = new HashMap<>();
			if (m.has("textures")) {
				for (Map.Entry<String, JsonElement> t : m.getAsJsonObject("textures").entrySet()) {
					if (!t.getValue().isJsonNull()) {
						textures.put(t.getKey(), dir.resolve(t.getValue().getAsString()));
					}
				}
			}
			textures.put("@lut", shared.resolve("LightFalloffArray.png"));
			textures.put("@env", shared.resolve("ENVPack.png"));
			textures.put("@swirl", shared.resolve("Swirls_D.png"));
			textures.put("@tertiary", shared.resolve("CarbonFiber_Flipped_N.png"));
			textures.put("@flat", shared.resolve("flat_normal.png"));
			textures.put("@flat_xa", shared.resolve("flat_normal_xa.png"));
			textures.put("@black", shared.resolve("black.png"));
			Map<String, float[]> params = new HashMap<>();
			if (m.has("params")) {
				for (Map.Entry<String, JsonElement> p : m.getAsJsonObject("params").entrySet()) {
					params.put(p.getKey(), floats(p.getValue()));
				}
			}
			materials.put(e.getKey(), new Material(kind, textures, params));
		}
		Map<String, float[]> teams = new HashMap<>();
		if (j.has("teams")) {
			for (Map.Entry<String, JsonElement> t : j.getAsJsonObject("teams").entrySet()) {
				teams.put(t.getKey(), floats(t.getValue()));
			}
		}
		return new Folder(materials, teams);
	}

	private static float[] floats(JsonElement e) {
		if (e.isJsonArray()) {
			JsonArray a = e.getAsJsonArray();
			float[] out = new float[a.size()];
			for (int i = 0; i < out.length; i++) {
				out[i] = a.get(i).getAsFloat();
			}
			return out;
		}
		return new float[] {e.getAsFloat()};
	}

	// ----------------------------------------------------------------------------- pipelines

	/** Registers a pipeline per material (per team for car bodies), before the first resource load compiles them. */
	public static void registerPipelines() {
		if (RlModels.root() == null) {
			return;
		}
		for (String preset : RlCarNative.PRESETS) {
			register("cars/" + preset);
		}
		register("wheel");
	}

	private static void register(String folderName) {
		Folder f = folder(folderName);
		if (f == null) {
			return;
		}
		for (Map.Entry<String, Material> e : f.materials.entrySet()) {
			Material m = e.getValue();
			for (String team : m.kind == Kind.BODY ? TEAMS : new String[] {"any"}) {
				String key = key(folderName, e.getKey(), team);
				RenderPipeline.Builder b = RenderPipeline.builder(RenderPipelines.MATRICES_FOG_SNIPPET)
					.withLocation(RlCar.id("pipeline/rl_" + key.replace('/', '_')))
					.withVertexShader(RlCar.id("core/rl_car"))
					.withFragmentShader(RlCar.id(m.kind.shader))
					.withBindGroupLayout(layout(m.kind))
					.withVertexBinding(0, DefaultVertexFormat.ENTITY)
					.withPrimitiveTopology(PrimitiveTopology.QUADS)
					.withDepthStencilState(DepthStencilState.DEFAULT)
					.withColorTargetState(ColorTargetState.DEFAULT)
					.withCull(false);
				defines(b, m, f, team);
				PIPELINES.put(key, RenderPipelines.register(b.build()));
			}
		}
	}

	private static BindGroupLayout layout(Kind kind) {
		BindGroupLayout.Builder b = BindGroupLayout.builder().withUniform("Sampler2", UniformType.COMBINED_IMAGE_SAMPLER);
		for (String s : kind.samplers) {
			b.withUniform(s, UniformType.COMBINED_IMAGE_SAMPLER);
		}
		return b.build();
	}

	private static String key(String folder, String material, String team) {
		return (folder + "/" + material + "/" + team).toLowerCase(Locale.ROOT).replaceAll("[^a-z0-9/._-]", "_");
	}

	/** The material's parameters, in the shader's cb0 register layout (see the shaders' headers). */
	private static void defines(RenderPipeline.Builder b, Material m, Folder f, String team) {
		Map<String, float[]> p = m.params;
		switch (m.kind) {
			case BODY -> {
				vec(b, 58, p.get("TertiaryMaterial_ControlA"));
				// The body's primary paint is CustomColor in the finish the cars use (Skin.a = 1 selects
				// it over TeamColor), so that is where the game puts the team's colour.
				float[] teamColor = f.teams.get(team);
				vec(b, 59, p.get("TeamColor"));
				vec(b, 60, teamColor != null ? teamColor : p.get("CustomColor"));
				vec(b, 61, p.get("F1ControlA"));
				vec(b, 62, p.get("F2ControlA"));
				vec(b, 63, p.get("F1ControlB"));
				vec(b, 64, p.get("F2ControlB"));
				vec(b, 65, p.get("PaintColor"));
				vec(b, 66, p.get("TrimColor"));
				vec(b, 67, p.get("TertiaryMaterial_Color"));
				vec(b, 68, p.get("TertiaryMaterial_ControlB"));
				vec(b, 69, new float[] {scalar(p, "TertiaryNormalTiling", 64.0F), 0, 0, scalar(p, "F1Type", 0) * 0.03125F});
				vec(b, 70, new float[] {0, 0, scalar(p, "F2Type", 0) * 0.03125F, scalar(p, "TertiaryMaterial_Type", 2.0F)});
			}
			case CHASSIS -> {
				vec(b, 59, p.get("TailLightColor"));
				vec(b, 60, p.get("HeadlightColor"));
				vec(b, 61, p.get("BoostGlowColor"));
				b.withShaderDefine("BRAKE", scalar(p, "Brake", 0));
			}
			case WHEEL -> {
				float power = scalar(p, "Rim_AdditionalNormal_Power", 1.0F);
				vec(b, 58, new float[] {0.1F * power, 0.1F * power, 0, 0});
				vec(b, 59, p.get("RimColor"));
				vec(b, 60, new float[] {power, scalar(p, "ReflectionBrightness", 0.36F), scalar(p, "SpecIntensity", 10.0F), scalar(p, "SpecPower", 40.0F)});
			}
			default -> {
			}
		}
	}

	private static void vec(RenderPipeline.Builder b, int register, float @Nullable [] v) {
		String[] c = {"X", "Y", "Z", "W"};
		for (int i = 0; i < 4; i++) {
			b.withShaderDefine("P" + register + "_" + c[i], v != null && i < v.length ? v[i] : (i == 3 ? 1.0F : 0.0F));
		}
	}

	private static float scalar(Map<String, float[]> p, String name, float fallback) {
		float[] v = p.get(name);
		return v != null && v.length > 0 ? v[0] : fallback;
	}

	// ----------------------------------------------------------------------------- render types

	private static @Nullable RenderType renderType(String folderName, String material, boolean orange, Identifier baseTexture) {
		Folder f = folder(folderName);
		Material m = f == null ? null : f.materials.get(material);
		if (m == null) {
			return null;
		}
		String key = key(folderName, material, m.kind == Kind.BODY ? (orange ? "orange" : "blue") : "any");
		String typeKey = m.kind == Kind.BASIC ? key + "@" + baseTexture : key;
		if (TYPES.containsKey(typeKey)) {
			return TYPES.get(typeKey);
		}
		RenderType type = null;
		RenderPipeline pipeline = PIPELINES.get(key);
		if (pipeline != null) {
			try {
				RenderSetup.RenderSetupBuilder setup = RenderSetup.builder(pipeline).useLightmap();
				switch (m.kind) {
					case BODY -> {
						bind(setup, "TertiaryNormalMap", m, "TertiaryMaterial_Normal", "@tertiary", true);
						bind(setup, "BodyMaskMap", m, "BodyMasks", "@black", true);
						bind(setup, "DiffuseMap", m, "Diffuse", "@black", true);
						bind(setup, "SkinMap", m, "Skin", "@black", true);
						bind(setup, "PartsNormalMap", m, "Normal", "@flat_xa", true);
						bind(setup, "Detail1Map", m, "F1DetailNormal", "@flat", true);
						bind(setup, "Detail2Map", m, "F2DetailNormal", "@flat", true);
						bind(setup, "LightRampMap", m, "@lut", "@lut", false);
						bind(setup, "CurvatureMap", m, "CurvaturePack", "@black", true);
						bind(setup, "EnvPackMap", m, "@env", "@env", true);
					}
					case CHASSIS -> {
						bind(setup, "LightRampMap", m, "@lut", "@lut", false);
						bind(setup, "DiffuseMap", m, "Diffuse", "@black", true);
						bind(setup, "MasksMap", m, "Masks", "@black", true);
					}
					case WHEEL -> {
						bind(setup, "RimNormalMap", m, "RimNormal", "@flat_xa", true);
						bind(setup, "RimAddNormalMap", m, "Rim_AdditionalNormal", "@flat_xa", true);
						bind(setup, "TireNormalMap", m, "TireNormal", "@flat", true);
						bind(setup, "SwirlMap", m, "@swirl", "@swirl", true);
						bind(setup, "RimDiffuseMap", m, "RimDiffuse", "@black", true);
						bind(setup, "TireDiffuseMap", m, "TireDiffuse", "@black", true);
						bind(setup, "TireMaskMap", m, "RimRGB", "@black", true);
					}
					case BASIC -> setup.withTexture("BaseMap", baseTexture);
					case GLASS -> {
					}
				}
				type = RenderType.create("rlcar_" + typeKey.replaceAll("[^a-z0-9_]", "_"), setup.createRenderSetup());
			} catch (IOException | RuntimeException e) {
				RlCar.LOG.error("RL Car: cannot set up the {} material; drawing it with plain textures", material, e);
			}
		}
		TYPES.put(typeKey, type);
		return type;
	}

	/**
	 * Binds a texture: the material's own, else a stand-in (flat normal, black mask). The lighting
	 * ramp atlas is read texel by texel (nearest, no mips, clamped) as the game addresses its rows.
	 */
	private static void bind(RenderSetup.RenderSetupBuilder setup, String sampler, Material m, String name, String fallback, boolean filtered) throws IOException {
		Path p = m.textures.getOrDefault(name, m.textures.get(fallback));
		if (p == null || !Files.isRegularFile(p)) {
			p = m.textures.get(fallback);
		}
		Supplier<GpuSampler> s = filtered
			? () -> RenderSystem.getSamplerCache().getRepeat(FilterMode.LINEAR, true)
			: () -> RenderSystem.getSamplerCache().getClampToEdge(FilterMode.NEAREST, false);
		setup.withTexture(sampler, RlModels.texture(p), s);
	}

	// ----------------------------------------------------------------------------- environment

	/** This frame's light, packed for the vertices: colour (ARGB) and the two overlay coordinates. */
	private record Env(int color, int uv1x, int uv1y) {
	}

	private static Env env() {
		Minecraft mc = Minecraft.getInstance();
		ClientLevel level = mc.level;
		if (level == null) {
			return new Env(pack(new Vector3f(0.3F, 1.0F, 0.2F).normalize(), 1.0F), 0x9F86, 0x00FF);
		}
		float pt = mc.getDeltaTracker().getGameTimeDeltaPartialTick(false);
		EnvironmentAttributeProbe probe = mc.gameRenderer.mainCamera().attributeProbe();
		// The sky renderer's transform: rotate Y by -90 degrees, then X by the angle; the body sits at +Y.
		float sunAngle = probe.getValue(EnvironmentAttributes.SUN_ANGLE, pt) * Mth.DEG_TO_RAD;
		float moonAngle = probe.getValue(EnvironmentAttributes.MOON_ANGLE, pt) * Mth.DEG_TO_RAD;
		Vector3f sun = new Vector3f(-Mth.sin(sunAngle), Mth.cos(sunAngle), 0);
		Vector3f moon = new Vector3f(-Mth.sin(moonAngle), Mth.cos(moonAngle), 0);
		boolean hasSky = level.dimensionType().skybox() == DimensionType.Skybox.OVERWORLD;
		boolean night = sun.y < -0.05F;
		Vector3f dir = night ? moon : sun;
		float clear = (1.0F - 0.65F * level.getRainLevel(pt)) * (1.0F - 0.5F * level.getThunderLevel(pt));
		float strength = hasSky ? smoothstep(-0.05F, 0.25F, dir.y) * clear : 0.0F;
		if (dir.y < 0.02F) {
			dir.y = 0.02F; // never light the car from below the horizon
			dir.normalize();
		}
		Vector3fc sky = probe.getValue(EnvironmentAttributes.SKY_COLOR, pt);
		int r = channel(sky.x()), g = channel(sky.y()), bl = channel(sky.z());
		return new Env(pack(dir, strength), r | g << 8, bl | (night ? 255 : 0) << 8);
	}

	private static float smoothstep(float a, float b, float x) {
		float t = Mth.clamp((x - a) / (b - a), 0.0F, 1.0F);
		return t * t * (3.0F - 2.0F * t);
	}

	private static int channel(float v) {
		return Math.round(Mth.clamp(v, 0.0F, 1.0F) * 255.0F);
	}

	private static int pack(Vector3f dir, float strength) {
		return channel(strength) << 24 | channel(dir.x * 0.5F + 0.5F) << 16 | channel(dir.y * 0.5F + 0.5F) << 8 | channel(dir.z * 0.5F + 0.5F);
	}

	// ----------------------------------------------------------------------------- drawing

	/**
	 * Draws a model of {@code folderName} ({@code cars/<preset>} or {@code wheel}) with the ported
	 * materials. False (nothing drawn) when its materials are not available.
	 */
	public static boolean submit(SubmitNodeCollector collector, PoseStack poseStack, RlModels.Model model, String folderName, boolean orange, int light) {
		if (folder(folderName) == null) {
			return false;
		}
		RenderType[] types = new RenderType[model.parts().size()];
		for (int i = 0; i < types.length; i++) {
			RlModels.Part part = model.parts().get(i);
			types[i] = renderType(folderName, part.material(), orange, part.texture());
			if (types[i] == null) {
				return false;
			}
		}
		Env env = env();
		for (int i = 0; i < types.length; i++) {
			RlModels.Part part = model.parts().get(i);
			collector.submitCustomGeometry(poseStack, types[i], (p, b) -> {
				float[] pos = part.positions();
				float[] nrm = part.normals();
				float[] uv = part.uvs();
				int[] tri = part.triangles();
				for (int t = 0; t + 2 < tri.length; t += 3) {
					for (int k = 0; k < 4; k++) {
						int v = tri[t + Math.min(k, 2)];
						b.addVertex(p, pos[v * 3], pos[v * 3 + 1], pos[v * 3 + 2])
							.setColor(env.color)
							.setUv(uv[v * 2], uv[v * 2 + 1])
							.setUv1(env.uv1x, env.uv1y)
							.setLight(light)
							.setNormal(p, nrm[v * 3], nrm[v * 3 + 1], nrm[v * 3 + 2]);
					}
				}
			});
		}
		return true;
	}
}
