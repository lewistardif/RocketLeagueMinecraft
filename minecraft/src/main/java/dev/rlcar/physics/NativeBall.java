package dev.rlcar.physics;

import static java.lang.foreign.ValueLayout.JAVA_FLOAT;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.ref.Cleaner;

/**
 * Rocket League's ball in the Rust core (Rocket League space and units). It is stepped on its own
 * ({@link #step}) or together with one car in a single solve ({@link #stepWith}, {@link #advanceWith}),
 * which is how car hits and dribbles work.
 *
 * <p>A ball whose velocity and spin are exactly zero sleeps (no gravity, no world contacts) until
 * something touches it, as in RocketSim; {@link #reset} with a small velocity wakes it.
 */
public final class NativeBall implements AutoCloseable {
	private final MemorySegment handle;
	private final Cleaner.Cleanable cleanable;
	private final MemorySegment pose = Arena.ofAuto().allocate(JAVA_FLOAT, RlCarNative.BALL_POSE_FLOATS);
	private final MemorySegment pos = Arena.ofAuto().allocate(JAVA_FLOAT, 3);
	private final MemorySegment vel = Arena.ofAuto().allocate(JAVA_FLOAT, 3);
	private final MemorySegment angVel = Arena.ofAuto().allocate(JAVA_FLOAT, 3);

	public NativeBall() {
		try {
			this.handle = (MemorySegment) RlCarNative.BALL_NEW.invokeExact();
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
		MemorySegment h = this.handle;
		this.cleanable = NativeWorld.CLEANER.register(this, () -> free(h));
	}

	/**
	 * Replaces the state with {@code state} ({@link RlCarNative#BALL_POSE_FLOATS} floats: position,
	 * velocity, angular velocity, RL space). Does not interpolate from the old one.
	 */
	public void reset(float[] state) {
		for (int i = 0; i < 3; i++) {
			this.pos.setAtIndex(JAVA_FLOAT, i, state[i]);
			this.vel.setAtIndex(JAVA_FLOAT, i, state[3 + i]);
			this.angVel.setAtIndex(JAVA_FLOAT, i, state[6 + i]);
		}
		try {
			RlCarNative.BALL_RESET.invokeExact(this.handle, this.pos, this.vel, this.angVel);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	/** Shifts the ball (and its cached contacts) by {@code dx, dy, dz} uu. */
	public void translate(float dx, float dy, float dz) {
		this.pos.setAtIndex(JAVA_FLOAT, 0, dx);
		this.pos.setAtIndex(JAVA_FLOAT, 1, dy);
		this.pos.setAtIndex(JAVA_FLOAT, 2, dz);
		try {
			RlCarNative.BALL_TRANSLATE.invokeExact(this.handle, this.pos);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	/** Writes the state interpolated {@code alpha} of the way through the last tick into {@code out} (9 floats, see {@link #reset}). */
	public void pose(float alpha, float[] out) {
		try {
			int written = (int) RlCarNative.BALL_POSE.invokeExact(this.handle, alpha, this.pose);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
		MemorySegment.copy(this.pose, JAVA_FLOAT, 0, out, 0, RlCarNative.BALL_POSE_FLOATS);
	}

	/** Runs the ball alone for exactly {@code ticks} 1/120 s ticks. */
	public void step(NativeWorld world, int ticks) {
		try {
			RlCarNative.BALL_STEP.invokeExact(this.handle, world.handle(), ticks);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	/** Runs the ball and {@code car} together for exactly {@code ticks} ticks (both in the same frame of reference). */
	public void stepWith(NativeCar car, NativeWorld world, int ticks, CarControls c) {
		try {
			RlCarNative.SCENE_STEP.invokeExact(car.handle(), this.handle, world.handle(), ticks, c.throttle(), c.steer(), c.pitch(), c.yaw(), c.roll(), c.buttons());
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	/** Like {@link NativeCar#advance}, with the ball in the same solve. Returns the ticks run. */
	public int advanceWith(NativeCar car, NativeWorld world, double dt, CarControls c) {
		try {
			return (int) RlCarNative.SCENE_ADVANCE.invokeExact(car.handle(), this.handle, world.handle(), dt, c.throttle(), c.steer(), c.pitch(), c.yaw(), c.roll(), c.buttons());
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	MemorySegment handle() {
		return this.handle;
	}

	@Override
	public void close() {
		this.cleanable.clean();
	}

	private static void free(MemorySegment h) {
		try {
			RlCarNative.BALL_FREE.invokeExact(h);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}
}
