package dev.rlcar.client;

import dev.rlcar.entity.CarEntity;
import dev.rlcar.net.CarNet;
import dev.rlcar.physics.CarControls;
import dev.rlcar.physics.CarPose;
import dev.rlcar.physics.CarSim;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayNetworking;
import net.minecraft.client.CameraType;
import net.minecraft.client.Minecraft;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;
import net.minecraft.world.phys.Vec3;
import org.jspecify.annotations.Nullable;

/**
 * The car the local player drives, simulated on this client every rendered frame (the core runs
 * at 120 Hz regardless of frame rate, and the pose is interpolated between its ticks). Input is
 * sampled every frame, so jump and dodge timing is as precise as in the Bevy demo.
 *
 * <p>The full state goes to the server every client tick; when the player gets out, the server
 * continues from it.
 */
public final class ClientDriving {
	private static @Nullable CarSim sim;
	private static @Nullable CarEntity car;
	private static CarNet.@Nullable DriverStart pending;
	private static final CarPose pose = new CarPose();
	private static float frameSeconds;
	/** The camera the player had before getting in; restored when they get out. */
	private static @Nullable CameraType cameraBefore;

	private ClientDriving() {
	}

	public static void onDriverStart(CarNet.DriverStart start) {
		pending = start;
	}

	/** The car being driven this frame, or null. */
	public static @Nullable CarPose pose() {
		return sim != null ? pose : null;
	}

	public static boolean isDriving() {
		return sim != null;
	}

	/**
	 * True while the local player sits in a car, including the few frames before the server's
	 * state arrives: the car's keys must not walk, dismount or use items during those either.
	 */
	public static boolean inCar() {
		Minecraft mc = Minecraft.getInstance();
		return sim != null || mc.player != null && mc.player.getVehicle() instanceof CarEntity;
	}

	/** Real game time of the last frame (seconds); 0 while paused. */
	public static float frameSeconds() {
		return frameSeconds;
	}

	/** Every frame, before the world is rendered. */
	public static void frame(Minecraft mc) {
		frameSeconds = mc.isPaused() ? 0 : mc.getDeltaTracker().getGameTimeDeltaTicks() / 20.0F;
		CarEntity riding = mc.player != null && mc.player.getVehicle() instanceof CarEntity c ? c : null;
		if (riding != car) {
			stop();
		}
		if (riding == null || mc.level == null) {
			return;
		}
		if (sim == null) {
			if (pending == null || pending.entityId() != riding.getId()) {
				return; // wait for the server's state
			}
			CarSim s = new CarSim(riding.preset(), BlockPos.of(pending.origin()));
			if (!s.load(pending.state(), BlockPos.of(pending.origin()))) {
				s.close();
				pending = null;
				return;
			}
			sim = s;
			car = riding;
			pending = null;
			// Rocket League is played from the chase camera; F5 still switches to the hood camera.
			cameraBefore = mc.options.getCameraType();
			mc.options.setCameraType(CameraType.THIRD_PERSON_BACK);
			// Replaces vanilla's "Press Left Shift to dismount" (Shift is boost here).
			mc.gui.hud.setOverlayMessage(Component.translatable("rlcar.onboard", CarKeys.EXIT.getTranslatedKeyMessage()), false);
		}

		if (CarKeys.consumeReset()) {
			Vec3 at = sim.position().add(0, 0.6, 0);
			sim.resetAt(at, mc.player.getYRot());
		}
		CarControls controls = mc.gui.screen() == null ? CarKeys.read() : CarControls.IDLE;
		sim.advance(mc.level, frameSeconds, controls);
		sim.pose(sim.alpha(), pose);
		riding.localPose = pose;
		// Keep the entity, and with it the riding player and chunk loading, on the car.
		riding.setPos(pose.x, pose.y, pose.z);
		riding.setOldPosAndRot();
	}

	/** Every client tick. */
	public static void tick(Minecraft mc) {
		if (sim != null && car != null) {
			ClientPlayNetworking.send(new CarNet.DriveState(car.getId(), sim.origin().asLong(), sim.save()));
			if (CarKeys.consumeExit()) {
				ClientPlayNetworking.send(new CarNet.ExitCar());
			}
		} else {
			// Do not let presses made outside the car trigger later.
			CarKeys.consumeExit();
			CarKeys.consumeReset();
		}
	}

	public static void stop() {
		if (car != null) {
			// Keep drawing it where we left it until the server's poses take over.
			car.receivePose(pose.copy());
			car.receivePose(pose.copy());
			car.localPose = null;
			car = null;
		}
		if (sim != null) {
			sim.close();
			sim = null;
		}
		if (cameraBefore != null) {
			Minecraft.getInstance().options.setCameraType(cameraBefore);
			cameraBefore = null;
		}
	}
}
