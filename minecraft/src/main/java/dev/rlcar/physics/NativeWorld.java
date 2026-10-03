package dev.rlcar.physics;

import static java.lang.foreign.ValueLayout.JAVA_FLOAT;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.ref.Cleaner;

/** A {@code BoxWorld}: the collision geometry around one car, rebuilt from block boxes. */
public final class NativeWorld implements AutoCloseable {
	static final Cleaner CLEANER = Cleaner.create();

	private final MemorySegment handle;
	private final Cleaner.Cleanable cleanable;
	private MemorySegment buffer = MemorySegment.NULL;
	private int capacityBoxes;

	public NativeWorld() {
		try {
			this.handle = (MemorySegment) RlCarNative.WORLD_NEW.invokeExact();
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
		MemorySegment h = this.handle;
		this.cleanable = CLEANER.register(this, () -> free(h));
	}

	MemorySegment handle() {
		return this.handle;
	}

	/** Replaces the geometry with {@code count} boxes ({@code 6 * count} floats, RL space). Returns the face count. */
	public int setBoxes(float[] boxes, int count) {
		if (count > this.capacityBoxes) {
			this.capacityBoxes = Math.max(count, this.capacityBoxes * 2);
			this.buffer = Arena.ofAuto().allocate(JAVA_FLOAT, (long) this.capacityBoxes * 6);
		}
		if (count > 0) {
			MemorySegment.copy(boxes, 0, this.buffer, JAVA_FLOAT, 0, count * 6);
		}
		// (A conditional expression as an invokeExact argument would be typed Object.)
		MemorySegment data = count > 0 ? this.buffer : MemorySegment.NULL;
		try {
			return (int) RlCarNative.WORLD_SET_BOXES.invokeExact(this.handle, data, count);
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
			RlCarNative.WORLD_FREE.invokeExact(h);
		} catch (Throwable t) {
			throw RlCarNative.rethrow(t);
		}
	}
}
