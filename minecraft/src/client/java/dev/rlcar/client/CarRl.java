package dev.rlcar.client;

import dev.rlcar.physics.CarPose;
import dev.rlcar.physics.Space;
import net.minecraft.world.phys.Vec3;
import org.joml.Matrix3f;
import org.joml.Vector3f;

/**
 * A car's pose in Rocket League space (uu, X forward / Y right / Z up for the car, Z up for the
 * world), absolute: the space the extracted sound and effect data is in, as in the Bevy demo.
 * Minecraft {@code (x, y, z)} blocks is Rocket League {@code (x, z, y) * 100} (see {@link Space}).
 */
final class CarRl {
	final Vector3f pos;
	/** Columns: forward, right, up. */
	final Matrix3f rot;
	final Vector3f vel;
	final Vector3f angVel;

	private CarRl(Vector3f pos, Matrix3f rot, Vector3f vel, Vector3f angVel) {
		this.pos = pos;
		this.rot = rot;
		this.vel = vel;
		this.angVel = angVel;
	}

	static CarRl of(CarPose p) {
		Vector3f f = toRlDir(p.rotation.transform(new Vector3f(1, 0, 0)));
		Vector3f r = toRlDir(p.rotation.transform(new Vector3f(0, 0, 1)));
		Vector3f u = toRlDir(p.rotation.transform(new Vector3f(0, 1, 0)));
		Matrix3f m = new Matrix3f(f, r, u);
		return new CarRl(toRl(p.x, p.y, p.z), m, new Vector3f(p.velocity), new Vector3f(p.angularVelocity));
	}

	Vector3f forward() {
		return this.rot.getColumn(0, new Vector3f());
	}

	Vector3f right() {
		return this.rot.getColumn(1, new Vector3f());
	}

	Vector3f up() {
		return this.rot.getColumn(2, new Vector3f());
	}

	/** A car-local offset (uu) to an absolute position. */
	Vector3f toWorld(Vector3f local) {
		return this.rot.transform(new Vector3f(local)).add(this.pos);
	}

	static Vector3f toRl(double x, double y, double z) {
		return new Vector3f((float) (x * Space.UU_PER_BLOCK), (float) (z * Space.UU_PER_BLOCK), (float) (y * Space.UU_PER_BLOCK));
	}

	static Vector3f toRlDir(Vector3f mc) {
		return new Vector3f(mc.x, mc.z, mc.y);
	}

	static Vec3 toMc(Vector3f rl) {
		return new Vec3(rl.x / Space.UU_PER_BLOCK, rl.z / Space.UU_PER_BLOCK, rl.y / Space.UU_PER_BLOCK);
	}
}
