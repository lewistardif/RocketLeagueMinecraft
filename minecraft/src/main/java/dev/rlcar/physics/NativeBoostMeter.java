package dev.rlcar.physics;

import static java.lang.foreign.ValueLayout.JAVA_FLOAT;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.ref.Cleaner;

/**
 * Rocket League's boost meter logic in the Rust core ({@code rl_car_core::boost_meter}, the port of
 * the HUD's {@code BoostMeterView} ActionScript): what the meter shows for the car's boost.
 */
public final class NativeBoostMeter implements AutoCloseable {
	private final MemorySegment handle;
	private final Cleaner.Cleanable cleanable;
	private final MemorySegment frame = Arena.ofAuto().allocate(JAVA_FLOAT, RlCarNative.BOOST_METER_FLOATS);

	public NativeBoostMeter() {
		try {
			this.handle = (MemorySegment) RlCarNative.BOOST_METER_NEW.invokeExact();
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
		MemorySegment h = this.handle;
		this.cleanable = NativeWorld.CLEANER.register(this, () -> free(h));
	}

	/**
	 * Advances {@code dt} seconds with the car's {@code boost} (0..100) and writes the frame
	 * ({@link RlCarNative#BOOST_METER_FLOATS} floats, layout in {@code rl_car_ffi::BOOST_METER_FLOATS})
	 * into {@code out}.
	 */
	public boolean update(float boost, float dt, float[] out) {
		int ok;
		try {
			ok = (int) RlCarNative.BOOST_METER_UPDATE.invokeExact(this.handle, boost, dt, this.frame);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
		if (ok == 0) {
			return false;
		}
		MemorySegment.copy(this.frame, JAVA_FLOAT, 0, out, 0, RlCarNative.BOOST_METER_FLOATS);
		return true;
	}

	@Override
	public void close() {
		this.cleanable.clean();
	}

	private static void free(MemorySegment h) {
		try {
			RlCarNative.BOOST_METER_FREE.invokeExact(h);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}
}
