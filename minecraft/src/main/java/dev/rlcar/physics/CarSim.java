package dev.rlcar.physics;

import java.util.Arrays;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.VoxelShape;
import org.jspecify.annotations.Nullable;

/**
 * A car plus the block geometry around it. Used by the server (cars nobody drives) and by the
 * driving client (the car it drives), so both run the exact same code.
 *
 * <p>Collision geometry is a snapshot of the block collision boxes within {@link #RADIUS} blocks
 * of the car, refreshed before the car can get near its edge. The Rust side merges the boxes into
 * a seamless surface, so the car does not catch on the seams between blocks.
 */
public final class CarSim implements AutoCloseable {
	/** Snapshot half-size in blocks. The car moves at most ~1.2 blocks per server tick. */
	public static final int RADIUS = 4;
	/** Refresh the snapshot once the car is this far (blocks) from where it was taken. */
	private static final double REFRESH_DISTANCE = 1.5;
	/**
	 * Unlimited boost for every car, for now. It is a simulation rule, so the server and the driving
	 * client must agree on it: both get it from here.
	 */
	public static final boolean UNLIMITED_BOOST = true;
	/** Move the origin once the car is this far from it (blocks), to keep floats precise. */
	private static final double RECENTER_DISTANCE = 512;

	public final NativeCar car;
	private final NativeWorld world = new NativeWorld();
	private BlockPos origin;
	private Vec3 snapshotCenter;
	private int snapshotHash;
	private float[] boxes = new float[6 * 256];
	private final float[] poseBuffer = new float[RlCarNative.POSE_FLOATS];
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
		this.snapshotCenter = null;
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
		this.snapshotCenter = null;
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
		return out.setFromNative(this.origin, this.poseBuffer, flags);
	}

	public Vec3 position() {
		this.car.pose(1, this.poseBuffer);
		return Space.toMc(this.origin, this.poseBuffer[0], this.poseBuffer[1], this.poseBuffer[2]);
	}

	/** Runs {@code ticks} 1/120 s ticks against the blocks of {@code level}. */
	public void step(Level level, int ticks, CarControls controls) {
		// In slices of one Minecraft tick, so the car cannot leave its block snapshot.
		while (ticks > 0) {
			int n = Math.min(ticks, 6);
			this.prepare(level);
			this.car.step(this.world, n, controls);
			ticks -= n;
		}
	}

	/** Runs as many ticks as fit in {@code dt} seconds (for per-frame stepping). */
	public void advance(Level level, double dt, CarControls controls) {
		// In slices, so a long frame cannot carry the car out of its block snapshot.
		while (dt > 0) {
			double slice = Math.min(dt, 1.0 / 20.0);
			this.prepare(level);
			this.car.advance(this.world, slice, controls);
			dt -= slice;
		}
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
		if (this.camera == null) {
			this.camera = new NativeCamera();
		}
		return this.camera.update(this.car, this.car.alpha(), dt, settings, lookRight, lookUp, flags, out);
	}

	/**
	 * Hash of the block geometry currently around the car; changes when blocks there change.
	 * Retakes the snapshot.
	 */
	public int geometryHash(Level level) {
		this.snapshotCenter = null;
		this.prepare(level);
		return this.snapshotHash;
	}

	private void prepare(Level level) {
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
			this.snapshotCenter = null;
		}
		if (this.snapshotCenter == null || pos.distanceToSqr(this.snapshotCenter) > REFRESH_DISTANCE * REFRESH_DISTANCE) {
			this.snapshot(level, pos);
		}
	}

	/** Collects the collision boxes of all blocks within {@link #RADIUS} of {@code center}. */
	private void snapshot(Level level, Vec3 center) {
		BlockPos c = BlockPos.containing(center);
		BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
		int count = 0;
		int hash = 1;
		for (int x = c.getX() - RADIUS; x <= c.getX() + RADIUS; x++) {
			for (int z = c.getZ() - RADIUS; z <= c.getZ() + RADIUS; z++) {
				pos.set(x, c.getY(), z);
				if (!level.isLoaded(pos)) {
					continue;
				}
				for (int y = c.getY() - RADIUS; y <= c.getY() + RADIUS; y++) {
					pos.set(x, y, z);
					BlockState state = level.getBlockState(pos);
					if (state.isAir()) {
						continue;
					}
					VoxelShape shape = state.getCollisionShape(level, pos, CollisionContext.empty());
					if (shape.isEmpty()) {
						continue;
					}
					double ox = x - this.origin.getX();
					double oy = y - this.origin.getY();
					double oz = z - this.origin.getZ();
					for (var box : shape.toAabbs()) {
						if (count * 6 + 6 > this.boxes.length) {
							this.boxes = Arrays.copyOf(this.boxes, this.boxes.length * 2);
						}
						// RL min/max: (x, z, y) * 100. Swapping axes keeps min below max.
						float[] b = this.boxes;
						int i = count * 6;
						b[i] = (float) ((ox + box.minX) * Space.UU_PER_BLOCK);
						b[i + 1] = (float) ((oz + box.minZ) * Space.UU_PER_BLOCK);
						b[i + 2] = (float) ((oy + box.minY) * Space.UU_PER_BLOCK);
						b[i + 3] = (float) ((ox + box.maxX) * Space.UU_PER_BLOCK);
						b[i + 4] = (float) ((oz + box.maxZ) * Space.UU_PER_BLOCK);
						b[i + 5] = (float) ((oy + box.maxY) * Space.UU_PER_BLOCK);
						for (int k = 0; k < 6; k++) {
							hash = hash * 31 + Float.floatToIntBits(b[i + k]);
						}
						count++;
					}
				}
			}
		}
		this.world.setBoxes(this.boxes, count);
		this.snapshotCenter = center;
		this.snapshotHash = hash;
	}

	@Override
	public void close() {
		this.car.close();
		this.world.close();
		if (this.camera != null) {
			this.camera.close();
		}
	}
}
