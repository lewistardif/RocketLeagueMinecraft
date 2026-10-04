package dev.rlcar.client;

import dev.rlcar.physics.BallPose;
import net.minecraft.client.renderer.entity.state.EntityRenderState;
import org.joml.Quaternionf;
import org.jspecify.annotations.Nullable;

public class BallRenderState extends EntityRenderState {
	public @Nullable BallPose pose;
	public final Quaternionf rotation = new Quaternionf();
}
