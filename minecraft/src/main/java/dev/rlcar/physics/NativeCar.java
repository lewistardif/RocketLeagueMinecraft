package dev.rlcar.physics;

import static java.lang.foreign.ValueLayout.JAVA_BYTE;
import static java.lang.foreign.ValueLayout.JAVA_FLOAT;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.ref.Cleaner;

/** One simulated car in the Rust core (Rocket League space and units). */
public final class NativeCar implements AutoCloseable {
	private final MemorySegment handle;
	private final Cleaner.Cleanable cleanable;
	private final MemorySegment pose = Arena.ofAuto().allocate(JAVA_FLOAT, RlCarNative.POSE_FLOATS);
	private final MemorySegment vec3 = Arena.ofAuto().allocate(JAVA_FLOAT, 3);

	public NativeCar(int preset) {
		try {
			this.handle = (MemorySegment) RlCarNative.CAR_NEW.invokeExact(preset);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
		MemorySegment h = this.handle;
		this.cleanable = NativeWorld.CLEANER.register(this, () -> free(h));
	}

	/** Teleports the car to {@code x, y, z} (uu) with RL Euler angles (radians), at rest. */
	public void reset(float x, float y, float z, float yaw, float pitch, float roll) {
		this.setVec3(x, y, z);
		try {
			RlCarNative.CAR_RESET.invokeExact(this.handle, this.vec3, yaw, pitch, roll);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	/** Shifts the car by {@code dx, dy, dz} uu (when the host moves its local origin). */
	public void translate(float dx, float dy, float dz) {
		this.setVec3(dx, dy, dz);
		try {
			RlCarNative.CAR_TRANSLATE.invokeExact(this.handle, this.vec3);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	/** Runs exactly {@code ticks} 1/120 s ticks. */
	public void step(NativeWorld world, int ticks, CarControls c) {
		try {
			RlCarNative.CAR_STEP.invokeExact(this.handle, world.handle(), ticks, c.throttle(), c.steer(), c.pitch(), c.yaw(), c.roll(), c.buttons());
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	/** Adds {@code dt} seconds and runs the ticks that fit. Returns how many ran. */
	public int advance(NativeWorld world, double dt, CarControls c) {
		try {
			return (int) RlCarNative.CAR_ADVANCE.invokeExact(this.handle, world.handle(), dt, c.throttle(), c.steer(), c.pitch(), c.yaw(), c.roll(), c.buttons());
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	/** Interpolation factor between the last two ticks after {@link #advance}. */
	public float alpha() {
		try {
			return (float) RlCarNative.CAR_ALPHA.invokeExact(this.handle);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	/** Writes the interpolated pose ({@link RlCarNative#POSE_FLOATS} floats, RL space) into {@code out}; returns the flags. */
	public int pose(float alpha, float[] out) {
		int flags;
		try {
			flags = (int) RlCarNative.CAR_POSE.invokeExact(this.handle, alpha, this.pose);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
		MemorySegment.copy(this.pose, JAVA_FLOAT, 0, out, 0, RlCarNative.POSE_FLOATS);
		return flags;
	}

	/** The full simulation state, losslessly encoded. */
	public byte[] save() {
		try {
			int size = (int) RlCarNative.CAR_SAVE.invokeExact(this.handle, MemorySegment.NULL, 0);
			try (Arena arena = Arena.ofConfined()) {
				MemorySegment buf = arena.allocate(size);
				int written = (int) RlCarNative.CAR_SAVE.invokeExact(this.handle, buf, size);
				return buf.asSlice(0, written).toArray(JAVA_BYTE);
			}
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	/** Replaces the state with one from {@link #save}. Returns false (and changes nothing) if invalid. */
	public boolean load(byte[] data) {
		try (Arena arena = Arena.ofConfined()) {
			MemorySegment buf = arena.allocate(Math.max(1, data.length));
			MemorySegment.copy(data, 0, buf, JAVA_BYTE, 0, data.length);
			return (int) RlCarNative.CAR_LOAD.invokeExact(this.handle, buf, data.length) != 0;
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	/** Rocket League's "Unlimited" boost mutator: keeps the tank full (and fills it now). */
	public void setUnlimitedBoost(boolean on) {
		try {
			RlCarNative.CAR_SET_UNLIMITED_BOOST.invokeExact(this.handle, on ? 1 : 0);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	/**
	 * Rocket League's bump rule for this car hitting {@code victim}: 0 = no bump, 1 = push (the
	 * velocity to add to the victim, uu/s, is written to {@code outVelocity}), 2 = demolition.
	 *
	 * @param victimPos victim position (RL space, same frame as this car)
	 * @param victimVel victim velocity (uu/s)
	 * @param victimUp the victim's up direction (world +Z for anything that is not a car)
	 * @param contactLocalX where the contact is along this car's length (uu forward of its origin)
	 */
	public int bump(float[] victimPos, float[] victimVel, boolean victimOnGround, float[] victimUp, float contactLocalX, boolean allowDemolish, float[] outVelocity) {
		try (Arena arena = Arena.ofConfined()) {
			MemorySegment p = arena.allocateFrom(JAVA_FLOAT, victimPos);
			MemorySegment v = arena.allocateFrom(JAVA_FLOAT, victimVel);
			MemorySegment u = arena.allocateFrom(JAVA_FLOAT, victimUp);
			MemorySegment out = arena.allocate(JAVA_FLOAT, 3);
			int r = (int) RlCarNative.CAR_BUMP.invokeExact(this.handle, p, v, victimOnGround ? 1 : 0, u, contactLocalX, 1.0F, allowDemolish ? 1 : 0, out);
			MemorySegment.copy(out, JAVA_FLOAT, 0, outVelocity, 0, 3);
			return r;
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	/** Adds {@code dx, dy, dz} uu/s to the car's velocity (it got bumped). */
	public void addVelocity(float dx, float dy, float dz) {
		this.setVec3(dx, dy, dz);
		try {
			RlCarNative.CAR_ADD_VELOCITY.invokeExact(this.handle, this.vec3);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	/** True if the car touched the ball since the last call. */
	public boolean consumeBallTouch() {
		try {
			return (int) RlCarNative.CAR_BALL_TOUCH.invokeExact(this.handle, MemorySegment.NULL) != 0;
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	MemorySegment handle() {
		return this.handle;
	}

	public int preset() {
		try {
			return (int) RlCarNative.CAR_PRESET.invokeExact(this.handle);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	@Override
	public void close() {
		this.cleanable.clean();
	}

	private void setVec3(float x, float y, float z) {
		this.vec3.setAtIndex(JAVA_FLOAT, 0, x);
		this.vec3.setAtIndex(JAVA_FLOAT, 1, y);
		this.vec3.setAtIndex(JAVA_FLOAT, 2, z);
	}

	private static void free(MemorySegment h) {
		try {
			RlCarNative.CAR_FREE.invokeExact(h);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}
}
