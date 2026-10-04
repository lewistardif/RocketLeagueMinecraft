package dev.rlcar.client.mixin;

import dev.rlcar.client.ChaseCamera;
import dev.rlcar.client.ClientDriving;
import dev.rlcar.physics.CarPose;
import dev.rlcar.physics.CarSim;
import net.minecraft.client.Camera;
import net.minecraft.client.CameraType;
import net.minecraft.client.Minecraft;
import net.minecraft.world.phys.Vec3;
import org.joml.Quaternionf;
import org.joml.Vector3f;
import org.joml.Vector3fc;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * While driving: Rocket League's car camera in third person (its position, rotation including its
 * slight roll, and its field of view), a hood camera in first person.
 */
@Mixin(Camera.class)
public abstract class CameraMixin {
	@Shadow
	@Final
	private static Vector3fc FORWARDS;

	@Shadow
	@Final
	private static Vector3fc UP;

	@Shadow
	@Final
	private static Vector3fc LEFT;

	@Shadow
	@Final
	private Quaternionf rotation;

	@Shadow
	@Final
	private Vector3f forwards;

	@Shadow
	@Final
	private Vector3f up;

	@Shadow
	@Final
	private Vector3f left;

	@Shadow
	protected abstract void setRotation(float yRot, float xRot);

	@Shadow
	protected abstract void setPosition(Vec3 position);

	@Inject(method = "alignWithEntity", at = @At("TAIL"))
	private void rlcar$carCamera(float partialTicks, CallbackInfo ci) {
		ChaseCamera.clearFrame();
		CarPose pose = ClientDriving.pose();
		CarSim sim = ClientDriving.sim();
		Minecraft mc = Minecraft.getInstance();
		if (pose == null || sim == null || mc.getCameraEntity() != mc.player) {
			return;
		}
		CameraType type = mc.options.getCameraType();
		ChaseCamera.View view;
		if (type == CameraType.FIRST_PERSON) {
			view = ChaseCamera.hood(pose);
		} else if (type == CameraType.THIRD_PERSON_BACK) {
			view = ChaseCamera.chase(mc, sim, partialTicks);
		} else {
			return;
		}
		if (view == null) {
			return;
		}
		// Yaw and pitch for everything that reads them (sounds, culling), then the full rotation.
		this.setRotation(view.yRot(), view.xRot());
		if (view.rotation() != null) {
			this.rotation.set(view.rotation());
			FORWARDS.rotate(this.rotation, this.forwards);
			UP.rotate(this.rotation, this.up);
			LEFT.rotate(this.rotation, this.left);
		}
		this.setPosition(view.position());
	}

	@Inject(method = "calculateFov", at = @At("RETURN"), cancellable = true)
	private void rlcar$carCameraFov(float partialTicks, CallbackInfoReturnable<Float> cir) {
		float fov = ChaseCamera.fov();
		if (!Float.isNaN(fov)) {
			cir.setReturnValue(fov);
		}
	}
}
