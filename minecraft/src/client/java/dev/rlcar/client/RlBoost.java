package dev.rlcar.client;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import com.mojang.renderpearl.api.pipeline.BlendFunction;
import com.mojang.renderpearl.api.pipeline.ColorTargetState;
import com.mojang.renderpearl.api.pipeline.CompareOp;
import com.mojang.renderpearl.api.pipeline.DepthStencilState;
import com.mojang.renderpearl.api.pipeline.PrimitiveTopology;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import dev.rlcar.RlCar;
import dev.rlcar.entity.CarEntity;
import dev.rlcar.physics.CarPose;
import dev.rlcar.physics.RlCarNative;
import dev.rlcar.physics.Space;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.WeakHashMap;
import net.minecraft.client.renderer.BindGroupLayouts;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.oit.OitPipelineSet;
import net.minecraft.client.renderer.rendertype.RenderSetup;
import net.minecraft.client.renderer.rendertype.RenderType;
import net.minecraft.client.renderer.rendertype.RenderTypes;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import net.minecraft.client.renderer.texture.OverlayTexture;
import org.joml.Vector3f;
import org.jspecify.annotations.Nullable;

/**
 * Boost visuals.
 *
 * <p>With the extracted Rocket League assets ({@code boost/} next to the car models, written by
 * {@code tools/rl_assets/extract.py}), the real cars show the game's default boost ("Standard"),
 * exactly as in the Bevy demo:
 * <ul>
 * <li>the flame cones, placed per car body as the game places them, drawn with a port of the boost
 * material's compiled pixel shader ({@code assets/rlcar/shaders/core/boost_flame.fsh});</li>
 * <li>the smoke trail ({@code Boost_Painted_PS}) while boosting and the small exhaust puffs
 * ({@code Drive_PS}) while only throttling, simulated here from the extracted particle module data
 * and drawn with a port of {@code SmokePuff_Mat} ({@code boost_smoke.fsh}).</li>
 * </ul>
 * Otherwise (box car, or no extracted boost) a simple flickering flame cone is drawn instead.
 *
 * <p>Model space is the car model's (+X forward, +Y up, +Z right, blocks); the extracted particle
 * data is in Unreal units (uu, Z up) and converted where it is used.
 */
public final class RlBoost {
	/** Particle colours go above 1 (HDR); vertex colours carry them divided by this. */
	private static final float COLOR_SCALE = 8.0F;
	private static final DepthStencilState TEST_NO_WRITE = new DepthStencilState(CompareOp.GREATER_THAN_OR_EQUAL, false);

	private static @Nullable Data data;
	private static boolean loaded;
	private static @Nullable RenderPipeline flamePipeline;
	private static @Nullable OitPipelineSet flameOit;
	private static @Nullable RenderPipeline smokePipeline;
	private static @Nullable OitPipelineSet smokeOit;
	/** The two pipelines drawing linear light for {@link LinearFx}, and their textures. */
	private static @Nullable RenderPipeline flameLinear;
	private static @Nullable RenderPipeline smokeLinear;
	private static List<LinearFx.Tex> flameTextures = List.of();
	private static List<LinearFx.Tex> smokeTextures = List.of();
	private static @Nullable RenderType flameType;
	private static @Nullable RenderType smokeType;
	private static boolean typesFailed;
	private static final Map<CarEntity, State> STATES = new WeakHashMap<>();

	private RlBoost() {
	}

	// ----------------------------------------------------------------------------- data

	/** A cooked UE3 distribution: a lookup table sampled like {@code FRawDistribution::GetValue}, or a uniform random range. */
	record Dist(float @Nullable [] table, boolean random, int chunk, float timeScale, float startTime, int dim, float @Nullable [] min, float @Nullable [] max) {
		static @Nullable Dist of(@Nullable JsonElement e) {
			if (e == null || !e.isJsonObject()) {
				return null;
			}
			JsonObject o = e.getAsJsonObject();
			if (o.has("table")) {
				return new Dist(floats(o.getAsJsonArray("table")), o.get("random").getAsBoolean(), o.get("chunk").getAsInt(), o.get("time_scale").getAsFloat(),
					o.get("start_time").getAsFloat(), o.get("dim").getAsInt(), null, null);
			}
			float[] min = floats(o.getAsJsonArray("min"));
			return new Dist(null, false, 0, 0, 0, min.length, min, floats(o.getAsJsonArray("max")));
		}

		float[] sample(float time, Rng rng) {
			float[] out = new float[3];
			if (this.table != null) {
				int entries = Math.max(1, this.table.length / this.chunk);
				float index = Math.max((time - this.startTime) * this.timeScale, 0.0F);
				int i = (int) index;
				float alpha = index - i;
				int e1 = Math.min(i, entries - 1) * this.chunk;
				int e2 = Math.min(i + 1, entries - 1) * this.chunk;
				for (int c = 0; c < this.dim; c++) {
					float lo = this.table[e1 + c] + (this.table[e2 + c] - this.table[e1 + c]) * alpha;
					if (this.random) {
						float hi = this.table[e1 + this.dim + c] + (this.table[e2 + this.dim + c] - this.table[e1 + this.dim + c]) * alpha;
						out[c] = lo + (hi - lo) * rng.next();
					} else {
						out[c] = lo;
					}
				}
			} else {
				for (int c = 0; c < Math.min(3, this.min.length); c++) {
					out[c] = this.min[c] + (this.max[c] - this.min[c]) * rng.next();
				}
			}
			return out;
		}

		float scalar(float time, Rng rng) {
			return this.sample(time, rng)[0];
		}
	}

	private static float[] sampleOr(@Nullable Dist d, float time, Rng rng, float fallback) {
		return d == null ? new float[] {fallback, fallback, fallback} : d.sample(time, rng);
	}

	/** One Cascade sprite emitter, as far as the boost's two systems use it (uu, s, UE axes). */
	private static final class EmitterDef {
		boolean localSpace;
		int cols;
		int rows;
		@Nullable Dist rate;
		@Nullable Dist rateScale;
		float unit;
		@Nullable Dist perUnit;
		float maxFrameDistance;
		float movementTolerance;
		Dist lifetime;
		Dist size;
		@Nullable Dist sizeLife;
		@Nullable Dist velocity;
		@Nullable Dist velLife;
		@Nullable Dist accel;
		@Nullable Dist rotation;
		Dist color;
		Dist alpha;
		@Nullable Dist colorLife;
		@Nullable Dist alphaLife;

		static EmitterDef of(JsonObject o) {
			EmitterDef d = new EmitterDef();
			d.localSpace = o.get("local_space").getAsBoolean();
			d.cols = o.getAsJsonArray("subuv").get(0).getAsInt();
			d.rows = o.getAsJsonArray("subuv").get(1).getAsInt();
			if (o.get("spawn_rate").isJsonObject()) {
				d.rate = Dist.of(o.getAsJsonObject("spawn_rate").get("rate"));
				d.rateScale = Dist.of(o.getAsJsonObject("spawn_rate").get("scale"));
			}
			if (o.get("spawn_per_unit").isJsonObject()) {
				JsonObject s = o.getAsJsonObject("spawn_per_unit");
				d.unit = s.get("unit").getAsFloat();
				d.perUnit = Dist.of(s.get("count"));
				d.maxFrameDistance = s.get("max_frame_distance").getAsFloat();
				d.movementTolerance = s.get("movement_tolerance").getAsFloat();
			}
			d.lifetime = Dist.of(o.get("lifetime"));
			d.size = Dist.of(o.get("size"));
			d.sizeLife = Dist.of(o.get("size_life"));
			d.velocity = Dist.of(o.get("velocity"));
			d.velLife = Dist.of(o.get("vel_life"));
			d.accel = Dist.of(o.get("accel"));
			d.rotation = Dist.of(o.get("rotation"));
			d.color = Dist.of(o.get("color"));
			d.alpha = Dist.of(o.get("alpha"));
			d.colorLife = Dist.of(o.get("color_life"));
			d.alphaLife = Dist.of(o.get("alpha_life"));
			return d;
		}
	}

	private record CarBoost(String cones, float coneDelay, float[][] sockets) {
	}

	private static final class Data {
		final Map<String, Float> flame = new HashMap<>();
		final float[] color = new float[3];
		final Path[] flameTextures = new Path[2];
		final Path[] smokeTextures = new Path[3];
		EmitterDef boost;
		EmitterDef drive;
		final Map<String, CarBoost> cars = new HashMap<>();
	}

	private static @Nullable Data data() {
		if (!loaded) {
			loaded = true;
			Path root = RlModels.root();
			Path file = root == null ? null : root.resolve("boost/boost.json");
			if (file != null && Files.isRegularFile(file)) {
				try {
					data = parse(file);
				} catch (IOException | RuntimeException e) {
					RlCar.LOG.error("RL Car: cannot read {}; using the simple boost flame", file, e);
				}
			}
		}
		return data;
	}

	private static Data parse(Path file) throws IOException {
		JsonObject j = JsonParser.parseString(Files.readString(file)).getAsJsonObject();
		Path dir = file.getParent();
		Data d = new Data();
		JsonObject flame = j.getAsJsonObject("flame");
		JsonObject params = flame.getAsJsonObject("params");
		for (String k : params.keySet()) {
			if (params.get(k).isJsonPrimitive()) {
				d.flame.put(k, params.get(k).getAsFloat());
			}
		}
		JsonArray color = params.getAsJsonArray("CustomColor");
		for (int i = 0; i < 3; i++) {
			d.color[i] = color.get(i).getAsFloat() * d.flame.get("Brightness");
		}
		JsonObject ft = flame.getAsJsonObject("textures");
		d.flameTextures[0] = dir.resolve(ft.get("noise").getAsString());
		d.flameTextures[1] = dir.resolve(ft.get("sparks").getAsString());
		JsonObject st = j.getAsJsonObject("smoke").getAsJsonObject("textures");
		d.smokeTextures[0] = dir.resolve(st.get("smoke").getAsString());
		d.smokeTextures[1] = dir.resolve(st.get("gradient").getAsString());
		d.smokeTextures[2] = dir.resolve(st.get("radial").getAsString());
		d.boost = EmitterDef.of(j.getAsJsonObject("emitters").getAsJsonObject("boost"));
		d.drive = EmitterDef.of(j.getAsJsonObject("emitters").getAsJsonObject("drive"));
		JsonObject cars = j.getAsJsonObject("cars");
		for (String preset : cars.keySet()) {
			JsonObject c = cars.getAsJsonObject(preset);
			JsonArray s = c.getAsJsonArray("emitters");
			float[][] sockets = new float[s.size()][];
			for (int i = 0; i < s.size(); i++) {
				sockets[i] = floats(s.get(i).getAsJsonArray());
			}
			d.cars.put(preset, new CarBoost(c.get("cones").getAsString(), c.get("cone_delay").getAsFloat(), sockets));
		}
		return d;
	}

	private static float[] floats(JsonArray a) {
		float[] out = new float[a.size()];
		for (int i = 0; i < out.length; i++) {
			out[i] = a.get(i).getAsFloat();
		}
		return out;
	}

	/** UE direction or offset (uu) -> Minecraft / model axes (blocks). */
	private static Vector3f ue(float[] v) {
		return new Vector3f(v[0], v[2], v[1]).div(Space.UU_PER_BLOCK);
	}

	// ----------------------------------------------------------------------------- pipelines

	/**
	 * Registers the two shader pipelines (before the first resource load compiles them). The flame
	 * material's parameters are compiled in as defines, so this needs the extracted boost.
	 */
	public static void registerPipelines() {
		Data d = data();
		if (d == null) {
			return;
		}
		RenderPipeline.Builder flame = RenderPipeline.builder()
			.withVertexShader(RlCar.id("core/boost_flame"))
			.withFragmentShader(RlCar.id("core/boost_flame"))
			.withBindGroupLayout(BindGroupLayouts.SAMPLER0_SAMPLER1)
			.withVertexBinding(0, DefaultVertexFormat.ENTITY)
			.withPrimitiveTopology(PrimitiveTopology.QUADS)
			.withDepthStencilState(TEST_NO_WRITE)
			.withCull(false)
			.withShaderDefine("COLOR_R", d.color[0])
			.withShaderDefine("COLOR_G", d.color[1])
			.withShaderDefine("COLOR_B", d.color[2]);
		String[][] defines = {
			{"INNER_SPEED", "Inner_Speed"}, {"OUTER_SPEED", "Outer_Speed"}, {"TILE_X", "TileX"}, {"TILE_Y", "TileY"},
			{"INNER_SPARKS", "Inner_Sparks"}, {"OUTER_SPARKS", "Outer_Sparks"}, {"GRADIENT_AMOUNT", "GradientAmount"},
			{"GRADIENT_SHARPNESS", "GradientSharpness"}, {"FRESNEL_BASE", "FresnelBase"}, {"FRESNEL_END", "FresnelEnd"}, {"OPACITY", "Opacity"},
		};
		for (String[] def : defines) {
			flame.withShaderDefine(def[0], d.flame.getOrDefault(def[1], 0.0F));
		}
		RenderPipeline.Snippet flameBase = flame.buildSnippet();
		flamePipeline = RenderPipelines.register(RenderPipeline.builder(RenderPipelines.MATRICES_FOG_SNIPPET, flameBase)
			.withLocation(RlCar.id("pipeline/boost_flame"))
			.withColorTargetState(new ColorTargetState(BlendFunction.ADDITIVE))
			.build());
		flameLinear = RenderPipelines.register(RenderPipeline.builder(RenderPipelines.MATRICES_FOG_SNIPPET, flameBase)
			.withLocation(RlCar.id("pipeline/boost_flame_linear"))
			.withShaderDefine("FX_LINEAR")
			.withColorTargetState(LinearFx.TARGET)
			.build());
		flameOit = RenderPipelines.register(OitPipelineSet.builder("rlcar_boost_flame", RenderPipeline.builder(flameBase)
			.withBindGroupLayout(BindGroupLayouts.DYNAMIC_TRANSFORMS)
			.withBindGroupLayout(BindGroupLayouts.FOG)
			.withShaderDefine("OIT_ADDITIVE")).build());

		RenderPipeline.Snippet smokeBase = RenderPipeline.builder()
			.withVertexShader(RlCar.id("core/boost_smoke"))
			.withFragmentShader(RlCar.id("core/boost_smoke"))
			.withBindGroupLayout(BindGroupLayouts.SAMPLER0_SAMPLER1_SAMPLER2)
			.withVertexBinding(0, DefaultVertexFormat.ENTITY)
			.withPrimitiveTopology(PrimitiveTopology.QUADS)
			.withDepthStencilState(TEST_NO_WRITE)
			.withCull(false)
			.withShaderDefine("COLOR_SCALE", COLOR_SCALE)
			.buildSnippet();
		smokePipeline = RenderPipelines.register(RenderPipeline.builder(RenderPipelines.MATRICES_FOG_SNIPPET, smokeBase)
			.withLocation(RlCar.id("pipeline/boost_smoke"))
			.withColorTargetState(new ColorTargetState(BlendFunction.TRANSLUCENT))
			.build());
		smokeLinear = RenderPipelines.register(RenderPipeline.builder(RenderPipelines.MATRICES_FOG_SNIPPET, smokeBase)
			.withLocation(RlCar.id("pipeline/boost_smoke_linear"))
			.withShaderDefine("FX_LINEAR")
			.withColorTargetState(LinearFx.TARGET)
			.build());
		smokeOit = RenderPipelines.register(OitPipelineSet.builder("rlcar_boost_smoke", RenderPipeline.builder(smokeBase)
			.withBindGroupLayout(BindGroupLayouts.DYNAMIC_TRANSFORMS)
			.withBindGroupLayout(BindGroupLayouts.FOG)).build());
	}

	/** Creates the render types on first use (their textures are registered then). False if unavailable. */
	private static boolean renderTypes(Data d) {
		if (flameType != null || typesFailed) {
			return !typesFailed;
		}
		if (flamePipeline == null || flameOit == null || smokePipeline == null || smokeOit == null) {
			typesFailed = true;
			return false;
		}
		try {
			flameType = RenderType.create("rlcar_boost_flame", RenderSetup.builder(flamePipeline)
				.setOitPipelines(flameOit)
				.withTexture("Sampler0", RlModels.texture(d.flameTextures[0]))
				.withTexture("Sampler1", RlModels.texture(d.flameTextures[1]))
				.createRenderSetup());
			smokeType = RenderType.create("rlcar_boost_smoke", RenderSetup.builder(smokePipeline)
				.setOitPipelines(smokeOit)
				.withTexture("Sampler0", RlModels.texture(d.smokeTextures[0]))
				.withTexture("Sampler1", RlModels.texture(d.smokeTextures[1]))
				.withTexture("Sampler2", RlModels.texture(d.smokeTextures[2]))
				.sortOnUpload()
				.createRenderSetup());
			flameTextures = List.of(new LinearFx.Tex("Sampler0", RlModels.texture(d.flameTextures[0]), null),
				new LinearFx.Tex("Sampler1", RlModels.texture(d.flameTextures[1]), null));
			smokeTextures = List.of(new LinearFx.Tex("Sampler0", RlModels.texture(d.smokeTextures[0]), null),
				new LinearFx.Tex("Sampler1", RlModels.texture(d.smokeTextures[1]), null),
				new LinearFx.Tex("Sampler2", RlModels.texture(d.smokeTextures[2]), null));
			return true;
		} catch (IOException e) {
			RlCar.LOG.error("RL Car: cannot load the boost textures; using the simple boost flame", e);
			typesFailed = true;
			return false;
		}
	}

	// ----------------------------------------------------------------------------- particles

	/** Small xorshift generator (particles only need cheap uniform randoms). */
	static final class Rng {
		private int s = 0x9E3779B9;

		float next() {
			int x = this.s;
			x ^= x << 13;
			x ^= x >>> 17;
			x ^= x << 5;
			this.s = x;
			return (x >>> 8) / (float) (1 << 24);
		}
	}

	private static final class Particle {
		/** World position (world-space emitters) or offset from the socket in model space (local ones), blocks. */
		double x;
		double y;
		double z;
		final Vector3f baseVel = new Vector3f();
		final Vector3f accel = new Vector3f();
		float size;
		/** Relative time 0..1 and its rate. */
		float rel;
		float invLife;
		final float[] color = new float[3];
		float alpha;
		float rotation;
		int cell;

		/** Cascade's per-frame update: velocity reset to the base velocity, acceleration added to both, scaled over life, then moved. */
		void tick(EmitterDef def, Rng rng, float dt) {
			this.rel += dt * this.invLife;
			this.baseVel.add(this.accel.x * dt, this.accel.y * dt, this.accel.z * dt);
			float[] s = sampleOr(def.velLife, this.rel, rng, 1.0F);
			// The scale is per UE axis; Minecraft swaps Y and Z.
			this.x += this.baseVel.x * s[0] * dt;
			this.y += this.baseVel.y * s[2] * dt;
			this.z += this.baseVel.z * s[1] * dt;
		}
	}

	private static final class Emitter {
		final List<Particle> particles = new ArrayList<>();
		/** Last world position of the socket while active (spawn per unit). */
		double @Nullable [] last;
		float travelled;
		float spawnAcc;

		void spawn(EmitterDef def, Rng rng, double x, double y, double z, float age) {
			Particle p = new Particle();
			p.x = x;
			p.y = y;
			p.z = z;
			p.invLife = 1.0F / Math.max(1e-3F, def.lifetime.scalar(0, rng));
			p.size = def.size.sample(0, rng)[0] / Space.UU_PER_BLOCK;
			if (def.velocity != null) {
				p.baseVel.set(ue(def.velocity.sample(0, rng)));
			}
			if (def.accel != null) {
				p.accel.set(ue(def.accel.sample(0, rng)));
			}
			float[] c = def.color.sample(0, rng);
			System.arraycopy(c, 0, p.color, 0, 3);
			p.alpha = def.alpha.scalar(0, rng);
			p.rotation = def.rotation == null ? 0 : def.rotation.scalar(0, rng) * (float) (Math.PI * 2);
			int cells = def.cols * def.rows;
			p.cell = Math.min((int) (rng.next() * cells), cells - 1);
			p.tick(def, rng, age);
			this.particles.add(p);
		}

		/** One frame: age and move the particles, then spawn. {@code (x, y, z)} is the socket's world position. */
		void update(EmitterDef def, Rng rng, float dt, boolean active, double x, double y, double z) {
			for (Particle p : this.particles) {
				p.tick(def, rng, dt);
			}
			this.particles.removeIf(p -> p.rel >= 1.0F);
			if (!active) {
				this.last = null;
				this.travelled = 0;
				this.spawnAcc = 0;
				return;
			}
			if (def.rate != null && def.rateScale != null) {
				float rate = def.rate.scalar(0, rng) * def.rateScale.scalar(0, rng);
				this.spawnAcc += rate * dt;
				while (this.spawnAcc >= 1.0F) {
					this.spawnAcc -= 1.0F;
					float age = Math.min(dt, this.spawnAcc / Math.max(rate, 1e-3F));
					if (def.localSpace) {
						this.spawn(def, rng, 0, 0, 0, age);
					} else {
						this.spawn(def, rng, x, y, z, age);
					}
				}
			}
			if (def.perUnit != null) {
				// ParticleModuleSpawnPerUnit: particles per `unit` uu travelled, spread along the path.
				if (this.last != null) {
					double dx = x - this.last[0], dy = y - this.last[1], dz = z - this.last[2];
					float travel = (float) Math.sqrt(dx * dx + dy * dy + dz * dz) * Space.UU_PER_BLOCK;
					if (def.maxFrameDistance > 0 && travel > def.maxFrameDistance) {
						this.travelled = 0;
					} else if (travel > def.movementTolerance * def.unit) {
						float perUnit = def.perUnit.scalar(0, rng);
						float total = travel + this.travelled;
						int n = (int) Math.floor(total * perUnit / def.unit);
						this.travelled = total - n * def.unit / Math.max(perUnit, 1e-3F);
						for (int k = 0; k < n; k++) {
							float f = (k + 1) / (float) n;
							this.spawn(def, rng, this.last[0] + dx * f, this.last[1] + dy * f, this.last[2] + dz * f, dt * (1 - f));
						}
					}
				}
				this.last = new double[] {x, y, z};
			}
		}
	}

	/** Per car: the emitters of each boost socket and how long it has been boosting. */
	private static final class State {
		final Rng rng = new Rng();
		@Nullable String preset;
		Emitter[] trails = new Emitter[0];
		Emitter[] drives = new Emitter[0];
		long lastNanos;
		float boostTime;
		float radius;
	}

	// ----------------------------------------------------------------------------- per frame

	/**
	 * Advances the car's boost effects to now and snapshots what to draw into the render state.
	 * Called once per frame per car (render state extraction).
	 */
	public static void extract(CarEntity car, CarPose pose, CarRenderState state, boolean realModel) {
		long now = System.nanoTime();
		State s = STATES.computeIfAbsent(car, c -> new State());
		float dt = s.lastNanos == 0 ? 0 : Math.min(0.1F, (now - s.lastNanos) / 1e9F);
		s.lastNanos = now;
		boolean boosting = pose.has(RlCarNative.FLAG_BOOSTING);
		boolean throttling = pose.has(RlCarNative.FLAG_THROTTLING) && !boosting;
		s.boostTime = boosting ? s.boostTime + dt : 0;

		Data d = realModel ? data() : null;
		String preset = RlCarNative.PRESETS[state.preset];
		CarBoost cb = d == null ? null : d.cars.get(preset);
		state.boostCones = null;
		state.simpleFlame = boosting && cb == null;
		state.smokeCount = 0;
		s.radius = 0;
		if (cb == null) {
			return;
		}
		if (boosting && s.boostTime >= cb.coneDelay) {
			state.boostCones = RlModels.boostModel(cb.cones);
		}

		if (!preset.equals(s.preset) || s.trails.length != cb.sockets.length) {
			s.preset = preset;
			s.trails = new Emitter[cb.sockets.length];
			s.drives = new Emitter[cb.sockets.length];
			for (int i = 0; i < cb.sockets.length; i++) {
				s.trails[i] = new Emitter();
				s.drives[i] = new Emitter();
			}
		}
		Vector3f[] sockets = new Vector3f[cb.sockets.length];
		for (int i = 0; i < sockets.length; i++) {
			sockets[i] = pose.rotation.transform(new Vector3f(cb.sockets[i]));
			s.trails[i].update(d.boost, s.rng, dt, boosting, pose.x + sockets[i].x, pose.y + sockets[i].y, pose.z + sockets[i].z);
			s.drives[i].update(d.drive, s.rng, dt, throttling, pose.x + sockets[i].x, pose.y + sockets[i].y, pose.z + sockets[i].z);
		}

		// Snapshot: per particle x, y, z (relative to the car origin), size, r, g, b (/ COLOR_SCALE), a, rotation, cell, cols, rows.
		int count = 0;
		for (int i = 0; i < sockets.length; i++) {
			count += s.trails[i].particles.size() + s.drives[i].particles.size();
		}
		if (state.smoke.length < count * SMOKE_STRIDE) {
			state.smoke = new float[count * SMOKE_STRIDE * 3 / 2];
		}
		int n = 0;
		for (int i = 0; i < sockets.length; i++) {
			for (Particle p : s.trails[i].particles) {
				n = snapshot(state.smoke, n, d.boost, p, (float) (p.x - pose.x), (float) (p.y - pose.y), (float) (p.z - pose.z), s.rng);
			}
			for (Particle p : s.drives[i].particles) {
				// Local space: the puffs move with the car.
				Vector3f w = pose.rotation.transform(new Vector3f((float) p.x, (float) p.y, (float) p.z)).add(sockets[i]);
				n = snapshot(state.smoke, n, d.drive, p, w.x, w.y, w.z, s.rng);
			}
		}
		state.smokeCount = n;
		for (int i = 0; i < n; i++) {
			int o = i * SMOKE_STRIDE;
			s.radius = Math.max(s.radius, Math.abs(state.smoke[o]) + Math.abs(state.smoke[o + 1]) + Math.abs(state.smoke[o + 2]) + state.smoke[o + 3]);
		}
	}

	private static final int SMOKE_STRIDE = 12;

	private static int snapshot(float[] out, int n, EmitterDef def, Particle p, float x, float y, float z, Rng rng) {
		int o = n * SMOKE_STRIDE;
		float[] c = sampleOr(def.colorLife, p.rel, rng, 1.0F);
		out[o] = x;
		out[o + 1] = y;
		out[o + 2] = z;
		out[o + 3] = p.size * sampleOr(def.sizeLife, p.rel, rng, 1.0F)[0];
		out[o + 4] = p.color[0] * c[0] / COLOR_SCALE;
		out[o + 5] = p.color[1] * c[1] / COLOR_SCALE;
		out[o + 6] = p.color[2] * c[2] / COLOR_SCALE;
		out[o + 7] = p.alpha * sampleOr(def.alphaLife, p.rel, rng, 1.0F)[0];
		out[o + 8] = p.rotation;
		out[o + 9] = p.cell;
		out[o + 10] = def.cols;
		out[o + 11] = def.rows;
		return n + 1;
	}

	/** The game's boost was extracted (and readable). */
	public static boolean extracted() {
		return data() != null;
	}

	/** How far the car's smoke reached from the car origin last frame (for culling), blocks. */
	public static float smokeRadius(CarEntity car) {
		State s = STATES.get(car);
		return s == null ? 0 : s.radius;
	}

	// ----------------------------------------------------------------------------- drawing

	/** Draws the flame cones; the pose stack is in the car's model space. */
	public static void submitCones(SubmitNodeCollector collector, PoseStack poseStack, RlModels.Model cones) {
		Data d = data();
		if (d == null || !renderTypes(d)) {
			return;
		}
		boolean linear = LinearFx.active() && flameLinear != null;
		for (RlModels.Part part : cones.parts()) {
			SubmitNodeCollector.CustomGeometryRenderer geometry = (p, b) -> {
				float[] pos = part.positions();
				float[] nrm = part.normals();
				float[] uv = part.uvs();
				float[] uv1 = part.uvs1();
				int[] tri = part.triangles();
				for (int t = 0; t + 2 < tri.length; t += 3) {
					for (int k = 0; k < 4; k++) {
						int v = tri[t + Math.min(k, 2)];
						b.addVertex(p, pos[v * 3], pos[v * 3 + 1], pos[v * 3 + 2])
							.setColor(-1)
							.setUv(uv[v * 2], uv[v * 2 + 1])
							.setUv1(Math.round(uv1[v * 2] * 1000.0F), 0)
							.setLight(0)
							.setNormal(p, nrm[v * 3], nrm[v * 3 + 1], nrm[v * 3 + 2]);
					}
				}
			};
			if (linear) {
				LinearFx.submit(flameLinear, flameTextures, poseStack, geometry);
			} else {
				collector.submitCustomGeometry(poseStack, flameType, geometry);
			}
		}
	}

	/** Draws the smoke as camera-facing quads, back to front; the pose stack is at the car origin, world-aligned. */
	public static void submitSmoke(SubmitNodeCollector collector, PoseStack poseStack, CarRenderState state, CameraRenderState camera) {
		Data d = data();
		int n = state.smokeCount;
		if (d == null || n == 0 || !renderTypes(d)) {
			return;
		}
		float[] s = state.smoke.clone();
		Vector3f right = camera.orientation.transform(new Vector3f(1, 0, 0));
		Vector3f up = camera.orientation.transform(new Vector3f(0, 1, 0));
		float cx = (float) (camera.pos.x - state.x), cy = (float) (camera.pos.y - state.y), cz = (float) (camera.pos.z - state.z);
		Integer[] order = new Integer[n];
		float[] dist = new float[n];
		for (int i = 0; i < n; i++) {
			order[i] = i;
			int o = i * SMOKE_STRIDE;
			float dx = s[o] - cx, dy = s[o + 1] - cy, dz = s[o + 2] - cz;
			dist[i] = dx * dx + dy * dy + dz * dz;
		}
		java.util.Arrays.sort(order, (a, b) -> Float.compare(dist[b], dist[a]));
		SubmitNodeCollector.CustomGeometryRenderer geometry = (p, b) -> {
			for (int i : order) {
				int o = i * SMOKE_STRIDE;
				float half = s[o + 3] * 0.5F;
				float sin = (float) Math.sin(s[o + 8]), cos = (float) Math.cos(s[o + 8]);
				float rx = (right.x * cos + up.x * sin) * half, ry = (right.y * cos + up.y * sin) * half, rz = (right.z * cos + up.z * sin) * half;
				float ux = (up.x * cos - right.x * sin) * half, uy = (up.y * cos - right.y * sin) * half, uz = (up.z * cos - right.z * sin) * half;
				int cols = (int) s[o + 10], rows = (int) s[o + 11], cell = (int) s[o + 9];
				float u0 = (cell % cols) / (float) cols, v0 = (cell / cols) / (float) rows, du = 1.0F / cols, dv = 1.0F / rows;
				int argb = argb(s[o + 4], s[o + 5], s[o + 6], s[o + 7]);
				float x = s[o], y = s[o + 1], z = s[o + 2];
				smokeVertex(b, p, x - rx + ux, y - ry + uy, z - rz + uz, u0, v0, argb);
				smokeVertex(b, p, x - rx - ux, y - ry - uy, z - rz - uz, u0, v0 + dv, argb);
				smokeVertex(b, p, x + rx - ux, y + ry - uy, z + rz - uz, u0 + du, v0 + dv, argb);
				smokeVertex(b, p, x + rx + ux, y + ry + uy, z + rz + uz, u0 + du, v0, argb);
			}
		};
		if (LinearFx.active() && smokeLinear != null) {
			LinearFx.submit(smokeLinear, smokeTextures, poseStack, geometry);
		} else {
			collector.submitCustomGeometry(poseStack, smokeType, geometry);
		}
	}

	private static void smokeVertex(VertexConsumer b, PoseStack.Pose p, float x, float y, float z, float u, float v, int argb) {
		b.addVertex(p, x, y, z).setColor(argb).setUv(u, v).setOverlay(OverlayTexture.NO_OVERLAY).setLight(0).setNormal(p, 0, 1, 0);
	}

	private static int argb(float r, float g, float b, float a) {
		return channel(a) << 24 | channel(r) << 16 | channel(g) << 8 | channel(b);
	}

	private static int channel(float v) {
		return Math.round(Math.clamp(v, 0.0F, 1.0F) * 255.0F);
	}

	// ----------------------------------------------------------------------------- simple flame

	/**
	 * The simple boost flame: an outer cone and a brighter inner one, fading towards the tip and
	 * flickering in length, pointing backwards from {@code (backX, y, z)} in model space.
	 */
	public static void submitSimpleFlame(SubmitNodeCollector collector, PoseStack poseStack, float backX, float y, float z) {
		float t = (System.nanoTime() % 1_000_000_000_000L) / 1e9F;
		collector.submitCustomGeometry(poseStack, RenderTypes.lightning(), (p, b) -> {
			cone(b, p, backX, y, z, 0.12F, 0.6F * (1 + 0.12F * (float) Math.sin(t * 41) + 0.08F * (float) Math.sin(t * 67)), 0xCCFF590D);
			cone(b, p, backX, y, z, 0.06F, 0.4F * (1 + 0.12F * (float) Math.sin(t * 41 + 1) + 0.08F * (float) Math.sin(t * 67)), 0xE6FFE68C);
		});
	}

	private static void cone(VertexConsumer b, PoseStack.Pose p, float x, float y, float z, float radius, float length, int argb) {
		int segments = 16;
		int tip = argb & 0x00FFFFFF;
		for (int i = 0; i < segments; i++) {
			double a0 = Math.PI * 2 * i / segments, a1 = Math.PI * 2 * (i + 1) / segments;
			// A triangle (base edge to apex) as a quad with a repeated apex.
			b.addVertex(p, x, y + radius * (float) Math.cos(a0), z + radius * (float) Math.sin(a0)).setColor(argb);
			b.addVertex(p, x, y + radius * (float) Math.cos(a1), z + radius * (float) Math.sin(a1)).setColor(argb);
			b.addVertex(p, x - length, y, z).setColor(tip);
			b.addVertex(p, x - length, y, z).setColor(tip);
		}
	}
}
