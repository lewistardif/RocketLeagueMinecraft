package dev.rlcar.client.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import dev.rlcar.client.PadBinds;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.options.controls.KeyBindsScreen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Key Binds screen, controller rows: while one waits for a controller button, the gamepad is
 * polled every frame, Escape unbinds it and a click or any other key cancels. Reset Keys also
 * resets the controller bindings.
 */
@Mixin(KeyBindsScreen.class)
public abstract class KeyBindsScreenMixin {
	@Shadow
	private Button resetButton;

	@Shadow
	public abstract void refreshKeybindLabels();

	@Inject(method = "extractRenderState", at = @At("HEAD"))
	private void rlcar$pollController(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a, CallbackInfo ci) {
		if (PadBinds.pollCapture()) {
			this.refreshKeybindLabels();
		}
	}

	@Inject(method = "extractRenderState", at = @At("TAIL"))
	private void rlcar$canResetController(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a, CallbackInfo ci) {
		if (!PadBinds.allDefault()) {
			this.resetButton.active = true;
		}
	}

	@Inject(method = "mouseClicked", at = @At("HEAD"), cancellable = true)
	private void rlcar$clickCancelsController(MouseButtonEvent event, boolean doubleClick, CallbackInfoReturnable<Boolean> cir) {
		if (PadBinds.capturing() != null) {
			PadBinds.cancelCapture();
			this.refreshKeybindLabels();
			cir.setReturnValue(true);
		}
	}

	@Inject(method = "keyPressed", at = @At("HEAD"), cancellable = true)
	private void rlcar$keyWhileController(KeyEvent event, CallbackInfoReturnable<Boolean> cir) {
		PadBinds.Action action = PadBinds.capturing();
		if (action != null) {
			PadBinds.cancelCapture();
			if (event.isEscape()) {
				PadBinds.set(action, PadBinds.Input.NONE);
			}
			this.refreshKeybindLabels();
			cir.setReturnValue(true);
		}
	}

	/** Reset Keys (the first button built in the footer) resets the controller bindings too. */
	@WrapOperation(method = "addFooter", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/gui/components/Button;builder(Lnet/minecraft/network/chat/Component;Lnet/minecraft/client/gui/components/Button$OnPress;)Lnet/minecraft/client/gui/components/Button$Builder;", ordinal = 0))
	private Button.Builder rlcar$resetAllController(Component message, Button.OnPress onPress, Operation<Button.Builder> original) {
		return original.call(message, (Button.OnPress) button -> {
			PadBinds.resetAll();
			onPress.onPress(button);
		});
	}
}
