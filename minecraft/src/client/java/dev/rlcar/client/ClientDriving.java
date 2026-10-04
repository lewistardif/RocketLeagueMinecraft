package dev.rlcar.client;

import dev.rlcar.entity.BallEntity;
import dev.rlcar.entity.CarEntity;
import dev.rlcar.net.CarNet;
import dev.rlcar.physics.BallPose;
import dev.rlcar.physics.BallSim;
import dev.rlcar.physics.CarControls;
import dev.rlcar.physics.CarPose;
import dev.rlcar.physics.CarSim;
import dev.rlcar.physics.NativeBall;
import dev.rlcar.physics.Space;
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
 *
 * <p>While the server lends this client a ball (its car is the nearest to it), the ball is stepped
 * here too, in one solve with the car, and its state streamed back the same way.
 */
public final class ClientDriving {
	private static @Nullable CarSim sim;
	private static @Nullable CarEntity car;
	private static CarNet.@Nullable DriverStart pending;
	private static final CarPose pose = new CarPose();
	private static float frameSeconds;
	/** The camera the player had before getting in; restored when they get out. */
	private static @Nullable CameraType cameraBefore;

	/** The ball this client simulates with its car, if the server lent it one. */
	private static @Nullable BallSim ballSim;
	private static @Nullable BallEntity ball;
	private static CarNet.@Nullable BallOwn pendingBall;
	private static final BallPose ballPose = new BallPose();
	/** Ball cam's target, in the driven car's frame of reference. */
	private static @Nullable NativeBall ballCamTarget;
	/** Ball cam looks at the nearest ball within this distance (blocks). */
	private static final double BALL_CAM_RANGE = 300;

	private ClientDriving() {
	}

	public static void onDriverStart(CarNet.DriverStart start) {
		pending = start;
	}

	/** The server lends this client a ball to step with its car. */
	public static void onBallOwn(CarNet.BallOwn own) {
		pendingBall = own;
	}

	/** The server took the ball back. */
	public static void onBallRelease(int entityId) {
		if (pendingBall != null && pendingBall.entityId() == entityId) {
			pendingBall = null;
		}
		if (ball != null && ball.getId() == entityId) {
			releaseBall();
		}
	}

	/** Another car bumped the one this client drives. */
	public static void onBumped(CarNet.Bumped bumped) {
		if (sim != null && car != null && car.getId() == bumped.entityId()) {
			sim.addVelocity(bumped.velocity());
		}
	}

	/** The car being driven this frame, or null. */
	public static @Nullable CarPose pose() {
		return sim != null ? pose : null;
	}

	/** The simulation of the car being driven, or null. */
	public static @Nullable CarSim sim() {
		return sim;
	}

	public static boolean isDriving() {
		return sim != null;
	}

	/** True while this client steps a ball the server lent it. */
	public static boolean simulatesBall() {
		return ballSim != null;
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
			// Rocket League is played from its car camera; F5 still switches to the hood camera.
			cameraBefore = mc.options.getCameraType();
			mc.options.setCameraType(CameraType.THIRD_PERSON_BACK);
			CarKeys.resetRearCamera();
			// Replaces vanilla's "Press Left Shift to dismount" (Shift is boost here).
			mc.gui.hud.setOverlayMessage(Component.translatable("rlcar.onboard", CarKeys.EXIT.getTranslatedKeyMessage()), false);
		}

		if (CarKeys.consumeReset()) {
			Vec3 at = sim.position().add(0, 0.6, 0);
			sim.resetAt(at, mc.player.getYRot());
		}
		takeBall(mc);
		CarControls controls = mc.gui.screen() == null ? CarKeys.read() : CarControls.IDLE;
		sim.advance(mc.level, frameSeconds, controls, ballSim);
		sim.pose(sim.alpha(), pose);
		riding.localPose = pose;
		// Keep the entity, and with it the riding player and chunk loading, on the car.
		riding.setPos(pose.x, pose.y, pose.z);
		riding.setOldPosAndRot();
		if (ballSim != null && ball != null) {
			ballSim.pose(sim.alpha(), ballPose);
			ball.localPose = ballPose;
			ball.setPos(ballPose.x, ballPose.y - BallEntity.RADIUS, ballPose.z);
			ball.setOldPosAndRot();
		}
	}

	/** Starts stepping the ball the server lent us, once both it and our car are here. */
	private static void takeBall(Minecraft mc) {
		if (ball != null && ball.isRemoved()) {
			releaseBall();
		}
		if (pendingBall == null || mc.level == null) {
			return;
		}
		if (!(mc.level.getEntity(pendingBall.entityId()) instanceof BallEntity entity)) {
			return; // not here yet
		}
		releaseBall();
		BallSim s = new BallSim(BlockPos.of(pendingBall.origin()));
		if (s.load(pendingBall.state(), BlockPos.of(pendingBall.origin()))) {
			ballSim = s;
			ball = entity;
		} else {
			s.close();
		}
		pendingBall = null;
	}

	/** Stops stepping the ball; it is drawn where we left it until the server's poses take over. */
	private static void releaseBall() {
		if (ball != null) {
			ball.receivePose(ballPose.copy());
			ball.receivePose(ballPose.copy());
			ball.localPose = null;
			ball = null;
		}
		if (ballSim != null) {
			ballSim.close();
			ballSim = null;
		}
	}

	/**
	 * Ball cam's target this frame: the nearest ball, placed in the driven car's frame of
	 * reference, or null if there is none in range.
	 */
	public static @Nullable NativeBall ballCamTarget(Minecraft mc, float partialTicks) {
		if (sim == null || mc.level == null) {
			return null;
		}
		Vec3 at = sim.position();
		BallPose best = null;
		double bestDistance = BALL_CAM_RANGE * BALL_CAM_RANGE;
		for (BallEntity b : mc.level.getEntitiesOfClass(BallEntity.class, new net.minecraft.world.phys.AABB(at, at).inflate(BALL_CAM_RANGE))) {
			BallPose p = b.renderPose(partialTicks);
			if (p != null && p.position().distanceToSqr(at) < bestDistance) {
				best = p;
				bestDistance = p.position().distanceToSqr(at);
			}
		}
		if (best == null) {
			return null;
		}
		if (ballCamTarget == null) {
			ballCamTarget = new NativeBall();
		}
		float[] rl = Space.toRl(sim.origin(), best.x, best.y, best.z);
		ballCamTarget.reset(new float[] {rl[0], rl[1], rl[2], 0, 0, 0, 0, 0, 0});
		return ballCamTarget;
	}

	/** Every client tick. */
	public static void tick(Minecraft mc) {
		if (sim != null && car != null) {
			ClientPlayNetworking.send(new CarNet.DriveState(car.getId(), sim.origin().asLong(), sim.save()));
			if (ballSim != null && ball != null) {
				ClientPlayNetworking.send(new CarNet.BallState(ball.getId(), ballSim.origin().asLong(), ballSim.save()));
			}
			if (CarKeys.consumeExit()) {
				ClientPlayNetworking.send(new CarNet.ExitCar());
			}
		} else {
			// Do not let presses made outside the car trigger later.
			CarKeys.consumeExit();
			CarKeys.consumeReset();
			while (CarKeys.REAR_CAMERA.consumeClick()) {
				// middle clicks outside the car (Pick Block) must not flip the view later
			}
			while (CarKeys.BALL_CAM.consumeClick()) {
				// nor presses of Ball Cam
			}
		}
	}

	/** Leaving the world: nothing the server sent may carry over to the next one. */
	public static void disconnect() {
		stop();
		pending = null;
		pendingBall = null;
	}

	public static void stop() {
		releaseBall();
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
