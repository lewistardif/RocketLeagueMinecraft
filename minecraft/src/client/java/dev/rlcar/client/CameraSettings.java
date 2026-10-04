package dev.rlcar.client;

import dev.rlcar.RlCar;
import dev.rlcar.physics.NativeCamera;
import dev.rlcar.physics.RlCarNative;
import java.io.IOException;
import java.io.Reader;
import java.io.Writer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Locale;
import java.util.Properties;
import net.fabricmc.loader.api.FabricLoader;

/**
 * Rocket League's camera settings (Settings > Camera, plus Controls > Invert Swivel Pitch and Rear
 * Camera Toggle), with
 * the game's ranges and presets. Edited from Options > Controls > RL Car Camera and saved to
 * {@code config/rlcar-camera.properties}. The camera itself is the Rust core's
 * ({@code rl_car_core::camera}).
 */
public final class CameraSettings {
	private static final Path FILE = FabricLoader.getInstance().getConfigDir().resolve("rlcar-camera.properties");

	/** One slider: its range and step, as in Rocket League. */
	public enum Setting {
		FOV("fov", 0, 60, 110, 1),
		DISTANCE("distance", 3, 100, 400, 10),
		HEIGHT("height", 1, 40, 200, 10),
		ANGLE("angle", 2, -15, 0, 1),
		STIFFNESS("stiffness", 4, 0, 1, 0.05F),
		SWIVEL_SPEED("swivel_speed", 5, 1, 10, 0.1F),
		TRANSITION_SPEED("transition_speed", 6, 1, 2, 0.1F);

		public final String id;
		/** Index in the native settings array. */
		final int index;
		public final float min;
		public final float max;
		public final float step;

		Setting(String id, int index, float min, float max, float step) {
			this.id = id;
			this.index = index;
			this.min = min;
			this.max = max;
			this.step = step;
		}

		public int steps() {
			return Math.round((this.max - this.min) / this.step);
		}

		public float get() {
			return VALUES[this.index];
		}

		public void set(float value) {
			VALUES[this.index] = Math.clamp(value, this.min, this.max);
		}

		/** The value as Rocket League shows it. */
		public String format(float value) {
			return this.step >= 1 ? Integer.toString(Math.round(value)) : String.format(Locale.ROOT, this.step < 0.1F ? "%.2f" : "%.1f", value);
		}
	}

	/** Rocket League's presets, in its order (rl_car_core's CameraSettings::PRESETS). */
	public static final String[] PRESETS = {"default", "balanced", "wide", "custom", "legacy", "modern"};
	private static final int INVERT_SWIVEL_PITCH = 7;

	private static final float[] VALUES = new float[RlCarNative.CAMERA_SETTINGS_FLOATS];
	private static int preset;
	/** Rocket League's Rear Camera Toggle: each press of Rear Camera switches the view instead of holding it. */
	private static boolean rearCameraToggle;

	static {
		applyPreset(0);
	}

	private CameraSettings() {
	}

	/** The settings in the layout {@code rlcar_camera_update} reads. */
	public static float[] values() {
		return VALUES;
	}

	/** The preset these values came from ({@code custom} once a slider is moved). */
	public static int preset() {
		return preset;
	}

	public static void applyPreset(int index) {
		boolean invert = invertSwivelPitch();
		NativeCamera.preset(index, VALUES);
		VALUES[INVERT_SWIVEL_PITCH] = invert ? 1 : 0;
		preset = index;
	}

	/** After a slider changed. */
	public static void markCustom() {
		preset = 3;
	}

	public static boolean invertSwivelPitch() {
		return VALUES[INVERT_SWIVEL_PITCH] != 0;
	}

	public static void setInvertSwivelPitch(boolean on) {
		VALUES[INVERT_SWIVEL_PITCH] = on ? 1 : 0;
	}

	public static boolean rearCameraToggle() {
		return rearCameraToggle;
	}

	public static void setRearCameraToggle(boolean on) {
		rearCameraToggle = on;
	}

	// ---- Persistence ---------------------------------------------------------------------------

	public static void load() {
		if (!Files.exists(FILE)) {
			return;
		}
		Properties p = new Properties();
		try (Reader r = Files.newBufferedReader(FILE)) {
			p.load(r);
		} catch (IOException e) {
			RlCar.LOG.warn("RL Car: could not read {}", FILE, e);
			return;
		}
		String presetName = p.getProperty("preset", PRESETS[0]).trim().toLowerCase(Locale.ROOT);
		for (int i = 0; i < PRESETS.length; i++) {
			if (PRESETS[i].equals(presetName)) {
				applyPreset(i);
			}
		}
		for (Setting s : Setting.values()) {
			String v = p.getProperty(s.id);
			if (v != null) {
				try {
					s.set(Float.parseFloat(v.trim()));
				} catch (NumberFormatException e) {
					RlCar.LOG.warn("RL Car: bad value '{}' for {} in {}", v, s.id, FILE);
				}
			}
		}
		setInvertSwivelPitch(Boolean.parseBoolean(p.getProperty("invert_swivel_pitch", "false").trim()));
		setRearCameraToggle(Boolean.parseBoolean(p.getProperty("rear_camera_toggle", "false").trim()));
	}

	public static void save() {
		StringBuilder sb = new StringBuilder("# RL Car camera, Rocket League's camera settings (edit in Options > Controls > RL Car Camera)\n");
		sb.append("preset=").append(PRESETS[preset]).append('\n');
		for (Setting s : Setting.values()) {
			sb.append(s.id).append('=').append(s.format(s.get())).append('\n');
		}
		sb.append("invert_swivel_pitch=").append(invertSwivelPitch()).append('\n');
		try {
			Files.createDirectories(FILE.getParent());
			try (Writer w = Files.newBufferedWriter(FILE)) {
				w.write(sb.toString());
			}
		} catch (IOException e) {
			RlCar.LOG.warn("RL Car: could not save {}", FILE, e);
		}
	}
}
