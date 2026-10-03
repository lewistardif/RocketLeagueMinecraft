package dev.rlcar.client;

import dev.rlcar.physics.CarPose;
import net.minecraft.client.renderer.entity.state.EntityRenderState;
import org.jspecify.annotations.Nullable;

public class CarRenderState extends EntityRenderState {
	public @Nullable CarPose pose;
	public int preset;
	public int color;
	/** Hitbox in blocks: length, width, height, then the box centre offset forward, right, up. */
	public float[] hitbox = new float[6];
	/** Wheel roll angle (radians). */
	public float wheelSpin;
}
