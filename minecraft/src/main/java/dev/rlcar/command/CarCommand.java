package dev.rlcar.command;

import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.arguments.StringArgumentType;
import com.mojang.brigadier.context.CommandContext;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import dev.rlcar.RlCar;
import dev.rlcar.entity.CarEntity;
import dev.rlcar.physics.CarControls;
import dev.rlcar.physics.CarPose;
import dev.rlcar.physics.CarSim;
import dev.rlcar.physics.RlCarNative;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.Commands;
import net.minecraft.commands.SharedSuggestionProvider;
import net.minecraft.core.BlockPos;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.Vec3;

/**
 * {@code /rlcar spawn [preset] [blue|orange]} and {@code /rlcar selftest}.
 *
 * <p>The self-test drives a car through the real Minecraft collision path (block snapshot ->
 * Rust box world -> core) on a small test track and checks the results, so the whole server
 * side can be verified headlessly.
 */
public final class CarCommand {
	private CarCommand() {
	}

	public static void register(CommandDispatcher<CommandSourceStack> dispatcher) {
		dispatcher.register(Commands.literal("rlcar")
			.requires(Commands.hasPermission(Commands.LEVEL_GAMEMASTERS))
			.then(Commands.literal("spawn")
				.executes(c -> spawn(c, "octane", "blue"))
				.then(Commands.argument("preset", StringArgumentType.word())
					.suggests((c, b) -> SharedSuggestionProvider.suggest(RlCarNative.PRESETS, b))
					.executes(c -> spawn(c, StringArgumentType.getString(c, "preset"), "blue"))
					.then(Commands.argument("color", StringArgumentType.word())
						.suggests((c, b) -> SharedSuggestionProvider.suggest(new String[] {"blue", "orange"}, b))
						.executes(c -> spawn(c, StringArgumentType.getString(c, "preset"), StringArgumentType.getString(c, "color"))))))
			.then(Commands.literal("selftest").executes(CarCommand::selftest)));
	}

	private static int spawn(CommandContext<CommandSourceStack> c, String presetName, String colorName) throws CommandSyntaxException {
		CommandSourceStack src = c.getSource();
		int preset = Arrays.asList(RlCarNative.PRESETS).indexOf(presetName.toLowerCase(Locale.ROOT));
		if (preset < 0) {
			src.sendFailure(Component.literal("Unknown preset " + presetName + "; one of " + String.join(", ", RlCarNative.PRESETS)));
			return 0;
		}
		int color = colorName.equalsIgnoreCase("orange") ? CarEntity.ORANGE : CarEntity.BLUE;
		Vec3 look = Vec3.directionFromRotation(0, src.getRotation().y);
		Vec3 pos = src.getPosition().add(look.scale(3)).add(0, 0.5, 0);
		CarEntity car = CarEntity.create(src.getLevel(), pos, src.getRotation().y, preset, color);
		src.getLevel().addFreshEntity(car);
		src.sendSuccess(() -> Component.literal("Spawned " + RlCarNative.PRESETS[preset]), true);
		return 1;
	}

	// ------------------------------------------------------------------------------ selftest

	private static int selftest(CommandContext<CommandSourceStack> c) {
		CommandSourceStack src = c.getSource();
		ServerLevel level = src.getLevel();
		List<String> failures = new ArrayList<>();
		BlockPos base = BlockPos.containing(src.getPosition()).atY(Math.min(level.getMaxY() - 20, 300));
		try {
			buildTrack(level, base);
			double floorY = base.getY() + 1;

			// 1. Settles on a block floor at Rocket League's rest height (17 uu above the ground).
			try (CarSim sim = new CarSim(0, base)) {
				sim.resetAt(Vec3.atBottomCenterOf(base.above()).add(0, 0.6, 0), -90.0F);
				sim.step(level, 240, CarControls.IDLE);
				CarPose p = sim.pose(1, new CarPose());
				check(failures, p.has(RlCarNative.FLAG_ON_GROUND), "car is on the ground");
				check(failures, Math.abs(p.y - (floorY + 0.17)) < 0.005, "rest height " + (p.y - floorY) + " blocks (want 0.17)");

				// 2. Facing: MC yaw -90 is +X (east). Driving forward moves the car east.
				double x0 = p.x;
				sim.step(level, 120, new CarControls(1, 0, 0, 0, 0, RlCarNative.BUTTON_BOOST));
				p = sim.pose(1, p);
				check(failures, p.x - x0 > 5, "drove east " + (p.x - x0) + " blocks in 1 s (want > 5)");
				check(failures, Math.abs(p.z - (base.getZ() + 0.5)) < 0.05, "stayed on its line (z drift " + (p.z - base.getZ() - 0.5) + ")");
				check(failures, Math.abs(p.y - (floorY + 0.17)) < 0.02, "no bumps over block seams (height " + (p.y - floorY) + ")");

				// 3. Into the wall at the end of the track. Throttle alone: the wall stops the car.
				// Boost (unlimited): the impact tips the car onto the wall and it drives up it, like
				// Rocket League's walls; it must never end up inside the wall's blocks.
				double wallX = base.getX() + 30;
				sim.resetAt(Vec3.atBottomCenterOf(base.above()).add(0, 0.2, 0), -90.0F);
				sim.step(level, 600, new CarControls(1, 0, 0, 0, 0, 0));
				p = sim.pose(1, p);
				check(failures, p.x < wallX && p.has(RlCarNative.FLAG_ON_GROUND), "throttle only: stopped by the wall (x " + (p.x - base.getX()) + ", wall at 30)");
				sim.resetAt(Vec3.atBottomCenterOf(base.above()).add(0, 0.2, 0), -90.0F);
				double climbed = 0;
				boolean insideWall = false;
				for (int t = 0; t < 360; t++) {
					sim.step(level, 1, new CarControls(1, 0, 0, 0, 0, RlCarNative.BUTTON_BOOST));
					p = sim.pose(1, p);
					climbed = Math.max(climbed, p.y - floorY);
					insideWall |= p.x > wallX && p.x < wallX + 1 && p.y < floorY + 3;
				}
				check(failures, !insideWall, "boosting: never inside the wall");
				check(failures, climbed > 2.5, "boosting: drove up the wall (" + climbed + " blocks)");

				// 4. Jump: leaves the ground and lands again.
				sim.resetAt(Vec3.atBottomCenterOf(base.above()).add(0, 0.2, 0), -90.0F);
				sim.step(level, 120, CarControls.IDLE);
				double maxY = 0;
				for (int t = 0; t < 240; t++) {
					sim.step(level, 1, new CarControls(0, 0, 0, 0, 0, t < 24 ? RlCarNative.BUTTON_JUMP : 0));
					maxY = Math.max(maxY, sim.pose(1, p).y - floorY);
				}
				check(failures, maxY > 1.5 && maxY < 3.0, "full jump apex " + maxY + " blocks (want ~2.3)");
				check(failures, sim.pose(1, p).has(RlCarNative.FLAG_ON_GROUND), "landed after the jump");

				// 5. Save / load round trip through Java.
				byte[] state = sim.save();
				try (CarSim other = new CarSim(3, base)) {
					check(failures, other.load(state, sim.origin()), "state loads");
					check(failures, other.position().distanceTo(sim.position()) < 1.0E-6, "loaded state matches");
				}
			}
		} catch (Throwable t) {
			RlCar.LOG.error("rlcar selftest crashed", t);
			failures.add("crashed: " + t);
		}
		String result = failures.isEmpty() ? "RLCAR SELFTEST PASS" : "RLCAR SELFTEST FAIL: " + String.join("; ", failures);
		RlCar.LOG.info(result);
		if (failures.isEmpty()) {
			src.sendSuccess(() -> Component.literal(result), false);
		} else {
			src.sendFailure(Component.literal(result));
		}
		return failures.isEmpty() ? 1 : 0;
	}

	/** A 40 x 5 stone strip at {@code base}, with a 3-block wall 30 blocks east, cleared above. */
	private static void buildTrack(ServerLevel level, BlockPos base) {
		for (int x = -5; x <= 35; x++) {
			for (int z = -2; z <= 2; z++) {
				level.setBlockAndUpdate(base.offset(x, 0, z), Blocks.STONE.defaultBlockState());
				for (int y = 1; y <= 6; y++) {
					boolean wall = x == 30 && y <= 3;
					level.setBlockAndUpdate(base.offset(x, y, z), wall ? Blocks.STONE.defaultBlockState() : Blocks.AIR.defaultBlockState());
				}
			}
		}
	}

	private static void check(List<String> failures, boolean ok, String what) {
		RlCar.LOG.info("rlcar selftest: {} {}", ok ? "ok  " : "FAIL", what);
		if (!ok) {
			failures.add(what);
		}
	}
}
