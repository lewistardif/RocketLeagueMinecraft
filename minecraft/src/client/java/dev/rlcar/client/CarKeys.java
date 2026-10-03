package dev.rlcar.client;

import com.mojang.blaze3d.platform.InputConstants;
import dev.rlcar.RlCar;
import dev.rlcar.physics.CarControls;
import dev.rlcar.physics.RlCarNative;
import java.nio.IntBuffer;
import java.util.List;
import java.util.Set;
import net.fabricmc.fabric.api.client.keymapping.v1.KeyMappingHelper;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Options;
import org.lwjgl.sdl.SDLGamepad;
import org.lwjgl.sdl.SDLInit;
import org.lwjgl.sdl.SDLStdinc;

/**
 * Car controls: Rocket League's own bindings, in their own "RL Car" section of Options >
 * Controls > Key Binds and in Rocket League's order, plus an SDL gamepad.
 *
 * <p>As in Rocket League, ground and air controls are separate bindings that share keys by
 * default: W is Throttle and Pitch Down, S is Reverse and Pitch Up, A/D are Steer and Yaw, and
 * Powerslide and Air Roll are both Left Ctrl. Any of them can be split onto other keys or mouse
 * buttons. Holding Air Roll turns the yaw input into roll. Reset Car and Get Out are this mod's
 * own; Rocket League's ball cam, scoreboard and chat bindings have nothing to act on here yet.
 *
 * <p>Gamepad: the left stick steers and aims in the air; the buttons and triggers are rebindable
 * (see {@link PadBinds}). Defaults (Xbox layout, as in the Bevy demo): RT/LT, A jump, B boost, X
 * powerslide and air roll, LB/RB air roll left/right, Y reset, Back get out.
 */
public final class CarKeys {
	private static final KeyMapping.Category CATEGORY = KeyMapping.Category.register(RlCar.id("car"));
	private static int order;

	// Keyboard defaults are SDL scancodes (InputConstants.KEY_*).
	public static final KeyMapping THROTTLE = key("throttle", InputConstants.KEY_W);
	public static final KeyMapping REVERSE = key("reverse", InputConstants.KEY_S);
	public static final KeyMapping STEER_RIGHT = key("steer_right", InputConstants.KEY_D);
	public static final KeyMapping STEER_LEFT = key("steer_left", InputConstants.KEY_A);
	public static final KeyMapping PITCH_UP = key("pitch_up", InputConstants.KEY_S);
	public static final KeyMapping PITCH_DOWN = key("pitch_down", InputConstants.KEY_W);
	public static final KeyMapping YAW_RIGHT = key("yaw_right", InputConstants.KEY_D);
	public static final KeyMapping YAW_LEFT = key("yaw_left", InputConstants.KEY_A);
	public static final KeyMapping AIR_ROLL_RIGHT = key("air_roll_right", InputConstants.KEY_E);
	public static final KeyMapping AIR_ROLL_LEFT = key("air_roll_left", InputConstants.KEY_Q);
	public static final KeyMapping AIR_ROLL = key("air_roll", InputConstants.KEY_LCONTROL);
	public static final KeyMapping JUMP = key("jump", InputConstants.KEY_SPACE);
	public static final KeyMapping BOOST = key("boost", InputConstants.KEY_LSHIFT);
	public static final KeyMapping POWERSLIDE = key("powerslide", InputConstants.KEY_LCONTROL);
	public static final KeyMapping RESET = key("reset", InputConstants.KEY_R);
	public static final KeyMapping EXIT = key("exit", InputConstants.KEY_F);

	public static final List<KeyMapping> ALL = List.of(
		THROTTLE, REVERSE, STEER_RIGHT, STEER_LEFT, PITCH_UP, PITCH_DOWN, YAW_RIGHT, YAW_LEFT,
		AIR_ROLL_RIGHT, AIR_ROLL_LEFT, AIR_ROLL, JUMP, BOOST, POWERSLIDE, RESET, EXIT
	);

	/** Pairs of car bindings meant to share a key (Rocket League's ground/air pairs). */
	private static final List<Set<KeyMapping>> SHARED = List.of(
		Set.of(THROTTLE, PITCH_DOWN), Set.of(REVERSE, PITCH_UP), Set.of(STEER_RIGHT, YAW_RIGHT), Set.of(STEER_LEFT, YAW_LEFT), Set.of(POWERSLIDE, AIR_ROLL)
	);

	private static final float STICK_DEADZONE = 0.1F;

	private static boolean gamepadInit;
	private static boolean gamepadBroken;
	private static long gamepad;
	private static boolean gamepadPlayStation;
	private static int gamepadPoll;
	private static boolean padExit;
	private static boolean padReset;
	private static boolean padExitHeld;
	private static boolean padResetHeld;

	private CarKeys() {
	}

	private static KeyMapping key(String name, int scancode) {
		return new KeyMapping("key.rlcar." + name, InputConstants.Type.KEYBOARD, scancode, CATEGORY, order++);
	}

	public static void register() {
		for (KeyMapping k : ALL) {
			KeyMappingHelper.registerKeyMapping(k);
		}
	}

	/** The vanilla bindings that do nothing while driving (see the client mixins), so car keys may reuse them. */
	private static List<KeyMapping> inactiveWhileDriving(Options o) {
		return List.of(
			o.keyUp, o.keyDown, o.keyLeft, o.keyRight, o.keyJump, o.keyShift, o.keySprint,
			o.keyInventory, o.keyDrop, o.keySwapOffhand, o.keyAttack, o.keyUse, o.keyPickItem
		);
	}

	/**
	 * True if two bindings may share a key without either stealing the other's input: a car
	 * binding with its Rocket League partner, or a car binding with a vanilla action that is off
	 * while driving (car bindings are only read while driving). Used by the Controls screen.
	 */
	public static boolean canShareKey(KeyMapping a, KeyMapping b) {
		boolean carA = ALL.contains(a);
		boolean carB = ALL.contains(b);
		if (carA && carB) {
			return SHARED.stream().anyMatch(pair -> pair.contains(a) && pair.contains(b));
		}
		if (carA == carB) {
			return false; // two vanilla bindings: not ours to judge
		}
		KeyMapping vanilla = carA ? b : a;
		return inactiveWhileDriving(Minecraft.getInstance().options).contains(vanilla);
	}

	private static float axis(KeyMapping neg, KeyMapping pos) {
		return (pos.isDown() ? 1 : 0) - (neg.isDown() ? 1 : 0);
	}

	private static float deadzone(float v) {
		return Math.abs(v) < STICK_DEADZONE ? 0 : (v - STICK_DEADZONE * Math.signum(v)) / (1 - STICK_DEADZONE);
	}

	private static float pick(float a, float b) {
		return Math.abs(b) > Math.abs(a) ? b : a;
	}

	/** This frame's controls from the keyboard/mouse and the first gamepad (larger input wins per axis). */
	public static CarControls read() {
		boolean airRoll = AIR_ROLL.isDown();
		float yawInput = axis(YAW_LEFT, YAW_RIGHT);
		float throttle = axis(REVERSE, THROTTLE);
		float steer = axis(STEER_LEFT, STEER_RIGHT);
		float pitch = axis(PITCH_DOWN, PITCH_UP);
		// Air Roll turns the yaw input into roll, like in Rocket League.
		float yaw = airRoll ? 0 : yawInput;
		float roll = axis(AIR_ROLL_LEFT, AIR_ROLL_RIGHT) + (airRoll ? yawInput : 0);
		boolean jump = JUMP.isDown();
		boolean boost = BOOST.isDown();
		boolean handbrake = POWERSLIDE.isDown();

		long pad = gamepad();
		if (pad != 0) {
			try {
				float sx = deadzone(SDLGamepad.SDL_GetGamepadAxis(pad, SDLGamepad.SDL_GAMEPAD_AXIS_LEFTX) / 32767.0F);
				float sy = deadzone(SDLGamepad.SDL_GetGamepadAxis(pad, SDLGamepad.SDL_GAMEPAD_AXIS_LEFTY) / 32767.0F);
				boolean padAirRoll = PadBinds.isDown(pad, PadBinds.Action.AIR_ROLL);
				float rollButtons = PadBinds.value(pad, PadBinds.Action.AIR_ROLL_RIGHT) - PadBinds.value(pad, PadBinds.Action.AIR_ROLL_LEFT);
				throttle = pick(throttle, PadBinds.value(pad, PadBinds.Action.THROTTLE) - PadBinds.value(pad, PadBinds.Action.REVERSE));
				steer = pick(steer, sx);
				// SDL stick Y is positive downwards; stick forward = nose down.
				pitch = pick(pitch, sy);
				yaw = pick(yaw, padAirRoll ? 0 : sx);
				roll = pick(roll, rollButtons + (padAirRoll ? sx : 0));
				jump |= PadBinds.isDown(pad, PadBinds.Action.JUMP);
				boost |= PadBinds.isDown(pad, PadBinds.Action.BOOST);
				handbrake |= PadBinds.isDown(pad, PadBinds.Action.POWERSLIDE);
				boolean exit = PadBinds.isDown(pad, PadBinds.Action.EXIT);
				boolean reset = PadBinds.isDown(pad, PadBinds.Action.RESET);
				padExit |= exit && !padExitHeld;
				padReset |= reset && !padResetHeld;
				padExitHeld = exit;
				padResetHeld = reset;
			} catch (Throwable t) {
				disableGamepad(t);
			}
		}

		int buttons = (jump ? RlCarNative.BUTTON_JUMP : 0) | (boost ? RlCarNative.BUTTON_BOOST : 0) | (handbrake ? RlCarNative.BUTTON_HANDBRAKE : 0);
		return new CarControls(clamp(throttle), clamp(steer), clamp(pitch), clamp(yaw), clamp(roll), buttons);
	}

	private static float clamp(float v) {
		return Math.clamp(v, -1.0F, 1.0F);
	}

	/** True once per press of Get Out (key or its gamepad button). */
	public static boolean consumeExit() {
		boolean pressed = EXIT.consumeClick() | padExit;
		padExit = false;
		return pressed;
	}

	/** True once per press of Reset Car (key or its gamepad button). */
	public static boolean consumeReset() {
		boolean pressed = RESET.consumeClick() | padReset;
		padReset = false;
		return pressed;
	}

	/** True if the open gamepad is a PlayStation one (for button names); false without a gamepad. */
	static boolean playStationPad() {
		return gamepad != 0 && gamepadPlayStation;
	}

	/** The first connected gamepad, opened lazily (Minecraft itself only initialises SDL video). */
	static long gamepad() {
		if (gamepadBroken) {
			return 0;
		}
		try {
			if (!gamepadInit) {
				gamepadInit = true;
				if (!SDLInit.SDL_InitSubSystem(SDLInit.SDL_INIT_GAMEPAD)) {
					gamepadBroken = true;
					return 0;
				}
			}
			if (gamepad != 0 && !SDLGamepad.SDL_GamepadConnected(gamepad)) {
				SDLGamepad.SDL_CloseGamepad(gamepad);
				gamepad = 0;
			}
			if (gamepad == 0 && gamepadPoll-- <= 0) {
				gamepadPoll = 60;
				if (SDLGamepad.SDL_HasGamepad()) {
					IntBuffer ids = SDLGamepad.SDL_GetGamepads();
					if (ids != null) {
						if (ids.remaining() > 0) {
							gamepad = SDLGamepad.SDL_OpenGamepad(ids.get(0));
							int type = gamepad != 0 ? SDLGamepad.SDL_GetGamepadType(gamepad) : SDLGamepad.SDL_GAMEPAD_TYPE_UNKNOWN;
							gamepadPlayStation = type == SDLGamepad.SDL_GAMEPAD_TYPE_PS3 || type == SDLGamepad.SDL_GAMEPAD_TYPE_PS4 || type == SDLGamepad.SDL_GAMEPAD_TYPE_PS5;
						}
						SDLStdinc.SDL_free(ids);
					}
				}
			}
			if (gamepad != 0) {
				SDLGamepad.SDL_UpdateGamepads();
			}
			return gamepad;
		} catch (Throwable t) {
			disableGamepad(t);
			return 0;
		}
	}

	private static void disableGamepad(Throwable t) {
		gamepadBroken = true;
		gamepad = 0;
		RlCar.LOG.warn("RL Car: gamepad support disabled", t);
	}
}
