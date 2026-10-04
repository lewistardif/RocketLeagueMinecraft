package dev.rlcar.item;

import dev.rlcar.entity.BallEntity;
import net.minecraft.core.Direction;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.phys.Vec3;

/** Places a Rocket League ball on the clicked block. */
public class BallItem extends Item {
	public BallItem(Properties properties) {
		super(properties);
	}

	@Override
	public InteractionResult useOn(UseOnContext context) {
		Vec3 on = context.getClickedFace() == Direction.UP
			? context.getClickLocation()
			: Vec3.atBottomCenterOf(context.getClickedPos().relative(context.getClickedFace()));
		if (context.getLevel() instanceof ServerLevel level) {
			// Spawn slightly above the surface; the ball drops onto it.
			level.addFreshEntity(BallEntity.create(level, on.add(0, BallEntity.RADIUS + 0.2, 0)));
			level.gameEvent(context.getPlayer(), GameEvent.ENTITY_PLACE, context.getClickedPos());
		}
		context.getItemInHand().consume(1, context.getPlayer());
		return InteractionResult.SUCCESS;
	}
}
