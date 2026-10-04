package dev.rlcar.client;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.renderpearl.api.pipeline.BlendFunction;
import com.mojang.renderpearl.api.pipeline.ColorTargetState;
import com.mojang.renderpearl.api.pipeline.CompareOp;
import com.mojang.renderpearl.api.pipeline.DepthStencilState;
import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import com.mojang.renderpearl.api.textures.FilterMode;
import dev.rlcar.RlCar;
import dev.rlcar.client.RlBoost.Dist;
import dev.rlcar.client.RlBoost.Rng;
import dev.rlcar.entity.CarEntity;
import dev.rlcar.physics.CarPose;
import dev.rlcar.physics.RlCarNative;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Set;
import net.minecraft.client.renderer.BindGroupLayouts;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.oit.OitPipelineSet;
import net.minecraft.client.renderer.rendertype.RenderSetup;
import net.minecraft.client.renderer.rendertype.RenderType;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import net.minecraft.client.renderer.texture.OverlayTexture;
import org.joml.Matrix3f;
import org.joml.Vector3f;
import org.jspecify.annotations.Nullable;

/**
 * The car's visual effects besides the boost, and its camera shakes: a port of the Bevy demo's
 * {@code crates/rl_car_bevy/src/fx.rs}.
 *
 * <p>With the extracted effects ({@code fx/fx.json} next to the car models, written by
 * {@code tools/rl_assets/extract.py} from the game's FX actors and particle systems), the cars show
 * what the game's cars show:
 * <ul>
 * <li>jump smoke ({@code Jump_Metal_PS}), double jump and dodge smoke, glow and the corner ribbons
 * ({@code Dodge_PS}), on the FX actor's Jump / DoubleJump / Dodge events;</li>
 * <li>while supersonic: the speed streaks around the car (your own car, by team, as in the game)
 * and the wheel trails ({@code WheelFX_Supersonic_PS}) on the back wheels touching the ground;</li>
 * <li>sparks where the body hits the world ({@code VehicleCollisionEffects.FX.Metal_PS});</li>
 * <li>the jump, double jump, dodge, landing, impact and boost camera shakes on the driven car's
 * camera (scaled by impact momentum as the game scales them).</li>
 * </ul>
 * Particle systems are simulated on the CPU from the extracted Cascade modules, in the emitters'
 * module order the way UE3 applies them, in Unreal space (uu, Z up), and drawn as camera-facing
 * sprites and ribbons with ports of the game's compiled pixel shaders
 * ({@code assets/rlcar/shaders/core/rl_fx.fsh}). Not reproduced (as in the Bevy demo): the jump's
 * distortion sphere, and where the native FX code places the wheel trails (here: on each touching
 * back wheel's hub). The gamepad rumble is not played: the mod reads pads through GLFW, which has no
 * rumble.
 */
public final class RlFx {
	/** Particle colours go above 1 (HDR); vertex colours carry them divided by this. */
	private static final float COLOR_SCALE = 8.0F;
	private static final DepthStencilState TEST_NO_WRITE = new DepthStencilState(CompareOp.GREATER_THAN_OR_EQUAL, false);

	private static @Nullable Data data;
	private static boolean loaded;
	private static final Map<String, Material> MATERIALS = new HashMap<>();
	private static final List<Instance> INSTANCES = new ArrayList<>();
	private static final Map<Integer, CarFx> CARS = new HashMap<>();
	private static final Rng RNG = new Rng();
	private static final Shakes SHAKES = new Shakes();
	/** This frame's camera shake: location (uu, camera X forward, Y right, Z up) and rotation (pitch, yaw, roll, radians). */
	private static final Vector3f SHAKE_LOC = new Vector3f();
	private static final Vector3f SHAKE_ROT = new Vector3f();

	private RlFx() {
	}

	// ----------------------------------------------------------------------------- data

	private enum ModType {
		LIFETIME, SIZE, SIZE_MULTIPLY_LIFE, VELOCITY, VELOCITY_OVER_LIFETIME, VELOCITY_INHERIT_PARENT, ACCELERATION, ROTATION, ROTATION_RATE,
		COLOR, COLOR_OVER_LIFE, COLOR_SCALE_OVER_LIFE, LOCATION, LOCATION_SPHERE, LOCATION_CYLINDER, CAMERA_OFFSET, TRAIL_SOURCE, OTHER
	}

	/** A Cascade module, as {@code fx.py} writes it (distributions in Unreal units and axes). */
	private static final class Module {
		ModType type = ModType.OTHER;
		final Map<String, Dist> d = new HashMap<>();
		final Map<String, Boolean> b = new HashMap<>();
		@Nullable String heightAxis;
		float @Nullable [][] sourceOffsets;
		float maxAddedVelocity;

		@Nullable Dist dist(String k) {
			return this.d.get(k);
		}

		/** A boolean property: true/false when set, {@code fallback} when not. */
		boolean flag(String k, boolean fallback) {
			Boolean v = this.b.get(k);
			return v == null ? fallback : v;
		}

		@Nullable Boolean opt(String k) {
			return this.b.get(k);
		}

		static Module of(JsonObject o) {
			Module m = new Module();
			m.type = switch (o.get("type").getAsString()) {
				case "Lifetime" -> ModType.LIFETIME;
				case "Size" -> ModType.SIZE;
				case "SizeMultiplyLife" -> ModType.SIZE_MULTIPLY_LIFE;
				case "Velocity" -> ModType.VELOCITY;
				case "VelocityOverLifetime" -> ModType.VELOCITY_OVER_LIFETIME;
				case "VelocityInheritParent" -> ModType.VELOCITY_INHERIT_PARENT;
				case "Acceleration" -> ModType.ACCELERATION;
				case "Rotation" -> ModType.ROTATION;
				case "RotationRate" -> ModType.ROTATION_RATE;
				case "Color" -> ModType.COLOR;
				case "ColorOverLife" -> ModType.COLOR_OVER_LIFE;
				case "ColorScaleOverLife" -> ModType.COLOR_SCALE_OVER_LIFE;
				case "Location" -> ModType.LOCATION;
				case "LocationPrimitiveSphere" -> ModType.LOCATION_SPHERE;
				case "LocationPrimitiveCylinder" -> ModType.LOCATION_CYLINDER;
				case "CameraOffset" -> ModType.CAMERA_OFFSET;
				case "TrailSource" -> ModType.TRAIL_SOURCE;
				default -> ModType.OTHER;
			};
			for (Map.Entry<String, JsonElement> e : o.entrySet()) {
				JsonElement v = e.getValue();
				if (v.isJsonObject()) {
					Dist d = Dist.of(v);
					if (d != null) {
						m.d.put(e.getKey(), d);
					}
				} else if (v.isJsonPrimitive() && v.getAsJsonPrimitive().isBoolean()) {
					m.b.put(e.getKey(), v.getAsBoolean());
				}
			}
			if (o.has("HeightAxis") && o.get("HeightAxis").isJsonPrimitive()) {
				m.heightAxis = o.get("HeightAxis").getAsString();
			}
			if (o.has("MaxAddedVelocity") && o.get("MaxAddedVelocity").isJsonPrimitive()) {
				m.maxAddedVelocity = o.get("MaxAddedVelocity").getAsFloat();
			}
			if (o.has("SourceOffsetDefaults") && o.get("SourceOffsetDefaults").isJsonArray()) {
				JsonArray a = o.getAsJsonArray("SourceOffsetDefaults");
				m.sourceOffsets = new float[a.size()][];
				for (int i = 0; i < a.size(); i++) {
					m.sourceOffsets[i] = floats(a.get(i).getAsJsonArray());
				}
			}
			return m;
		}
	}

	private static final class EmitterDef {
		boolean ribbon;
		String material = "";
		boolean localSpace;
		boolean velocityAligned;
		int cols = 1;
		int rows = 1;
		boolean randomCell;
		float duration;
		int loops;
		float delay;
		// spawn
		boolean hasSpawn;
		@Nullable Dist rate;
		@Nullable Dist rateScale;
		boolean processRate;
		int[][] bursts = new int[0][];
		float[] burstTimes = new float[0];
		// spawn per unit
		boolean hasSpawnPerUnit;
		float unit;
		@Nullable Dist perUnit;
		float maxFrameDistance;
		float movementTolerance;
		boolean spuProcessRate;
		boolean ignoreRateWhenMoving;
		// ribbon
		int maxTrailCount = 1;
		int maxParticleInTrailCount;
		float tilingDistance;
		boolean worldUp;
		boolean spawnInitialParticle;
		List<Module> modules = new ArrayList<>();

		static EmitterDef of(JsonObject o) {
			EmitterDef e = new EmitterDef();
			e.ribbon = o.get("kind").getAsString().equals("ribbon");
			e.material = o.get("material").getAsString();
			e.localSpace = o.get("local_space").getAsBoolean();
			e.velocityAligned = o.get("alignment").getAsString().equals("PSA_Velocity");
			e.cols = Math.max(1, o.getAsJsonArray("subuv").get(0).getAsInt());
			e.rows = Math.max(1, o.getAsJsonArray("subuv").get(1).getAsInt());
			e.randomCell = o.get("subuv_mode").getAsString().equals("PSUVIM_Random");
			e.duration = o.get("duration").getAsFloat();
			e.loops = o.get("loops").getAsInt();
			e.delay = o.get("delay").getAsFloat();
			if (o.get("spawn").isJsonObject()) {
				JsonObject s = o.getAsJsonObject("spawn");
				e.hasSpawn = true;
				e.rate = Dist.of(s.get("rate"));
				e.rateScale = Dist.of(s.get("scale"));
				e.processRate = s.get("process_rate").getAsBoolean();
				JsonArray b = s.getAsJsonArray("bursts");
				e.bursts = new int[b.size()][];
				e.burstTimes = new float[b.size()];
				for (int i = 0; i < b.size(); i++) {
					JsonArray x = b.get(i).getAsJsonArray();
					e.bursts[i] = new int[] {x.get(0).getAsInt(), x.get(1).getAsInt()};
					e.burstTimes[i] = x.get(2).getAsFloat();
				}
			}
			if (o.get("spawn_per_unit").isJsonObject()) {
				JsonObject s = o.getAsJsonObject("spawn_per_unit");
				e.hasSpawnPerUnit = true;
				e.unit = s.get("unit").getAsFloat();
				e.perUnit = Dist.of(s.get("count"));
				e.maxFrameDistance = s.get("max_frame_distance").getAsFloat();
				e.movementTolerance = s.get("movement_tolerance").getAsFloat();
				e.spuProcessRate = s.get("process_rate").getAsBoolean();
				e.ignoreRateWhenMoving = s.get("ignore_rate_when_moving").getAsBoolean();
			}
			if (o.get("ribbon").isJsonObject()) {
				JsonObject r = o.getAsJsonObject("ribbon");
				e.maxTrailCount = intOr(r, "MaxTrailCount", 1);
				e.maxParticleInTrailCount = intOr(r, "MaxParticleInTrailCount", 0);
				e.tilingDistance = r.has("TilingDistance") && r.get("TilingDistance").isJsonPrimitive() ? r.get("TilingDistance").getAsFloat() : 0;
				e.worldUp = r.has("RenderAxis") && r.get("RenderAxis").isJsonPrimitive() && r.get("RenderAxis").getAsString().equals("Trails_WorldUp");
				e.spawnInitialParticle = r.has("bSpawnInitialParticle") && r.get("bSpawnInitialParticle").isJsonPrimitive() && r.get("bSpawnInitialParticle").getAsBoolean();
			}
			for (JsonElement m : o.getAsJsonArray("modules")) {
				e.modules.add(Module.of(m.getAsJsonObject()));
			}
			return e;
		}
	}

	private static int intOr(JsonObject o, String k, int fallback) {
		return o.has(k) && o.get(k).isJsonPrimitive() ? o.get(k).getAsInt() : fallback;
	}

	/** A ported particle material: its program ({@code FX_KIND}), blending and texture. */
	private record Material(int kind, boolean additive, @Nullable Path texture) {
	}

	private record EffectDef(String name, @Nullable String system, List<String> attachAny, List<String> attachAll, Vector3f offset, boolean localOnly) {
	}

	private record Osc(float amplitude, float frequency, boolean randomOffset) {
	}

	private record ShakeDef(float duration, float blendIn, float blendOut, Map<String, Osc> rot, Map<String, Osc> loc) {
	}

	private record ShakeEntry(@Nullable ShakeDef shake, float @Nullable [][] scaleCurve, float minMomentum) {
	}

	private static final class Data {
		final Map<String, List<EmitterDef>> systems = new HashMap<>();
		final List<EffectDef> effects = new ArrayList<>();
		@Nullable String wheelSupersonic;
		@Nullable String bodyImpact;
		final Map<String, ShakeEntry> shakes = new HashMap<>();
	}

	private static float[] floats(JsonArray a) {
		float[] out = new float[a.size()];
		for (int i = 0; i < out.length; i++) {
			out[i] = a.get(i).getAsFloat();
		}
		return out;
	}

	private static List<String> strings(@Nullable JsonElement a) {
		List<String> out = new ArrayList<>();
		if (a != null && a.isJsonArray()) {
			for (JsonElement e : a.getAsJsonArray()) {
				out.add(e.getAsString());
			}
		}
		return out;
	}

	private static @Nullable String str(JsonObject o, String k) {
		return o.has(k) && o.get(k).isJsonPrimitive() ? o.get(k).getAsString() : null;
	}

	private static @Nullable Data data() {
		if (!loaded) {
			loaded = true;
			Path root = RlModels.root();
			Path file = root == null ? null : root.resolve("fx/fx.json");
			if (file != null && Files.isRegularFile(file)) {
				try {
					data = parse(file);
				} catch (IOException | RuntimeException e) {
					RlCar.LOG.error("RL Car: cannot read {}; no car effects", file, e);
				}
			}
		}
		return data;
	}

	/** The ported material programs ({@code rl_fx.fsh}), by base material. */
	private static int materialKind(String base) {
		String name = base.substring(base.lastIndexOf('.') + 1);
		return switch (name) {
			case "SupersonicStreaks_Mat" -> 1;
			case "Smoke_Puff_01_Mat" -> 2;
			case "Unlit_Translucent_Mat" -> 3;
			case "Spark_Mat" -> 4;
			case "Glow_Translucent_Mat" -> 5;
			case "Wheel_Trail_Mat" -> 6;
			case "DodgeRibbon_Mat" -> 7;
			case "StandardFlare_Mat" -> 8;
			default -> 0;
		};
	}

	private static Data parse(Path file) throws IOException {
		JsonObject j = JsonParser.parseString(Files.readString(file)).getAsJsonObject();
		Path dir = file.getParent();
		Data d = new Data();
		for (Map.Entry<String, JsonElement> e : j.getAsJsonObject("systems").entrySet()) {
			List<EmitterDef> emitters = new ArrayList<>();
			for (JsonElement em : e.getValue().getAsJsonObject().getAsJsonArray("emitters")) {
				emitters.add(EmitterDef.of(em.getAsJsonObject()));
			}
			d.systems.put(e.getKey(), emitters);
		}
		for (Map.Entry<String, JsonElement> e : j.getAsJsonObject("materials").entrySet()) {
			if (!e.getValue().isJsonObject()) {
				continue;
			}
			JsonObject m = e.getValue().getAsJsonObject();
			int kind = materialKind(m.get("base").getAsString());
			if (kind == 0) {
				RlCar.LOG.warn("RL Car: no port of particle material {}; its particles are not drawn", m.get("base").getAsString());
				continue;
			}
			boolean additive = kind == 1 || kind == 3 || kind == 6 || kind == 7 || kind == 8 || m.get("blend").getAsString().equals("BLEND_Additive");
			List<String> tex = strings(m.get("textures"));
			MATERIALS.put(e.getKey(), new Material(kind, additive, tex.isEmpty() ? null : dir.resolve(tex.getFirst())));
		}
		for (JsonElement ee : j.getAsJsonArray("effects")) {
			JsonObject e = ee.getAsJsonObject();
			float[] off = floats(e.getAsJsonArray("offset"));
			d.effects.add(new EffectDef(e.get("name").getAsString(), str(e, "system"), strings(e.get("attach_any")), strings(e.get("attach_all")),
				new Vector3f(off[0], off[1], off[2]), e.has("local_only") && e.get("local_only").getAsBoolean()));
		}
		d.wheelSupersonic = str(j, "wheel_supersonic");
		d.bodyImpact = str(j, "body_impact");
		for (Map.Entry<String, JsonElement> e : j.getAsJsonObject("shakes").entrySet()) {
			JsonObject s = e.getValue().getAsJsonObject();
			ShakeDef def = null;
			if (s.has("shake") && s.get("shake").isJsonObject()) {
				JsonObject sh = s.getAsJsonObject("shake");
				def = new ShakeDef(sh.get("duration").getAsFloat(), sh.get("blend_in").getAsFloat(), sh.get("blend_out").getAsFloat(), oscillators(sh.get("rot")), oscillators(sh.get("loc")));
			}
			float[][] curve = null;
			if (s.has("scale_curve") && s.get("scale_curve").isJsonArray()) {
				JsonArray c = s.getAsJsonArray("scale_curve");
				curve = new float[c.size()][];
				for (int i = 0; i < c.size(); i++) {
					curve[i] = floats(c.get(i).getAsJsonArray());
				}
			}
			float min = s.has("min_momentum") && s.get("min_momentum").isJsonPrimitive() ? s.get("min_momentum").getAsFloat() : 0;
			d.shakes.put(e.getKey(), new ShakeEntry(def, curve, min));
		}
		return d;
	}

	private static Map<String, Osc> oscillators(@Nullable JsonElement e) {
		Map<String, Osc> out = new HashMap<>();
		if (e != null && e.isJsonObject()) {
			for (Map.Entry<String, JsonElement> o : e.getAsJsonObject().entrySet()) {
				JsonObject v = o.getValue().getAsJsonObject();
				out.put(o.getKey(), new Osc(v.get("amplitude").getAsFloat(), v.get("frequency").getAsFloat(), v.get("random_offset").getAsBoolean()));
			}
		}
		return out;
	}

	/** Piecewise-linear InterpCurveFloat (the game's shake scale curves are linear or near enough). */
	private static float evalCurve(float @Nullable [][] points, float x) {
		if (points == null || points.length == 0) {
			return 1.0F;
		}
		if (x <= points[0][0]) {
			return points[0][1];
		}
		for (int i = 0; i + 1 < points.length; i++) {
			if (x < points[i + 1][0]) {
				float t = (x - points[i][0]) / Math.max(points[i + 1][0] - points[i][0], 1e-6F);
				return points[i][1] + (points[i + 1][1] - points[i][1]) * t;
			}
		}
		return points[points.length - 1][1];
	}

	// ----------------------------------------------------------------------------- pipelines

	private static final Map<String, RenderPipeline> PIPELINES = new HashMap<>();
	private static final Map<String, OitPipelineSet> OIT = new HashMap<>();
	private static final Map<String, @Nullable RenderType> TYPES = new HashMap<>();

	/** Registers a pipeline per ported material (before the first resource load compiles them). */
	public static void registerPipelines() {
		if (data() == null) {
			return;
		}
		for (Map.Entry<String, Material> e : MATERIALS.entrySet()) {
			Material m = e.getValue();
			String key = e.getKey().toLowerCase(java.util.Locale.ROOT).replaceAll("[^a-z0-9_]", "_");
			RenderPipeline.Builder base = RenderPipeline.builder()
				.withVertexShader(RlCar.id("core/rl_fx"))
				.withFragmentShader(RlCar.id("core/rl_fx"))
				.withBindGroupLayout(BindGroupLayouts.SAMPLER0)
				.withVertexBinding(0, DefaultVertexFormat.ENTITY)
				.withPrimitiveTopology(PrimitiveTopology.QUADS)
				.withDepthStencilState(TEST_NO_WRITE)
				.withCull(false)
				.withShaderDefine("FX_KIND", m.kind)
				.withShaderDefine("COLOR_SCALE", COLOR_SCALE);
			if (m.additive) {
				base.withShaderDefine("ADDITIVE");
			}
			RenderPipeline.Snippet snippet = base.buildSnippet();
			PIPELINES.put(e.getKey(), RenderPipelines.register(RenderPipeline.builder(RenderPipelines.MATRICES_FOG_SNIPPET, snippet)
				.withLocation(RlCar.id("pipeline/fx_" + key))
				.withColorTargetState(new ColorTargetState(m.additive ? BlendFunction.ADDITIVE : BlendFunction.TRANSLUCENT))
				.build()));
			RenderPipeline.Builder oit = RenderPipeline.builder(snippet)
				.withBindGroupLayout(BindGroupLayouts.DYNAMIC_TRANSFORMS)
				.withBindGroupLayout(BindGroupLayouts.FOG);
			if (m.additive) {
				oit.withShaderDefine("OIT_ADDITIVE");
			}
			OIT.put(e.getKey(), RenderPipelines.register(OitPipelineSet.builder("rlcar_fx_" + key, oit).build()));
		}
	}

	/** The render type of a material, created on first use (its texture is registered then); null if unavailable. */
	private static @Nullable RenderType renderType(String material) {
		if (TYPES.containsKey(material)) {
			return TYPES.get(material);
		}
		RenderType type = null;
		Material m = MATERIALS.get(material);
		RenderPipeline pipeline = PIPELINES.get(material);
		OitPipelineSet oit = OIT.get(material);
		if (m != null && pipeline != null && oit != null) {
			try {
				// Spark_Mat and DodgeRibbon_Mat sample nothing; they still need a texture bound.
				Path tex = m.texture != null ? m.texture : firstTexture();
				RenderSetup.RenderSetupBuilder setup = RenderSetup.builder(pipeline).setOitPipelines(oit);
				if (tex != null) {
					setup.withTexture("Sampler0", RlModels.texture(tex), () -> RenderSystem.getSamplerCache().getRepeat(FilterMode.LINEAR, true));
				}
				if (!m.additive) {
					setup.sortOnUpload();
				}
				type = RenderType.create("rlcar_fx_" + material.toLowerCase(java.util.Locale.ROOT).replaceAll("[^a-z0-9_]", "_"), setup.createRenderSetup());
			} catch (IOException | RuntimeException e) {
				RlCar.LOG.error("RL Car: cannot set up the particle material {}", material, e);
			}
		}
		TYPES.put(material, type);
		return type;
	}

	private static @Nullable Path firstTexture() {
		for (Material m : MATERIALS.values()) {
			if (m.texture != null) {
				return m.texture;
			}
		}
		return null;
	}

	// ----------------------------------------------------------------------------- simulation

	/** Where a system is: its component's location and rotation (Unreal world space, uu), and the owner's velocity. */
	private record Frame(Vector3f pos, Matrix3f rot, Vector3f ownerVel) {
		Vector3f toWorld(Vector3f local) {
			return this.rot.transform(new Vector3f(local)).add(this.pos);
		}

		Frame at(Vector3f p) {
			return new Frame(p, this.rot, this.ownerVel);
		}
	}

	private static final class Particle {
		final Vector3f pos = new Vector3f();
		final Vector3f baseVel = new Vector3f();
		final Vector3f vel = new Vector3f();
		final Vector3f accel = new Vector3f();
		final Vector3f baseSize = new Vector3f();
		final Vector3f size = new Vector3f();
		float rel;
		float invLife;
		final float[] baseColor = {1, 1, 1, 1};
		final float[] color = {1, 1, 1, 1};
		float rotation;
		float baseRotRate;
		float rotRate;
		int cell;
		float cameraOffset;
		/** Ribbons: distance along the trail from its first particle (uu). */
		float distance;
	}

	private static final class EmitterState {
		float time;
		int loopsDone;
		boolean finished;
		float spawnAcc;
		boolean[] burstsFired = new boolean[0];
		@Nullable Vector3f lastPos;
		float travelled;
		final List<Particle> particles = new ArrayList<>();
		/** Ribbons: particles per trail, oldest first. */
		List<List<Particle>> trails = new ArrayList<>();
		@Nullable Vector3f @Nullable [] trailLast;
		float[] trailTravelled = new float[0];
		float[] trailDistance = new float[0];
		boolean started;

		boolean empty() {
			if (!this.particles.isEmpty()) {
				return false;
			}
			for (List<Particle> t : this.trails) {
				if (!t.isEmpty()) {
					return false;
				}
			}
			return true;
		}
	}

	private static final class Instance {
		final String system;
		final List<EmitterDef> defs;
		final EmitterState[] emitters;
		final int car;
		boolean active = true;
		Frame frame;
		/** Continuous attachments (supersonic): key, so the same one is kept across frames. */
		final @Nullable String key;
		/** Attached to the car at this offset (car frame, uu): the frame follows the car. */
		final @Nullable Vector3f attached;

		Instance(String system, List<EmitterDef> defs, int car, Frame frame, @Nullable String key, @Nullable Vector3f attached) {
			this.system = system;
			this.defs = defs;
			this.car = car;
			this.frame = frame;
			this.key = key;
			this.attached = attached;
			this.emitters = new EmitterState[defs.size()];
			for (int i = 0; i < this.emitters.length; i++) {
				this.emitters[i] = new EmitterState();
			}
		}

		boolean alive() {
			if (this.active) {
				return true;
			}
			for (EmitterState e : this.emitters) {
				if (!e.empty()) {
					return true;
				}
			}
			return false;
		}
	}

	private static @Nullable Instance instance(Data d, @Nullable String system, int car, Frame frame, @Nullable String key, @Nullable Vector3f attached) {
		List<EmitterDef> defs = system == null ? null : d.systems.get(system);
		return defs == null ? null : new Instance(system, defs, car, frame, key, attached);
	}

	private static Vector3f sample3(@Nullable Dist d, float t) {
		if (d == null) {
			return new Vector3f();
		}
		float[] v = d.sample(t, RNG);
		return new Vector3f(v[0], v[1], v[2]);
	}

	private static float sample1(@Nullable Dist d, float t, float fallback) {
		return d == null ? fallback : d.scalar(t, RNG);
	}

	private static Particle spawn(EmitterDef def, Frame frame, Vector3f at, float age, float emitterTime) {
		Particle p = new Particle();
		if (!def.localSpace) {
			p.pos.set(at);
		}
		for (Module m : def.modules) {
			switch (m.type) {
				case LIFETIME -> {
					float life = sample1(m.dist("LifeTime"), emitterTime, 0);
					p.invLife = life > 0 ? 1.0F / life : 0;
				}
				case SIZE -> {
					p.baseSize.add(sample3(m.dist("StartSize"), emitterTime));
					p.size.set(p.baseSize);
				}
				case SIZE_MULTIPLY_LIFE -> sizeMult(p, m);
				case VELOCITY -> {
					Vector3f v = sample3(m.dist("StartVelocity"), emitterTime);
					if (!m.flag("bInWorldSpace", false)) {
						v = toWorld(def, frame, v);
					}
					float radial = sample1(m.dist("StartVelocityRadial"), emitterTime, 0);
					if (radial != 0) {
						Vector3f origin = def.localSpace ? new Vector3f() : frame.pos;
						v.add(normalizeOrZero(new Vector3f(p.pos).sub(origin)).mul(radial));
					}
					p.baseVel.add(v);
					p.vel.add(v);
				}
				case VELOCITY_INHERIT_PARENT -> {
					Dist scale = m.dist("Scale");
					Vector3f s = scale == null ? new Vector3f(1) : sample3(scale, emitterTime);
					Vector3f v = new Vector3f(frame.ownerVel).mul(s);
					if (m.maxAddedVelocity > 0 && v.length() > m.maxAddedVelocity) {
						v.normalize(m.maxAddedVelocity);
					}
					if (def.localSpace) {
						frame.rot.transformTranspose(v);
					}
					p.baseVel.add(v);
					p.vel.add(v);
				}
				case ACCELERATION -> {
					Vector3f a = sample3(m.dist("Acceleration"), emitterTime);
					p.accel.set(m.flag("bAlwaysInWorldSpace", false) && !def.localSpace ? a : toWorld(def, frame, a));
				}
				case ROTATION -> p.rotation += sample1(m.dist("StartRotation"), emitterTime, 0) * (float) (Math.PI * 2);
				case ROTATION_RATE -> {
					p.baseRotRate += sample1(m.dist("StartRotationRate"), emitterTime, 0) * (float) (Math.PI * 2);
					p.rotRate = p.baseRotRate;
				}
				case COLOR -> {
					Dist c = m.dist("StartColor");
					Vector3f rgb = c == null ? new Vector3f(1) : sample3(c, emitterTime);
					float a = sample1(m.dist("StartAlpha"), emitterTime, 1.0F);
					setColor(p.baseColor, rgb, a);
					System.arraycopy(p.baseColor, 0, p.color, 0, 4);
				}
				case COLOR_OVER_LIFE -> {
					float[] c = colorAt(m.dist("ColorOverLife"), m.dist("AlphaOverLife"), 0, p.color);
					System.arraycopy(c, 0, p.baseColor, 0, 4);
					System.arraycopy(c, 0, p.color, 0, 4);
				}
				case COLOR_SCALE_OVER_LIFE -> {
					float t = m.flag("bEmitterTime", false) ? emitterTime : 0;
					mulColor(p.color, colorAt(m.dist("ColorScaleOverLife"), m.dist("AlphaScaleOverLife"), t, new float[] {1, 1, 1, 1}));
				}
				case LOCATION -> p.pos.add(toWorld(def, frame, sample3(m.dist("StartLocation"), emitterTime)));
				case LOCATION_SPHERE -> {
					Vector3f dir = unitDirection(m);
					if (m.flag("SurfaceOnly", false)) {
						normalizeOrZero(dir);
					}
					Vector3f offset = dir.mul(sample1(m.dist("StartRadius"), emitterTime, 0)).add(sample3(m.dist("StartLocation"), emitterTime));
					p.pos.add(toWorld(def, frame, offset));
					if (m.flag("Velocity", false)) {
						Vector3f v = toWorld(def, frame, offset).mul(sample1(m.dist("VelocityScale"), emitterTime, 1.0F));
						p.vel.add(v);
						p.baseVel.add(v);
					}
				}
				case LOCATION_CYLINDER -> {
					int axis = "PMLPC_HEIGHTAXIS_X".equals(m.heightAxis) ? 0 : "PMLPC_HEIGHTAXIS_Y".equals(m.heightAxis) ? 1 : 2;
					Vector3f dir = unitDirection(m);
					float along = dir.get(axis);
					dir.setComponent(axis, 0);
					if (m.flag("SurfaceOnly", false)) {
						normalizeOrZero(dir);
					}
					float radius = sample1(m.dist("StartRadius"), emitterTime, 0);
					float height = sample1(m.dist("StartHeight"), emitterTime, 0);
					Vector3f offset = new Vector3f(dir).mul(radius);
					offset.setComponent(axis, along * height * 0.5F);
					offset.add(sample3(m.dist("StartLocation"), emitterTime));
					p.pos.add(toWorld(def, frame, offset));
					if (m.flag("Velocity", false)) {
						Vector3f v = new Vector3f(offset).mul(sample1(m.dist("VelocityScale"), emitterTime, 1.0F));
						if (m.flag("RadialVelocity", false)) {
							v.setComponent(axis, 0);
						}
						v = toWorld(def, frame, v);
						p.vel.add(v);
						p.baseVel.add(v);
					}
				}
				case VELOCITY_OVER_LIFETIME -> {
					// Absolute: the velocity is the curve's (in world space) from the start.
					Dist d = m.dist("VelOverLife");
					if (m.flag("Absolute", false) && d != null) {
						Vector3f v = sample3(d, 0);
						p.vel.set(v);
						p.baseVel.set(v);
					}
				}
				case CAMERA_OFFSET -> p.cameraOffset = sample1(m.dist("CameraOffset"), emitterTime, 0);
				default -> {
				}
			}
		}
		int cells = def.cols * def.rows;
		if (cells > 1 && def.randomCell) {
			p.cell = Math.min((int) (RNG.next() * cells), cells - 1);
		}
		// Spawned during the frame: catch up with the time since then.
		p.rel += age * p.invLife;
		p.pos.add(new Vector3f(p.vel).mul(age));
		return p;
	}

	/** Directions in the component's frame for world-space emitters; local-space emitters keep everything in the component frame. */
	private static Vector3f toWorld(EmitterDef def, Frame frame, Vector3f v) {
		return def.localSpace ? v : frame.rot.transform(new Vector3f(v));
	}

	private static Vector3f normalizeOrZero(Vector3f v) {
		float l = v.length();
		return l > 1e-12F ? v.div(l) : v.zero();
	}

	private static Vector3f unitDirection(Module m) {
		String[] axes = {"X", "Y", "Z"};
		Vector3f v = new Vector3f();
		for (int i = 0; i < 3; i++) {
			boolean pos = m.flag("Positive_" + axes[i], true);
			boolean neg = m.flag("Negative_" + axes[i], true);
			float c;
			if (pos && neg) {
				c = RNG.next() * 2 - 1;
			} else if (pos) {
				c = RNG.next();
			} else if (neg) {
				c = -RNG.next();
			} else {
				c = 0;
			}
			v.setComponent(i, c);
		}
		return v;
	}

	private static void setColor(float[] out, Vector3f rgb, float a) {
		out[0] = rgb.x;
		out[1] = rgb.y;
		out[2] = rgb.z;
		out[3] = a;
	}

	private static void mulColor(float[] c, float[] m) {
		for (int i = 0; i < 4; i++) {
			c[i] *= m[i];
		}
	}

	private static float[] colorAt(@Nullable Dist c, @Nullable Dist a, float t, float[] fallback) {
		float[] out = fallback.clone();
		if (c != null) {
			float[] v = c.sample(t, RNG);
			out[0] = v[0];
			out[1] = v[1];
			out[2] = v[2];
		}
		if (a != null) {
			out[3] = a.scalar(t, RNG);
		}
		return out;
	}

	private static void sizeMult(Particle p, Module m) {
		Dist d = m.dist("LifeMultiplier");
		float[] mul = d == null ? new float[] {1, 1, 1} : d.sample(p.rel, RNG);
		String[] axes = {"MultiplyX", "MultiplyY", "MultiplyZ"};
		for (int i = 0; i < 3; i++) {
			if (m.flag(axes[i], i < 2)) {
				p.size.setComponent(i, p.size.get(i) * mul[i]);
			}
		}
	}

	/** One frame of a particle: reset to the base values, the update modules in order, then move. */
	private static void updateParticle(Particle p, EmitterDef def, float dt, float emitterTime) {
		p.rel += dt * p.invLife;
		p.vel.set(p.baseVel);
		p.size.set(p.baseSize);
		p.rotRate = p.baseRotRate;
		System.arraycopy(p.baseColor, 0, p.color, 0, 4);
		for (Module m : def.modules) {
			switch (m.type) {
				case SIZE_MULTIPLY_LIFE -> sizeMult(p, m);
				case VELOCITY_OVER_LIFETIME -> {
					Dist d = m.dist("VelOverLife");
					if (d != null) {
						Vector3f v = sample3(d, p.rel);
						if (m.flag("Absolute", false)) {
							p.vel.set(v);
							p.baseVel.set(v);
						} else {
							p.vel.mul(v);
						}
					}
				}
				case ACCELERATION -> {
					p.vel.add(new Vector3f(p.accel).mul(dt));
					p.baseVel.add(new Vector3f(p.accel).mul(dt));
				}
				case COLOR_OVER_LIFE -> System.arraycopy(colorAt(m.dist("ColorOverLife"), m.dist("AlphaOverLife"), p.rel, p.color), 0, p.color, 0, 4);
				case COLOR_SCALE_OVER_LIFE -> {
					float t = m.flag("bEmitterTime", false) ? emitterTime : p.rel;
					mulColor(p.color, colorAt(m.dist("ColorScaleOverLife"), m.dist("AlphaScaleOverLife"), t, new float[] {1, 1, 1, 1}));
				}
				default -> {
				}
			}
		}
		p.pos.add(new Vector3f(p.vel).mul(dt));
		p.rotation += p.rotRate * dt;
	}

	private static boolean dead(Particle p) {
		return p.invLife != 0 && p.rel >= 1.0F;
	}

	private static void tickEmitter(EmitterState st, EmitterDef def, Frame frame, boolean active, float dt) {
		boolean first = !st.started;
		st.started = true;
		if (first) {
			st.burstsFired = new boolean[def.bursts.length];
		}
		// Emitter time and loops (UE3: EmitterTime over EmitterDuration, EmitterLoops 0 = forever).
		float prevTime = st.time;
		st.time += dt;
		float duration = Math.max(def.duration, 1e-4F);
		float localTime = Math.max(st.time - def.delay, 0);
		if (localTime >= duration * (st.loopsDone + 1)) {
			st.loopsDone++;
			java.util.Arrays.fill(st.burstsFired, false);
			if (def.loops > 0 && st.loopsDone >= def.loops) {
				st.finished = true;
			}
		}
		float emitterTime = (localTime % duration) / duration;

		for (Particle p : st.particles) {
			updateParticle(p, def, dt, emitterTime);
		}
		st.particles.removeIf(RlFx::dead);
		for (List<Particle> t : st.trails) {
			for (Particle p : t) {
				updateParticle(p, def, dt, emitterTime);
			}
			t.removeIf(RlFx::dead);
		}

		boolean spawning = active && !st.finished && st.time >= def.delay;
		if (def.ribbon) {
			tickRibbon(st, def, frame, spawning, first, dt, emitterTime);
			return;
		}
		if (!spawning) {
			st.lastPos = null;
			return;
		}
		Vector3f world = frame.pos;
		int count = 0;
		boolean moving = false;
		if (def.hasSpawnPerUnit) {
			if (st.lastPos != null) {
				Vector3f last = st.lastPos;
				float travel = new Vector3f(world).sub(last).length();
				moving = travel > def.movementTolerance * def.unit;
				if (def.maxFrameDistance > 0 && travel > def.maxFrameDistance) {
					st.travelled = 0;
				} else if (moving) {
					float perUnit = sample1(def.perUnit, emitterTime, 0);
					float total = travel + st.travelled;
					int n = (int) Math.floor(total * perUnit / def.unit);
					st.travelled = total - n * def.unit / Math.max(perUnit, 1e-3F);
					for (int k = 0; k < n; k++) {
						float f = (k + 1) / (float) n;
						st.particles.add(spawn(def, frame, new Vector3f(last).lerp(world, f), dt * (1 - f), emitterTime));
						count++;
					}
				}
			}
			st.lastPos = new Vector3f(world);
		}
		boolean processRate = !def.hasSpawnPerUnit || def.spuProcessRate && !(def.ignoreRateWhenMoving && moving);
		if (def.hasSpawn) {
			if (def.processRate && processRate) {
				float rate = sample1(def.rate, emitterTime, 0) * sample1(def.rateScale, emitterTime, 1.0F);
				st.spawnAcc += rate * dt;
				while (st.spawnAcc >= 1.0F) {
					st.spawnAcc -= 1.0F;
					float age = rate > 0 ? Math.min(st.spawnAcc / rate, dt) : 0;
					st.particles.add(spawn(def, frame, world, age, emitterTime));
					if (++count > 4096) {
						break;
					}
				}
			}
			float fracPrev = (Math.max(prevTime - def.delay, 0) % duration) / duration;
			for (int i = 0; i < def.bursts.length; i++) {
				if (!st.burstsFired[i] && (emitterTime >= def.burstTimes[i] || fracPrev > emitterTime)) {
					st.burstsFired[i] = true;
					int n = def.bursts[i][0], low = def.bursts[i][1];
					if (low >= 0) {
						n = low + (int) ((n - low + 1) * RNG.next());
					}
					for (int k = 0; k < Math.max(n, 0); k++) {
						st.particles.add(spawn(def, frame, world, 0, emitterTime));
					}
				}
			}
		}
	}

	/** Ribbon emitters (TypeDataRibbon): a trail per source (TrailSource offsets in the component's frame, or the component itself), particles laid along it by spawn rate / distance. */
	private static void tickRibbon(EmitterState st, EmitterDef def, Frame frame, boolean spawning, boolean first, float dt, float emitterTime) {
		float[][] offsets = null;
		for (Module m : def.modules) {
			if (m.type == ModType.TRAIL_SOURCE && m.sourceOffsets != null) {
				offsets = m.sourceOffsets;
			}
		}
		if (offsets == null || offsets.length == 0) {
			offsets = new float[][] {{0, 0, 0}};
		}
		int trails = Math.clamp(def.maxTrailCount, 1, Math.max(offsets.length, 1));
		int maxParticles = def.maxParticleInTrailCount > 0 ? def.maxParticleInTrailCount : 256;
		if (st.trails.size() != trails) {
			st.trails = new ArrayList<>();
			for (int i = 0; i < trails; i++) {
				st.trails.add(new ArrayList<>());
			}
			st.trailLast = new Vector3f[trails];
			st.trailTravelled = new float[trails];
			st.trailDistance = new float[trails];
		}
		if (!spawning) {
			java.util.Arrays.fill(st.trailLast, null);
			return;
		}
		for (int i = 0; i < trails; i++) {
			float[] o = offsets[Math.min(i, offsets.length - 1)];
			Vector3f source = frame.toWorld(new Vector3f(o[0], o[1], o[2]));
			if (first && def.spawnInitialParticle) {
				spawnOnTrail(st, def, frame, i, source, 0, emitterTime, maxParticles);
			}
			Vector3f last = st.trailLast[i];
			if (def.hasSpawnPerUnit && last != null) {
				float travel = new Vector3f(source).sub(last).length();
				if (travel > def.movementTolerance * def.unit) {
					float perUnit = sample1(def.perUnit, emitterTime, 0);
					float total = travel + st.trailTravelled[i];
					int n = (int) Math.floor(total * perUnit / def.unit);
					st.trailTravelled[i] = total - n * def.unit / Math.max(perUnit, 1e-3F);
					for (int k = 0; k < n; k++) {
						float f = (k + 1) / (float) n;
						spawnOnTrail(st, def, frame, i, new Vector3f(last).lerp(source, f), dt * (1 - f), emitterTime, maxParticles);
					}
				}
			}
			st.trailLast[i] = source;
			if (def.hasSpawn && def.processRate) {
				// The rate is shared by the trails (UE3 spawns each trail's share).
				float rate = sample1(def.rate, emitterTime, 0) * sample1(def.rateScale, emitterTime, 1.0F) / trails;
				float acc = st.spawnAcc + rate * dt;
				int n = (int) Math.floor(acc);
				if (i + 1 == trails) {
					st.spawnAcc = acc - n;
				}
				for (int k = 0; k < n; k++) {
					spawnOnTrail(st, def, frame, i, source, 0, emitterTime, maxParticles);
				}
			}
		}
	}

	private static void spawnOnTrail(EmitterState st, EmitterDef def, Frame frame, int i, Vector3f at, float age, float emitterTime, int maxParticles) {
		List<Particle> t = st.trails.get(i);
		if (!t.isEmpty()) {
			st.trailDistance[i] += new Vector3f(at).sub(t.getLast().pos).length();
		}
		Particle p = spawn(def, frame, at, age, emitterTime);
		p.distance = st.trailDistance[i];
		t.add(p);
		if (t.size() > maxParticles) {
			t.removeFirst();
		}
	}

	// ----------------------------------------------------------------------------- the cars

	private static final class CarFx {
		@Nullable CarPose last;
		float lastImpact = -10;
		float clock;
	}

	private static Frame carFrame(CarRl s) {
		return new Frame(new Vector3f(s.pos), new Matrix3f(s.rot), new Vector3f(s.vel));
	}

	/**
	 * Runs one car's FX actor logic between its previous pose and this frame's. {@code local}: the
	 * car this client drives (its camera shakes). Called every frame for every car being drawn,
	 * then {@link #endFrame}.
	 */
	static void update(CarEntity car, CarPose pose, boolean local, float dt) {
		Data d = data();
		if (d == null) {
			return;
		}
		int id = car.getId();
		CarFx c = CARS.computeIfAbsent(id, k -> new CarFx());
		CarRl s = CarRl.of(pose);
		Frame frame = carFrame(s);
		CarPose prev = c.last;
		c.last = pose.copy();
		if (prev != null && dt > 0) {
			c.clock += dt;
			List<String> events = new ArrayList<>();
			if (pose.hasContact(RlCarNative.CONTACT_HAS_JUMPED) && !prev.hasContact(RlCarNative.CONTACT_HAS_JUMPED)) {
				events.add("Jump");
			}
			if (pose.hasContact(RlCarNative.CONTACT_HAS_DOUBLE_JUMPED) && !prev.hasContact(RlCarNative.CONTACT_HAS_DOUBLE_JUMPED)) {
				events.add("DoubleJump");
			}
			if (pose.hasContact(RlCarNative.CONTACT_HAS_FLIPPED) && !prev.hasContact(RlCarNative.CONTACT_HAS_FLIPPED)) {
				events.add("Dodge");
			}
			if (local && pose.has(RlCarNative.FLAG_BOOSTING) && !prev.has(RlCarNative.FLAG_BOOSTING)) {
				shake(d, "BoostActive", 1.0F);
			}
			for (String ev : events) {
				for (EffectDef e : d.effects) {
					if (!e.attachAny.contains(ev) || e.localOnly && !local) {
						continue;
					}
					Instance i = instance(d, e.system, id, frame.at(frame.toWorld(e.offset)), null, new Vector3f(e.offset));
					if (i != null) {
						INSTANCES.add(i);
					}
				}
				if (local) {
					shake(d, ev, 1.0F);
				}
			}
			// Wheel landings: the landing shake, scaled by the impact momentum (ShakeScaleCurve).
			ShakeEntry wheel = d.shakes.get("WheelImpact");
			if (local && wheel != null) {
				float best = 0;
				for (int i = 0; i < 4; i++) {
					Vector3f n = normal(pose, 3 + i * 3);
					if (n != null && normal(prev, 3 + i * 3) == null) {
						best = Math.max(best, -dot(prev.velocity, n));
					}
				}
				if (best >= wheel.minMomentum && best > 0) {
					shake(d, "WheelImpact", evalCurve(wheel.scaleCurve, best));
				}
			}
			// Body impacts: sparks at the contact, and the impact shake.
			Vector3f n = normal(pose, 0);
			if (n != null && normal(prev, 0) == null) {
				float momentum = -dot(prev.velocity, n);
				ShakeEntry entry = d.shakes.get("BodyImpact");
				if (momentum >= (entry == null ? 0 : entry.minMomentum) && c.clock - c.lastImpact >= 0.15F) {
					c.lastImpact = c.clock;
					if (local) {
						shake(d, "BodyImpact", entry == null ? 1.0F : evalCurve(entry.scaleCurve, momentum));
					}
					if (d.bodyImpact != null) {
						// The hit point: the car's hitbox face towards the surface; the effect's X axis is
						// the surface normal (the game spawns it with the hit normal's rotation).
						float[] h = RlCarNative.presetHitbox(car.preset());
						Vector3f half = new Vector3f(h[0], h[1], h[2]).mul(0.5F);
						Vector3f ln = frame.rot.transformTranspose(new Vector3f(n));
						float reach = Math.abs(ln.x * half.x) + Math.abs(ln.y * half.y) + Math.abs(ln.z * half.z);
						Vector3f pos = new Vector3f(frame.pos).sub(new Vector3f(n).mul(reach));
						Vector3f x = new Vector3f(n);
						Vector3f y = Math.abs(x.z) < 0.9F ? new Vector3f(0, 0, 1).cross(x).normalize() : new Vector3f(1, 0, 0).cross(x).normalize();
						Frame f = new Frame(pos, new Matrix3f(x, y, new Vector3f(x).cross(y)), frame.ownerVel);
						Instance i = instance(d, d.bodyImpact, id, f, null, null);
						if (i != null) {
							INSTANCES.add(i);
						}
					}
				}
			}
		}

		// Continuous attachments: the supersonic streaks (by team) and wheel trails.
		boolean supersonic = pose.has(RlCarNative.FLAG_SUPERSONIC);
		String team = car.color() == CarEntity.ORANGE ? "Team1" : "Team0";
		Map<String, Object[]> wanted = new HashMap<>();
		for (EffectDef e : d.effects) {
			if (e.system == null || e.attachAll.isEmpty() || e.localOnly && !local) {
				continue;
			}
			boolean on = true;
			for (String a : e.attachAll) {
				on &= switch (a) {
					case "SuperSonic" -> supersonic;
					case "Team0", "Team1" -> a.equals(team);
					default -> false;
				};
			}
			if (on) {
				wanted.put(id + ":" + e.name, new Object[] {e.system, frame.at(frame.toWorld(e.offset))});
			}
		}
		if (d.wheelSupersonic != null && supersonic) {
			// Only the back wheels (2, 3: back right, back left): the game attaches its supersonic trail
			// product's two FX actors (LeftFXActor, RightFXActor) there, never on the front wheels.
			for (int i = 2; i < 4; i++) {
				if ((pose.flags & 1 << RlCarNative.FLAG_WHEEL_CONTACT_SHIFT + i) == 0) {
					continue;
				}
				// The wheel's hub (model space, blocks: forward, up, right) in car-local uu.
				Vector3f hub = new Vector3f(pose.wheels[i * 3], pose.wheels[i * 3 + 2], pose.wheels[i * 3 + 1]).mul(100.0F);
				wanted.put(id + ":wheel" + i, new Object[] {d.wheelSupersonic, frame.at(frame.toWorld(hub))});
			}
		}
		for (Instance inst : INSTANCES) {
			if (inst.car != id) {
				continue;
			}
			if (inst.key != null) {
				Object[] w = wanted.remove(inst.key);
				if (w != null) {
					inst.frame = (Frame) w[1];
					inst.active = true;
				} else {
					inst.active = false;
				}
			} else if (inst.attached != null) {
				// One-shot systems on the FX actor ride along with the car.
				inst.frame = frame.at(frame.toWorld(inst.attached));
			}
		}
		for (Map.Entry<String, Object[]> w : wanted.entrySet()) {
			Instance i = instance(d, (String) w.getValue()[0], id, (Frame) w.getValue()[1], w.getKey(), null);
			if (i != null) {
				INSTANCES.add(i);
			}
		}
	}

	private static float dot(float[] a, Vector3f b) {
		return a[0] * b.x + a[1] * b.y + a[2] * b.z;
	}

	private static @Nullable Vector3f normal(CarPose p, int at) {
		float x = p.contacts[at], y = p.contacts[at + 1], z = p.contacts[at + 2];
		return x == 0 && y == 0 && z == 0 ? null : new Vector3f(x, y, z);
	}

	/**
	 * After every car's {@link #update}: the effects of cars not seen this frame stop spawning,
	 * every instance is simulated over {@code dt}, and the camera shakes advance.
	 */
	static void endFrame(Set<Integer> seen, float dt) {
		CARS.keySet().retainAll(seen);
		dt = Math.min(dt, 0.1F);
		for (Iterator<Instance> it = INSTANCES.iterator(); it.hasNext();) {
			Instance inst = it.next();
			if (!seen.contains(inst.car)) {
				it.remove();
				continue;
			}
			if (dt > 0) {
				for (int i = 0; i < inst.emitters.length; i++) {
					tickEmitter(inst.emitters[i], inst.defs.get(i), inst.frame, inst.active, dt);
				}
				if (inst.key == null) {
					boolean done = true;
					for (EmitterState e : inst.emitters) {
						done &= e.finished;
					}
					if (done) {
						inst.active = false;
					}
				}
			}
			if (!inst.alive()) {
				it.remove();
			}
		}
		SHAKES.advance(dt, SHAKE_LOC, SHAKE_ROT);
	}

	/** Drops everything (leaving the world). */
	static void clear() {
		INSTANCES.clear();
		CARS.clear();
		SHAKES.list.clear();
		SHAKE_LOC.zero();
		SHAKE_ROT.zero();
	}

	/** Particles alive right now, all cars (for the tests). */
	public static int particles() {
		int n = 0;
		for (Instance inst : INSTANCES) {
			for (EmitterState e : inst.emitters) {
				n += e.particles.size();
				for (List<Particle> t : e.trails) {
					n += t.size();
				}
			}
		}
		return n;
	}

	/** The game's effects were extracted (and readable). */
	public static boolean extracted() {
		return data() != null;
	}

	/** How far a car's effects reach from the car (blocks), for culling. */
	static float radius(CarEntity car, double x, double y, double z) {
		float r = 0;
		for (Instance inst : INSTANCES) {
			if (inst.car != car.getId()) {
				continue;
			}
			for (int i = 0; i < inst.emitters.length; i++) {
				EmitterState e = inst.emitters[i];
				boolean local = inst.defs.get(i).localSpace;
				for (Particle p : e.particles) {
					r = Math.max(r, reach(inst, local, p, x, y, z));
				}
				for (List<Particle> t : e.trails) {
					for (Particle p : t) {
						r = Math.max(r, reach(inst, local, p, x, y, z));
					}
				}
			}
		}
		return r;
	}

	private static float reach(Instance inst, boolean local, Particle p, double x, double y, double z) {
		Vector3f w = local ? inst.frame.toWorld(p.pos) : p.pos;
		return (float) CarRl.toMc(w).subtract(x, y, z).length() + Math.max(p.size.x, p.size.y) / 100.0F;
	}

	// ----------------------------------------------------------------------------- drawing

	/** Vertex data of one material: per vertex x, y, z (blocks, from the car's render origin), u, v, ub, vb, r, g, b, a, and quads with their distance. */
	private static final class Batch {
		float[] v = new float[11 * 64];
		int n;
		final List<float[]> quads = new ArrayList<>();

		int push(Vector3f rl, double ox, double oy, double oz, float u, float vv, float ub, float vb, float[] c) {
			if ((this.n + 1) * 11 > this.v.length) {
				this.v = java.util.Arrays.copyOf(this.v, this.v.length * 2);
			}
			int o = this.n * 11;
			this.v[o] = (float) (rl.x / 100.0 - ox);
			this.v[o + 1] = (float) (rl.z / 100.0 - oy);
			this.v[o + 2] = (float) (rl.y / 100.0 - oz);
			this.v[o + 3] = u;
			this.v[o + 4] = vv;
			this.v[o + 5] = ub;
			this.v[o + 6] = vb;
			System.arraycopy(c, 0, this.v, o + 7, 4);
			return this.n++;
		}
	}

	/**
	 * Draws one car's effects, as camera-facing sprites and ribbons; the pose stack is at the car's
	 * render origin {@code (ox, oy, oz)}, world-aligned.
	 */
	static void submit(SubmitNodeCollector collector, PoseStack poseStack, int car, double ox, double oy, double oz, CameraRenderState camera) {
		if (data == null || INSTANCES.isEmpty()) {
			return;
		}
		Vector3f cam = CarRl.toRl(camera.pos.x, camera.pos.y, camera.pos.z);
		Vector3f camRight = CarRl.toRlDir(camera.orientation.transform(new Vector3f(1, 0, 0)));
		Vector3f camUp = CarRl.toRlDir(camera.orientation.transform(new Vector3f(0, 1, 0)));
		Map<String, Batch> batches = new HashMap<>();
		for (Instance inst : INSTANCES) {
			if (inst.car != car) {
				continue;
			}
			for (int i = 0; i < inst.emitters.length; i++) {
				EmitterDef def = inst.defs.get(i);
				EmitterState e = inst.emitters[i];
				if (!MATERIALS.containsKey(def.material) || e.empty()) {
					continue;
				}
				Batch b = batches.computeIfAbsent(def.material, k -> new Batch());
				if (def.ribbon) {
					float tiling = def.tilingDistance > 0 ? def.tilingDistance : 1.0F;
					for (List<Particle> trail : e.trails) {
						ribbon(b, inst, def, trail, cam, tiling, ox, oy, oz);
					}
				} else {
					for (Particle p : e.particles) {
						sprite(b, def, inst, p, cam, camRight, camUp, ox, oy, oz);
					}
				}
			}
		}
		for (Map.Entry<String, Batch> e : batches.entrySet()) {
			Batch b = e.getValue();
			RenderType type = renderType(e.getKey());
			if (type == null || b.quads.isEmpty()) {
				continue;
			}
			// Back to front (the translucent materials need it; additive ones do not mind).
			b.quads.sort((x, y) -> Float.compare(y[4], x[4]));
			float[] v = b.v;
			List<float[]> quads = b.quads;
			collector.submitCustomGeometry(poseStack, type, (pose, buf) -> {
				for (float[] q : quads) {
					for (int k = 0; k < 4; k++) {
						int o = (int) q[k] * 11;
						buf.addVertex(pose, v[o], v[o + 1], v[o + 2])
							.setColor(argb(v[o + 7] / COLOR_SCALE, v[o + 8] / COLOR_SCALE, v[o + 9] / COLOR_SCALE, v[o + 10]))
							.setUv(v[o + 3], v[o + 4])
							.setUv1(Math.round(Math.clamp(v[o + 5], -32.0F, 32.0F) * 1000.0F), Math.round(Math.clamp(v[o + 6], -32.0F, 32.0F) * 1000.0F))
							.setLight(0)
							.setNormal(pose, 0, 1, 0);
					}
				}
			});
		}
	}

	private static void sprite(Batch b, EmitterDef def, Instance inst, Particle p, Vector3f cam, Vector3f right, Vector3f up, double ox, double oy, double oz) {
		Vector3f center = def.localSpace ? inst.frame.toWorld(p.pos) : new Vector3f(p.pos);
		Vector3f toCam = normalizeOrZero(new Vector3f(cam).sub(center));
		center.add(new Vector3f(toCam).mul(p.cameraOffset));
		Vector3f r, u;
		if (def.velocityAligned) {
			// The sprite's up axis along the velocity, its right axis facing the camera.
			Vector3f v = def.localSpace ? inst.frame.rot.transform(new Vector3f(p.vel)) : new Vector3f(p.vel);
			Vector3f vu = normalizeOrZero(v);
			if (vu.lengthSquared() == 0) {
				return;
			}
			Vector3f vr = normalizeOrZero(new Vector3f(toCam).cross(vu));
			r = vr.mul(p.size.x * 0.5F);
			u = vu.mul(p.size.y * 0.5F);
		} else {
			float s = (float) Math.sin(p.rotation), c = (float) Math.cos(p.rotation);
			r = new Vector3f(right).mul(c).add(new Vector3f(up).mul(s)).mul(p.size.x * 0.5F);
			u = new Vector3f(up).mul(c).sub(new Vector3f(right).mul(s)).mul(p.size.x * 0.5F);
		}
		int cols = def.cols, rows = def.rows;
		float u0 = (p.cell % cols) / (float) cols, v0 = (p.cell / cols) / (float) rows, du = 1.0F / cols, dv = 1.0F / rows;
		int a = b.push(new Vector3f(center).sub(r).add(u), ox, oy, oz, u0, v0, 0, 0, p.color);
		int bb = b.push(new Vector3f(center).add(r).add(u), ox, oy, oz, u0 + du, v0, 0, 0, p.color);
		int c = b.push(new Vector3f(center).add(r).sub(u), ox, oy, oz, u0 + du, v0 + dv, 0, 0, p.color);
		int d = b.push(new Vector3f(center).sub(r).sub(u), ox, oy, oz, u0, v0 + dv, 0, 0, p.color);
		b.quads.add(new float[] {a, bb, c, d, center.distanceSquared(cam)});
	}

	/**
	 * A strip through a trail's points, head (newest) first: UV (along 0..1, across 0..1), UV_B
	 * (across 0..1, distance from the head / TilingDistance).
	 */
	private static void ribbon(Batch b, Instance inst, EmitterDef def, List<Particle> trail, Vector3f cam, float tiling, double ox, double oy, double oz) {
		int n = trail.size();
		if (n < 2) {
			return;
		}
		Vector3f[] pts = new Vector3f[n];
		Particle[] ps = new Particle[n];
		for (int k = 0; k < n; k++) {
			Particle p = trail.get(n - 1 - k);
			ps[k] = p;
			pts[k] = def.localSpace ? inst.frame.toWorld(p.pos) : new Vector3f(p.pos);
		}
		float head = ps[0].distance;
		int pa = -1, pb = -1;
		for (int k = 0; k < n; k++) {
			Vector3f pos = pts[k];
			Vector3f tangent = normalizeOrZero(k + 1 < n ? new Vector3f(pts[k + 1]).sub(pos) : new Vector3f(pos).sub(pts[k - 1]));
			Vector3f side = def.worldUp ? new Vector3f(0, 0, 1) : normalizeOrZero(new Vector3f(tangent).cross(normalizeOrZero(new Vector3f(cam).sub(pos))));
			Vector3f half = side.mul(ps[k].size.x * 0.5F);
			float along = k / (float) (n - 1);
			float dist = Math.abs(head - ps[k].distance) / tiling;
			int a = b.push(new Vector3f(pos).sub(half), ox, oy, oz, along, 0, 0, dist, ps[k].color);
			int bb = b.push(new Vector3f(pos).add(half), ox, oy, oz, along, 1, 1, dist, ps[k].color);
			if (pa >= 0) {
				b.quads.add(new float[] {pa, pb, bb, a, pos.distanceSquared(cam)});
			}
			pa = a;
			pb = bb;
		}
	}

	private static int argb(float r, float g, float b, float a) {
		return channel(a) << 24 | channel(r) << 16 | channel(g) << 8 | channel(b);
	}

	private static int channel(float v) {
		return Math.round(Math.clamp(v, 0.0F, 1.0F) * 255.0F);
	}

	// ----------------------------------------------------------------------------- shakes

	private static void shake(Data d, String name, float scale) {
		ShakeEntry e = d.shakes.get(name);
		if (e == null || e.shake == null || scale <= 0 || e.shake.duration == 0) {
			return;
		}
		SHAKES.start(e.shake, scale);
	}

	/**
	 * Playing camera shakes (UE3 CameraShake oscillations: each oscillator {@code Amplitude *
	 * sin(offset + Frequency * t)}, blended in and out, for OscillationDuration seconds; less than
	 * 0 means forever).
	 */
	private static final class Shakes {
		private record Shake(ShakeDef def, float scale, float[] time, Map<String, Float> offsets) {
		}

		final List<Shake> list = new ArrayList<>();

		void start(ShakeDef def, float scale) {
			Map<String, Float> offsets = new HashMap<>();
			for (Map.Entry<String, Osc> o : def.rot.entrySet()) {
				offsets.put("rot." + o.getKey(), o.getValue().randomOffset ? RNG.next() * (float) (Math.PI * 2) : 0.0F);
			}
			for (Map.Entry<String, Osc> o : def.loc.entrySet()) {
				offsets.put("loc." + o.getKey(), o.getValue().randomOffset ? RNG.next() * (float) (Math.PI * 2) : 0.0F);
			}
			this.list.add(new Shake(def, scale, new float[1], offsets));
		}

		/** Advances the shakes and writes the camera's location offset (uu) and rotation offset (radians). */
		void advance(float dt, Vector3f loc, Vector3f rot) {
			loc.zero();
			rot.zero();
			for (Shake s : this.list) {
				s.time[0] += dt;
				float t = s.time[0];
				ShakeDef d = s.def;
				float w = 1.0F;
				if (d.blendIn > 0) {
					w = Math.min(w, t / d.blendIn);
				}
				if (d.duration > 0 && d.blendOut > 0) {
					w = Math.min(w, (d.duration - t) / d.blendOut);
				}
				w = Math.clamp(w, 0.0F, 1.0F) * s.scale;
				for (Map.Entry<String, Osc> o : d.loc.entrySet()) {
					float v = osc(s, "loc." + o.getKey(), o.getValue(), t) * w;
					switch (o.getKey()) {
						case "X" -> loc.x += v;
						case "Y" -> loc.y += v;
						case "Z" -> loc.z += v;
						default -> {
						}
					}
				}
				for (Map.Entry<String, Osc> o : d.rot.entrySet()) {
					// Rotator units: 65536 per turn.
					float v = osc(s, "rot." + o.getKey(), o.getValue(), t) * w * (float) (Math.PI * 2) / 65536.0F;
					switch (o.getKey()) {
						case "Pitch" -> rot.x += v;
						case "Yaw" -> rot.y += v;
						case "Roll" -> rot.z += v;
						default -> {
						}
					}
				}
			}
			this.list.removeIf(s -> s.def.duration >= 0 && s.time[0] >= s.def.duration);
		}

		private static float osc(Shake s, String k, Osc o, float t) {
			return o.amplitude * (float) Math.sin(s.offsets.getOrDefault(k, 0.0F) + o.frequency * t);
		}
	}

	/** This frame's camera shake location offset (uu: camera forward, right, up). */
	public static Vector3f shakeLocation() {
		return SHAKE_LOC;
	}

	/** This frame's camera shake rotation offset (radians: pitch, yaw, roll). */
	public static Vector3f shakeRotation() {
		return SHAKE_ROT;
	}
}
