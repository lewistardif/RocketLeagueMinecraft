package dev.rlcar.entity;

import dev.rlcar.physics.CarPose;
import dev.rlcar.physics.CarSim;
import dev.rlcar.physics.RlCarNative;
import dev.rlcar.physics.Space;
import java.util.Map;
import net.minecraft.core.particles.ParticleTypes;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import org.joml.Vector3f;

/**
 * Rocket League's bumps and demolitions ({@code rl_car_core::bump}), applied by the server to what
 * a car drives into: other cars and mobs (players on foot included).
 *
 * <p>The core decides from the attacker's state: hit with the front bumper while driving at the
 * victim faster than it moves away and the victim gets Rocket League's bump velocity; do that
 * supersonic and it is demolished. A slower or glancing hit just shoves the victim aside, so cars
 * do not drive through mobs. The bodies themselves still pass through each other: the core has no
 * car-car contact solver.
 */
final class CarBumps {
	/** Rocket League's bump cooldown between the same two cars (0.25 s). */
	private static final int COOLDOWN_TICKS = 5;
	/** Below this speed (blocks/s) a car does not hit anything. */
	private static final double MIN_SPEED = 2;
	/** A glancing hit closing faster than this (blocks/s) shoves the victim aside. */
	private static final double MIN_IMPACT_SPEED = 3;
	/** Share of the closing speed a shove gives (a car is heavier than a mob, as heavy as a car). */
	private static final double SHOVE_MOB = 0.9;
	private static final double SHOVE_CAR = 0.6;
	private static final float[][] HITBOX = new float[RlCarNative.PRESETS.length][];

	private CarBumps() {
	}

	/** Checks what {@code car} (at {@code pose}, simulated by {@code sim}) hits this tick. */
	static void check(ServerLevel level, CarEntity car, CarSim sim, CarPose pose, Map<Integer, Integer> cooldowns) {
		Vec3 velocity = sim.velocity();
		cooldowns.values().removeIf(until -> until <= car.tickCount);
		if (velocity.lengthSqr() < MIN_SPEED * MIN_SPEED) {
			return;
		}
		Box me = carBox(car.preset(), pose);
		for (Entity e : level.getEntities(car, me.bounds().inflate(0.5), e -> isVictim(car, e))) {
			if (cooldowns.containsKey(e.getId())) {
				continue;
			}
			boolean isCar = e instanceof CarEntity;
			CarPose victimPose = isCar ? ((CarEntity) e).serverPose() : null;
			if (isCar && victimPose == null) {
				continue;
			}
			Box other = isCar ? carBox(((CarEntity) e).preset(), victimPose) : entityBox(e.getBoundingBox());
			if (!me.overlaps(other)) {
				continue;
			}
			Vec3 victimVel = isCar ? ((CarEntity) e).serverVelocity() : e.getDeltaMovement().scale(20);
			boolean onGround = isCar ? victimPose.has(RlCarNative.FLAG_ON_GROUND) : e.onGround();
			Vector3f up = isCar ? victimPose.up() : new Vector3f(0, 1, 0);

			// Where on the car the contact is: the point of its box closest to the victim.
			double[] local = me.closestLocal(other.center());
			float contactLocalX = (float) (local[0] * Space.UU_PER_BLOCK) + hitbox(car.preset())[3];
			float[] dv = new float[3];
			int result = sim.car.bump(
				Space.toRl(sim.origin(), other.center().x, other.center().y, other.center().z),
				Space.dirToRl(victimVel, Space.UU_PER_BLOCK), onGround,
				Space.dirToRl(new Vec3(up.x, up.y, up.z), 1), contactLocalX, true, dv);

			Vec3 push = Vec3.ZERO;
			if (result == 1) {
				push = Space.dirToMc(dv[0], dv[1], dv[2], Space.UU_PER_BLOCK);
			} else if (result == 0) {
				Vec3 contact = me.toWorld(local);
				Vec3 n = other.center().subtract(contact).multiply(1, 0.25, 1);
				n = n.lengthSqr() > 1.0E-6 ? n.normalize() : velocity.normalize();
				double closing = velocity.subtract(victimVel).dot(n);
				if (closing < MIN_IMPACT_SPEED) {
					continue;
				}
				push = n.scale(closing * (isCar ? SHOVE_CAR : SHOVE_MOB));
			}
			cooldowns.put(e.getId(), car.tickCount + COOLDOWN_TICKS);
			if (result == 2) {
				demolish(level, car, e);
			} else if (e instanceof CarEntity victim) {
				victim.bumped(push);
			} else {
				e.setDeltaMovement(e.getDeltaMovement().add(push.scale(1 / 20.0)));
				e.needsSync = true;
			}
		}
	}

	private static boolean isVictim(CarEntity car, Entity e) {
		if (e.isSpectator() || !e.isAlive() || e.getVehicle() instanceof CarEntity) {
			return false; // drivers sit inside their cars
		}
		if (e instanceof CarEntity other) {
			return !other.demolished();
		}
		return e instanceof LivingEntity;
	}

	private static void demolish(ServerLevel level, CarEntity attacker, Entity victim) {
		explode(level, victim.getBoundingBox().getCenter());
		if (victim instanceof CarEntity car) {
			car.demolish();
		} else {
			victim.hurtServer(level, level.damageSources().explosion(attacker, attacker.driver()), 1000.0F);
		}
	}

	/** The effect of a demolition (no block damage). */
	static void explode(ServerLevel level, Vec3 at) {
		level.sendParticles(ParticleTypes.EXPLOSION_EMITTER, at.x, at.y, at.z, 1, 0, 0, 0, 0);
		level.playSound(null, at.x, at.y, at.z, SoundEvents.GENERIC_EXPLODE, SoundSource.NEUTRAL, 2.0F, 1.0F);
	}

	static float[] hitbox(int preset) {
		float[] h = HITBOX[preset];
		if (h == null) {
			h = RlCarNative.presetHitbox(preset);
			HITBOX[preset] = h;
		}
		return h;
	}

	/** A car's hitbox in Minecraft space. */
	private static Box carBox(int preset, CarPose pose) {
		float[] h = hitbox(preset);
		Vector3f f = pose.rotation.transform(new Vector3f(1, 0, 0));
		Vector3f u = pose.rotation.transform(new Vector3f(0, 1, 0));
		Vector3f r = pose.rotation.transform(new Vector3f(0, 0, 1));
		Vec3[] axes = {new Vec3(f.x, f.y, f.z), new Vec3(r.x, r.y, r.z), new Vec3(u.x, u.y, u.z)};
		Vec3 c = pose.position();
		for (int i = 0; i < 3; i++) {
			c = c.add(axes[i].scale(h[3 + i] / Space.UU_PER_BLOCK));
		}
		return new Box(c, axes, new double[] {h[0] / 2 / Space.UU_PER_BLOCK, h[1] / 2 / Space.UU_PER_BLOCK, h[2] / 2 / Space.UU_PER_BLOCK});
	}

	private static Box entityBox(AABB b) {
		Vec3[] axes = {new Vec3(1, 0, 0), new Vec3(0, 0, 1), new Vec3(0, 1, 0)};
		return new Box(b.getCenter(), axes, new double[] {b.getXsize() / 2, b.getZsize() / 2, b.getYsize() / 2});
	}

	/** An oriented box: centre, three unit axes and the half-size along each. */
	private record Box(Vec3 center, Vec3[] axes, double[] half) {
		AABB bounds() {
			double[] e = new double[3];
			for (int i = 0; i < 3; i++) {
				e[0] += Math.abs(this.axes[i].x) * this.half[i];
				e[1] += Math.abs(this.axes[i].y) * this.half[i];
				e[2] += Math.abs(this.axes[i].z) * this.half[i];
			}
			return new AABB(this.center.x - e[0], this.center.y - e[1], this.center.z - e[2], this.center.x + e[0], this.center.y + e[1], this.center.z + e[2]);
		}

		private double radiusAlong(Vec3 axis) {
			double r = 0;
			for (int i = 0; i < 3; i++) {
				r += Math.abs(this.axes[i].dot(axis)) * this.half[i];
			}
			return r;
		}

		/** Separating axis test over both boxes' axes and their cross products. */
		boolean overlaps(Box o) {
			Vec3 d = o.center.subtract(this.center);
			for (int i = 0; i < 15; i++) {
				Vec3 axis;
				if (i < 3) {
					axis = this.axes[i];
				} else if (i < 6) {
					axis = o.axes[i - 3];
				} else {
					axis = this.axes[(i - 6) / 3].cross(o.axes[(i - 6) % 3]);
					if (axis.lengthSqr() < 1.0E-8) {
						continue; // parallel edges: covered by the face axes
					}
					axis = axis.normalize();
				}
				if (Math.abs(d.dot(axis)) > this.radiusAlong(axis) + o.radiusAlong(axis)) {
					return false;
				}
			}
			return true;
		}

		/** The point of this box closest to {@code p}, in box coordinates (along each axis). */
		double[] closestLocal(Vec3 p) {
			Vec3 d = p.subtract(this.center);
			double[] out = new double[3];
			for (int i = 0; i < 3; i++) {
				out[i] = Math.clamp(d.dot(this.axes[i]), -this.half[i], this.half[i]);
			}
			return out;
		}

		Vec3 toWorld(double[] local) {
			Vec3 p = this.center;
			for (int i = 0; i < 3; i++) {
				p = p.add(this.axes[i].scale(local[i]));
			}
			return p;
		}
	}
}
