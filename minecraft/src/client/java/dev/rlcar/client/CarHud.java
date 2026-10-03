package dev.rlcar.client;

import dev.rlcar.physics.CarPose;
import dev.rlcar.physics.RlCarNative;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;

/** Boost meter and speed, bottom right, while driving. */
public final class CarHud {
	private CarHud() {
	}

	public static void draw(GuiGraphicsExtractor g, DeltaTracker delta) {
		CarPose pose = ClientDriving.pose();
		Minecraft mc = Minecraft.getInstance();
		if (pose == null) {
			return;
		}
		int w = g.guiWidth();
		int h = g.guiHeight();
		int barW = 80;
		int barH = 6;
		int x = w - barW - 10;
		int y = h - 30;
		int fill = Math.round(barW * Math.clamp(pose.boost / 100.0F, 0.0F, 1.0F));
		g.fill(x - 1, y - 1, x + barW + 1, y + barH + 1, 0xA0000000);
		g.fill(x, y, x + fill, y + barH, pose.has(RlCarNative.FLAG_BOOSTING) ? 0xFFFFC040 : 0xFFF08A1C);
		g.text(mc.font, "BOOST " + Math.round(pose.boost), x, y - 11, 0xFFFFFFFF, true);
		// uu/s -> km/h (1 uu = 1 cm).
		int kmh = Math.round(Math.abs(pose.forwardSpeed) * 0.036F);
		String speed = kmh + " km/h" + (pose.has(RlCarNative.FLAG_SUPERSONIC) ? "  SUPERSONIC" : "");
		g.text(mc.font, speed, x, y + barH + 4, 0xFFFFFFFF, true);
	}
}
