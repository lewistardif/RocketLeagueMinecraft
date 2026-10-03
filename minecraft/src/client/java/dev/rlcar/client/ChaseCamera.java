package dev.rlcar.client;

import dev.rlcar.physics.CarPose;
import dev.rlcar.physics.Space;
import net.minecraft.client.Minecraft;
import net.minecraft.util.Mth;
import net.minecraft.world.level.ClipContext;
import net.minecraft.world.phys.HitResult;
import net.minecraft.world.phys.Vec3;
import org.joml.Vector3f;

/**
 * Rocket League's default "car cam" while driving in third person: 2.7 blocks behind and 1 block
 * above the car (RL: distance 270, height 100), looking 3 degrees down, swinging round to follow
 * the car's heading. In first person it becomes a hood camera.
 */
public final class ChaseCamera {
	private static final double DISTANCE = 2.7;
	private static final double HEIGHT = 1.0;
	private static final float PITCH = 3.0F;
	/** How fast the camera swings round to the car's heading (1/s). */
	private static final float STIFFNESS = 8.0F;

	private static float yaw = Float.NaN;

	private ChaseCamera() {
	}

	public record View(Vec3 position, float yRot, float xRot) {
	}

	/** Third-person chase view for this frame. */
	public static View chase(Minecraft mc, CarPose pose) {
		Vector3f f = pose.forward();
		float horizontal = f.x * f.x + f.z * f.z;
		// Driving up a wall the nose points up; keep swinging by the last good heading then.
		if (horizontal > 0.04F || Float.isNaN(yaw)) {
			float target = Space.yawOfMc(f.x, f.z);
			if (Float.isNaN(yaw)) {
				yaw = target;
			} else {
				float k = 1.0F - (float) Math.exp(-STIFFNESS * ClientDriving.frameSeconds());
				yaw += Mth.wrapDegrees(target - yaw) * k;
			}
		}
		Vec3 car = pose.position();
		Vec3 back = Vec3.directionFromRotation(0, yaw).scale(-DISTANCE);
		Vec3 pivot = car.add(0, 0.3, 0);
		Vec3 wanted = car.add(back).add(0, HEIGHT, 0);
		// Pull in against blocks, like the vanilla third-person camera.
		if (mc.level != null && mc.player != null) {
			HitResult hit = mc.level.clip(new ClipContext(pivot, wanted, ClipContext.Block.VISUAL, ClipContext.Fluid.NONE, mc.player));
			if (hit.getType() != HitResult.Type.MISS) {
				Vec3 dir = wanted.subtract(pivot).normalize();
				wanted = hit.getLocation().subtract(dir.scale(0.2));
			}
		}
		return new View(wanted, yaw, PITCH);
	}

	/** First-person hood view: on the roof, looking where the car points. */
	public static View hood(CarPose pose) {
		Vector3f f = pose.forward();
		Vector3f u = pose.up();
		Vec3 eye = pose.position().add(u.x * 0.55, u.y * 0.55, u.z * 0.55).add(f.x * 0.1, f.y * 0.1, f.z * 0.1);
		float pitch = (float) -Math.toDegrees(Math.asin(Mth.clamp(f.y, -1, 1)));
		yaw = Space.yawOfMc(f.x, f.z);
		return new View(eye, yaw, pitch);
	}

	public static void reset() {
		yaw = Float.NaN;
	}
}
