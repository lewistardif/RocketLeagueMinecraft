package dev.rlcar.client;

import com.google.common.collect.ImmutableList;
import dev.rlcar.RlCar;
import java.util.List;
import net.minecraft.ChatFormatting;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.components.Tooltip;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.narration.NarratableEntry;
import net.minecraft.client.gui.screens.options.controls.KeyBindsList;
import net.minecraft.client.gui.screens.options.controls.KeyBindsScreen;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.MutableComponent;

/**
 * One row of the "RL Car (Controller)" section of the Key Binds screen, laid out like the vanilla
 * key rows: the action, its controller button, and Reset. Click the button, then press a button
 * or trigger on the controller; Escape unbinds it, a click cancels.
 */
public final class PadBindEntry extends KeyBindsList.Entry {
	/** Section header only; not registered, since no {@link KeyMapping} uses it. */
	public static final KeyMapping.Category CATEGORY = new KeyMapping.Category(RlCar.id("controller"));

	private final KeyBindsList list;
	private final PadBinds.Action action;
	private final Component name;
	private final Button changeButton;
	private final Button resetButton;
	private List<PadBinds.Action> conflicts = List.of();

	public PadBindEntry(KeyBindsList list, KeyBindsScreen screen, PadBinds.Action action) {
		this.list = list;
		this.action = action;
		this.name = action.label();
		this.changeButton = Button.builder(this.name, button -> {
				screen.selectedKey = null;
				PadBinds.startCapture(action);
				screen.refreshKeybindLabels();
			})
			.bounds(0, 0, 75, 20)
			.createNarration(message -> PadBinds.get(action) == PadBinds.Input.NONE
				? Component.translatable("narrator.controls.unbound", this.name)
				: Component.translatable("narrator.controls.bound", this.name, message.get()))
			.build();
		this.resetButton = Button.builder(Component.translatable("controls.reset"), button -> {
				PadBinds.cancelCapture();
				PadBinds.set(action, action.defaultInput);
				screen.refreshKeybindLabels();
			})
			.bounds(0, 0, 50, 20)
			.createNarration(message -> Component.translatable("narrator.controls.reset", this.name))
			.build();
		this.refreshEntry();
	}

	@Override
	public void extractContent(GuiGraphicsExtractor graphics, int mouseX, int mouseY, boolean hovered, float a) {
		this.updateChangeButtonMessage();
		// Same columns as the key rows above (KeyBindsList.KeyEntry).
		int resetButtonX = this.list.getRowRight() + this.list.scrollbarWidth() + 2 - this.resetButton.getWidth() - 10;
		int buttonY = this.getContentY() - 2;
		this.resetButton.setPosition(resetButtonX, buttonY);
		this.resetButton.extractRenderState(graphics, mouseX, mouseY, a);
		int changeButtonX = resetButtonX - 5 - this.changeButton.getWidth();
		this.changeButton.setPosition(changeButtonX, buttonY);
		this.changeButton.extractRenderState(graphics, mouseX, mouseY, a);
		graphics.text(Minecraft.getInstance().font, this.name, this.getContentX(), this.getContentYMiddle() - 9 / 2, -1);
		if (!this.conflicts.isEmpty()) {
			int stripeLeft = this.changeButton.getX() - 6;
			graphics.fill(stripeLeft, this.getContentY() - 1, stripeLeft + 3, this.getContentBottom(), -256);
		}
	}

	@Override
	public List<? extends GuiEventListener> children() {
		return ImmutableList.of(this.changeButton, this.resetButton);
	}

	@Override
	public List<? extends NarratableEntry> narratables() {
		return ImmutableList.of(this.changeButton, this.resetButton);
	}

	@Override
	public void refreshEntry() {
		this.resetButton.active = !PadBinds.isDefault(this.action);
		this.conflicts = PadBinds.conflicts(this.action);
		if (this.conflicts.isEmpty()) {
			this.changeButton.setTooltip(null);
		} else {
			MutableComponent names = Component.empty();
			for (int i = 0; i < this.conflicts.size(); i++) {
				names.append(i > 0 ? Component.literal(", ") : Component.empty()).append(this.conflicts.get(i).label());
			}
			this.changeButton.setTooltip(Tooltip.create(Component.translatable("controls.keybinds.duplicateKeybinds", names)));
		}
		this.updateChangeButtonMessage();
	}

	private void updateChangeButtonMessage() {
		Component message = PadBinds.get(this.action).label();
		if (!this.conflicts.isEmpty()) {
			message = Component.literal("[ ").append(message.copy().withStyle(ChatFormatting.WHITE)).append(" ]").withStyle(ChatFormatting.YELLOW);
		}
		if (PadBinds.capturing() == this.action) {
			message = Component.literal("> ")
				.append(message.copy().withStyle(ChatFormatting.WHITE, ChatFormatting.UNDERLINE))
				.append(" <")
				.withStyle(ChatFormatting.YELLOW);
		}
		this.changeButton.setMessage(message);
	}
}
