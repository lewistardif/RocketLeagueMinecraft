package dev.rlcar.test;

import dev.rlcar.RlCar;
import dev.rlcar.client.CarKeys;
import dev.rlcar.client.ClientDriving;
import dev.rlcar.client.RlFx;
import dev.rlcar.entity.BallEntity;
import dev.rlcar.entity.CarEntity;
import dev.rlcar.physics.RlCarNative;
import java.nio.file.Path;
import net.fabricmc.fabric.api.client.gametest.v1.FabricClientGameTest;
import net.fabricmc.fabric.api.client.gametest.v1.TestInput;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestSingleplayerContext;
import net.fabricmc.fabric.api.client.gametest.v1.screenshot.TestScreenshotOptions;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.phys.Vec3;

/**
 * Look development of the car and ball materials (`gradlew runClientGameTest -Prlcar.lookdev`): a
 * blue and an orange Octane parked at noon, photographed close up from the back, the side and the
 * front; then the ball and its markers. Skipped unless the {@code rlcar.lookdev} system property is
 * set.
 */
public class LookdevClientGameTest implements FabricClientGameTest {
	static boolean enabled() {
		return System.getProperty("rlcar.lookdev") != null;
	}

	@Override
	public void runTest(ClientGameTestContext ctx) {
		if (!enabled()) {
			return;
		}
		try (TestSingleplayerContext sp = ctx.worldBuilder().create()) {
			sp.getConnection().waitForChunksRender();
			sp.getServer().runCommand("time set noon");
			sp.getServer().runCommand("weather clear");
			// Spectators do not fall between the teleport and the screenshot.
			sp.getServer().runCommand("gamemode spectator @a");
			BlockPos p = sp.getServer().computeOnServer(server -> {
				ServerLevel level = server.overworld();
				BlockPos at = server.getPlayerList().getPlayers().getFirst().blockPosition();
				level.addFreshEntity(CarEntity.create(level, Vec3.atBottomCenterOf(at).add(4, 0.3, 0), 0.0F, 0, CarEntity.BLUE));
				level.addFreshEntity(CarEntity.create(level, Vec3.atBottomCenterOf(at).add(4, 0.3, 4), 0.0F, 0, CarEntity.ORANGE));
				return at;
			});
			ctx.runOnClient(mc -> mc.gui.hud.toggle());
			ctx.waitTicks(60);
			// The blue car is at (4, 0, 0) from the player, facing south (+Z); the eye goes around it.
			Vec3 car = Vec3.atBottomCenterOf(p).add(4, 0.35, 0);
			view(ctx, sp, car, -1.8, 1.2, -2.0, "back");
			view(ctx, sp, car, 0.6, 0.9, -2.6, "back-low");
			view(ctx, sp, car, 2.8, 0.7, 0.2, "side");
			view(ctx, sp, car, 1.6, 0.9, 2.2, "front");
			view(ctx, sp, car, -0.6, 1.9, -1.2, "engine");
			// The wheels close up: the front and rear right ones, from the side.
			view(ctx, sp, car.add(0.42, -0.2, 0.86), 1.0, 0.3, 0.3, "wheel-front");
			view(ctx, sp, car.add(0.42, -0.2, -0.75), 1.0, 0.3, -0.3, "wheel-rear");

			// Every hitbox preset (every chassis material) in a row, from behind and the front.
			sp.getServer().runCommand("kill @e[type=rlcar:car]");
			sp.getServer().runOnServer(server -> {
				ServerLevel level = server.overworld();
				for (int i = 0; i < RlCarNative.PRESETS.length; i++) {
					level.addFreshEntity(CarEntity.create(level, Vec3.atBottomCenterOf(p).add(-10 + i * 2.6, 0.3, 12), 0.0F, i, i % 2 == 0 ? CarEntity.BLUE : CarEntity.ORANGE));
				}
			});
			ctx.waitTicks(40);
			Vec3 row = Vec3.atBottomCenterOf(p).add(-10 + 3 * 2.6, 0.35, 12);
			view(ctx, sp, row, 0, 3.0, -7.5, "lineup-back");
			view(ctx, sp, row, 0, 3.0, 7.5, "lineup-front");
			for (int i : new int[] {1, 3, 6}) {
				view(ctx, sp, Vec3.atBottomCenterOf(p).add(-10 + i * 2.6, 0.35, 12), -1.4, 1.4, -2.4, "preset-" + RlCarNative.PRESETS[i]);
			}
			sp.getServer().runCommand("kill @e[type=rlcar:car]");
			ball(ctx, sp, p);
			supersonic(ctx, sp, p);
		}
	}

	/** Supersonic: the blue Octane on full boost, from its chase camera (the streaks are only drawn for the driver). */
	private static void supersonic(ClientGameTestContext ctx, TestSingleplayerContext sp, BlockPos p) {
		sp.getServer().runCommand("kill @e[type=rlcar:ball]");
		sp.getServer().runCommand("gamemode creative @a");
		sp.getServer().runOnServer(server -> {
			ServerLevel level = server.overworld();
			CarEntity car = CarEntity.create(level, Vec3.atBottomCenterOf(p).add(0, 0.3, 20), -90.0F, 0, CarEntity.BLUE);
			level.addFreshEntity(car);
			if (!server.getPlayerList().getPlayers().getFirst().startRiding(car)) {
				throw new AssertionError("player could not get into the car");
			}
		});
		ctx.waitTicks(20);
		ctx.runOnClient(mc -> mc.gui.hud.toggle());
		TestInput input = ctx.getInput();
		input.holdKey(CarKeys.THROTTLE);
		input.holdKey(CarKeys.BOOST);
		ctx.waitTicks(70);
		for (int i = 0; i < 3; i++) {
			RlCar.LOG.info("rlcar lookdev: supersonic {}, {} particles", ClientDriving.isDriving(), ctx.computeOnClient(mc -> RlFx.particles()));
			Path path = ctx.takeScreenshot(TestScreenshotOptions.of("rlcar-lookdev-supersonic-" + i).withSize(1600, 900));
			RlCar.LOG.info("rlcar lookdev: screenshot {}", path.toAbsolutePath());
			ctx.waitTicks(3);
		}
		input.releaseKey(CarKeys.BOOST);
		input.releaseKey(CarKeys.THROTTLE);
	}

	/**
	 * The ball: its material on the kickoff spot (white lights) and on the blue and orange halves of
	 * the field, its lights' pulse, and its markers (the ground reticle and the line under a ball in
	 * the air; the outline and the dark halo from far away, also through a wall).
	 */
	private static void ball(ClientGameTestContext ctx, TestSingleplayerContext sp, BlockPos p) {
		// The markers hide with the HUD, as the game hides them on HideWorldUI.
		ctx.runOnClient(mc -> mc.gui.hud.toggle());
		Vec3 ground = Vec3.atBottomCenterOf(p).add(0, 0, -14);
		BlockPos pillar = BlockPos.containing(ground.add(-7, 5, 0));
		sp.getServer().runCommand("setblock " + pillar.getX() + " " + pillar.getY() + " " + pillar.getZ() + " minecraft:barrier");
		Vec3 high = Vec3.atBottomCenterOf(pillar).add(0, 1 + BallEntity.RADIUS + 0.02, 0);
		sp.getServer().runOnServer(server -> {
			ServerLevel level = server.overworld();
			double[] xs = {0, 3, -3};
			double[] kickoffDz = {0, -12, 12}; // on the spot; 12 blocks into the orange (+Z) half; into the blue half
			for (int i = 0; i < 3; i++) {
				Vec3 at = ground.add(xs[i], BallEntity.RADIUS + 0.02, 0);
				BallEntity b = BallEntity.create(level, at);
				b.setKickoff(at.add(0, 0, kickoffDz[i]));
				level.addFreshEntity(b);
			}
			level.addFreshEntity(BallEntity.create(level, high));
		});
		ctx.waitTicks(60);
		Vec3 ball = ground.add(0, BallEntity.RADIUS, 0);
		view(ctx, sp, ball, -1.6, 0.9, -1.6, "ball-close");
		view(ctx, sp, ball, 0, 2.2, -6.5, "ball-teams");
		// The strips light up in the last eighth of every second (the material's Time is game time).
		// view() takes the picture 10 ticks after it starts: aim for tick 19 of a second.
		long ticks = sp.getServer().computeOnServer(server -> server.overworld().getGameTime());
		ctx.waitTicks((int) Math.floorMod(9 - ticks, 20L));
		view(ctx, sp, ball, -1.6, 0.9, -1.6, "ball-pulse");
		view(ctx, sp, high, 4.5, -1.0, -7.5, "ball-air");
		view(ctx, sp, high, 0.5, 4.0, -3.0, "ball-air-above");
		view(ctx, sp, high, 0, 4.0, -48.0, "ball-far");
		BlockPos wall = BlockPos.containing(high.add(0, 0, -20));
		sp.getServer().runCommand("fill " + (wall.getX() - 3) + " " + (wall.getY() - 6) + " " + wall.getZ() + " " + (wall.getX() + 3) + " " + (wall.getY() + 6) + " " + wall.getZ() + " minecraft:stone");
		view(ctx, sp, high, 0, 4.0, -48.0, "ball-far-wall");
	}

	private static void view(ClientGameTestContext ctx, TestSingleplayerContext sp, Vec3 car, double dx, double dy, double dz, String name) {
		// tp places the feet, and "facing" aims from them; the eye is 1.62 above them.
		Vec3 eye = car.add(dx, dy, dz);
		sp.getServer().runCommand("tp @p " + eye.x + " " + (eye.y - 1.62) + " " + eye.z + " facing " + car.x + " " + (car.y - 1.62) + " " + car.z);
		ctx.waitTicks(10);
		Path path = ctx.takeScreenshot(TestScreenshotOptions.of("rlcar-lookdev-" + name).withSize(1600, 900));
		RlCar.LOG.info("rlcar lookdev: screenshot {}", path.toAbsolutePath());
	}
}
