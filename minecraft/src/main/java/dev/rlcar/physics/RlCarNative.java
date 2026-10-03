package dev.rlcar.physics;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_DOUBLE;
import static java.lang.foreign.ValueLayout.JAVA_FLOAT;
import static java.lang.foreign.ValueLayout.JAVA_INT;

import java.io.IOException;
import java.io.InputStream;
import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.SymbolLookup;
import java.lang.invoke.MethodHandle;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HexFormat;
import java.util.Locale;
import net.fabricmc.loader.api.FabricLoader;

/**
 * Bindings to the Rust physics library ({@code crates/rl_car_ffi}) through Java's FFM API.
 *
 * <p>The library ships inside the mod jar under {@code natives/<os>-<arch>/}. It is extracted
 * once to {@code <game dir>/.rlcar/natives/<content hash>/} and loaded from there; setting the
 * {@code rlcar.native} system property to a library path loads that file instead (handy while
 * iterating on the Rust side).
 *
 * <p>Everything crossing this boundary is in Rocket League space and units; {@link Space}
 * converts to and from Minecraft.
 */
public final class RlCarNative {
	/** Must match {@code rl_car_ffi::ABI_VERSION}. */
	public static final int ABI_VERSION = 2;
	/** Must match {@code rl_car_ffi::POSE_FLOATS}. */
	public static final int POSE_FLOATS = 40;

	public static final int FLAG_ON_GROUND = 1;
	public static final int FLAG_BOOSTING = 1 << 1;
	public static final int FLAG_SUPERSONIC = 1 << 2;
	public static final int FLAG_HAS_FLIP_OR_JUMP = 1 << 3;
	public static final int FLAG_FLIPPING = 1 << 4;
	public static final int FLAG_JUMPING = 1 << 5;
	public static final int FLAG_WHEEL_CONTACT_SHIFT = 8;

	public static final int BUTTON_JUMP = 1;
	public static final int BUTTON_BOOST = 1 << 1;
	public static final int BUTTON_HANDBRAKE = 1 << 2;

	/** Hitbox presets, in {@code HitboxPreset::ALL} order. */
	public static final String[] PRESETS = {"octane", "dominus", "plank", "breakout", "hybrid", "merc", "psyclops"};

	static final MethodHandle WORLD_NEW;
	static final MethodHandle WORLD_FREE;
	static final MethodHandle WORLD_SET_BOXES;
	static final MethodHandle CAR_NEW;
	static final MethodHandle CAR_FREE;
	static final MethodHandle CAR_RESET;
	static final MethodHandle CAR_TRANSLATE;
	static final MethodHandle CAR_STEP;
	static final MethodHandle CAR_ADVANCE;
	static final MethodHandle CAR_ALPHA;
	static final MethodHandle CAR_POSE;
	static final MethodHandle CAR_SAVE;
	static final MethodHandle CAR_LOAD;
	static final MethodHandle CAR_PRESET;
	static final MethodHandle CAR_SET_UNLIMITED_BOOST;
	static final MethodHandle PRESET_HITBOX;

	static {
		SymbolLookup lib = SymbolLookup.libraryLookup(libraryPath(), Arena.global());
		Linker linker = Linker.nativeLinker();
		MethodHandle abi = bind(linker, lib, "rlcar_abi_version", FunctionDescriptor.of(JAVA_INT));
		int version;
		try {
			version = (int) abi.invokeExact();
		} catch (Throwable t) {
			throw new IllegalStateException("rl_car_ffi: cannot query ABI version", t);
		}
		if (version != ABI_VERSION) {
			throw new IllegalStateException("rl_car_ffi ABI " + version + " does not match the mod (" + ABI_VERSION + "); rebuild the native library");
		}

		FunctionDescriptor controls = FunctionDescriptor.ofVoid(ADDRESS, ADDRESS, JAVA_INT, JAVA_FLOAT, JAVA_FLOAT, JAVA_FLOAT, JAVA_FLOAT, JAVA_FLOAT, JAVA_INT);
		WORLD_NEW = bind(linker, lib, "rlcar_world_new", FunctionDescriptor.of(ADDRESS));
		WORLD_FREE = bind(linker, lib, "rlcar_world_free", FunctionDescriptor.ofVoid(ADDRESS));
		WORLD_SET_BOXES = bind(linker, lib, "rlcar_world_set_boxes", FunctionDescriptor.of(JAVA_INT, ADDRESS, ADDRESS, JAVA_INT));
		CAR_NEW = bind(linker, lib, "rlcar_car_new", FunctionDescriptor.of(ADDRESS, JAVA_INT));
		CAR_FREE = bind(linker, lib, "rlcar_car_free", FunctionDescriptor.ofVoid(ADDRESS));
		CAR_RESET = bind(linker, lib, "rlcar_car_reset", FunctionDescriptor.ofVoid(ADDRESS, ADDRESS, JAVA_FLOAT, JAVA_FLOAT, JAVA_FLOAT));
		CAR_TRANSLATE = bind(linker, lib, "rlcar_car_translate", FunctionDescriptor.ofVoid(ADDRESS, ADDRESS));
		CAR_STEP = bind(linker, lib, "rlcar_car_step", controls);
		CAR_ADVANCE = bind(linker, lib, "rlcar_car_advance",
			FunctionDescriptor.of(JAVA_INT, ADDRESS, ADDRESS, JAVA_DOUBLE, JAVA_FLOAT, JAVA_FLOAT, JAVA_FLOAT, JAVA_FLOAT, JAVA_FLOAT, JAVA_INT));
		CAR_ALPHA = bind(linker, lib, "rlcar_car_alpha", FunctionDescriptor.of(JAVA_FLOAT, ADDRESS));
		CAR_POSE = bind(linker, lib, "rlcar_car_pose", FunctionDescriptor.of(JAVA_INT, ADDRESS, JAVA_FLOAT, ADDRESS));
		CAR_SAVE = bind(linker, lib, "rlcar_car_save", FunctionDescriptor.of(JAVA_INT, ADDRESS, ADDRESS, JAVA_INT));
		CAR_LOAD = bind(linker, lib, "rlcar_car_load", FunctionDescriptor.of(JAVA_INT, ADDRESS, ADDRESS, JAVA_INT));
		CAR_PRESET = bind(linker, lib, "rlcar_car_preset", FunctionDescriptor.of(JAVA_INT, ADDRESS));
		CAR_SET_UNLIMITED_BOOST = bind(linker, lib, "rlcar_car_set_unlimited_boost", FunctionDescriptor.ofVoid(ADDRESS, JAVA_INT));
		PRESET_HITBOX = bind(linker, lib, "rlcar_preset_hitbox", FunctionDescriptor.ofVoid(JAVA_INT, ADDRESS));
	}

	private RlCarNative() {
	}

	/** Forces the library to load (and fail early with a clear message if it cannot). */
	public static void init() {
	}

	/**
	 * Hitbox of a preset in car-local uu: {length, width, height, forward offset, right offset, up
	 * offset} (offsets of the box centre from the car origin).
	 */
	public static float[] presetHitbox(int preset) {
		try (Arena arena = Arena.ofConfined()) {
			MemorySegment out = arena.allocate(JAVA_FLOAT, 6);
			PRESET_HITBOX.invokeExact(preset, out);
			return out.toArray(JAVA_FLOAT);
		} catch (Throwable t) {
			throw rethrow(t);
		}
	}

	static RuntimeException rethrow(Throwable t) {
		return t instanceof RuntimeException r ? r : new IllegalStateException(t);
	}

	private static MethodHandle bind(Linker linker, SymbolLookup lib, String name, FunctionDescriptor desc) {
		MemorySegment symbol = lib.find(name).orElseThrow(() -> new IllegalStateException("rl_car_ffi: missing symbol " + name));
		return linker.downcallHandle(symbol, desc);
	}

	private static Path libraryPath() {
		String override = System.getProperty("rlcar.native");
		if (override != null) {
			return Path.of(override);
		}
		String os = System.getProperty("os.name").toLowerCase(Locale.ROOT);
		String arch = System.getProperty("os.arch").toLowerCase(Locale.ROOT);
		String platform = (os.contains("win") ? "windows" : os.contains("mac") ? "macos" : "linux") + "-"
			+ (arch.equals("aarch64") || arch.equals("arm64") ? "aarch64" : "x86_64");
		String file = os.contains("win") ? "rl_car_ffi.dll" : os.contains("mac") ? "librl_car_ffi.dylib" : "librl_car_ffi.so";
		String resource = "/natives/" + platform + "/" + file;
		try (InputStream in = RlCarNative.class.getResourceAsStream(resource)) {
			if (in == null) {
				throw new IllegalStateException("rl_car_ffi: no native library for " + platform + " in the mod jar (" + resource + ")");
			}
			byte[] bytes = in.readAllBytes();
			String hash = HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes)).substring(0, 16);
			Path dir = FabricLoader.getInstance().getGameDir().resolve(".rlcar").resolve("natives").resolve(hash);
			Path target = dir.resolve(file);
			if (!Files.isRegularFile(target) || Files.size(target) != bytes.length) {
				Files.createDirectories(dir);
				Path tmp = Files.createTempFile(dir, file, ".tmp");
				Files.write(tmp, bytes);
				Files.move(tmp, target, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
			}
			return target;
		} catch (IOException | NoSuchAlgorithmException e) {
			throw new IllegalStateException("rl_car_ffi: cannot extract " + resource, e);
		}
	}
}
