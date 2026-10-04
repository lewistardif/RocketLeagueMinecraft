package dev.rlcar.test;

import dev.rlcar.RlCar;
import dev.rlcar.entity.CarEntity;
import dev.rlcar.physics.RlCarNative;
import java.nio.file.Path;
import net.fabricmc.fabric.api.client.gametest.v1.FabricClientGameTest;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestSingleplayerContext;
import net.fabricmc.fabric.api.client.gametest.v1.screenshot.TestScreenshotOptions;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.phys.Vec3;

/**
 * Look development of the car materials (`gradlew runClientGameTest -Prlcar.lookdev`): a blue and
 * an orange Octane parked at noon, photographed close up from the back, the side and the front.
 * Skipped unless the {@code rlcar.lookdev} system property is set.
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
		}
	}

	private static void view(ClientGameTestContext ctx, TestSingleplayerContext sp, Vec3 car, double dx, double dy, double dz, String name) {
		// tp places the feet; the eye is 1.62 above them.
		Vec3 eye = car.add(dx, dy, dz);
		sp.getServer().runCommand("tp @p " + eye.x + " " + (eye.y - 1.62) + " " + eye.z + " facing " + car.x + " " + car.y + " " + car.z);
		ctx.waitTicks(10);
		Path path = ctx.takeScreenshot(TestScreenshotOptions.of("rlcar-lookdev-" + name).withSize(1600, 900));
		RlCar.LOG.info("rlcar lookdev: screenshot {}", path.toAbsolutePath());
	}
}
