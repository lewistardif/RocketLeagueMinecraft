package dev.rlcar.physics;

import net.minecraft.core.BlockPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;

/**
 * A ball plus the block geometry around it. The server simulates it on its own; the client of the
 * player whose car is near it simulates it together with that car ({@link CarSim#advance(Level,
 * double, CarControls, BallSim)}), so hits respond within the frame.
 *
 * <p>The ball is fast (up to 6000 uu/s, three blocks per Minecraft tick), so it is stepped in
 * slices of {@link #SLICE_TICKS} ticks and its block snapshot is checked between them.
 */
public final class BallSim implements AutoCloseable {
	/** Ticks per slice: at full speed the ball moves one block per slice, well inside its snapshot. */
	static final int SLICE_TICKS = 2;
	/**
	 * A car this close (blocks, centre to centre) is stepped in one solve with the ball. Contact
	 * starts at about 1.5 blocks; per slice the two close in by at most 1.4.
	 */
	static final double NEAR = 5.0;
	/** Speed the ball is woken with when it was at rest (uu/s, downwards): too small to see. */
	private static final float WAKE_SPEED = 1.0F;
	private static final double RECENTER_DISTANCE = 512;

	public final NativeBall ball = new NativeBall();
	private final BlockSnapshot snapshot = new BlockSnapshot();
	/** The blocks around the ball and the car it is played with. */
	final BlockSnapshot pairSnapshot = new BlockSnapshot();
	private BlockPos origin;
	private final float[] state = new float[RlCarNative.BALL_POSE_FLOATS];

	public BallSim(BlockPos origin) {
		this.origin = origin;
	}

	public BlockPos origin() {
		return this.origin;
	}

	/** Places the ball's centre at {@code pos} moving at {@code velocity} (blocks/s), awake. */
	public void resetAt(Vec3 pos, Vec3 velocity) {
		this.origin = BlockPos.containing(pos);
		float[] p = Space.toRl(this.origin, pos.x, pos.y, pos.z);
		float[] v = Space.dirToRl(velocity, Space.UU_PER_BLOCK);
		float[] s = {p[0], p[1], p[2], v[0], v[1], v[2], 0, 0, 0};
		if (v[0] == 0 && v[1] == 0 && v[2] == 0) {
			s[5] = -WAKE_SPEED;
		}
		this.ball.reset(s);
		this.snapshot.invalidate();
	}

	/** Replaces the state with one from {@link #save} for a ball whose origin was {@code origin}. */
	public boolean load(float[] state, BlockPos origin) {
		if (state.length != RlCarNative.BALL_POSE_FLOATS) {
			return false;
		}
		for (float f : state) {
			if (!Float.isFinite(f)) {
				return false;
			}
		}
		this.origin = origin;
		this.ball.reset(state);
		this.snapshot.invalidate();
		return true;
	}

	/** Position, velocity and spin (RL space, relative to {@link #origin()}). */
	public float[] save() {
		this.ball.pose(1, this.state);
		return this.state.clone();
	}

	public BallPose pose(float alpha, BallPose out) {
		this.ball.pose(alpha, this.state);
		return out.setFromNative(this.origin, this.state);
	}

	public Vec3 position() {
		this.ball.pose(1, this.state);
		return Space.toMc(this.origin, this.state[0], this.state[1], this.state[2]);
	}

	/** True when the ball neither moves nor spins: the core does not simulate it until something touches it. */
	public boolean asleep() {
		this.ball.pose(1, this.state);
		for (int i = 3; i < 9; i++) {
			if (this.state[i] != 0) {
				return false;
			}
		}
		return true;
	}

	/** Stops the ball where it is (it sleeps until touched or woken). */
	public void sleep() {
		this.ball.pose(1, this.state);
		java.util.Arrays.fill(this.state, 3, 9, 0);
		this.ball.reset(this.state);
	}

	/** Lets a sleeping ball fall again (blocks under it changed). */
	public void wake() {
		if (this.asleep()) {
			this.state[5] = -WAKE_SPEED;
			this.ball.reset(this.state);
		}
	}

	/** Adds {@code dv} (Minecraft axes, blocks/s) to the ball's velocity (a kick). */
	public void push(Vec3 dv) {
		this.ball.pose(1, this.state);
		float[] d = Space.dirToRl(dv, Space.UU_PER_BLOCK);
		for (int i = 0; i < 3; i++) {
			this.state[3 + i] += d[i];
		}
		this.ball.reset(this.state);
	}

	/** Runs the ball alone for {@code ticks} 1/120 s ticks against the blocks of {@code level}. */
	public void step(Level level, int ticks) {
		while (ticks > 0) {
			int n = Math.min(ticks, SLICE_TICKS);
			this.recenter();
			this.snapshot.ensure(level, this.origin, this.position());
			this.ball.step(this.snapshot.world, n);
			ticks -= n;
		}
	}

	/** Hash of the block geometry around the ball (retakes the snapshot); changes when blocks there change. */
	public int geometryHash(Level level) {
		this.snapshot.invalidate();
		this.snapshot.ensure(level, this.origin, this.position());
		return this.snapshot.hash();
	}

	/** True if {@code carPos} is close enough for the car and the ball to be stepped together. */
	boolean near(Vec3 carPos) {
		return carPos.distanceToSqr(this.position()) < NEAR * NEAR;
	}

	/** Re-expresses the ball relative to {@code newOrigin} (same physical place). */
	void rebase(BlockPos newOrigin) {
		if (newOrigin.equals(this.origin)) {
			return;
		}
		BlockPos d = this.origin.subtract(newOrigin);
		float[] delta = Space.toRl(BlockPos.ZERO, d.getX(), d.getY(), d.getZ());
		this.ball.translate(delta[0], delta[1], delta[2]);
		this.origin = newOrigin;
		this.snapshot.invalidate();
	}

	private void recenter() {
		Vec3 pos = this.position();
		if (pos.distanceToSqr(Vec3.atCenterOf(this.origin)) > RECENTER_DISTANCE * RECENTER_DISTANCE) {
			this.rebase(BlockPos.containing(pos));
		}
	}

	@Override
	public void close() {
		this.ball.close();
		this.snapshot.close();
		this.pairSnapshot.close();
	}
}
