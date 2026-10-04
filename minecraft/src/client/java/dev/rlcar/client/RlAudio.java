package dev.rlcar.client;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import dev.rlcar.RlCar;
import dev.rlcar.physics.CarPose;
import dev.rlcar.physics.RlCarNative;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import javax.sound.sampled.AudioFormat;
import javax.sound.sampled.AudioInputStream;
import javax.sound.sampled.AudioSystem;
import net.minecraft.client.Minecraft;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.phys.Vec3;
import org.joml.Vector3f;
import org.jspecify.annotations.Nullable;
import org.lwjgl.BufferUtils;
import org.lwjgl.openal.AL10;
import org.lwjgl.openal.ALC10;

/**
 * Rocket League's car sounds, a port of the Bevy demo's {@code crates/rl_car_bevy/src/audio.rs}.
 *
 * <p>With the extracted sounds ({@code audio/} next to the car models, written by
 * {@code tools/rl_assets/extract.py} from the game's Wwise sound banks) every car plays what the
 * game's car plays, on the same occasions: the engine and exhaust loops, the tyres, jump, double
 * jump and dodge, the in-air whoosh (own car only, as in the game), wheel landings, body impacts
 * and the body slide, entering supersonic and its loop, the boost loop and tail, and the empty-tank
 * "dry fire". The sounds are driven by a small player for the part of Wwise those events use
 * ({@link Bank}, {@link Emitter}): play/stop actions with fades and delays, random/sequence,
 * switch and blend containers, actor-mixers and buses, with volume and pitch from properties,
 * random ranges and RTPC curves, initial delays and instance limits. The game parameters (RTPCs)
 * are set from each car's state under the names the game's native code uses ({@code Speed},
 * {@code RPM}, {@code Throttle_Input}, {@code WheelForwardSpeed}, ...). The engine RPM, computed by
 * native game code that is not in the packages, is reconstructed from the engine audio profile
 * ({@link Engine}).
 *
 * <p>The voices play through OpenAL directly, on Minecraft's context: Minecraft's own sound
 * instances clamp pitch to 0.5..2 and volume to 1 and change them 20 times a second, while the
 * engine needs several octaves at frame rate. The driven car is heard as in the Bevy demo (not
 * positioned); other cars are positioned in the world and fade with distance. The volume follows
 * Minecraft's Players slider; the voices pause with the game.
 *
 * <p>Not reproduced (as in the Bevy demo): filters, Wwise's own 3D attenuation curves, effects,
 * states, modulators, and parameters with no source here (replays, split screen, crowd...), which
 * keep the bank's default value.
 */
public final class RlAudio {
	/** Wwise treats anything this quiet as silent. */
	private static final float SILENT_DB = -96.3F;
	/** Other cars: full volume up to this distance (blocks), silent at {@link #FAR}. */
	private static final float NEAR = 4.0F;
	private static final float FAR = 64.0F;

	private static @Nullable Bank bank;
	private static boolean loaded;
	private static long context;
	private static boolean paused;
	/** {@code -Drlcar.audioLog}: log the driven car's parameters and voices twice a second. */
	private static final boolean LOG = System.getProperty("rlcar.audioLog") != null;
	private static float logTimer;

	private RlAudio() {
	}

	// ----------------------------------------------------------------------------- data

	private enum Interp {
		LOG3, SINE, LOG1, INV_S_CURVE, LINEAR, S_CURVE, EXP1, SINE_RECIP, EXP3, CONSTANT;

		static Interp parse(String s) {
			return switch (s) {
				case "Log3" -> LOG3;
				case "Sine" -> SINE;
				case "Log1" -> LOG1;
				case "InvSCurve" -> INV_S_CURVE;
				case "SCurve" -> S_CURVE;
				case "Exp1" -> EXP1;
				case "SineRecip" -> SINE_RECIP;
				case "Exp3" -> EXP3;
				case "Constant" -> CONSTANT;
				default -> LINEAR;
			};
		}

		/** Shape of a segment, 0..1 -> 0..1 (Wwise's curve shapes). */
		float shape(float t) {
			return switch (this) {
				case LINEAR -> t;
				case CONSTANT -> 0.0F;
				case LOG1 -> 1.0F - (float) Math.pow(1.0F - t, 1.41F);
				case LOG3 -> 1.0F - (1.0F - t) * (1.0F - t) * (1.0F - t);
				case EXP1 -> (float) Math.pow(t, 1.41F);
				case EXP3 -> t * t * t;
				case SINE -> (float) Math.sin(t * Math.PI / 2);
				case SINE_RECIP -> 1.0F - (float) Math.cos(t * Math.PI / 2);
				case S_CURVE -> 0.5F - 0.5F * (float) Math.cos(t * Math.PI);
				case INV_S_CURVE -> 0.5F + (float) (Math.asin(Math.clamp(2.0F * t - 1.0F, -1.0F, 1.0F)) / Math.PI);
			};
		}
	}

	/** A Wwise curve: points (x, y) with the shape of the segment that starts at each. */
	private record Curve(float[] x, float[] y, Interp[] shape) {
		static Curve of(JsonArray points) {
			int n = points.size();
			float[] x = new float[n], y = new float[n];
			Interp[] s = new Interp[n];
			for (int i = 0; i < n; i++) {
				JsonArray p = points.get(i).getAsJsonArray();
				x[i] = p.get(0).getAsFloat();
				y[i] = p.get(1).getAsFloat();
				s[i] = Interp.parse(p.get(2).getAsString());
			}
			return new Curve(x, y, s);
		}

		float eval(float v) {
			int n = this.x.length;
			if (n == 0) {
				return 0.0F;
			}
			if (v <= this.x[0]) {
				return this.y[0];
			}
			for (int i = 0; i + 1 < n; i++) {
				if (v < this.x[i + 1]) {
					float t = this.x[i + 1] > this.x[i] ? (v - this.x[i]) / (this.x[i + 1] - this.x[i]) : 1.0F;
					return this.y[i] + (this.y[i + 1] - this.y[i]) * this.shape[i].shape(t);
				}
			}
			return this.y[n - 1];
		}
	}

	/** An RTPC on volume (dB, or "dB" curves stored as linear gain - 1) or pitch (cents). */
	private record Rtpc(String name, boolean pitch, boolean dbScaled, Curve curve) {
		static @Nullable Rtpc of(JsonObject r) {
			if (r.has("rtpc_type") && !r.get("rtpc_type").isJsonNull() && r.get("rtpc_type").getAsString().equals("Modulator")) {
				return null;
			}
			String param = key(r.get("param"));
			boolean volume = param.equals("Volume") || param.equals("MakeUpGain") || param.equals("BusVolume");
			if (!volume && !param.equals("Pitch")) {
				return null;
			}
			boolean db = r.has("scaling") && !r.get("scaling").isJsonNull() && r.get("scaling").getAsString().equals("dB");
			return new Rtpc(key(r.get("rtpc")), !volume, db, Curve.of(r.getAsJsonArray("points")));
		}

		/** Adds this curve's (dB, cents) at the emitter's parameter value to {@code acc}. */
		void eval(Emitter e, float[] acc) {
			float y = this.curve.eval(e.param(this.name));
			if (this.pitch) {
				acc[1] += y;
			} else {
				acc[0] += this.dbScaled ? gainToDb(1.0F + y) : y;
			}
		}
	}

	private static List<Rtpc> rtpcs(@Nullable JsonElement a) {
		List<Rtpc> out = new ArrayList<>();
		if (a != null && a.isJsonArray()) {
			for (JsonElement e : a.getAsJsonArray()) {
				Rtpc r = Rtpc.of(e.getAsJsonObject());
				if (r != null) {
					out.add(r);
				}
			}
		}
		return out;
	}

	private enum Kind { SOUND, RANDOM, SWITCH, LAYER, CONTAINER }

	private record Layer(List<Rtpc> rtpcs, @Nullable String crossfade, Map<Integer, Curve> assoc) {
	}

	private static final class Node {
		Kind kind = Kind.CONTAINER;
		@Nullable Integer parent;
		@Nullable Integer bus;
		float volumeDb;
		float pitchCents;
		float delay;
		float @Nullable [] volumeRange;
		float @Nullable [] pitchRange;
		List<Rtpc> rtpcs = List.of();
		int maxInstances;
		boolean killNewest;
		int[] children = new int[0];
		// sound
		String media = "";
		boolean looping;
		// random / sequence
		boolean sequence;
		boolean continuous;
		long loopCount = 1;
		int avoidRepeat;
		int[] playlist = new int[0];
		float[] weights = new float[0];
		// switch
		String group = "";
		String defaultValue = "";
		Map<String, int[]> switches = Map.of();
		// layer
		List<Layer> layers = List.of();
	}

	private record Action(boolean play, int target, float fade, float delay) {
	}

	private record Cue(@Nullable String play, @Nullable String stop, boolean localOnly, JsonObject params) {
		float param(String name, float fallback) {
			JsonElement e = this.params.get(name);
			return e != null && e.isJsonPrimitive() && e.getAsJsonPrimitive().isNumber() ? e.getAsFloat() : fallback;
		}
	}

	private record RtpcSwitch(String rtpc, float[] x, String[] values) {
	}

	/** {@code EngineAudioProfile_TA} of the default car (cooked values over the class defaults). */
	private static final class EngineProfile {
		float[] shiftDownMin = {1000};
		float[] shiftDownRand = {0};
		float[] shiftUpMin = {7000};
		float[] shiftUpRand = {0};
		float gearSwitchTime;
		float rpmAccelClutched;
		float rpmDecelClutched;
		float rpmMaxClutched;
		float rpmAccelFactor;
		float rpmDecelFactor;
		float rpmShiftUpBoost;
		float airMaxThrottleTime;
		float revLimitRpm;
		float revLimitRpmDecel;
		float wheelForwardSpeedInterpRate;
		float wheelSideSpeedInterpRate;

		int gears() {
			return this.shiftUpMin.length;
		}

		static EngineProfile of(JsonObject o) {
			EngineProfile p = new EngineProfile();
			JsonArray g = o.getAsJsonArray("Gears");
			int n = Math.max(1, g == null ? 0 : g.size());
			p.shiftDownMin = new float[n];
			p.shiftDownRand = new float[n];
			p.shiftUpMin = new float[n];
			p.shiftUpRand = new float[n];
			for (int i = 0; g != null && i < g.size(); i++) {
				JsonObject gear = g.get(i).getAsJsonObject();
				p.shiftDownMin[i] = gear.getAsJsonObject("RPMShiftDownRange").get("min").getAsFloat();
				p.shiftDownRand[i] = gear.getAsJsonObject("RPMShiftDownRange").get("rand").getAsFloat();
				p.shiftUpMin[i] = gear.getAsJsonObject("RPMShiftUpRange").get("min").getAsFloat();
				p.shiftUpRand[i] = gear.getAsJsonObject("RPMShiftUpRange").get("rand").getAsFloat();
			}
			p.gearSwitchTime = num(o, "GearSwitchTime");
			p.rpmAccelClutched = num(o, "RPMAccelClutched");
			p.rpmDecelClutched = num(o, "RPMDecelClutched");
			p.rpmMaxClutched = num(o, "RPMMaxClutched");
			p.rpmAccelFactor = num(o, "RPMAccelFactor");
			p.rpmDecelFactor = num(o, "RPMDecelFactor");
			p.rpmShiftUpBoost = num(o, "RPMShiftUpBoost");
			p.airMaxThrottleTime = num(o, "AirMaxThrottleTime");
			p.revLimitRpm = num(o, "RevLimitRPM");
			p.revLimitRpmDecel = num(o, "RevLimitRPMDecel");
			p.wheelForwardSpeedInterpRate = num(o, "WheelForwardSpeedInterpRate");
			p.wheelSideSpeedInterpRate = num(o, "WheelSideSpeedInterpRate");
			return p;
		}
	}

	/** The extracted part of the game's Wwise project. */
	private static final class Bank {
		final Map<Integer, Node> nodes = new HashMap<>();
		final Map<String, List<Action>> events = new HashMap<>();
		final Map<String, Cue> cues = new HashMap<>();
		final Map<String, RtpcSwitch> rtpcSwitches = new HashMap<>();
		final Map<String, Float> defaults = new HashMap<>();
		EngineProfile engine = new EngineProfile();
		Path dir;
		final Map<String, Media> media = new ConcurrentHashMap<>();

		@Nullable String cuePlay(String cue) {
			Cue c = this.cues.get(cue);
			return c == null ? null : c.play;
		}

		@Nullable String cueStop(String cue) {
			Cue c = this.cues.get(cue);
			return c == null ? null : c.stop;
		}

		float cueParam(String cue, String name, float fallback) {
			Cue c = this.cues.get(cue);
			return c == null ? fallback : c.param(name, fallback);
		}
	}

	/** A JSON id or name as the string key used everywhere (names when the extractor resolved them, decimal ids otherwise). */
	private static String key(@Nullable JsonElement v) {
		if (v == null || v.isJsonNull()) {
			return "";
		}
		if (v.isJsonPrimitive() && v.getAsJsonPrimitive().isString()) {
			return v.getAsString();
		}
		return Long.toString(v.getAsLong());
	}

	private static int id(JsonElement v) {
		return (int) v.getAsLong();
	}

	private static int[] ids(@Nullable JsonElement a) {
		if (a == null || !a.isJsonArray()) {
			return new int[0];
		}
		JsonArray arr = a.getAsJsonArray();
		int[] out = new int[arr.size()];
		for (int i = 0; i < out.length; i++) {
			out[i] = id(arr.get(i));
		}
		return out;
	}

	private static float num(@Nullable JsonObject o, String k) {
		if (o == null) {
			return 0.0F;
		}
		JsonElement e = o.get(k);
		return e != null && e.isJsonPrimitive() && e.getAsJsonPrimitive().isNumber() ? e.getAsFloat() : 0.0F;
	}

	private static float @Nullable [] range(JsonObject ranges, String k) {
		JsonElement e = ranges.get(k);
		if (e == null || !e.isJsonArray()) {
			return null;
		}
		return new float[] {e.getAsJsonArray().get(0).getAsFloat(), e.getAsJsonArray().get(1).getAsFloat()};
	}

	private static @Nullable Bank bank() {
		if (!loaded) {
			loaded = true;
			Path root = RlModels.root();
			Path file = root == null ? null : root.resolve("audio/audio.json");
			if (file != null && Files.isRegularFile(file)) {
				try {
					bank = parse(file);
					startDecoding(bank);
				} catch (IOException | RuntimeException e) {
					RlCar.LOG.error("RL Car: cannot read {}; no car sounds", file, e);
				}
			}
		}
		return bank;
	}

	private static Bank parse(Path file) throws IOException {
		JsonObject j = JsonParser.parseString(Files.readString(file)).getAsJsonObject();
		Bank b = new Bank();
		b.dir = file.getParent();
		for (Map.Entry<String, JsonElement> e : j.getAsJsonObject("nodes").entrySet()) {
			JsonObject n = e.getValue().getAsJsonObject();
			Node node = new Node();
			String kind = n.get("kind").getAsString();
			JsonObject props = n.has("props") ? n.getAsJsonObject("props") : new JsonObject();
			JsonObject ranges = n.has("ranges") ? n.getAsJsonObject("ranges") : new JsonObject();
			node.parent = n.has("parent") && !n.get("parent").isJsonNull() ? id(n.get("parent")) : null;
			node.bus = n.has("bus") && !n.get("bus").isJsonNull() ? id(n.get("bus")) : null;
			node.volumeDb = num(props, "Volume") + num(props, "MakeUpGain") + num(props, "BusVolume");
			node.pitchCents = num(props, "Pitch");
			node.delay = num(props, "InitialDelay");
			node.volumeRange = range(ranges, "Volume");
			node.pitchRange = range(ranges, "Pitch");
			node.rtpcs = rtpcs(n.get("rtpcs"));
			if (n.has("max_instances") && n.get("max_instances").isJsonObject()) {
				JsonObject m = n.getAsJsonObject("max_instances");
				node.maxInstances = m.get("count").getAsInt();
				node.killNewest = m.get("kill_newest").getAsBoolean();
			}
			node.children = ids(n.get("children"));
			long loop = n.has("loop") && !n.get("loop").isJsonNull() ? n.get("loop").getAsLong() : 1;
			switch (kind) {
				case "sound" -> {
					node.kind = Kind.SOUND;
					node.media = n.has("media") && !n.get("media").isJsonNull() ? n.get("media").getAsString() : "";
					node.looping = n.has("loop") && !n.get("loop").isJsonNull() && loop == 0;
				}
				case "random" -> {
					node.kind = Kind.RANDOM;
					node.sequence = n.has("mode") && !n.get("mode").isJsonNull() && n.get("mode").getAsString().equals("Sequence");
					node.continuous = n.has("continuous") && n.get("continuous").getAsBoolean();
					node.loopCount = loop;
					node.avoidRepeat = n.has("avoid_repeat") && !n.get("avoid_repeat").isJsonNull() ? n.get("avoid_repeat").getAsInt() : 0;
					JsonArray pl = n.has("playlist") ? n.getAsJsonArray("playlist") : new JsonArray();
					node.playlist = new int[pl.size()];
					node.weights = new float[pl.size()];
					for (int i = 0; i < pl.size(); i++) {
						node.playlist[i] = id(pl.get(i).getAsJsonArray().get(0));
						node.weights[i] = pl.get(i).getAsJsonArray().get(1).getAsFloat();
					}
				}
				case "switch" -> {
					node.kind = Kind.SWITCH;
					node.group = key(n.get("group"));
					node.defaultValue = key(n.get("default"));
					Map<String, int[]> sw = new HashMap<>();
					if (n.has("switches")) {
						for (Map.Entry<String, JsonElement> s : n.getAsJsonObject("switches").entrySet()) {
							sw.put(s.getKey(), ids(s.getValue()));
						}
					}
					node.switches = sw;
				}
				case "layer" -> {
					node.kind = Kind.LAYER;
					List<Layer> layers = new ArrayList<>();
					if (n.has("layers")) {
						for (JsonElement le : n.getAsJsonArray("layers")) {
							JsonObject l = le.getAsJsonObject();
							Map<Integer, Curve> assoc = new HashMap<>();
							for (Map.Entry<String, JsonElement> a : l.getAsJsonObject("assoc").entrySet()) {
								assoc.put((int) Long.parseLong(a.getKey()), Curve.of(a.getValue().getAsJsonArray()));
							}
							String cross = l.has("crossfade") && !l.get("crossfade").isJsonNull() ? key(l.get("crossfade")) : null;
							layers.add(new Layer(rtpcs(l.get("rtpcs")), cross, assoc));
						}
					}
					node.layers = layers;
				}
				default -> node.kind = Kind.CONTAINER;
			}
			b.nodes.put((int) Long.parseLong(e.getKey()), node);
		}
		for (Map.Entry<String, JsonElement> e : j.getAsJsonObject("events").entrySet()) {
			List<Action> acts = new ArrayList<>();
			for (JsonElement ae : e.getValue().getAsJsonArray()) {
				JsonObject a = ae.getAsJsonObject();
				String type = a.get("type").getAsString();
				if ((type.equals("play") || type.equals("stop")) && a.has("target") && !a.get("target").isJsonNull()) {
					acts.add(new Action(type.equals("play"), id(a.get("target")), num(a, "fade_ms") / 1000.0F, num(a, "delay_ms") / 1000.0F));
				}
			}
			b.events.put(e.getKey(), acts);
		}
		for (Map.Entry<String, JsonElement> e : j.getAsJsonObject("cues").entrySet()) {
			JsonObject c = e.getValue().getAsJsonObject();
			String play = c.has("play") && !c.get("play").isJsonNull() ? c.get("play").getAsString() : null;
			String stop = c.has("stop") && !c.get("stop").isJsonNull() ? c.get("stop").getAsString() : null;
			boolean local = c.has("local_only") && c.get("local_only").getAsBoolean();
			b.cues.put(e.getKey(), new Cue(play, stop, local, c.has("params") ? c.getAsJsonObject("params") : new JsonObject()));
		}
		JsonObject game = j.getAsJsonObject("game");
		for (Map.Entry<String, JsonElement> e : game.getAsJsonObject("params").entrySet()) {
			b.defaults.put(e.getKey(), num(e.getValue().getAsJsonObject(), "default"));
		}
		for (Map.Entry<String, JsonElement> e : game.getAsJsonObject("rtpc_switches").entrySet()) {
			JsonObject s = e.getValue().getAsJsonObject();
			JsonArray pts = s.getAsJsonArray("points");
			float[] x = new float[pts.size()];
			String[] v = new String[pts.size()];
			for (int i = 0; i < pts.size(); i++) {
				x[i] = pts.get(i).getAsJsonArray().get(0).getAsFloat();
				v[i] = pts.get(i).getAsJsonArray().get(1).getAsString();
			}
			b.rtpcSwitches.put(e.getKey(), new RtpcSwitch(key(s.get("rtpc")), x, v));
		}
		b.engine = EngineProfile.of(j.getAsJsonObject("engine"));
		return b;
	}

	// ----------------------------------------------------------------------------- media

	/** A decoded sound: 16-bit PCM, and its OpenAL buffers once uploaded (as is, and mixed to mono for positioned voices). */
	private static final class Media {
		final ByteBuffer pcm;
		final int channels;
		final int rate;
		int buffer;
		int monoBuffer;

		Media(ByteBuffer pcm, int channels, int rate) {
			this.pcm = pcm;
			this.channels = channels;
			this.rate = rate;
		}

		/** The OpenAL buffer for a voice (mono if positioned); 0 if it cannot be made. */
		int buffer(boolean positioned) {
			boolean mono = positioned && this.channels == 2;
			int b = mono ? this.monoBuffer : this.buffer;
			if (b != 0) {
				return b;
			}
			b = AL10.alGenBuffers();
			if (AL10.alGetError() != AL10.AL_NO_ERROR) {
				return 0;
			}
			if (mono) {
				ByteBuffer src = this.pcm.duplicate().order(ByteOrder.LITTLE_ENDIAN);
				int frames = src.remaining() / 4;
				ByteBuffer out = BufferUtils.createByteBuffer(frames * 2).order(ByteOrder.LITTLE_ENDIAN);
				for (int i = 0; i < frames; i++) {
					out.putShort((short) ((src.getShort(i * 4) + src.getShort(i * 4 + 2)) / 2));
				}
				out.flip();
				AL10.alBufferData(b, AL10.AL_FORMAT_MONO16, out, this.rate);
				this.monoBuffer = b;
			} else {
				AL10.alBufferData(b, this.channels == 2 ? AL10.AL_FORMAT_STEREO16 : AL10.AL_FORMAT_MONO16, this.pcm.duplicate(), this.rate);
				this.buffer = b;
			}
			return b;
		}
	}

	/** Decodes every sound on a background thread (they are short; a voice waits for its sound). */
	private static void startDecoding(Bank b) {
		List<String> files = new ArrayList<>();
		for (Node n : b.nodes.values()) {
			if (n.kind == Kind.SOUND && !n.media.isEmpty() && !files.contains(n.media)) {
				files.add(n.media);
			}
		}
		Thread t = new Thread(() -> {
			for (String f : files) {
				try (AudioInputStream in = AudioSystem.getAudioInputStream(b.dir.resolve(f).toFile())) {
					AudioFormat fmt = in.getFormat();
					if (fmt.getSampleSizeInBits() != 16 || fmt.isBigEndian() || fmt.getChannels() > 2 || fmt.getEncoding() != AudioFormat.Encoding.PCM_SIGNED) {
						RlCar.LOG.warn("RL Car: {} is not 16-bit PCM; skipped", f);
						continue;
					}
					byte[] data = in.readAllBytes();
					ByteBuffer pcm = BufferUtils.createByteBuffer(data.length);
					pcm.put(data).flip();
					b.media.put(f, new Media(pcm, fmt.getChannels(), (int) fmt.getSampleRate()));
				} catch (Exception e) {
					RlCar.LOG.warn("RL Car: cannot decode {}", f, e);
				}
			}
		}, "RL Car sound decoder");
		t.setDaemon(true);
		t.start();
	}

	// ----------------------------------------------------------------------------- emitters

	/** One playing sound. */
	private static final class Voice {
		long play;
		/** The node the play action targeted (stop actions match it). */
		int target;
		int[] path;
		/** Random volume (dB) / pitch (cents) offsets rolled at play, per node of {@code path}. */
		float[][] offsets;
		float age;
		float delay;
		float fadeIn;
		float stopAt = Float.NaN;
		float stopLen;
		/** Continuous random container this sound belongs to and its loops left (0 = forever), or none. */
		int continuous;
		long loops;
		boolean hasContinuous;
		boolean started;
		int source;
		boolean positioned;

		boolean stopping() {
			return !Float.isNaN(this.stopAt);
		}
	}

	/** A Wwise game object: one car's parameters, switches, container history and voices. */
	private static final class Emitter {
		final Bank bank;
		final Map<String, Float> params = new HashMap<>();
		final Map<Integer, List<Integer>> history = new HashMap<>();
		final Map<Integer, Integer> sequence = new HashMap<>();
		final List<Voice> voices = new ArrayList<>();
		/** Delayed play actions: (time left, target, fade). */
		final List<float[]> pending = new ArrayList<>();
		int rng = 0x2545F491;
		long nextPlay;
		boolean positioned;
		float x;
		float y;
		float z;
		float gain = 1.0F;

		Emitter(Bank bank) {
			this.bank = bank;
		}

		void set(String name, float v) {
			this.params.put(name, v);
		}

		float param(String name) {
			Float v = this.params.get(name);
			if (v == null) {
				v = this.bank.defaults.get(name);
			}
			return v == null ? 0.0F : v;
		}

		float random() {
			int x = this.rng;
			x ^= x << 13;
			x ^= x >>> 17;
			x ^= x << 5;
			this.rng = x;
			return (x >>> 8) / (float) (1 << 24);
		}

		String switchValue(String group, String fallback) {
			RtpcSwitch s = this.bank.rtpcSwitches.get(group);
			if (s != null && s.x.length > 0) {
				float v = this.param(s.rtpc);
				for (int i = s.x.length - 1; i >= 0; i--) {
					if (v >= s.x[i]) {
						return s.values[i];
					}
				}
				return s.values[0];
			}
			return fallback;
		}

		/** The ancestors of a node (itself first), following the Wwise parent links. */
		int[] ancestors(int id) {
			List<Integer> out = new ArrayList<>();
			out.add(id);
			int cur = id;
			while (true) {
				Node n = this.bank.nodes.get(cur);
				if (n == null || n.parent == null || out.contains(n.parent)) {
					break;
				}
				out.add(n.parent);
				cur = n.parent;
			}
			return out.stream().mapToInt(Integer::intValue).toArray();
		}

		/** Picks a random/sequence container's next child, or null. */
		@Nullable Integer pick(int id) {
			Node n = this.bank.nodes.get(id);
			if (n == null || n.kind != Kind.RANDOM || n.playlist.length == 0) {
				return null;
			}
			if (n.sequence) {
				int i = this.sequence.getOrDefault(id, 0);
				this.sequence.put(id, i + 1);
				return n.playlist[i % n.playlist.length];
			}
			List<Integer> recent = this.history.computeIfAbsent(id, k -> new ArrayList<>());
			List<Integer> pool = new ArrayList<>();
			List<Float> weights = new ArrayList<>();
			for (int i = 0; i < n.playlist.length; i++) {
				if (!recent.contains(n.playlist[i])) {
					pool.add(n.playlist[i]);
					weights.add(n.weights[i]);
				}
			}
			if (pool.isEmpty()) {
				for (int i = 0; i < n.playlist.length; i++) {
					pool.add(n.playlist[i]);
					weights.add(n.weights[i]);
				}
			}
			float total = 0;
			for (float w : weights) {
				total += Math.max(w, 0);
			}
			float r = this.random() * total;
			int child = pool.getLast();
			for (int i = 0; i < pool.size(); i++) {
				if (r < weights.get(i)) {
					child = pool.get(i);
					break;
				}
				r -= weights.get(i);
			}
			int keep = Math.min(n.avoidRepeat, Math.max(0, n.playlist.length - 1));
			recent.add(child);
			while (recent.size() > keep) {
				recent.removeFirst();
			}
			return child;
		}

		/** The sounds a play of {@code id} starts, each with the continuous container it belongs to (id, loops) or null. */
		void resolve(int id, long @Nullable [] continuous, List<Object[]> out) {
			Node n = this.bank.nodes.get(id);
			if (n == null) {
				return;
			}
			switch (n.kind) {
				case SOUND -> out.add(new Object[] {id, continuous});
				case RANDOM -> {
					long[] seq = n.continuous ? new long[] {id, n.loopCount} : continuous;
					Integer c = this.pick(id);
					if (c != null) {
						this.resolve(c, seq, out);
					}
				}
				case SWITCH -> {
					int[] children = n.switches.get(this.switchValue(n.group, n.defaultValue));
					if (children != null) {
						for (int c : children) {
							this.resolve(c, continuous, out);
						}
					}
				}
				case LAYER, CONTAINER -> {
					for (int c : n.children) {
						this.resolve(c, continuous, out);
					}
				}
			}
		}

		/** Posts a Wwise event by name. */
		void post(@Nullable String event) {
			List<Action> actions = event == null ? null : this.bank.events.get(event);
			if (actions == null) {
				return;
			}
			for (Action a : actions) {
				if (a.play && a.delay > 0) {
					this.pending.add(new float[] {a.delay, a.target, a.fade});
				} else if (a.play) {
					this.play(a.target, a.fade);
				} else {
					for (Voice v : this.voices) {
						if ((v.target == a.target || contains(v.path, a.target)) && !v.stopping()) {
							v.stopAt = v.age;
							v.stopLen = a.fade;
						}
					}
				}
			}
		}

		void play(int target, float fade) {
			long play = ++this.nextPlay;
			List<Object[]> sounds = new ArrayList<>();
			this.resolve(target, null, sounds);
			for (Object[] s : sounds) {
				this.start((Integer) s[0], play, target, fade, (long[]) s[1]);
			}
		}

		void start(int sound, long play, int target, float fade, long @Nullable [] continuous) {
			int[] path = this.ancestors(sound);
			// Instance limits: count the plays already sounding under each limited node.
			for (int n : path) {
				Node node = this.bank.nodes.get(n);
				if (node == null || node.maxInstances <= 0) {
					continue;
				}
				List<long[]> plays = new ArrayList<>();
				for (Voice v : this.voices) {
					if (contains(v.path, n) && !v.stopping() && v.play != play && plays.stream().noneMatch(p -> p[0] == v.play)) {
						plays.add(new long[] {v.play, Float.floatToIntBits(v.age)});
					}
				}
				if (plays.size() >= node.maxInstances) {
					if (node.killNewest) {
						return;
					}
					long oldest = -1;
					float age = -1;
					for (long[] p : plays) {
						float a = Float.intBitsToFloat((int) p[1]);
						if (a > age) {
							age = a;
							oldest = p[0];
						}
					}
					for (Voice v : this.voices) {
						if (v.play == oldest) {
							v.stopAt = v.age;
							v.stopLen = 0;
						}
					}
				}
			}
			Node node = this.bank.nodes.get(sound);
			if (node == null || node.kind != Kind.SOUND || node.media.isEmpty()) {
				return;
			}
			Voice v = new Voice();
			v.play = play;
			v.target = target;
			v.path = path;
			v.offsets = new float[path.length][2];
			for (int i = 0; i < path.length; i++) {
				Node p = this.bank.nodes.get(path[i]);
				if (p == null) {
					continue;
				}
				if (p.volumeRange != null) {
					v.offsets[i][0] = p.volumeRange[0] + (p.volumeRange[1] - p.volumeRange[0]) * this.random();
				}
				if (p.pitchRange != null) {
					v.offsets[i][1] = p.pitchRange[0] + (p.pitchRange[1] - p.pitchRange[0]) * this.random();
				}
				v.delay += p.delay;
			}
			v.fadeIn = fade;
			if (continuous != null) {
				v.hasContinuous = true;
				v.continuous = (int) continuous[0];
				v.loops = continuous[1];
			}
			v.positioned = this.positioned;
			this.voices.add(v);
		}

		/** Volume (dB) and pitch (cents) of a voice now: properties, random offsets and RTPCs of the sound and its ancestors, layer crossfades, then the output bus chain. */
		float[] level(Voice v) {
			float[] acc = new float[2];
			float gain = 1.0F;
			Integer bus = null;
			Integer child = null;
			for (int i = 0; i < v.path.length; i++) {
				Node n = this.bank.nodes.get(v.path[i]);
				if (n == null) {
					continue;
				}
				acc[0] += n.volumeDb + v.offsets[i][0];
				acc[1] += n.pitchCents + v.offsets[i][1];
				for (Rtpc r : n.rtpcs) {
					r.eval(this, acc);
				}
				if (n.kind == Kind.LAYER && child != null) {
					for (Layer l : n.layers) {
						Curve c = l.assoc.get(child);
						if (c == null) {
							continue;
						}
						if (l.crossfade != null) {
							gain *= Math.clamp(c.eval(this.param(l.crossfade)), 0.0F, 1.0F);
						}
						for (Rtpc r : l.rtpcs) {
							r.eval(this, acc);
						}
					}
				}
				if (bus == null) {
					bus = n.bus;
				}
				child = v.path[i];
			}
			int seen = 0;
			while (bus != null && seen++ < 32) {
				Node b = this.bank.nodes.get(bus);
				if (b == null) {
					break;
				}
				acc[0] += b.volumeDb;
				float[] busAcc = new float[2];
				for (Rtpc r : b.rtpcs) {
					r.eval(this, busAcc);
				}
				acc[0] += busAcc[0];
				bus = b.parent;
			}
			acc[0] += gainToDb(gain);
			return acc;
		}

		/** Ages the voices, applies fades and levels to their OpenAL sources, ends finished ones and moves continuous playlists on. */
		void update(float dt, float volume) {
			for (Iterator<float[]> it = this.pending.iterator(); it.hasNext();) {
				float[] p = it.next();
				p[0] -= dt;
				if (p[0] <= 0) {
					it.remove();
					this.play((int) p[1], p[2]);
				}
			}
			List<Voice> next = new ArrayList<>();
			for (Iterator<Voice> it = this.voices.iterator(); it.hasNext();) {
				Voice v = it.next();
				v.age += dt;
				if (!v.started) {
					if (v.stopping()) {
						it.remove();
						continue;
					}
					if (v.age < v.delay) {
						continue;
					}
					Node n = this.bank.nodes.get(v.path[0]);
					Media m = n == null ? null : this.bank.media.get(n.media);
					if (m == null) {
						if (v.age > v.delay + 2.0F) {
							it.remove(); // never decoded
						}
						continue;
					}
					if (!startSource(v, m, n.looping)) {
						it.remove();
						continue;
					}
				}
				float[] lv = this.level(v);
				float t = v.age - v.delay;
				float fade = v.fadeIn > 0 ? Math.min(t / v.fadeIn, 1.0F) : 1.0F;
				if (v.stopping()) {
					float f = v.stopLen > 0 ? 1.0F - (v.age - v.stopAt) / v.stopLen : 0.0F;
					if (f <= 0) {
						release(v);
						it.remove();
						continue;
					}
					fade *= f;
				}
				float db = Math.max(lv[0] + gainToDb(fade), SILENT_DB);
				float g = db <= SILENT_DB ? 0.0F : (float) Math.pow(10.0, db / 20.0);
				AL10.alSourcef(v.source, AL10.AL_GAIN, Math.min(g * volume * this.gain, 16.0F));
				AL10.alSourcef(v.source, AL10.AL_PITCH, centsToSpeed(lv[1]));
				if (v.positioned) {
					AL10.alSource3f(v.source, AL10.AL_POSITION, this.x, this.y, this.z);
				}
				if (AL10.alGetSourcei(v.source, AL10.AL_SOURCE_STATE) == AL10.AL_STOPPED && !paused) {
					release(v);
					it.remove();
					if (v.hasContinuous && !v.stopping() && v.loops != 1) {
						next.add(v);
					}
				}
			}
			for (Voice v : next) {
				Integer child = this.pick(v.continuous);
				if (child == null) {
					continue;
				}
				List<Object[]> sounds = new ArrayList<>();
				this.resolve(child, new long[] {v.continuous, v.loops > 1 ? v.loops - 1 : v.loops}, sounds);
				for (Object[] s : sounds) {
					this.start((Integer) s[0], v.play, v.target, 0.0F, (long[]) s[1]);
				}
			}
		}

		void setPaused(boolean pause) {
			for (Voice v : this.voices) {
				if (v.started && v.source != 0) {
					if (pause) {
						AL10.alSourcePause(v.source);
					} else {
						AL10.alSourcePlay(v.source);
					}
				}
			}
		}

		/** Stops everything at once (car gone, demolished, or the sound engine went away). */
		void stopAll(boolean release) {
			for (Voice v : this.voices) {
				if (release) {
					release(v);
				}
			}
			this.voices.clear();
			this.pending.clear();
		}
	}

	private static boolean contains(int[] a, int v) {
		for (int x : a) {
			if (x == v) {
				return true;
			}
		}
		return false;
	}

	private static boolean startSource(Voice v, Media m, boolean looping) {
		int buffer = m.buffer(v.positioned);
		if (buffer == 0) {
			return false;
		}
		int s = AL10.alGenSources();
		if (AL10.alGetError() != AL10.AL_NO_ERROR || s == 0) {
			return false; // out of sources: the sound is skipped
		}
		AL10.alSourcei(s, AL10.AL_BUFFER, buffer);
		AL10.alSourcei(s, AL10.AL_LOOPING, looping ? AL10.AL_TRUE : AL10.AL_FALSE);
		AL10.alSourcef(s, AL10.AL_MAX_GAIN, 16.0F);
		// Distance is handled here (other cars fade out by NEAR..FAR); OpenAL only pans.
		AL10.alSourcef(s, AL10.AL_ROLLOFF_FACTOR, 0.0F);
		if (v.positioned) {
			AL10.alSourcei(s, AL10.AL_SOURCE_RELATIVE, AL10.AL_FALSE);
		} else {
			AL10.alSourcei(s, AL10.AL_SOURCE_RELATIVE, AL10.AL_TRUE);
			AL10.alSource3f(s, AL10.AL_POSITION, 0, 0, 0);
		}
		AL10.alSourcef(s, AL10.AL_GAIN, 0.0F);
		AL10.alSourcePlay(s);
		if (paused) {
			AL10.alSourcePause(s);
		}
		v.source = s;
		v.started = true;
		return true;
	}

	private static void release(Voice v) {
		if (v.source != 0) {
			AL10.alSourceStop(v.source);
			AL10.alDeleteSources(v.source);
			v.source = 0;
		}
	}

	static float gainToDb(float g) {
		return g <= 1e-5F ? SILENT_DB : 20.0F * (float) Math.log10(g);
	}

	static float centsToSpeed(float cents) {
		return (float) Math.pow(2.0, Math.clamp(cents, -4800.0F, 4800.0F) / 1200.0F);
	}

	// ----------------------------------------------------------------------------- the cars

	/** FInterpTo: moves {@code current} towards {@code target} by {@code rate} per second of the remaining distance. */
	private static float interpTo(float current, float target, float dt, float rate) {
		return rate <= 0 ? target : current + (target - current) * Math.min(dt * rate, 1.0F);
	}

	/** What a car's sound components remember between frames. */
	private static final class CarSounds {
		final Emitter emitter;
		final Engine engine = new Engine();
		@Nullable CarPose last;
		boolean started;
		boolean whoosh;
		boolean supersonic;
		boolean boosting;
		float slide = -1;
		boolean sliding;
		float lastImpact = -10;
		float clock;
		float spamBoost;
		float wheelForward;
		float wheelSide;
		boolean local;

		CarSounds(Bank bank) {
			this.emitter = new Emitter(bank);
		}

		void post(String cue, boolean play) {
			Cue c = this.emitter.bank.cues.get(cue);
			if (c == null || c.localOnly && !this.local) {
				return;
			}
			this.emitter.post(play ? c.play : c.stop);
		}
	}

	private static final Map<Integer, CarSounds> CARS = new HashMap<>();

	/**
	 * Advances one car's sounds to this frame. {@code local}: the car this client drives. Called
	 * every frame for every car being drawn, then {@link #endFrame}.
	 */
	static void update(int id, CarPose pose, boolean local, float dt) {
		Bank b = bank();
		if (b == null || !contextReady()) {
			return;
		}
		CarSounds c = CARS.computeIfAbsent(id, k -> new CarSounds(b));
		c.local = local;
		Emitter w = c.emitter;
		w.set("IsLocal", local ? 1.0F : 0.0F);
		w.set("NumOfLocalPlayers", 1.0F);
		w.positioned = !local;
		Vec3 cam = Minecraft.getInstance().gameRenderer.mainCamera().position();
		w.x = (float) pose.x;
		w.y = (float) pose.y;
		w.z = (float) pose.z;
		float d = (float) cam.distanceTo(new Vec3(pose.x, pose.y, pose.z));
		w.gain = local ? 1.0F : Math.clamp(1.0F - (d - NEAR) / (FAR - NEAR), 0.0F, 1.0F);
		w.gain *= w.gain;
		if (!c.started) {
			for (String cue : new String[] {"engine.EngineAudio", "engine.ExhaustAudio", "car_fx.AkWheelDriveSound"}) {
				c.post(cue, true);
			}
			c.started = true;
		}
		CarRl s = CarRl.of(pose);
		CarPose prev = c.last;
		c.last = pose.copy();
		if (prev != null && dt > 0) {
			events(b, c, prev, pose, s, dt);
		}
		params(b, c, pose, s, dt);
		w.update(dt, volume());
		if (LOG && local && (logTimer += dt) >= 0.5F) {
			logTimer = 0;
			RlCar.LOG.info("rlcar audio: Speed={} RPM={} Throttle_Input={} WheelForwardSpeed={} volume={}", Math.round(w.param("Speed")), Math.round(w.param("RPM")),
				w.param("Throttle_Input"), Math.round(w.param("WheelForwardSpeed")), volume());
			for (Voice v : w.voices) {
				Node n = b.nodes.get(v.path[0]);
				float[] lv = w.level(v);
				RlCar.LOG.info("rlcar audio:   {} {} dB {} cents{}", n == null ? "?" : n.media, Math.round(lv[0] * 10) / 10.0F, Math.round(lv[1]), v.stopping() ? " (stopping)" : "");
			}
		}
	}

	/** The car's FX actor, impact and boost events between the previous frame's pose and this one. */
	private static void events(Bank b, CarSounds c, CarPose prev, CarPose s, CarRl rl, float dt) {
		Emitter w = c.emitter;
		c.clock += dt;
		if (s.hasContact(RlCarNative.CONTACT_HAS_JUMPED) && !prev.hasContact(RlCarNative.CONTACT_HAS_JUMPED)) {
			c.post("car_fx.JumpSound", true);
		}
		if (s.hasContact(RlCarNative.CONTACT_HAS_DOUBLE_JUMPED) && !prev.hasContact(RlCarNative.CONTACT_HAS_DOUBLE_JUMPED)) {
			c.post("car_fx.DoubleJumpSound", true);
		}
		if (s.hasContact(RlCarNative.CONTACT_HAS_FLIPPED) && !prev.hasContact(RlCarNative.CONTACT_HAS_FLIPPED)) {
			c.post("car_fx.DodgeSound", true);
		}
		boolean inAir = !s.has(RlCarNative.FLAG_ON_GROUND);
		if (inAir != c.whoosh) {
			c.post("car_fx.WhooshSound", inAir);
			c.whoosh = inAir;
		}
		boolean supersonic = s.has(RlCarNative.FLAG_SUPERSONIC);
		if (supersonic != c.supersonic) {
			if (supersonic) {
				c.post("car_fx.AkEnterSupersonicSound", true);
			}
			c.post("car_fx.AkLoopSupersonicSound", supersonic);
			c.supersonic = supersonic;
		}
		boolean boosting = s.has(RlCarNative.FLAG_BOOSTING);
		if (boosting != c.boosting) {
			if (boosting) {
				// AkRTPCDecayComponent on the Boost FX event: +1 per activation, up to MaxValue.
				c.spamBoost = Math.min(c.spamBoost + 1.0F, 5.0F);
			}
			c.post("boost.BoostSound", boosting);
			c.boosting = boosting;
		}
		if (s.hasContact(RlCarNative.CONTACT_BOOST_HELD) && !prev.hasContact(RlCarNative.CONTACT_BOOST_HELD) && s.boost <= 0) {
			c.post("boost.DryFireSound", true);
		}
		// Wheel landings: per wheel, with the speed into the surface as the impact momentum.
		float minWheel = b.cueParam("car_fx.AkWheelImpactSound", "MinImpactMomentum", 50.0F);
		for (int i = 0; i < 4; i++) {
			Vector3f n = wheelNormal(s, i);
			if (n != null && wheelNormal(prev, i) == null) {
				float momentum = -dot(prev.velocity, n);
				if (momentum >= minWheel) {
					w.set("ImpactIntensity", momentum);
					c.post("car_fx.AkWheelImpactSound", true);
				}
			}
		}
		// Car body against the world (ImpactEffectsComponent): impacts and the slide loop.
		float minBody = b.cueParam("impacts.AkImpactSound", "MinImpactMomentum", 50.0F);
		float minDelay = b.cueParam("impacts.AkImpactSound", "MinImpactDelay", 0.15F);
		float slideDelay = b.cueParam("impacts.AkSlideSound", "AkSlideSoundDelay", 0.15F);
		float slideMin = b.cueParam("impacts.AkSlideSound", "AkSlideMomentumMin", 200.0F);
		if (s.hasContact(RlCarNative.CONTACT_WORLD)) {
			Vector3f n = new Vector3f(s.contacts[0], s.contacts[1], s.contacts[2]);
			if (!prev.hasContact(RlCarNative.CONTACT_WORLD)) {
				float momentum = -dot(prev.velocity, n);
				if (momentum >= minBody && c.clock - c.lastImpact >= minDelay) {
					w.set("ImpactIntensity", momentum);
					c.post("impacts.AkImpactSound", true);
					c.lastImpact = c.clock;
				}
			}
			Vector3f v = new Vector3f(s.velocity[0], s.velocity[1], s.velocity[2]);
			float speed = new Vector3f(v).sub(new Vector3f(n).mul(v.dot(n))).length();
			c.slide = Math.max(c.slide, 0) + dt;
			w.set("Car_SlideAngle", v.length() > 1 ? speed / v.length() : 0);
			boolean slide = c.slide >= slideDelay && speed >= slideMin;
			if (slide != c.sliding) {
				c.post("impacts.AkSlideSound", slide);
				c.sliding = slide;
			}
		} else {
			c.slide = -1;
			if (c.sliding) {
				c.post("impacts.AkSlideSound", false);
				c.sliding = false;
			}
		}
	}

	/** The game parameters from the latest state. */
	private static void params(Bank b, CarSounds c, CarPose s, CarRl rl, float dt) {
		Emitter w = c.emitter;
		dt = Math.min(dt, 0.1F);
		w.set("Speed", rl.vel.length());
		// Angular speed about each car axis, as a fraction of the maximum.
		float maxAng = 5.5F;
		w.set("Car_Roll", Math.min(Math.abs(rl.angVel.dot(rl.forward())) / maxAng, 1.0F));
		w.set("Car_Pitch", Math.min(Math.abs(rl.angVel.dot(rl.right())) / maxAng, 1.0F));
		w.set("Car_Yaw", Math.min(Math.abs(rl.angVel.dot(rl.up())) / maxAng, 1.0F));
		// WheelSpeedComponent_TA: forward and sideways speed of the wheels on the ground, eased.
		EngineProfile p = b.engine;
		int on = Integer.bitCount(s.flags >>> RlCarNative.FLAG_WHEEL_CONTACT_SHIFT & 0xF);
		float fwd = on > 0 ? Math.abs(rl.vel.dot(rl.forward())) : 0;
		float side = on > 0 ? Math.abs(rl.vel.dot(rl.right())) : 0;
		c.wheelForward = interpTo(c.wheelForward, fwd, dt, p.wheelForwardSpeedInterpRate);
		c.wheelSide = interpTo(c.wheelSide, side, dt, p.wheelSideSpeedInterpRate);
		w.set("WheelForwardSpeed", c.wheelForward);
		w.set("WheelSideSpeed", c.wheelSide);
		w.set("WheelsOnGround", on);
		// SpamControl_Boost decays (DecayPerSecond: 0.2/s at 0 up to 3/s at 5).
		float decay = 0.2F + (3.0F - 0.2F) * Math.clamp(c.spamBoost / 5.0F, 0.0F, 1.0F);
		c.spamBoost = Math.max(c.spamBoost - decay * dt, 0.0F);
		w.set("SpamControl_Boost", c.spamBoost);
		float throttle = s.has(RlCarNative.FLAG_THROTTLING) ? 1.0F : 0.0F;
		float[] rpm = c.engine.update(p, s, rl, throttle, dt);
		w.set("RPM", rpm[0]);
		w.set("Throttle_Input", rpm[1]);
	}

	private static float dot(float[] a, Vector3f b) {
		return a[0] * b.x + a[1] * b.y + a[2] * b.z;
	}

	private static @Nullable Vector3f wheelNormal(CarPose p, int i) {
		float x = p.contacts[3 + i * 3], y = p.contacts[4 + i * 3], z = p.contacts[5 + i * 3];
		return x == 0 && y == 0 && z == 0 ? null : new Vector3f(x, y, z);
	}

	/** Silences a car (demolished: it plays again when it comes back). */
	static void silence(int id) {
		CarSounds c = CARS.remove(id);
		if (c != null && contextReady()) {
			c.emitter.stopAll(true);
		}
	}

	/** After every car's {@link #update}: the cars not seen this frame are gone. */
	static void endFrame(java.util.Set<Integer> seen) {
		boolean ready = context != 0 && ALC10.alcGetCurrentContext() == context;
		for (Iterator<Map.Entry<Integer, CarSounds>> it = CARS.entrySet().iterator(); it.hasNext();) {
			Map.Entry<Integer, CarSounds> e = it.next();
			if (!seen.contains(e.getKey())) {
				e.getValue().emitter.stopAll(ready);
				it.remove();
			}
		}
	}

	/** Pauses and resumes the voices with the game. */
	static void setPaused(boolean pause) {
		if (pause == paused) {
			return;
		}
		paused = pause;
		if (contextReady()) {
			for (CarSounds c : CARS.values()) {
				c.emitter.setPaused(pause);
			}
		}
	}

	/** Stops everything (leaving the world). */
	static void stopAll() {
		boolean ready = context != 0 && ALC10.alcGetCurrentContext() == context;
		for (CarSounds c : CARS.values()) {
			c.emitter.stopAll(ready);
		}
		CARS.clear();
	}

	/**
	 * Minecraft's OpenAL context is current; when it changed (the sound engine was reloaded, or the
	 * device changed) every source and buffer made on the old one is gone, so start over.
	 */
	private static boolean contextReady() {
		long ctx = ALC10.alcGetCurrentContext();
		if (ctx == 0) {
			return false;
		}
		if (ctx != context) {
			context = ctx;
			for (CarSounds c : CARS.values()) {
				c.emitter.stopAll(false);
				c.started = false;
			}
			CARS.clear();
			Bank b = bank;
			if (b != null) {
				for (Media m : b.media.values()) {
					m.buffer = 0;
					m.monoBuffer = 0;
				}
			}
		}
		return true;
	}

	/** The volume of the car sounds: Minecraft's master volume times its Players slider. */
	private static float volume() {
		return Minecraft.getInstance().options.getFinalSoundSourceVolume(SoundSource.PLAYERS);
	}

	/** The game's sounds were extracted (and readable). */
	public static boolean extracted() {
		return bank() != null;
	}

	/** Minecraft's sound engine is running (there is an OpenAL context to play on). */
	public static boolean ready() {
		return ALC10.alcGetCurrentContext() != 0;
	}

	/** Voices playing right now, all cars (for the tests). */
	public static int voices() {
		int n = 0;
		for (CarSounds c : CARS.values()) {
			for (Voice v : c.emitter.voices) {
				if (v.started) {
					n++;
				}
			}
		}
		return n;
	}

	// ----------------------------------------------------------------------------- engine

	/**
	 * The engine's RPM. The game computes it natively from the engine audio profile; this follows
	 * the profile's values by their names (as in the Bevy demo): forward gears with randomised
	 * shift-up/down points, a gear switch time, the clutched behaviour in the air, the higher shift
	 * point while boosting, the rev limiter at the last gear, and rising / falling smoothing. On the
	 * ground the gear and the RPM within it follow the car's forward speed, the gears splitting the
	 * car's 0..2300 uu/s evenly.
	 */
	private static final class Engine {
		float rpm;
		int gear;
		float shifting;
		float airThrottle;
		float shiftUp;
		float shiftDown;
		float limiter;
		int rng;

		float roll() {
			this.rng = this.rng * 1_664_525 + 1_013_904_223;
			return (this.rng >>> 8) / (float) (1 << 24);
		}

		void pickPoints(EngineProfile p) {
			int g = Math.min(this.gear, p.gears() - 1);
			this.shiftUp = p.shiftUpMin[g] + p.shiftUpRand[g] * this.roll();
			this.shiftDown = p.shiftDownMin[g] + p.shiftDownRand[g] * this.roll();
		}

		/** (RPM, Throttle_Input) after {@code dt}. */
		float[] update(EngineProfile p, CarPose s, CarRl rl, float throttle, float dt) {
			int gears = Math.max(1, p.gears());
			float idle = p.shiftDownMin[0];
			if (this.rpm == 0) {
				this.rpm = idle;
				this.pickPoints(p);
			}
			float input = Math.abs(throttle);
			boolean onGround = s.has(RlCarNative.FLAG_ON_GROUND);
			float target;
			if (onGround) {
				this.airThrottle = 0;
				float band = 2300.0F / gears;
				float fwd = Math.abs(rl.vel.dot(rl.forward()));
				int gear = Math.min((int) (fwd / band), gears - 1);
				if (gear != this.gear) {
					this.gear = gear;
					this.shifting = p.gearSwitchTime;
					this.pickPoints(p);
				}
				float t = Math.clamp((fwd - band * gear) / band, 0.0F, 1.0F);
				float top = this.shiftUp + (s.has(RlCarNative.FLAG_BOOSTING) ? p.rpmShiftUpBoost : 0);
				float low = gear == 0 ? idle : this.shiftDown;
				target = low + (top - low) * t;
			} else {
				// In the air the engine is clutched: throttle revs it, for a limited time.
				this.airThrottle += dt;
				if (input > 0 && this.airThrottle <= p.airMaxThrottleTime) {
					target = Math.min(this.rpm + p.rpmAccelClutched * input * dt, p.rpmMaxClutched);
				} else {
					target = Math.max(this.rpm - p.rpmDecelClutched * dt, idle);
				}
			}
			this.shifting = Math.max(this.shifting - dt, 0);
			if (onGround) {
				float rate = target > this.rpm ? p.rpmAccelFactor : p.rpmDecelFactor;
				// While a gear engages the engine is clutched and only falls.
				float goal = this.shifting > 0 ? Math.min(target, this.rpm) : target;
				this.rpm = interpTo(this.rpm, goal, dt, rate);
			} else {
				this.rpm = target;
			}
			// Rev limiter: at the last gear's shift point the RPM bounces back.
			boolean last = this.gear + 1 >= gears;
			this.limiter = Math.max(this.limiter - p.revLimitRpmDecel * dt, 0);
			if (onGround && last && input > 0 && this.rpm >= this.shiftUp - 1 && this.limiter == 0) {
				this.limiter = p.revLimitRpm;
			}
			return new float[] {this.rpm - this.limiter, input};
		}
	}
}
