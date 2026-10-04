package dev.rlcar.client;

import dev.rlcar.physics.BallPose;
import net.minecraft.client.renderer.entity.state.EntityRenderState;
import org.joml.Quaternionf;
import org.jspecify.annotations.Nullable;

public class BallRenderState extends EntityRenderState {
	public @Nullable BallPose pose;
	public final Quaternionf rotation = new Quaternionf();
	/** Position along the field (RL Y, uu from the kickoff spot / 1024), for the team-coloured lights. */
	public float fieldY;

	// The markers (BallMarker).
	public boolean markers;
	public boolean line;
	/** The ground decal's quads, 4 vertices of (x, y, z, u, v) relative to the ball centre. */
	public float[] decal = new float[80];
	public int decalQuads;
	/** The ball's height over the ground under it, 0..1024 uu as 0..1. */
	public float altitude;
}
