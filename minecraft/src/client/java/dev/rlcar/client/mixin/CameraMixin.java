package dev.rlcar.client.mixin;

import dev.rlcar.client.ChaseCamera;
import dev.rlcar.client.ClientDriving;
import dev.rlcar.physics.CarPose;
import net.minecraft.client.Camera;
import net.minecraft.client.CameraType;
import net.minecraft.client.Minecraft;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** While driving: Rocket League's car cam in third person, a hood cam in first person. */
@Mixin(Camera.class)
public abstract class CameraMixin {
	@Shadow
	protected abstract void setRotation(float yRot, float xRot);

	@Shadow
	protected abstract void setPosition(Vec3 position);

	@Inject(method = "alignWithEntity", at = @At("TAIL"))
	private void rlcar$carCamera(float partialTicks, CallbackInfo ci) {
		CarPose pose = ClientDriving.pose();
		Minecraft mc = Minecraft.getInstance();
		if (pose == null || mc.getCameraEntity() != mc.player) {
			ChaseCamera.reset();
			return;
		}
		CameraType type = mc.options.getCameraType();
		ChaseCamera.View view;
		if (type == CameraType.FIRST_PERSON) {
			view = ChaseCamera.hood(pose);
		} else if (type == CameraType.THIRD_PERSON_BACK) {
			view = ChaseCamera.chase(mc, pose);
		} else {
			return;
		}
		this.setRotation(view.yRot(), view.xRot());
		this.setPosition(view.position());
	}
}
