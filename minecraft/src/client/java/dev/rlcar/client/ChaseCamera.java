package dev.rlcar.client;

import dev.rlcar.physics.CarPose;
import dev.rlcar.physics.CarSim;
import dev.rlcar.physics.NativeBall;
import dev.rlcar.physics.RlCarNative;
import dev.rlcar.physics.Space;
import net.minecraft.client.Minecraft;
import net.minecraft.util.Mth;
import net.minecraft.world.level.ClipContext;
import net.minecraft.world.phys.HitResult;
import net.minecraft.world.phys.Vec3;
import org.joml.Matrix3f;
import org.joml.Quaternionf;
import org.joml.Vector3f;
import org.jspecify.annotations.Nullable;

/**
 * Rocket League's car camera while driving in third person (the Rust core's
 * {@code rl_car_core::camera}, with the player's {@link CameraSettings}): the right stick swivels
 * it, Rear Camera looks behind (held, or switched with Rear Camera Toggle), Ball Cam keeps the
 * nearest ball in view, and in the air it keeps looking where the car is going however the car
 * flips. Unlike Rocket League's see-through arena walls, blocks pull it in. In first person it
 * becomes a hood camera.
 */
public final class ChaseCamera {
	private static final float[] VIEW = new float[RlCarNative.CAMERA_VIEW_FLOATS];
	/** Vertical FOV (degrees) of this frame's chase view, or NaN to leave Minecraft's. */
	private static float fov = Float.NaN;
	private static boolean activeLastFrame;
	private static boolean activeThisFrame;

	private ChaseCamera() {
	}

	/**
	 * A camera pose for this frame.
	 *
	 * @param rotation the full rotation (camera looks down local -Z, +Y up), roll included, or
	 *     null for plain yaw/pitch
	 */
	public record View(Vec3 position, float yRot, float xRot, @Nullable Quaternionf rotation) {
	}

	/** Third-person chase view for this frame, advancing the camera by the frame time. */
	public static @Nullable View chase(Minecraft mc, CarSim sim, float partialTicks) {
		if (!activeLastFrame) {
			sim.resetCamera(); // back from the hood camera or another view: no swing from the old pose
		}
		activeThisFrame = true;
		CarKeys.CameraInput in = CarKeys.readCamera(mc.gui.screen() == null);
		NativeBall ball = in.ballCam() ? ClientDriving.ballCamTarget(mc, partialTicks) : null;
		int flags = (in.rearView() ? RlCarNative.CAMERA_REAR_VIEW : 0) | (ball != null ? RlCarNative.CAMERA_BALL_CAM : 0);
		if (!sim.camera(ClientDriving.frameSeconds(), CameraSettings.values(), in.lookRight(), in.lookUp(), flags, ball, VIEW)) {
			return null;
		}
		Vec3 focus = Space.toMc(sim.origin(), VIEW[14], VIEW[15], VIEW[16]);
		Vec3 wanted = Space.toMc(sim.origin(), VIEW[0], VIEW[1], VIEW[2]);
		// Pull in against blocks, like the vanilla third-person camera.
		if (mc.level != null && mc.player != null) {
			HitResult hit = mc.level.clip(new ClipContext(focus, wanted, ClipContext.Block.VISUAL, ClipContext.Fluid.NONE, mc.player));
			if (hit.getType() != HitResult.Type.MISS) {
				Vec3 dir = wanted.subtract(focus).normalize();
				wanted = hit.getLocation().subtract(dir.scale(0.2));
			}
		}
		// RL axes (x, y, z) -> Minecraft (x, z, y); the camera's local frame is right, up, back.
		Vector3f forward = new Vector3f(VIEW[3], VIEW[5], VIEW[4]);
		Vector3f right = new Vector3f(VIEW[6], VIEW[8], VIEW[7]);
		Vector3f up = new Vector3f(VIEW[9], VIEW[11], VIEW[10]);
		Quaternionf rotation = new Matrix3f(right, up, forward.negate(new Vector3f())).getNormalizedRotation(new Quaternionf());
		float yRot = Space.yawOfMc(forward.x, forward.z);
		float xRot = (float) -Math.toDegrees(Math.asin(Mth.clamp(forward.y, -1, 1)));
		fov = VIEW[13];
		return new View(wanted, yRot, xRot, rotation);
	}

	/** First-person hood view: on the roof, looking where the car points. */
	public static View hood(CarPose pose) {
		Vector3f f = pose.forward();
		Vector3f u = pose.up();
		Vec3 eye = pose.position().add(u.x * 0.55, u.y * 0.55, u.z * 0.55).add(f.x * 0.1, f.y * 0.1, f.z * 0.1);
		float pitch = (float) -Math.toDegrees(Math.asin(Mth.clamp(f.y, -1, 1)));
		return new View(eye, Space.yawOfMc(f.x, f.z), pitch, null);
	}

	/** Rocket League's FOV for this frame (vertical, degrees), or NaN when not in the chase view. */
	public static float fov() {
		return fov;
	}

	/** Called at the start of every camera update: no chase view (yet) this frame. */
	public static void clearFrame() {
		fov = Float.NaN;
		activeLastFrame = activeThisFrame;
		activeThisFrame = false;
	}
}
