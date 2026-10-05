package dev.rlcar.client;

import dev.rlcar.physics.CarPose;
import net.minecraft.client.renderer.entity.state.EntityRenderState;
import org.jspecify.annotations.Nullable;

public class CarRenderState extends EntityRenderState {
	public @Nullable CarPose pose;
	public int preset;
	/** The car entity (its effects are drawn with it). */
	public int carId;
	public int color;
	/** Hitbox in blocks: length, width, height, then the box centre offset forward, right, up. */
	public float[] hitbox = new float[6];
	/** Wheel roll angle (radians). */
	public float wheelSpin;
	/** The game's boost flame cones to draw (model space), or null. */
	public RlModels.@Nullable Model boostCones;
	/** Draw the simple boost flame instead. */
	public boolean simpleFlame;
	/** Boost smoke particles ({@link RlBoost}'s snapshot layout) and their count. */
	public float[] smoke = new float[0];
	public int smokeCount;
}
