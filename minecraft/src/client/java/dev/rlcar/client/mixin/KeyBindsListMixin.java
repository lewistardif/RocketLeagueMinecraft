package dev.rlcar.client.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import dev.rlcar.client.CarKeys;
import net.minecraft.client.KeyMapping;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/**
 * Controls screen: a car binding sharing a key with its Rocket League partner (Throttle and Pitch
 * Down, ...) or with a vanilla action that is off while driving (walking, Drop, Inventory,
 * Attack, ...) is not a conflict, so it is not marked as one.
 */
@Mixin(targets = "net.minecraft.client.gui.screens.options.controls.KeyBindsList$KeyEntry")
public abstract class KeyBindsListMixin {
	@WrapOperation(method = "refreshEntry", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/KeyMapping;same(Lnet/minecraft/client/KeyMapping;)Z"))
	private boolean rlcar$intendedSharing(KeyMapping self, KeyMapping other, Operation<Boolean> original) {
		return original.call(self, other) && !CarKeys.canShareKey(self, other);
	}
}
