package dev.rlcar.client;

import dev.rlcar.RlCar;
import java.io.IOException;
import java.io.Reader;
import java.io.Writer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.EnumMap;
import java.util.EnumSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Properties;
import java.util.Set;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.network.chat.Component;
import org.lwjgl.sdl.SDLGamepad;

/**
 * Controller bindings, rebindable like Rocket League's: each car action (Boost, Powerslide, Air
 * Roll, ...) is bound to one gamepad button or trigger. The left stick always steers and aims in
 * the air. Edited in the "RL Car (Controller)" section of Options > Controls > Key Binds and
 * saved to {@code config/rlcar-controller.properties}.
 *
 * <p>As with the keys, Powerslide and Air Roll share a button by default (X) and that is not a
 * conflict; any other shared button is.
 */
public final class PadBinds {
	/** A trigger counts as pressed past this (for actions that are on/off, like Boost). */
	private static final float PRESS = 0.5F;
	private static final Path FILE = FabricLoader.getInstance().getConfigDir().resolve("rlcar-controller.properties");

	/** A gamepad input: a button, or a trigger (read as 0..1). Named by position, Xbox style, as SDL does. */
	public enum Input {
		NONE(-1, -1, "", ""),
		A(SDLGamepad.SDL_GAMEPAD_BUTTON_SOUTH, -1, "A", "Cross"),
		B(SDLGamepad.SDL_GAMEPAD_BUTTON_EAST, -1, "B", "Circle"),
		X(SDLGamepad.SDL_GAMEPAD_BUTTON_WEST, -1, "X", "Square"),
		Y(SDLGamepad.SDL_GAMEPAD_BUTTON_NORTH, -1, "Y", "Triangle"),
		LB(SDLGamepad.SDL_GAMEPAD_BUTTON_LEFT_SHOULDER, -1, "LB", "L1"),
		RB(SDLGamepad.SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER, -1, "RB", "R1"),
		LT(-1, SDLGamepad.SDL_GAMEPAD_AXIS_LEFT_TRIGGER, "LT", "L2"),
		RT(-1, SDLGamepad.SDL_GAMEPAD_AXIS_RIGHT_TRIGGER, "RT", "R2"),
		LS(SDLGamepad.SDL_GAMEPAD_BUTTON_LEFT_STICK, -1, "LS", "L3"),
		RS(SDLGamepad.SDL_GAMEPAD_BUTTON_RIGHT_STICK, -1, "RS", "R3"),
		BACK(SDLGamepad.SDL_GAMEPAD_BUTTON_BACK, -1, "View", "Share"),
		START(SDLGamepad.SDL_GAMEPAD_BUTTON_START, -1, "Menu", "Options"),
		DPAD_UP(SDLGamepad.SDL_GAMEPAD_BUTTON_DPAD_UP, -1, "D-Pad Up", "D-Pad Up"),
		DPAD_DOWN(SDLGamepad.SDL_GAMEPAD_BUTTON_DPAD_DOWN, -1, "D-Pad Down", "D-Pad Down"),
		DPAD_LEFT(SDLGamepad.SDL_GAMEPAD_BUTTON_DPAD_LEFT, -1, "D-Pad Left", "D-Pad Left"),
		DPAD_RIGHT(SDLGamepad.SDL_GAMEPAD_BUTTON_DPAD_RIGHT, -1, "D-Pad Right", "D-Pad Right");

		private final int button;
		private final int axis;
		private final String xbox;
		private final String playStation;

		Input(int button, int axis, String xbox, String playStation) {
			this.button = button;
			this.axis = axis;
			this.xbox = xbox;
			this.playStation = playStation;
		}

		/** 0..1: a button is 0 or 1, a trigger anything in between. */
		float value(long pad) {
			if (this.axis >= 0) {
				return Math.max(0, SDLGamepad.SDL_GetGamepadAxis(pad, this.axis) / 32767.0F);
			}
			return this.button >= 0 && SDLGamepad.SDL_GetGamepadButton(pad, this.button) ? 1 : 0;
		}

		/** The name printed on the connected controller (PlayStation or Xbox names). */
		public Component label() {
			if (this == NONE) {
				return Component.translatable("key.keyboard.unknown");
			}
			return Component.literal(CarKeys.playStationPad() ? this.playStation : this.xbox);
		}
	}

	/** The rebindable car actions, in Rocket League's order, with this mod's defaults (as in the Bevy demo). */
	public enum Action {
		THROTTLE("throttle", Input.RT),
		REVERSE("reverse", Input.LT),
		AIR_ROLL_RIGHT("air_roll_right", Input.RB),
		AIR_ROLL_LEFT("air_roll_left", Input.LB),
		AIR_ROLL("air_roll", Input.X),
		JUMP("jump", Input.A),
		BOOST("boost", Input.B),
		POWERSLIDE("powerslide", Input.X),
		BALL_CAM("ball_cam", Input.Y),
		REAR_CAMERA("rear_camera", Input.RS),
		RESET("reset", Input.DPAD_UP),
		EXIT("exit", Input.BACK);

		public final String id;
		public final Input defaultInput;

		Action(String id, Input defaultInput) {
			this.id = id;
			this.defaultInput = defaultInput;
		}

		/** Same name as the keyboard binding. */
		public Component label() {
			return Component.translatable("key.rlcar." + this.id);
		}
	}

	/** Actions meant to share a button (Rocket League's Powerslide / Air Roll). */
	private static final List<Set<Action>> SHARED = List.of(EnumSet.of(Action.POWERSLIDE, Action.AIR_ROLL));

	private static final Map<Action, Input> BINDS = new EnumMap<>(Action.class);
	private static Action capturing;
	private static final Set<Input> heldAtCapture = EnumSet.noneOf(Input.class);

	static {
		for (Action a : Action.values()) {
			BINDS.put(a, a.defaultInput);
		}
	}

	private PadBinds() {
	}

	public static Input get(Action action) {
		return BINDS.get(action);
	}

	public static void set(Action action, Input input) {
		BINDS.put(action, input);
		save();
	}

	public static boolean isDefault(Action action) {
		return get(action) == action.defaultInput;
	}

	public static boolean allDefault() {
		for (Action a : Action.values()) {
			if (!isDefault(a)) {
				return false;
			}
		}
		return true;
	}

	/** Every action back to its default button (the Controls screen's Reset Keys). */
	public static void resetAll() {
		capturing = null;
		for (Action a : Action.values()) {
			BINDS.put(a, a.defaultInput);
		}
		save();
	}

	/** 0..1 for the action (a trigger gives partial values, for Throttle/Reverse and air roll). */
	static float value(long pad, Action action) {
		return get(action).value(pad);
	}

	static boolean isDown(long pad, Action action) {
		return value(pad, action) > PRESS;
	}

	/** The other actions on the same input, excluding the intended pairs (empty if none). */
	public static List<Action> conflicts(Action action) {
		List<Action> out = new ArrayList<>();
		Input input = get(action);
		if (input == Input.NONE) {
			return out;
		}
		for (Action other : Action.values()) {
			if (other != action && get(other) == input && SHARED.stream().noneMatch(s -> s.contains(action) && s.contains(other))) {
				out.add(other);
			}
		}
		return out;
	}

	// ---- Rebinding from the Controls screen ----------------------------------------------------

	/** The action waiting for a controller button, or null. */
	public static Action capturing() {
		return capturing;
	}

	/** Wait for the next controller button for this action. Inputs already held must be released first. */
	public static void startCapture(Action action) {
		capturing = action;
		heldAtCapture.clear();
		long pad = CarKeys.gamepad();
		if (pad != 0) {
			for (Input in : Input.values()) {
				if (in != Input.NONE && in.value(pad) > PRESS) {
					heldAtCapture.add(in);
				}
			}
		}
	}

	public static void cancelCapture() {
		capturing = null;
	}

	/** Polled every frame on the Controls screen: binds the first newly pressed input. True if a binding changed. */
	public static boolean pollCapture() {
		if (capturing == null) {
			return false;
		}
		long pad = CarKeys.gamepad();
		if (pad == 0) {
			return false;
		}
		try {
			for (Input in : Input.values()) {
				if (in == Input.NONE) {
					continue;
				}
				boolean down = in.value(pad) > PRESS;
				if (!down) {
					heldAtCapture.remove(in);
				} else if (!heldAtCapture.contains(in)) {
					Action action = capturing;
					capturing = null;
					set(action, in);
					return true;
				}
			}
		} catch (Throwable t) {
			RlCar.LOG.warn("RL Car: could not read the gamepad", t);
			capturing = null;
		}
		return false;
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
		for (Action a : Action.values()) {
			String v = p.getProperty(a.id);
			if (v != null) {
				try {
					BINDS.put(a, Input.valueOf(v.trim().toUpperCase(Locale.ROOT)));
				} catch (IllegalArgumentException e) {
					RlCar.LOG.warn("RL Car: unknown controller input '{}' for {} in {}", v, a.id, FILE);
				}
			}
		}
	}

	private static void save() {
		StringBuilder sb = new StringBuilder("# RL Car controller bindings (edit in Options > Controls > Key Binds)\n");
		sb.append("# Inputs: ");
		for (Input in : Input.values()) {
			sb.append(in.name()).append(in.ordinal() < Input.values().length - 1 ? ", " : "\n");
		}
		for (Action a : Action.values()) {
			sb.append(a.id).append('=').append(get(a).name()).append('\n');
		}
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
