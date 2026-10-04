package dev.rlcar.client;

import dev.rlcar.entity.CarEntity;
import dev.rlcar.physics.CarPose;
import java.util.HashSet;
import java.util.Set;
import net.minecraft.client.Minecraft;
import net.minecraft.world.entity.Entity;

/**
 * Every frame, after the driven car has stepped: each car's sounds ({@link RlAudio}) and effects
 * ({@link RlFx}) follow its pose, the way the Bevy demo runs them after its physics ticks.
 */
public final class CarEffects {
	private CarEffects() {
	}

	public static void frame(Minecraft mc) {
		if (mc.level == null) {
			RlAudio.stopAll();
			RlFx.clear();
			return;
		}
		RlAudio.setPaused(mc.isPaused());
		float dt = ClientDriving.frameSeconds();
		float partial = mc.getDeltaTracker().getGameTimeDeltaPartialTick(false);
		CarEntity driven = ClientDriving.isDriving() && mc.player != null && mc.player.getVehicle() instanceof CarEntity c ? c : null;
		Set<Integer> seen = new HashSet<>();
		for (Entity e : mc.level.entitiesForRendering()) {
			if (!(e instanceof CarEntity car) || car.demolished()) {
				continue;
			}
			CarPose pose = car.renderPose(partial);
			if (pose == null) {
				continue;
			}
			seen.add(car.getId());
			RlAudio.update(car.getId(), pose, car == driven, dt);
			RlFx.update(car, pose, car == driven, dt);
		}
		RlAudio.endFrame(seen);
		RlFx.endFrame(seen, dt);
	}
}
