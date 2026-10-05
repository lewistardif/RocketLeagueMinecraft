package dev.rlcar;

import dev.rlcar.command.CarCommand;
import dev.rlcar.entity.BallEntity;
import dev.rlcar.entity.CarEntity;
import dev.rlcar.item.BallItem;
import dev.rlcar.item.CarItem;
import dev.rlcar.net.CarNet;
import dev.rlcar.physics.RlCarNative;
import net.fabricmc.api.ModInitializer;
import net.fabricmc.fabric.api.command.v2.CommandRegistrationCallback;
import net.fabricmc.fabric.api.creativetab.v1.CreativeModeTabEvents;
import net.fabricmc.fabric.api.entity.event.v1.ServerLivingEntityEvents;
import net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents;
import net.fabricmc.fabric.api.event.player.AttackBlockCallback;
import net.fabricmc.fabric.api.event.player.AttackEntityCallback;
import net.fabricmc.fabric.api.event.player.UseBlockCallback;
import net.fabricmc.fabric.api.event.player.UseEntityCallback;
import net.fabricmc.fabric.api.event.player.UseItemCallback;
import net.fabricmc.fabric.api.networking.v1.EntityTrackingEvents;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.tags.DamageTypeTags;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.damagesource.DamageTypes;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.MobCategory;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.CreativeModeTabs;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.Items;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Rocket League car physics in Minecraft (unofficial fan project).
 *
 * <p>The physics is the Rust core of this repository ({@code crates/rl_car_core}), called through
 * {@code crates/rl_car_ffi}. This mod only adapts it: block collision boxes in, car poses out.
 */
public final class RlCar implements ModInitializer {
	public static final String MOD_ID = "rlcar";
	public static final Logger LOG = LoggerFactory.getLogger(MOD_ID);

	public static final ResourceKey<EntityType<?>> CAR_KEY = ResourceKey.create(Registries.ENTITY_TYPE, id("car"));
	public static final EntityType<CarEntity> CAR = Registry.register(
		BuiltInRegistries.ENTITY_TYPE,
		CAR_KEY,
		EntityType.Builder.<CarEntity>of(CarEntity::new, MobCategory.MISC)
			// Roughly an Octane (1.2 x 0.84 x 0.36 blocks); the entity box only matters for
			// clicking and culling, the physics uses the real hitbox.
			.sized(1.25F, 0.6F)
			.clientTrackingRange(10)
			// Vanilla position sync is ignored by the client (cars send their own poses).
			.updateInterval(20)
			.build(CAR_KEY)
	);

	public static final ResourceKey<EntityType<?>> BALL_KEY = ResourceKey.create(Registries.ENTITY_TYPE, id("ball"));
	public static final EntityType<BallEntity> BALL = Registry.register(
		BuiltInRegistries.ENTITY_TYPE,
		BALL_KEY,
		EntityType.Builder.<BallEntity>of(BallEntity::new, MobCategory.MISC)
			// The entity box is the ball (Rocket League's radius, 91.25 uu).
			.sized(BallEntity.RADIUS * 2, BallEntity.RADIUS * 2)
			.clientTrackingRange(10)
			// Vanilla position sync is ignored by the client (balls send their own poses).
			.updateInterval(20)
			.build(BALL_KEY)
	);

	public static final ResourceKey<Item> CAR_ITEM_KEY = ResourceKey.create(Registries.ITEM, id("car"));
	public static final Item CAR_ITEM = Registry.register(
		BuiltInRegistries.ITEM, CAR_ITEM_KEY, new CarItem(new Item.Properties().setId(CAR_ITEM_KEY).stacksTo(1))
	);

	public static final ResourceKey<Item> BALL_ITEM_KEY = ResourceKey.create(Registries.ITEM, id("ball"));
	public static final Item BALL_ITEM = Registry.register(
		BuiltInRegistries.ITEM, BALL_ITEM_KEY, new BallItem(new Item.Properties().setId(BALL_ITEM_KEY).stacksTo(16))
	);

	public static Identifier id(String path) {
		return Identifier.fromNamespaceAndPath(MOD_ID, path);
	}

	@Override
	public void onInitialize() {
		RlCarNative.init();
		CarNet.init();
		CommandRegistrationCallback.EVENT.register((dispatcher, context, selection) -> CarCommand.register(dispatcher));
		CreativeModeTabEvents.modifyOutputEvent(CreativeModeTabs.TOOLS_AND_UTILITIES).register(out -> {
			out.insertAfter(Items.MINECART, CAR_ITEM);
			out.insertAfter(CAR_ITEM, BALL_ITEM);
		});

		EntityTrackingEvents.START_TRACKING.register((entity, player) -> {
			if (entity instanceof CarEntity car) {
				car.onStartTracking(player);
			} else if (entity instanceof BallEntity ball) {
				ball.onStartTracking(player);
			}
		});

		// The driver's player entity sits inside the car: its head can poke into a ceiling the
		// car drives on, and it must not suffocate or get hurt by the car's landings.
		ServerLivingEntityEvents.ALLOW_DAMAGE.register((entity, source, amount) ->
			!(entity.getVehicle() instanceof CarEntity && (source.is(DamageTypes.IN_WALL) || source.is(DamageTypeTags.IS_FALL))));

		// Hands are on the wheel: no block breaking, placing or hitting while driving.
		AttackBlockCallback.EVENT.register((player, level, hand, pos, dir) -> driving(player) ? InteractionResult.FAIL : InteractionResult.PASS);
		AttackEntityCallback.EVENT.register((player, level, hand, entity, hit) -> driving(player) ? InteractionResult.FAIL : InteractionResult.PASS);
		UseBlockCallback.EVENT.register((player, level, hand, hit) -> driving(player) ? InteractionResult.FAIL : InteractionResult.PASS);
		UseEntityCallback.EVENT.register((player, level, hand, entity, hit) -> driving(player) ? InteractionResult.FAIL : InteractionResult.PASS);
		UseItemCallback.EVENT.register((player, level, hand) -> driving(player) ? InteractionResult.FAIL : InteractionResult.PASS);

		// Development: `gradlew runSelftest` starts a dedicated server, runs /rlcar selftest, stops.
		if (Boolean.getBoolean("rlcar.selftest")) {
			ServerLifecycleEvents.SERVER_STARTED.register(server -> {
				server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), "rlcar selftest");
				server.halt(false);
			});
		}

		LOG.info("RL Car loaded (native physics ABI {})", RlCarNative.ABI_VERSION);
	}

	public static boolean driving(Player player) {
		return player.getVehicle() instanceof CarEntity;
	}
}
