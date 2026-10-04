package dev.rlcar.physics;

import java.util.Arrays;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.VoxelShape;

/**
 * A snapshot of the block collision boxes around one or more points (a car, a ball, or a car and
 * the ball it plays), as a native {@code BoxWorld} in Rocket League space relative to an origin.
 * It is retaken once a point gets near the edge, so the bodies never leave it. The Rust side merges
 * the boxes into a seamless surface, so nothing catches on the seams between blocks.
 */
final class BlockSnapshot implements AutoCloseable {
	/** Half-size of the box collected around each point, in blocks. */
	static final int RADIUS = 4;
	/** Retake the snapshot once a point is this far (blocks) from where it was taken. */
	private static final double REFRESH_DISTANCE = 1.5;

	final NativeWorld world = new NativeWorld();
	private Vec3[] centers = new Vec3[0];
	private BlockPos origin = BlockPos.ZERO;
	private int hash;
	private float[] boxes = new float[6 * 256];

	/** Retakes the snapshot unless it already covers {@code points} (in the same order) for {@code origin}. */
	void ensure(Level level, BlockPos origin, Vec3... points) {
		boolean fresh = origin.equals(this.origin) && points.length == this.centers.length;
		for (int i = 0; fresh && i < points.length; i++) {
			fresh = points[i].distanceToSqr(this.centers[i]) <= REFRESH_DISTANCE * REFRESH_DISTANCE;
		}
		if (!fresh) {
			this.take(level, origin, points);
		}
	}

	/** The next {@link #ensure} retakes the snapshot. */
	void invalidate() {
		this.centers = new Vec3[0];
	}

	/** Hash of the boxes in the current snapshot; changes when blocks there change. */
	int hash() {
		return this.hash;
	}

	/** Collects the collision boxes of all blocks within {@link #RADIUS} of any of {@code points}. */
	private void take(Level level, BlockPos origin, Vec3[] points) {
		int minX = Integer.MAX_VALUE, minY = Integer.MAX_VALUE, minZ = Integer.MAX_VALUE;
		int maxX = Integer.MIN_VALUE, maxY = Integer.MIN_VALUE, maxZ = Integer.MIN_VALUE;
		for (Vec3 p : points) {
			BlockPos c = BlockPos.containing(p);
			minX = Math.min(minX, c.getX() - RADIUS);
			minY = Math.min(minY, c.getY() - RADIUS);
			minZ = Math.min(minZ, c.getZ() - RADIUS);
			maxX = Math.max(maxX, c.getX() + RADIUS);
			maxY = Math.max(maxY, c.getY() + RADIUS);
			maxZ = Math.max(maxZ, c.getZ() + RADIUS);
		}
		BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
		int count = 0;
		int hash = 1;
		for (int x = minX; x <= maxX; x++) {
			for (int z = minZ; z <= maxZ; z++) {
				pos.set(x, minY, z);
				if (!level.isLoaded(pos)) {
					continue;
				}
				for (int y = minY; y <= maxY; y++) {
					pos.set(x, y, z);
					BlockState state = level.getBlockState(pos);
					if (state.isAir()) {
						continue;
					}
					VoxelShape shape = state.getCollisionShape(level, pos, CollisionContext.empty());
					if (shape.isEmpty()) {
						continue;
					}
					double ox = x - origin.getX();
					double oy = y - origin.getY();
					double oz = z - origin.getZ();
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
		this.origin = origin;
		this.centers = points.clone();
		this.hash = hash;
	}

	@Override
	public void close() {
		this.world.close();
	}
}
