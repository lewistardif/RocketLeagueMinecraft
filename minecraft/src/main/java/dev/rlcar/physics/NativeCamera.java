package dev.rlcar.physics;

import static java.lang.foreign.ValueLayout.JAVA_FLOAT;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.ref.Cleaner;
import org.jspecify.annotations.Nullable;

/**
 * Rocket League's car camera in the Rust core ({@code rl_car_core::camera}), in Rocket League
 * space and units. One per followed car; updated once per rendered frame.
 */
public final class NativeCamera implements AutoCloseable {
	private final MemorySegment handle;
	private final Cleaner.Cleanable cleanable;
	private final MemorySegment settings = Arena.ofAuto().allocate(JAVA_FLOAT, RlCarNative.CAMERA_SETTINGS_FLOATS);
	private final MemorySegment view = Arena.ofAuto().allocate(JAVA_FLOAT, RlCarNative.CAMERA_VIEW_FLOATS);
	private final MemorySegment vec3 = Arena.ofAuto().allocate(JAVA_FLOAT, 3);

	public NativeCamera() {
		try {
			this.handle = (MemorySegment) RlCarNative.CAMERA_NEW.invokeExact();
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
		MemorySegment h = this.handle;
		this.cleanable = NativeWorld.CLEANER.register(this, () -> free(h));
	}

	/** Starts over from the car's pose on the next update (after a teleport). */
	public void reset() {
		try {
			RlCarNative.CAMERA_RESET.invokeExact(this.handle);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	/** Shifts the camera by {@code dx, dy, dz} uu, together with its car. */
	public void translate(float dx, float dy, float dz) {
		this.vec3.setAtIndex(JAVA_FLOAT, 0, dx);
		this.vec3.setAtIndex(JAVA_FLOAT, 1, dy);
		this.vec3.setAtIndex(JAVA_FLOAT, 2, dz);
		try {
			RlCarNative.CAMERA_TRANSLATE.invokeExact(this.handle, this.vec3);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	/**
	 * Advances the camera by {@code dt} seconds following {@code car} ({@code alpha} of the way
	 * through its last tick) and writes the view ({@link RlCarNative#CAMERA_VIEW_FLOATS} floats:
	 * position, axes forward/right/up, horizontal and vertical FOV in degrees, focus) into {@code out}.
	 *
	 * @param settings {@link RlCarNative#CAMERA_SETTINGS_FLOATS} floats, see {@link #preset}
	 * @param lookRight swivel input, -1..1 (right stick X)
	 * @param lookUp swivel input, -1..1 (right stick Y, up positive)
	 * @param flags {@link RlCarNative#CAMERA_REAR_VIEW}
	 */
	public boolean update(NativeCar car, float alpha, float dt, float[] settings, float lookRight, float lookUp, int flags, float[] out) {
		MemorySegment.copy(settings, 0, this.settings, JAVA_FLOAT, 0, RlCarNative.CAMERA_SETTINGS_FLOATS);
		int ok;
		try {
			ok = (int) RlCarNative.CAMERA_UPDATE.invokeExact(this.handle, car.handle(), alpha, dt, this.settings, lookRight, lookUp, flags, this.view);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
		if (ok == 0) {
			return false;
		}
		MemorySegment.copy(this.view, JAVA_FLOAT, 0, out, 0, RlCarNative.CAMERA_VIEW_FLOATS);
		return true;
	}

	/**
	 * Like {@link #update}, with Rocket League's ball cam: while {@code flags} has
	 * {@link RlCarNative#CAMERA_BALL_CAM} and {@code ball} is not null, the camera turns to keep the
	 * ball in view (blending at the Transition Speed setting). {@code ball} must be in the car's
	 * frame of reference; it is read {@code alpha} of the way through its last tick.
	 */
	public boolean updateWithBall(NativeCar car, @Nullable NativeBall ball, float alpha, float dt, float[] settings, float lookRight, float lookUp, int flags, float[] out) {
		MemorySegment.copy(settings, 0, this.settings, JAVA_FLOAT, 0, RlCarNative.CAMERA_SETTINGS_FLOATS);
		MemorySegment b = ball != null ? ball.handle() : MemorySegment.NULL;
		int ok;
		try {
			ok = (int) RlCarNative.CAMERA_UPDATE_BALL.invokeExact(this.handle, car.handle(), b, alpha, dt, this.settings, lookRight, lookUp, flags, this.view);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
		if (ok == 0) {
			return false;
		}
		MemorySegment.copy(this.view, JAVA_FLOAT, 0, out, 0, RlCarNative.CAMERA_VIEW_FLOATS);
		return true;
	}

	/**
	 * Writes Rocket League's camera preset {@code index} into {@code out} (FOV, height, angle,
	 * distance, stiffness, swivel speed, transition speed, invert swivel pitch as 0/1) and returns
	 * the number of presets; {@code out} is left alone when {@code index} is out of range.
	 */
	public static int preset(int index, float[] out) {
		try (Arena arena = Arena.ofConfined()) {
			MemorySegment buf = arena.allocate(JAVA_FLOAT, RlCarNative.CAMERA_SETTINGS_FLOATS);
			int count = (int) RlCarNative.CAMERA_PRESET.invokeExact(index, buf);
			if (index >= 0 && index < count) {
				MemorySegment.copy(buf, JAVA_FLOAT, 0, out, 0, RlCarNative.CAMERA_SETTINGS_FLOATS);
			}
			return count;
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}

	@Override
	public void close() {
		this.cleanable.clean();
	}

	private static void free(MemorySegment h) {
		try {
			RlCarNative.CAMERA_FREE.invokeExact(h);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}
}
