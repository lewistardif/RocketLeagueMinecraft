package dev.rlcar.physics;

import net.minecraft.core.BlockPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;
import org.jspecify.annotations.Nullable;

/**
 * A car plus the block geometry around it. Used by the server (cars nobody drives) and by the
 * driving client (the car it drives), so both run the exact same code.
 *
 * <p>Collision geometry is a {@link BlockSnapshot} of the block collision boxes within
 * {@link #RADIUS} blocks of the car, refreshed before the car can get near its edge.
 *
 * <p>With a ball ({@link #advance(Level, double, CarControls, BallSim)}), the car and the ball are
 * stepped in one solve while they are near each other, against a snapshot around both; otherwise
 * each is stepped on its own.
 */
public final class CarSim implements AutoCloseable {
	/** Snapshot half-size in blocks. The car moves at most ~1.2 blocks per server tick. */
	public static final int RADIUS = BlockSnapshot.RADIUS;
	/**
	 * Unlimited boost for every car, for now. It is a simulation rule, so the server and the driving
	 * client must agree on it: both get it from here.
	 */
	public static final boolean UNLIMITED_BOOST = true;
	/** Move the origin once the car is this far from it (blocks), to keep floats precise. */
	private static final double RECENTER_DISTANCE = 512;

	public final NativeCar car;
	private final BlockSnapshot snapshot = new BlockSnapshot();
	private BlockPos origin;
	private final float[] poseBuffer = new float[RlCarNative.POSE_FLOATS];
	private final float[] contactBuffer = new float[RlCarNative.CONTACT_FLOATS];
	private @Nullable NativeCamera camera;

	public CarSim(int preset, BlockPos origin) {
		this.car = new NativeCar(preset);
		this.car.setUnlimitedBoost(UNLIMITED_BOOST);
		this.origin = origin;
	}

	public BlockPos origin() {
		return this.origin;
	}

	/** Places the car at an absolute Minecraft position facing {@code mcYaw}, at rest. */
	public void resetAt(Vec3 pos, float mcYaw) {
		this.origin = BlockPos.containing(pos);
		float[] p = Space.toRl(this.origin, pos.x, pos.y, pos.z);
		this.car.reset(p[0], p[1], p[2], Space.yawToRl(mcYaw), 0, 0);
		this.snapshot.invalidate();
		if (this.camera != null) {
			this.camera.reset();
		}
	}

	/** Loads a state saved by {@link #save} for a car whose origin was {@code origin}. */
	public boolean load(byte[] state, BlockPos origin) {
		if (!this.car.load(state)) {
			return false;
		}
		this.origin = origin;
		this.snapshot.invalidate();
		if (this.camera != null) {
			this.camera.reset();
		}
		return true;
	}

	public byte[] save() {
		return this.car.save();
	}

	/** Current pose (interpolated {@code alpha} of the way through the last tick). */
	public CarPose pose(float alpha, CarPose out) {
		int flags = this.car.pose(alpha, this.poseBuffer);
		int contactFlags = this.car.contacts(this.contactBuffer);
		return out.setFromNative(this.origin, this.poseBuffer, flags, this.contactBuffer, contactFlags);
	}

	public Vec3 position() {
		this.car.pose(1, this.poseBuffer);
		return Space.toMc(this.origin, this.poseBuffer[0], this.poseBuffer[1], this.poseBuffer[2]);
	}

	/** Current velocity in Minecraft axes (blocks/s). */
	public Vec3 velocity() {
		this.car.pose(1, this.poseBuffer);
		return Space.dirToMc(this.poseBuffer[12], this.poseBuffer[13], this.poseBuffer[14], Space.UU_PER_BLOCK);
	}

	/** Adds {@code v} (Minecraft axes, blocks/s) to the car's velocity. */
	public void addVelocity(Vec3 v) {
		float[] rl = Space.dirToRl(v, Space.UU_PER_BLOCK);
		this.car.addVelocity(rl[0], rl[1], rl[2]);
	}

	/** Runs {@code ticks} 1/120 s ticks against the blocks of {@code level}. */
	public void step(Level level, int ticks, CarControls controls) {
		this.step(level, ticks, controls, null);
	}

	/** Runs {@code ticks} ticks, with {@code ball} (if any) simulated alongside for the same ticks. */
	public void step(Level level, int ticks, CarControls controls, @Nullable BallSim ball) {
		// In slices of at most one Minecraft tick, so the car cannot leave its block snapshot.
		int slice = ball != null ? BallSim.SLICE_TICKS : 6;
		while (ticks > 0) {
			int n = Math.min(ticks, slice);
			this.recenter();
			if (ball != null && ball.near(this.position())) {
				this.preparePair(level, ball);
				ball.ball.stepWith(this.car, ball.pairSnapshot.world, n, controls);
			} else {
				this.snapshot.ensure(level, this.origin, this.position());
				this.car.step(this.snapshot.world, n, controls);
				if (ball != null) {
					ball.step(level, n);
				}
			}
			ticks -= n;
		}
	}

	/** Runs as many ticks as fit in {@code dt} seconds (for per-frame stepping). */
	public void advance(Level level, double dt, CarControls controls) {
		this.advance(level, dt, controls, null);
	}

	/**
	 * Runs as many ticks as fit in {@code dt} seconds, with {@code ball} (if any) simulated
	 * alongside for the same ticks: draw it at {@link #alpha()} too.
	 */
	public void advance(Level level, double dt, CarControls controls, @Nullable BallSim ball) {
		// In slices, so a long frame cannot carry the car (or the ball) out of its block snapshot.
		double maxSlice = ball != null ? BallSim.SLICE_TICKS / 120.0 : 1.0 / 20.0;
		while (dt > 0) {
			double slice = Math.min(dt, maxSlice);
			this.recenter();
			if (ball != null && ball.near(this.position())) {
				this.preparePair(level, ball);
				ball.ball.advanceWith(this.car, ball.pairSnapshot.world, slice, controls);
			} else {
				this.snapshot.ensure(level, this.origin, this.position());
				int ticks = this.car.advance(this.snapshot.world, slice, controls);
				if (ball != null) {
					ball.step(level, ticks);
				}
			}
			dt -= slice;
		}
	}

	/** The ball in this car's frame of reference, and the blocks around both. */
	private void preparePair(Level level, BallSim ball) {
		ball.rebase(this.origin);
		ball.pairSnapshot.ensure(level, this.origin, this.position(), ball.position());
	}

	public float alpha() {
		return this.car.alpha();
	}

	/** The next {@link #camera} call starts over from the car's pose. */
	public void resetCamera() {
		if (this.camera != null) {
			this.camera.reset();
		}
	}

	/**
	 * Advances Rocket League's camera following this car (created on first use) by {@code dt}
	 * seconds and writes its view ({@link RlCarNative#CAMERA_VIEW_FLOATS} floats, RL space relative
	 * to {@link #origin()}) into {@code out}. See {@link NativeCamera#update}.
	 */
	public boolean camera(float dt, float[] settings, float lookRight, float lookUp, int flags, float[] out) {
		return this.camera(dt, settings, lookRight, lookUp, flags, null, out);
	}

	/**
	 * As {@link #camera(float, float[], float, float, int, float[])}, with ball cam: {@code ballTarget}
	 * holds the ball to look at, already in this car's frame of reference (or null for none).
	 * See {@link NativeCamera#updateWithBall}.
	 */
	public boolean camera(float dt, float[] settings, float lookRight, float lookUp, int flags, @Nullable NativeBall ballTarget, float[] out) {
		if (this.camera == null) {
			this.camera = new NativeCamera();
		}
		return this.camera.updateWithBall(this.car, ballTarget, this.car.alpha(), dt, settings, lookRight, lookUp, flags, out);
	}

	/**
	 * Hash of the block geometry currently around the car; changes when blocks there change.
	 * Retakes the snapshot.
	 */
	public int geometryHash(Level level) {
		this.recenter();
		this.snapshot.invalidate();
		this.snapshot.ensure(level, this.origin, this.position());
		return this.snapshot.hash();
	}

	private void recenter() {
		Vec3 pos = this.position();
		if (pos.distanceToSqr(Vec3.atCenterOf(this.origin)) > RECENTER_DISTANCE * RECENTER_DISTANCE) {
			BlockPos next = BlockPos.containing(pos);
			BlockPos d = this.origin.subtract(next);
			// Same physical place, new origin: shift the car by the whole-block difference.
			float[] delta = Space.toRl(BlockPos.ZERO, d.getX(), d.getY(), d.getZ());
			this.car.translate(delta[0], delta[1], delta[2]);
			if (this.camera != null) {
				this.camera.translate(delta[0], delta[1], delta[2]);
			}
			this.origin = next;
			this.snapshot.invalidate();
		}
	}

	@Override
	public void close() {
		this.car.close();
		this.snapshot.close();
		if (this.camera != null) {
			this.camera.close();
		}
	}
}
