package dev.rlcar.client;

import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.OptionInstance;
import net.minecraft.client.Options;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.options.OptionsSubScreen;
import net.minecraft.network.chat.Component;

/**
 * Rocket League's camera settings screen: a preset button, its sliders (with the game's ranges and
 * steps), Invert Swivel Pitch and Rear Camera Toggle. Opened from Options > Controls.
 */
public final class CameraSettingsScreen extends OptionsSubScreen {
	private Button presetButton;

	public CameraSettingsScreen(Screen lastScreen, Options options) {
		super(lastScreen, options, Component.translatable("rlcar.camera.title"));
	}

	@Override
	protected void addOptions() {
		this.presetButton = Button.builder(presetLabel(), b -> {
			CameraSettings.applyPreset(nextPreset());
			CameraSettings.save();
			this.rebuildWidgets();
		}).build();
		this.list.addBig(this.presetButton);

		List<AbstractWidget> widgets = new ArrayList<>();
		for (CameraSettings.Setting s : CameraSettings.Setting.values()) {
			widgets.add(slider(s).createButton(this.options));
		}
		OptionInstance<Boolean> invert = OptionInstance.createBoolean("rlcar.camera.invert_swivel_pitch", CameraSettings.invertSwivelPitch(), on -> {
			CameraSettings.setInvertSwivelPitch(on);
			CameraSettings.save();
		});
		widgets.add(invert.createButton(this.options));
		OptionInstance<Boolean> rearToggle = OptionInstance.createBoolean("rlcar.camera.rear_camera_toggle",
			OptionInstance.cachedConstantTooltip(Component.translatable("rlcar.camera.rear_camera_toggle.tooltip")), CameraSettings.rearCameraToggle(), on -> {
				CameraSettings.setRearCameraToggle(on);
				CameraSettings.save();
			});
		widgets.add(rearToggle.createButton(this.options));
		this.list.addSmall(widgets);
	}

	/** The presets a player can pick (Custom is what moving a slider gives). */
	private static int nextPreset() {
		int next = (CameraSettings.preset() + 1) % CameraSettings.PRESETS.length;
		return CameraSettings.PRESETS[next].equals("custom") ? (next + 1) % CameraSettings.PRESETS.length : next;
	}

	private static Component presetLabel() {
		return Component.translatable("rlcar.camera.preset", Component.translatable("rlcar.camera.preset." + CameraSettings.PRESETS[CameraSettings.preset()]));
	}

	private OptionInstance<Integer> slider(CameraSettings.Setting s) {
		int initial = Math.round((s.get() - s.min) / s.step);
		return new OptionInstance<>(
			"rlcar.camera." + s.id,
			OptionInstance.cachedConstantTooltip(Component.translatable("rlcar.camera." + s.id + ".tooltip")),
			(caption, step) -> Options.genericValueLabel(caption, Component.literal(s.format(s.min + step * s.step))),
			new OptionInstance.IntRange(0, s.steps()),
			initial,
			step -> {
				s.set(s.min + step * s.step);
				CameraSettings.markCustom();
				this.presetButton.setMessage(presetLabel());
			}
		);
	}

	@Override
	public void removed() {
		super.removed();
		CameraSettings.save();
	}
}
