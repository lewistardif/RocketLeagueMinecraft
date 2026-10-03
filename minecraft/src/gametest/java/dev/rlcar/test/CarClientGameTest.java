package dev.rlcar.test;

import com.mojang.blaze3d.platform.InputConstants;
import dev.rlcar.RlCar;
import dev.rlcar.client.CarKeys;
import dev.rlcar.client.ClientDriving;
import dev.rlcar.client.PadBindEntry;
import dev.rlcar.client.PadBinds;
import dev.rlcar.entity.CarEntity;
import dev.rlcar.physics.CarPose;
import dev.rlcar.physics.RlCarNative;
import java.nio.file.Path;
import java.util.List;
import net.fabricmc.fabric.api.client.gametest.v1.FabricClientGameTest;
import net.fabricmc.fabric.api.client.gametest.v1.TestInput;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestSingleplayerContext;
import net.minecraft.client.CameraType;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.options.controls.KeyBindsList;
import net.minecraft.client.gui.screens.options.controls.KeyBindsScreen;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.item.DyeColor;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.Vec3;
import org.joml.Vector3f;

/**
 * Drives a car in the real game: gets in, boosts (with Boost rebound to the left mouse button, as
 * in Rocket League), jumps, turns, air rolls, resets, gets out, with a screenshot at each step.
 * Fails if the car does not respond the way the physics and the bindings say it should.
 */
public class CarClientGameTest implements FabricClientGameTest {
	private static final int ROAD_LENGTH = 160;

	@Override
	public void runTest(ClientGameTestContext ctx) {
		checkBindings(ctx);
		try (TestSingleplayerContext sp = ctx.worldBuilder().create()) {
			sp.getConnection().waitForChunksRender();
			sp.getServer().runCommand("time set noon");
			sp.getServer().runCommand("weather clear");
			TestInput input = ctx.getInput();

			// A concrete road heading east (+X) with a wall at the end, and a car on it.
			BlockPos start = sp.getServer().computeOnServer(server -> {
				ServerLevel level = server.overworld();
				ServerPlayer player = server.getPlayerList().getPlayers().getFirst();
				BlockPos p = player.blockPosition();
				for (int x = -6; x <= ROAD_LENGTH; x++) {
					for (int z = -4; z <= 4; z++) {
						level.setBlockAndUpdate(p.offset(x, -1, z), road(z));
					}
				}
				for (int z = -4; z <= 4; z++) {
					for (int y = 0; y < 2; y++) {
						level.setBlockAndUpdate(p.offset(ROAD_LENGTH + 1, y, z), Blocks.CONCRETE.pick(DyeColor.ORANGE).defaultBlockState());
					}
				}
				CarEntity car = CarEntity.create(level, Vec3.atBottomCenterOf(p).add(2, 0.3, 0), -90.0F, 0, CarEntity.BLUE);
				level.addFreshEntity(car);
				CarEntity parked = CarEntity.create(level, Vec3.atBottomCenterOf(p).add(8, 0.3, 3), 180.0F, 1, CarEntity.ORANGE);
				level.addFreshEntity(parked);
				if (!player.startRiding(car)) {
					throw new AssertionError("player could not get into the car");
				}
				return p;
			});
			ctx.waitTicks(40);
			check(ctx.computeOnClient(mc -> ClientDriving.isDriving()), "client is driving the car");
			check(ctx.computeOnClient(mc -> mc.options.getCameraType()) == CameraType.THIRD_PERSON_BACK, "switched to the chase camera");
			CarPose rest = pose(ctx);
			check(rest.has(RlCarNative.FLAG_ON_GROUND), "car rests on its wheels");
			check(Math.abs(rest.y - (start.getY() + 0.17)) < 0.01, "rest height " + (rest.y - start.getY()));
			check(rest.boost == 100, "unlimited boost: spawns with a full tank (" + rest.boost + ")");
			shot(ctx, "1-parked");

			// Throttle + Boost, with Boost rebound to the left mouse button (Rocket League's default).
			// Left click is Attack in Minecraft; while driving it must only boost.
			rebind(ctx, CarKeys.BOOST, InputConstants.Type.MOUSE.getOrCreate(InputConstants.MOUSE_BUTTON_LEFT));
			input.holdKey(CarKeys.THROTTLE);
			input.holdMouse(InputConstants.MOUSE_BUTTON_LEFT);
			ctx.waitTicks(12);
			check(pose(ctx).has(RlCarNative.FLAG_BOOSTING), "boosting with Boost on the left mouse button");
			shot(ctx, "2-boosting");
			ctx.waitTicks(18);
			CarPose fast = pose(ctx);
			check(fast.x - rest.x > 15, "boosted east: moved " + (fast.x - rest.x) + " blocks");
			check(fast.boost == 100, "unlimited boost: tank still full after 1.5 s (" + fast.boost + ")");
			check(!ctx.computeOnClient(mc -> mc.gameMode.isDestroying()), "left click did not mine while driving");
			input.releaseMouse(InputConstants.MOUSE_BUTTON_LEFT);
			rebind(ctx, CarKeys.BOOST, CarKeys.BOOST.getDefaultKey());

			// Jump (W would pitch the nose down in the air, so let go of it first).
			input.releaseKey(CarKeys.THROTTLE);
			input.holdKey(CarKeys.JUMP);
			ctx.waitTicks(4);
			input.releaseKey(CarKeys.JUMP);
			ctx.waitTicks(8);
			CarPose air = pose(ctx);
			shot(ctx, "3-jump");
			check(!air.has(RlCarNative.FLAG_ON_GROUND) && air.y - start.getY() > 0.8, "in the air after a jump: height " + (air.y - start.getY()));
			ctx.waitTicks(30);
			check(pose(ctx).has(RlCarNative.FLAG_ON_GROUND), "landed");

			// Turn right: heading swings from +X towards +Z (right, in Minecraft).
			input.holdKey(CarKeys.THROTTLE);
			input.holdKey(CarKeys.STEER_RIGHT);
			ctx.waitTicks(15);
			shot(ctx, "4-turn");
			CarPose turned = pose(ctx);
			check(turned.forward().z > 0.2, "turned right, forward " + turned.forward());
			input.releaseKey(CarKeys.STEER_RIGHT);
			input.releaseKey(CarKeys.THROTTLE);
			ctx.waitTicks(40);

			// Air Roll + Yaw Right (D) rolls the car instead of yawing it.
			input.holdKey(CarKeys.JUMP);
			ctx.waitTicks(4);
			input.releaseKey(CarKeys.JUMP);
			Vector3f forwardBefore = pose(ctx).forward();
			input.holdKey(CarKeys.AIR_ROLL);
			input.holdKey(CarKeys.YAW_RIGHT);
			ctx.waitTicks(10);
			CarPose rolled = pose(ctx);
			shot(ctx, "5-air-roll");
			input.releaseKey(CarKeys.YAW_RIGHT);
			input.releaseKey(CarKeys.AIR_ROLL);
			check(rolled.up().y < 0.8, "air rolled: up " + rolled.up());
			check(rolled.forward().dot(forwardBefore) > 0.9, "did not yaw while air rolling: forward " + rolled.forward());

			// Reset Car puts it back on its wheels.
			ctx.waitTicks(30);
			input.pressKey(CarKeys.RESET);
			ctx.waitTicks(40);
			CarPose reset = pose(ctx);
			check(reset.has(RlCarNative.FLAG_ON_GROUND) && reset.up().y > 0.99, "reset upright: up " + reset.up());

			// The server sees the car where the client drives it.
			sp.getConnection().waitForServerboundPackets();
			double serverX = sp.getServer().computeOnServer(server ->
				server.getPlayerList().getPlayers().getFirst().getVehicle() instanceof CarEntity c ? c.getX() : Double.NaN);
			check(Math.abs(serverX - pose(ctx).x) < 1.5, "server follows the driver (server x " + serverX + ", client x " + pose(ctx).x + ")");

			// Get out; the server keeps simulating the car.
			input.pressKey(CarKeys.EXIT);
			ctx.waitTicks(10);
			check(!ctx.computeOnClient(mc -> ClientDriving.isDriving()), "client stopped driving");
			check(ctx.computeOnClient(mc -> mc.options.getCameraType()) == CameraType.FIRST_PERSON, "camera restored");
			check(sp.getServer().computeOnServer(server -> server.getPlayerList().getPlayers().getFirst().getVehicle() == null), "player is out");
			ctx.waitTicks(60);
			shot(ctx, "6-outside");
			boolean roadIntact = sp.getServer().computeOnServer(server -> {
				for (int x = -6; x <= ROAD_LENGTH; x++) {
					for (int z = -4; z <= 4; z++) {
						if (server.overworld().getBlockState(start.offset(x, -1, z)) != road(z)) {
							return false;
						}
					}
				}
				return true;
			});
			check(roadIntact, "no block was broken while driving");
			boolean carsSettled = sp.getServer().computeOnServer(server -> {
				for (var e : server.overworld().getEntities(RlCar.CAR, c -> true)) {
					if (e.getY() < start.getY()) {
						return false;
					}
				}
				return true;
			});
			check(carsSettled, "cars stay on the road after the driver left");

			// Showroom: every hitbox preset side by side, alternating teams, facing the camera.
			sp.getServer().runCommand("kill @e[type=rlcar:car]");
			sp.getServer().runOnServer(server -> {
				ServerLevel level = server.overworld();
				for (int i = 0; i < RlCarNative.PRESETS.length; i++) {
					Vec3 at = Vec3.atBottomCenterOf(start).add(4, 0.3, (i - 3) * 1.7);
					level.addFreshEntity(CarEntity.create(level, at, 90.0F, i, i % 2 == 0 ? CarEntity.BLUE : CarEntity.ORANGE));
				}
			});
			sp.getServer().runCommand("tp @p " + (start.getX() - 3.5) + " " + (start.getY() + 1.5) + " " + (start.getZ() + 0.5) + " -90 20");
			ctx.waitTicks(60);
			shot(ctx, "7-showroom");
		}
	}

	/** The RL Car sections of the Controls screen: Rocket League's key and controller bindings, in its order, with sensible conflicts. */
	private static void checkBindings(ClientGameTestContext ctx) {
		ctx.runOnClient(mc -> {
			List<KeyMapping> all = List.of(mc.options.keyMappings);
			KeyMapping.Category category = CarKeys.THROTTLE.getCategory();
			List<KeyMapping> section = all.stream().filter(k -> k.getCategory() == category).sorted().toList();
			check(section.equals(CarKeys.ALL), "RL Car section lists the bindings in Rocket League's order: " + section.stream().map(KeyMapping::getName).toList());
			check(CarKeys.canShareKey(CarKeys.THROTTLE, CarKeys.PITCH_DOWN), "Throttle and Pitch Down may share a key");
			check(CarKeys.canShareKey(CarKeys.AIR_ROLL, CarKeys.POWERSLIDE), "Air Roll and Powerslide may share a key");
			check(!CarKeys.canShareKey(CarKeys.BOOST, CarKeys.JUMP), "Boost and Jump on one key is a conflict");
			check(CarKeys.canShareKey(CarKeys.BOOST, mc.options.keyAttack), "Boost on Attack's button is fine (no attacking while driving)");
			check(CarKeys.canShareKey(mc.options.keyDrop, CarKeys.AIR_ROLL_LEFT), "Air Roll Left on Drop's key is fine");
			check(!CarKeys.canShareKey(CarKeys.BOOST, mc.options.keyChat), "Boost on the chat key is a conflict");
		});
		// The section as players see it (it is the last one, so scroll to the bottom).
		ctx.setScreen(() -> new KeyBindsScreen(null, Minecraft.getInstance().options));
		ctx.waitTicks(2);
		ctx.getInput().setCursorPos(200, 200);
		ctx.getInput().scroll(-10000);
		ctx.waitTicks(2);
		shot(ctx, "0-controls");

		// Controller section (no gamepad needed): rebinding, conflicts, Escape unbinds, Reset Keys.
		ctx.runOnClient(mc -> {
			PadBinds.resetAll();
			KeyBindsList list = (KeyBindsList) mc.gui.screen().children().stream().filter(c -> c instanceof KeyBindsList).findFirst().orElseThrow();
			check(list.children().stream().filter(e -> e instanceof PadBindEntry).count() == PadBinds.Action.values().length, "one controller row per car action");
			check(PadBinds.conflicts(PadBinds.Action.POWERSLIDE).isEmpty(), "Powerslide and Air Roll may share X");
			PadBinds.set(PadBinds.Action.BOOST, PadBinds.Input.RT);
			check(PadBinds.conflicts(PadBinds.Action.BOOST).equals(List.of(PadBinds.Action.THROTTLE)), "Boost on RT conflicts with Throttle");
			PadBinds.startCapture(PadBinds.Action.JUMP);
			((KeyBindsScreen) mc.gui.screen()).refreshKeybindLabels();
		});
		ctx.waitTicks(2);
		shot(ctx, "0-controls-controller");
		ctx.getInput().pressKey(InputConstants.KEY_ESCAPE);
		ctx.waitTicks(2);
		ctx.runOnClient(mc -> {
			check(mc.gui.screen() instanceof KeyBindsScreen, "Escape while waiting for a controller button keeps the screen open");
			check(PadBinds.capturing() == null && PadBinds.get(PadBinds.Action.JUMP) == PadBinds.Input.NONE, "Escape unbinds the controller button");
			PadBinds.resetAll();
			check(PadBinds.allDefault() && PadBinds.get(PadBinds.Action.BOOST) == PadBinds.Input.B, "controller bindings back to defaults");
		});
		ctx.setScreen(() -> null);
	}

	private static BlockState road(int z) {
		return Blocks.CONCRETE.pick(Math.abs(z) == 4 ? DyeColor.WHITE : DyeColor.GRAY).defaultBlockState();
	}

	private static void rebind(ClientGameTestContext ctx, KeyMapping mapping, InputConstants.Key key) {
		ctx.runOnClient(mc -> {
			mapping.setKey(key);
			KeyMapping.resetMapping();
		});
	}

	private static CarPose pose(ClientGameTestContext ctx) {
		CarPose p = ctx.computeOnClient(mc -> ClientDriving.pose() != null ? ClientDriving.pose().copy() : null);
		if (p == null) {
			throw new AssertionError("not driving a car");
		}
		return p;
	}

	private static void shot(ClientGameTestContext ctx, String name) {
		Path path = ctx.takeScreenshot("rlcar-" + name);
		RlCar.LOG.info("rlcar gametest: screenshot {}", path.toAbsolutePath());
	}

	private static void check(boolean ok, String what) {
		RlCar.LOG.info("rlcar gametest: {} {}", ok ? "ok  " : "FAIL", what);
		if (!ok) {
			throw new AssertionError(what);
		}
	}
}
