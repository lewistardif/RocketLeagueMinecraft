package dev.rlcar.client.mixin;

import dev.rlcar.client.PadBindEntry;
import dev.rlcar.client.PadBinds;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.ContainerObjectSelectionList;
import net.minecraft.client.gui.screens.options.controls.KeyBindsList;
import net.minecraft.client.gui.screens.options.controls.KeyBindsScreen;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Key Binds screen: an "RL Car (Controller)" section after the key sections, one row per {@link PadBinds.Action}. */
@Mixin(KeyBindsList.class)
public abstract class KeyBindsListPadMixin extends ContainerObjectSelectionList<KeyBindsList.Entry> {
	private KeyBindsListPadMixin(Minecraft minecraft, int width, int height, int y, int itemHeight) {
		super(minecraft, width, height, y, itemHeight);
	}

	@Inject(method = "<init>", at = @At("TAIL"))
	private void rlcar$addControllerSection(KeyBindsScreen screen, Minecraft minecraft, CallbackInfo ci) {
		PadBinds.cancelCapture();
		KeyBindsList self = (KeyBindsList) (Object) this;
		this.addEntry(self.new CategoryEntry(PadBindEntry.CATEGORY));
		for (PadBinds.Action action : PadBinds.Action.values()) {
			this.addEntry(new PadBindEntry(self, screen, action));
		}
	}
}
