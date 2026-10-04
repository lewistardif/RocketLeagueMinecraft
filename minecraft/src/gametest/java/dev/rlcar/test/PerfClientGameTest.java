package dev.rlcar.test;

import dev.rlcar.RlCar;
import dev.rlcar.client.CarKeys;
import dev.rlcar.client.ClientDriving;
import dev.rlcar.client.RlAudio;
import dev.rlcar.client.RlFx;
import dev.rlcar.entity.BallEntity;
import dev.rlcar.entity.CarEntity;
import dev.rlcar.physics.CarPose;
import java.util.ArrayList;
import java.util.List;
import net.fabricmc.fabric.api.client.gametest.v1.FabricClientGameTest;
import net.fabricmc.fabric.api.client.gametest.v1.TestInput;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestSingleplayerContext;
import net.minecraft.client.Minecraft;
import net.minecraft.core.BlockPos;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.item.DyeColor;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.entity.EntityTypeTest;
import net.minecraft.world.phys.Vec3;
import org.joml.Vector3f;

/**
 * Frame rate while playing the ball (`gradlew runClientGameTest -Prlcar.perf`): a walled concrete
 * pitch, a car and a ball, then half a minute of boosting at the ball, steering towards it,
 * logging the frame rate and the effect counts every second. Skipped unless the
 * {@code rlcar.perf} system property is set.
 */
public class PerfClientGameTest implements FabricClientGameTest {
	private static final int HALF = 24;
	private static final int SECONDS = 30;

	static boolean enabled() {
		return System.getProperty("rlcar.perf") != null;
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
			TestInput input = ctx.getInput();
			BlockPos centre = sp.getServer().computeOnServer(server -> {
				ServerLevel level = server.overworld();
				ServerPlayer player = server.getPlayerList().getPlayers().getFirst();
				BlockPos p = player.blockPosition();
				for (int x = -HALF; x <= HALF; x++) {
					for (int z = -HALF; z <= HALF; z++) {
						boolean wall = Math.abs(x) == HALF || Math.abs(z) == HALF;
						level.setBlockAndUpdate(p.offset(x, -1, z), Blocks.CONCRETE.pick(DyeColor.GRAY).defaultBlockState());
						for (int y = 0; y < 6; y++) {
							level.setBlockAndUpdate(p.offset(x, y, z), wall ? Blocks.CONCRETE.pick(DyeColor.ORANGE).defaultBlockState() : Blocks.AIR.defaultBlockState());
						}
					}
				}
				CarEntity car = CarEntity.create(level, Vec3.atBottomCenterOf(p).add(-10, 0.3, 0), -90.0F, 0, CarEntity.BLUE);
				level.addFreshEntity(car);
				level.addFreshEntity(BallEntity.create(level, Vec3.atBottomCenterOf(p).add(0, BallEntity.RADIUS + 0.1, 0)));
				if (!player.startRiding(car)) {
					throw new AssertionError("player could not get into the car");
				}
				return p;
			});
			ctx.waitTicks(60);
			ctx.runOnClient(mc -> {
				mc.options.enableVsync().set(false);
				mc.options.framerateLimit().set(260);
			});

			input.holdKey(CarKeys.THROTTLE);
			List<String> lines = new ArrayList<>();
			int steer = 0;
			int stuck = 0;
			for (int t = 0; t < SECONDS * 20; t++) {
				// Steer at the ball (or the centre while it is not here), boost on straights.
				double[] aim = ctx.computeOnClient(mc -> aim(mc, centre));
				int want = aim[0] > 0.15 ? 1 : aim[0] < -0.15 ? -1 : 0;
				if (want != steer) {
					if (steer == 1) {
						input.releaseKey(CarKeys.STEER_RIGHT);
					} else if (steer == -1) {
						input.releaseKey(CarKeys.STEER_LEFT);
					}
					if (want == 1) {
						input.holdKey(CarKeys.STEER_RIGHT);
					} else if (want == -1) {
						input.holdKey(CarKeys.STEER_LEFT);
					}
					steer = want;
				}
				if (aim[1] > 0) {
					input.holdKey(CarKeys.BOOST);
				} else {
					input.releaseKey(CarKeys.BOOST);
				}
				if (t % 100 == 99) {
					input.pressKey(CarKeys.JUMP);
				}
				// Stuck (on its side, nose in a wall): back on its wheels. The ball left the pitch: a new one.
				stuck = aim[2] < 300 ? stuck + 1 : 0;
				if (stuck > 30) {
					input.pressKey(CarKeys.RESET);
					stuck = 0;
				}
				if (t % 20 == 0 && sp.getServer().computeOnServer(server -> outside(server, centre))) {
					sp.getServer().runCommand("kill @e[type=rlcar:ball]");
					sp.getServer().runOnServer(server -> server.overworld().addFreshEntity(
						BallEntity.create(server.overworld(), Vec3.atBottomCenterOf(centre).add(0, BallEntity.RADIUS + 0.1, 0))));
				}
				ctx.waitTick();
				if (t % 20 == 19) {
					int second = t / 20 + 1;
					lines.add(ctx.computeOnClient(mc -> {
						CarPose p = ClientDriving.pose();
						double speed = p == null ? 0 : Math.sqrt(p.velocity[0] * p.velocity[0] + p.velocity[1] * p.velocity[1] + p.velocity[2] * p.velocity[2]);
						double toBall = -1;
						for (BallEntity b : mc.level.getEntitiesOfClass(BallEntity.class, mc.player.getBoundingBox().inflate(HALF * 3))) {
							toBall = p == null ? -1 : b.center().distanceTo(p.position());
						}
						return String.format("rlcar perf: %2d s  fps %4d  particles %4d  voices %3d  speed %5.0f  to ball %5.1f  lent %s  heap %d MB", second, mc.getFps(),
							RlFx.particles(), RlAudio.voices(), speed, toBall, ClientDriving.simulatesBall(), (Runtime.getRuntime().totalMemory() - Runtime.getRuntime().freeMemory()) >> 20);
					}));
					RlCar.LOG.info(lines.getLast());
				}
			}
			input.releaseKey(CarKeys.THROTTLE);
			input.releaseKey(CarKeys.BOOST);
			input.releaseKey(CarKeys.STEER_LEFT);
			input.releaseKey(CarKeys.STEER_RIGHT);
			for (String l : lines) {
				RlCar.LOG.info(l);
			}
		}
	}

	/** The ball is gone or off the pitch. */
	private static boolean outside(MinecraftServer server, BlockPos centre) {
		var balls = server.overworld().getEntities(EntityTypeTest.forClass(BallEntity.class), b -> true);
		if (balls.isEmpty()) {
			return true;
		}
		Vec3 c = balls.getFirst().center();
		return Math.abs(c.x - centre.getX()) > HALF - 1 || Math.abs(c.z - centre.getZ()) > HALF - 1 || c.y < centre.getY() - 1;
	}

	/** (sideways error towards the target, 1 to boost, speed in uu/s) for the driven car. */
	private static double[] aim(Minecraft mc, BlockPos centre) {
		CarPose pose = ClientDriving.pose();
		if (pose == null) {
			return new double[] {0, 0, 1000};
		}
		Vec3 target = Vec3.atCenterOf(centre);
		for (BallEntity b : mc.level.getEntitiesOfClass(BallEntity.class, mc.player.getBoundingBox().inflate(HALF * 2))) {
			target = b.center();
		}
		Vector3f f = pose.forward();
		double dx = target.x - pose.x, dz = target.z - pose.z;
		double len = Math.max(1e-6, Math.sqrt(dx * dx + dz * dz));
		double flen = Math.max(1e-6, Math.sqrt(f.x * f.x + f.z * f.z));
		// Right of the car in Minecraft is forward rotated towards +Z: (−f.z, f.x).
		double side = (-f.z * dx + f.x * dz) / (len * flen);
		double ahead = (f.x * dx + f.z * dz) / (len * flen);
		double speed = Math.sqrt(pose.velocity[0] * pose.velocity[0] + pose.velocity[1] * pose.velocity[1] + pose.velocity[2] * pose.velocity[2]);
		return new double[] {side, ahead > 0.8 ? 1 : 0, speed};
	}
}
