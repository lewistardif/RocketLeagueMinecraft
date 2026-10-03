package dev.rlcar.physics;

/** One frame of car input, in Rocket League conventions (see {@code rl_car_core::Controls}). */
public record CarControls(float throttle, float steer, float pitch, float yaw, float roll, int buttons) {
	public static final CarControls IDLE = new CarControls(0, 0, 0, 0, 0, 0);
}
