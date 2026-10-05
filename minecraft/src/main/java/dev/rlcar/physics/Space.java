package dev.rlcar.physics;

import net.minecraft.core.BlockPos;
import net.minecraft.world.phys.Vec3;
import org.joml.Matrix3f;
import org.joml.Quaternionf;

/**
 * The one conversion layer between Rocket League space (the physics core) and Minecraft.
 *
 * <table>
 * <tr><th></th><th>Rocket League (core)</th><th>Minecraft</th></tr>
 * <tr><td>unit</td><td>uu (~1 cm)</td><td>block (1 m)</td></tr>
 * <tr><td>up</td><td>+Z</td><td>+Y</td></tr>
 * <tr><td>handedness</td><td>left-handed</td><td>right-handed</td></tr>
 * <tr><td>car forward / right / up</td><td>+X / +Y / +Z (local)</td><td>+X / +Z / +Y (local)</td></tr>
 * </table>
 *
 * <p>Mapping: {@code mc = (rl.x, rl.z, rl.y) / 100}. Swapping two axes is a reflection, which is
 * exactly what turns the left-handed basis into a right-handed one, so "turn right" stays "turn
 * right". Rotations convert by conjugation with the same swap. This is the same mapping as the
 * Bevy demo (Bevy is also Y-up and right-handed).
 *
 * <p>Positions are relative to a per-car integer block <em>origin</em>, so the 32-bit floats of
 * the core stay precise far from the world centre.
 */
public final class Space {
	public static final float UU_PER_BLOCK = 100.0F;

	private Space() {
	}

	/** Absolute Minecraft position to RL uu relative to {@code origin}. */
	public static float[] toRl(BlockPos origin, double x, double y, double z) {
		return new float[] {
			(float) ((x - origin.getX()) * UU_PER_BLOCK),
			(float) ((z - origin.getZ()) * UU_PER_BLOCK),
			(float) ((y - origin.getY()) * UU_PER_BLOCK)
		};
	}

	/** RL uu relative to {@code origin} to an absolute Minecraft position. */
	public static Vec3 toMc(BlockPos origin, float x, float y, float z) {
		return new Vec3(origin.getX() + x / UU_PER_BLOCK, origin.getY() + z / UU_PER_BLOCK, origin.getZ() + y / UU_PER_BLOCK);
	}

	/** An RL direction or velocity (any unit) to Minecraft axes, divided by {@code scale}. */
	public static Vec3 dirToMc(float x, float y, float z, float scale) {
		return new Vec3(x / scale, z / scale, y / scale);
	}

	/** A Minecraft direction or velocity to RL axes, multiplied by {@code scale}. */
	public static float[] dirToRl(Vec3 v, float scale) {
		return new float[] {(float) (v.x * scale), (float) (v.z * scale), (float) (v.y * scale)};
	}

	/**
	 * An RL angular velocity (rad/s) to Minecraft axes. Angular velocity is a pseudovector, so the
	 * axis swap (a reflection) also flips its sign.
	 */
	public static Vec3 angularToMc(float x, float y, float z) {
		return new Vec3(-x, -z, -y);
	}

	/**
	 * Car orientation (RL columns forward, right, up) to the Minecraft rotation of a model built
	 * with local +X forward, +Y up, +Z right.
	 */
	public static Quaternionf rotationToMc(float[] p, int offset) {
		// Columns of the MC basis: forward, up, right, each swizzled (x, z, y).
		Matrix3f m = new Matrix3f(
			p[offset], p[offset + 2], p[offset + 1],
			p[offset + 6], p[offset + 8], p[offset + 7],
			p[offset + 3], p[offset + 5], p[offset + 4]
		);
		return m.getNormalizedRotation(new Quaternionf());
	}

	/**
	 * Minecraft yaw (degrees; 0 = facing +Z, 90 = facing -X) to the RL yaw angle (radians) of a car
	 * facing the same way.
	 */
	public static float yawToRl(float mcYawDegrees) {
		double r = Math.toRadians(mcYawDegrees);
		// MC facing (-sin, 0, cos) -> RL (x, y) = (-sin, cos).
		return (float) Math.atan2(Math.cos(r), -Math.sin(r));
	}

	/** Minecraft yaw (degrees) of a horizontal MC direction. */
	public static float yawOfMc(double dx, double dz) {
		return (float) Math.toDegrees(Math.atan2(-dx, dz));
	}
}
