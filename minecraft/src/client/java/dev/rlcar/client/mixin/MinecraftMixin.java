package dev.rlcar.client.mixin;

import dev.rlcar.client.ClientDriving;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Options;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(Minecraft.class)
public abstract class MinecraftMixin {
	@Shadow
	@Final
	public Options options;

	/** Step the driven car once per frame, after this frame's ticks and before rendering. */
	@Inject(method = "renderFrame", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/renderer/GameRenderer;render()V"))
	private void rlcar$frame(boolean advanceGameTime, CallbackInfo ci) {
		ClientDriving.frame((Minecraft) (Object) this);
	}

	/**
	 * While driving, keys shared with the car must not also drop items, open the inventory, swap
	 * hands, hit or use things (a held click is stopped by the cancellations below).
	 */
	@Inject(method = "handleKeybinds", at = @At("HEAD"))
	private void rlcar$swallowVanillaKeys(CallbackInfo ci) {
		if (ClientDriving.inCar()) {
			for (KeyMapping k : new KeyMapping[] {
				this.options.keyInventory, this.options.keyDrop, this.options.keySwapOffhand,
				this.options.keyAttack, this.options.keyUse, this.options.keyPickItem
			}) {
				while (k.consumeClick()) {
				}
			}
		}
	}

	@Inject(method = "startAttack", at = @At("HEAD"), cancellable = true)
	private void rlcar$noAttackWhileDriving(CallbackInfoReturnable<Boolean> cir) {
		if (ClientDriving.inCar()) {
			cir.setReturnValue(false);
		}
	}

	@Inject(method = "continueAttack", at = @At("HEAD"), cancellable = true)
	private void rlcar$noMiningWhileDriving(boolean down, CallbackInfo ci) {
		if (ClientDriving.inCar()) {
			ci.cancel();
		}
	}

	@Inject(method = "startUseItem", at = @At("HEAD"), cancellable = true)
	private void rlcar$noUseWhileDriving(CallbackInfo ci) {
		if (ClientDriving.inCar()) {
			ci.cancel();
		}
	}

	@Inject(method = "pickBlockOrEntity", at = @At("HEAD"), cancellable = true)
	private void rlcar$noPickWhileDriving(CallbackInfo ci) {
		if (ClientDriving.inCar()) {
			ci.cancel();
		}
	}
}
