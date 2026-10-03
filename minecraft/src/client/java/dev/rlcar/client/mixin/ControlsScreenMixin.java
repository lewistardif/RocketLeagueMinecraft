package dev.rlcar.client.mixin;

import dev.rlcar.client.CameraSettingsScreen;
import net.minecraft.client.Options;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.options.OptionsSubScreen;
import net.minecraft.client.gui.screens.options.controls.ControlsScreen;
import net.minecraft.network.chat.Component;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Options > Controls: a button to Rocket League's camera settings, under Key Binds. */
@Mixin(ControlsScreen.class)
public abstract class ControlsScreenMixin extends OptionsSubScreen {
	private ControlsScreenMixin(Screen lastScreen, Options options, Component title) {
		super(lastScreen, options, title);
	}

	@Inject(method = "addOptions", at = @At("TAIL"))
	private void rlcar$cameraSettingsButton(CallbackInfo ci) {
		this.list.addBig(Button.builder(Component.translatable("rlcar.camera.open"), b -> this.minecraft.gui.setScreen(new CameraSettingsScreen(this, this.options))).build());
	}
}
