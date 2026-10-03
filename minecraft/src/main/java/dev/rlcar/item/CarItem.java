package dev.rlcar.item;

import dev.rlcar.entity.CarEntity;
import net.minecraft.core.Direction;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.phys.Vec3;

/** Places a car on the clicked block, facing where the player looks. */
public class CarItem extends Item {
	public CarItem(Properties properties) {
		super(properties);
	}

	@Override
	public InteractionResult useOn(UseOnContext context) {
		Vec3 pos = context.getClickedFace() == Direction.UP
			? context.getClickLocation()
			: Vec3.atBottomCenterOf(context.getClickedPos().relative(context.getClickedFace()));
		if (context.getLevel() instanceof ServerLevel level) {
			int color = context.getPlayer() != null && context.getPlayer().isSecondaryUseActive() ? CarEntity.ORANGE : CarEntity.BLUE;
			// Spawn slightly above the surface; the car drops onto its wheels.
			CarEntity car = CarEntity.create(level, pos.add(0, 0.3, 0), context.getRotation(), 0, color);
			level.addFreshEntity(car);
			level.gameEvent(context.getPlayer(), GameEvent.ENTITY_PLACE, context.getClickedPos());
		}
		context.getItemInHand().consume(1, context.getPlayer());
		return InteractionResult.SUCCESS;
	}
}
