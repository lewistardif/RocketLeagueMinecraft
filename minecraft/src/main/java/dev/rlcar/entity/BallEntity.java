package dev.rlcar.entity;

import dev.rlcar.RlCar;
import dev.rlcar.net.CarNet;
import dev.rlcar.physics.BallPose;
import dev.rlcar.physics.BallSim;
import dev.rlcar.physics.RlCarNative;
import dev.rlcar.physics.Space;
import net.fabricmc.fabric.api.networking.v1.PlayerLookup;
import net.fabricmc.fabric.api.networking.v1.ServerPlayNetworking;
import net.minecraft.core.BlockPos;
import net.minecraft.network.syncher.SynchedEntityData;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.InterpolationHandler;
import net.minecraft.world.entity.PositionPath;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.storage.ValueInput;
import net.minecraft.world.level.storage.ValueOutput;
import net.minecraft.world.phys.Vec3;
import org.joml.Quaternionf;
import org.jspecify.annotations.Nullable;

/**
 * Rocket League's ball. The entity's position is the bottom of the ball (so its box is the ball);
 * the physics works with its centre.
 *
 * <p>Server side it owns the authoritative {@link BallSim}. When a driver's car comes near, the
 * server lends the ball to that driver's client, which steps it in one solve with the car (so hits
 * and dribbles respond within the frame, like the car itself) and streams its state back. The
 * server takes it back when the car leaves, the driver gets out or stops sending, and then
 * simulates it itself, putting it to sleep once it rests.
 *
 * <p>Players on foot can hit the ball (a kick in the direction they look); sneak and hit to pick
 * it up.
 */
public class BallEntity extends Entity {
	public static final float RADIUS = RlCarNative.BALL_RADIUS / Space.UU_PER_BLOCK;
	/** Lend the ball to a driver whose car is this close to its centre (blocks). */
	private static final double GRAB_DISTANCE = 8;
	/** Take it back once their car is this far. */
	private static final double RELEASE_DISTANCE = 12;
	/** Take it back if the owner sent nothing for this many ticks. */
	private static final int OWNER_TIMEOUT_TICKS = 20;
	/** Ticks of the core per Minecraft tick (120 Hz / 20 Hz). */
	private static final int SUBSTEPS = 6;
	private static final int SLEEP_AFTER_TICKS = 40;
	/** Moving less than this per tick (blocks) counts as resting. */
	private static final double REST_DISTANCE = 0.005;
	/** A kick from a player on foot (blocks/s along the look direction, plus a little lift). */
	private static final double KICK_SPEED = 12;
	private static final double KICK_LIFT = 4;

	// Server.
	private @Nullable BallSim sim;
	private float @Nullable [] savedState;
	private BlockPos savedOrigin = BlockPos.ZERO;
	private @Nullable BallPose lastPose;
	private @Nullable ServerPlayer owner;
	private int ownerSilentTicks;
	private int restTicks;
	private int sleepHash;

	// Client.
	private @Nullable BallPose previousPose;
	private @Nullable BallPose targetPose;
	private int targetTick;
	/** Set every frame while the local player's client simulates this ball; overrides the server's poses. */
	public @Nullable BallPose localPose;
	/** Visual rotation, integrated from the spin while rendering, and the render age it was last advanced at. */
	public final Quaternionf rotation = new Quaternionf();
	public float rotationAge;

	public BallEntity(EntityType<? extends BallEntity> type, Level level) {
		super(type, level);
	}

	/** A ball centred at {@code center}, at rest (it drops onto whatever is below). */
	public static BallEntity create(Level level, Vec3 center) {
		BallEntity ball = new BallEntity(RlCar.BALL, level);
		ball.setPos(center.subtract(0, RADIUS, 0));
		return ball;
	}

	/** Server: true while a driver's client simulates this ball. */
	public boolean lent() {
		return this.owner != null;
	}

	public Vec3 center() {
		return this.position().add(0, RADIUS, 0);
	}

	@Override
	protected void defineSynchedData(SynchedEntityData.Builder builder) {
	}

	// ------------------------------------------------------------------------------- ticking

	@Override
	public void tick() {
		super.tick();
		if (this.level() instanceof ServerLevel level) {
			this.serverTick(level);
		}
	}

	private BallSim sim() {
		if (this.sim == null) {
			this.sim = new BallSim(this.blockPosition());
			if (this.savedState == null || !this.sim.load(this.savedState, this.savedOrigin)) {
				this.sim.resetAt(this.center(), Vec3.ZERO);
			}
			this.savedState = null;
		}
		return this.sim;
	}

	private void serverTick(ServerLevel level) {
		BallSim sim = this.sim();
		this.updateOwner(level);
		if (this.owner != null) {
			// The owner's client simulates; states arrive through acceptOwnerState.
			this.ownerSilentTicks++;
			return;
		}
		if (sim.asleep()) {
			if (this.tickCount % 10 != 0 || sim.geometryHash(level) == this.sleepHash) {
				return;
			}
			sim.wake();
			this.restTicks = 0;
		}
		Vec3 before = sim.position();
		sim.step(level, SUBSTEPS);
		BallPose pose = sim.pose(1.0F, new BallPose());
		if (pose.y < level.getMinY() - 64) {
			this.discard();
			return;
		}
		boolean resting = before.distanceToSqr(pose.position()) < REST_DISTANCE * REST_DISTANCE;
		this.restTicks = resting ? this.restTicks + 1 : 0;
		if (this.restTicks > SLEEP_AFTER_TICKS) {
			sim.sleep();
			this.sleepHash = sim.geometryHash(level);
			pose = sim.pose(1.0F, pose);
		}
		this.applyPose(pose);
		this.broadcast(pose, null);
	}

	/** Lends the ball to the nearest driver, or takes it back from its owner. */
	private void updateOwner(ServerLevel level) {
		Vec3 center = this.sim().position();
		if (this.owner != null && !this.keeps(this.owner, center)) {
			this.release();
		}
		if (this.owner != null) {
			return;
		}
		ServerPlayer best = null;
		double bestDistance = GRAB_DISTANCE * GRAB_DISTANCE;
		for (ServerPlayer p : level.players()) {
			if (p.getVehicle() instanceof CarEntity car && car.driver() == p) {
				double d = car.position().distanceToSqr(center);
				if (d < bestDistance && !this.ownsAnotherBall(level, p)) {
					best = p;
					bestDistance = d;
				}
			}
		}
		if (best != null) {
			this.owner = best;
			this.ownerSilentTicks = 0;
			BallSim sim = this.sim();
			ServerPlayNetworking.send(best, new CarNet.BallOwn(this.getId(), sim.origin().asLong(), sim.save()));
		}
	}

	private boolean keeps(ServerPlayer p, Vec3 center) {
		return !p.isRemoved() && p.level() == this.level() && this.ownerSilentTicks < OWNER_TIMEOUT_TICKS
			&& p.getVehicle() instanceof CarEntity car && car.position().distanceToSqr(center) < RELEASE_DISTANCE * RELEASE_DISTANCE;
	}

	/** A client steps at most one ball with its car (the core solves one car with one ball). */
	private boolean ownsAnotherBall(ServerLevel level, ServerPlayer p) {
		return !level.getEntitiesOfClass(BallEntity.class, p.getBoundingBox().inflate(RELEASE_DISTANCE + 2), b -> b != this && b.owner == p).isEmpty();
	}

	/** The server continues from the owner's last state. */
	private void release() {
		ServerPlayer old = this.owner;
		this.owner = null;
		this.restTicks = 0;
		if (old != null && !old.hasDisconnected()) {
			ServerPlayNetworking.send(old, new CarNet.BallRelease(this.getId()));
		}
	}

	/** A state streamed by the client that simulates this ball. */
	public void acceptOwnerState(ServerPlayer player, BlockPos origin, float[] state) {
		if (player != this.owner || !this.sim().load(state, origin)) {
			return;
		}
		this.ownerSilentTicks = 0;
		BallPose pose = this.sim.pose(1.0F, new BallPose());
		this.applyPose(pose);
		this.broadcast(pose, player);
	}

	private void applyPose(BallPose pose) {
		this.lastPose = pose;
		this.setPos(pose.x, pose.y - RADIUS, pose.z);
	}

	private void broadcast(BallPose pose, @Nullable ServerPlayer except) {
		CarNet.BallPoseMsg msg = new CarNet.BallPoseMsg(this.getId(), pose);
		for (ServerPlayer p : PlayerLookup.tracking(this)) {
			if (p != except) {
				ServerPlayNetworking.send(p, msg);
			}
		}
	}

	/** A player started watching this ball: send them where it is right away. */
	public void onStartTracking(ServerPlayer player) {
		if (this.lastPose != null) {
			ServerPlayNetworking.send(player, new CarNet.BallPoseMsg(this.getId(), this.lastPose));
		}
	}

	// ------------------------------------------------------------------------------ hitting

	@Override
	public boolean hurtServer(ServerLevel level, DamageSource source, float amount) {
		if (!(source.getEntity() instanceof Player player) || this.isRemoved()) {
			return false;
		}
		if (player.isSecondaryUseActive()) {
			if (!player.getAbilities().instabuild) {
				this.spawnAtLocation(level, new ItemStack(RlCar.BALL_ITEM));
			}
			this.discard();
			return true;
		}
		if (this.owner != null) {
			return false; // a driver's client is playing it
		}
		Vec3 look = player.getLookAngle();
		this.sim().push(new Vec3(look.x * KICK_SPEED, Math.max(0, look.y) * KICK_SPEED + KICK_LIFT, look.z * KICK_SPEED));
		this.restTicks = 0;
		level.playSound(null, this.getX(), this.getY(), this.getZ(), SoundEvents.SLIME_BLOCK_HIT, SoundSource.NEUTRAL, 1.0F, 0.6F);
		return true;
	}

	@Override
	public boolean isPickable() {
		return !this.isRemoved();
	}

	// ------------------------------------------------------------------------------ client

	@Override
	protected InterpolationHandler createInterpolationHandler() {
		// Client: ignore vanilla position updates; balls are placed by their own pose packets
		// (or by the local owner's simulation).
		return new InterpolationHandler.NoOpInterpolationHandler() {
			@Override
			public boolean interpolateTo(@Nullable PositionPath position, float yRot, float xRot, boolean hasRotation) {
				return BallEntity.this.level().isClientSide();
			}
		};
	}

	/** Client: a pose from the server. Rendering blends from the previous one over one tick. */
	public void receivePose(BallPose pose) {
		this.previousPose = this.targetPose != null ? this.targetPose : pose;
		this.targetPose = pose;
		this.targetTick = this.tickCount;
		if (this.localPose == null) {
			this.setPos(pose.x, pose.y - RADIUS, pose.z);
		}
	}

	/** Client: the pose to draw this frame, or null before the first one arrived. */
	public @Nullable BallPose renderPose(float partialTick) {
		if (this.localPose != null) {
			return this.localPose;
		}
		if (this.targetPose == null || this.previousPose == null) {
			return this.targetPose;
		}
		float t = Math.clamp(this.tickCount - this.targetTick + partialTick, 0.0F, 1.0F);
		return BallPose.lerp(this.previousPose, this.targetPose, t);
	}

	// ------------------------------------------------------------------------- lifecycle

	@Override
	public ItemStack getPickResult() {
		return new ItemStack(RlCar.BALL_ITEM);
	}

	@Override
	public void onRemoval(RemovalReason reason) {
		super.onRemoval(reason);
		if (this.owner != null) {
			this.release();
		}
		if (this.sim != null) {
			this.sim.close();
			this.sim = null;
		}
	}

	@Override
	protected void addAdditionalSaveData(ValueOutput output) {
		float[] state = this.sim != null ? this.sim.save() : this.savedState;
		if (state != null) {
			output.putLong("Origin", (this.sim != null ? this.sim.origin() : this.savedOrigin).asLong());
			int[] bits = new int[state.length];
			for (int i = 0; i < state.length; i++) {
				bits[i] = Float.floatToRawIntBits(state[i]);
			}
			output.putIntArray("State", bits);
		}
	}

	@Override
	protected void readAdditionalSaveData(ValueInput input) {
		this.savedOrigin = BlockPos.of(input.getLongOr("Origin", BlockPos.ZERO.asLong()));
		this.savedState = input.getIntArray("State").filter(b -> b.length == RlCarNative.BALL_POSE_FLOATS).map(bits -> {
			float[] s = new float[bits.length];
			for (int i = 0; i < bits.length; i++) {
				s[i] = Float.intBitsToFloat(bits[i]);
			}
			return s;
		}).orElse(null);
	}
}
