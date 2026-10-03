package dev.rlcar.entity;

import dev.rlcar.RlCar;
import dev.rlcar.net.CarNet;
import dev.rlcar.physics.CarControls;
import dev.rlcar.physics.CarPose;
import dev.rlcar.physics.CarSim;
import dev.rlcar.physics.RlCarNative;
import dev.rlcar.physics.Space;
import java.util.Base64;
import net.fabricmc.fabric.api.networking.v1.PlayerLookup;
import net.fabricmc.fabric.api.networking.v1.ServerPlayNetworking;
import net.minecraft.core.BlockPos;
import net.minecraft.network.syncher.EntityDataAccessor;
import net.minecraft.network.syncher.EntityDataSerializers;
import net.minecraft.network.syncher.SynchedEntityData;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityDimensions;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.InterpolationHandler;
import net.minecraft.world.entity.PositionPath;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.entity.vehicle.VehicleEntity;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.storage.ValueInput;
import net.minecraft.world.level.storage.ValueOutput;
import net.minecraft.world.phys.Vec3;
import org.joml.Vector3f;
import org.jspecify.annotations.Nullable;

/**
 * A Rocket League car. The entity's position is the car's centre of mass; its orientation lives
 * in the physics state (Minecraft entities only have yaw and pitch).
 *
 * <p>Server side it owns the authoritative {@link CarSim}. While a player drives, their client
 * simulates and the server adopts the states it sends ({@link #acceptDriverState}); otherwise the
 * server simulates the car itself (it rolls, falls and settles) and puts it to sleep once it
 * rests, waking it when blocks around it change.
 */
public class CarEntity extends VehicleEntity {
	private static final EntityDataAccessor<Integer> DATA_PRESET = SynchedEntityData.defineId(CarEntity.class, EntityDataSerializers.INT);
	private static final EntityDataAccessor<Integer> DATA_COLOR = SynchedEntityData.defineId(CarEntity.class, EntityDataSerializers.INT);

	public static final int BLUE = 0x2F6FE0;
	public static final int ORANGE = 0xF08A1C;

	/** Ticks of the core per Minecraft tick (120 Hz / 20 Hz). */
	private static final int SUBSTEPS = 6;
	private static final int SLEEP_AFTER_TICKS = 40;

	// Server.
	private @Nullable CarSim sim;
	private byte @Nullable [] savedState;
	private BlockPos savedOrigin = BlockPos.ZERO;
	private @Nullable CarPose lastPose;
	private int restTicks;
	private boolean asleep;
	private int sleepHash;

	// Client.
	private @Nullable CarPose previousPose;
	private @Nullable CarPose targetPose;
	private int targetTick;
	/** Set every frame by the local driver's simulation; overrides the server's poses. */
	public @Nullable CarPose localPose;
	/** Wheel roll angle for rendering, and the render age it was last advanced at. */
	public float wheelSpin;
	public float wheelSpinAge;

	public CarEntity(EntityType<? extends CarEntity> type, Level level) {
		super(type, level);
	}

	public static CarEntity create(Level level, Vec3 pos, float yaw, int preset, int color) {
		CarEntity car = new CarEntity(RlCar.CAR, level);
		car.setPos(pos);
		car.setYRot(yaw);
		car.entityData.set(DATA_PRESET, Math.floorMod(preset, RlCarNative.PRESETS.length));
		car.entityData.set(DATA_COLOR, color);
		return car;
	}

	@Override
	protected void defineSynchedData(SynchedEntityData.Builder builder) {
		super.defineSynchedData(builder);
		builder.define(DATA_PRESET, 0);
		builder.define(DATA_COLOR, BLUE);
	}

	public int preset() {
		return this.entityData.get(DATA_PRESET);
	}

	public int color() {
		return this.entityData.get(DATA_COLOR);
	}

	public @Nullable Player driver() {
		return this.getFirstPassenger() instanceof Player p ? p : null;
	}

	// ------------------------------------------------------------------------------- ticking

	@Override
	public void tick() {
		super.tick();
		if (this.getHurtTime() > 0) {
			this.setHurtTime(this.getHurtTime() - 1);
		}
		if (this.getDamage() > 0.0F) {
			this.setDamage(this.getDamage() - 1.0F);
		}
		if (this.level() instanceof ServerLevel level) {
			this.serverTick(level);
		}
	}

	private CarSim sim() {
		if (this.sim == null) {
			this.sim = new CarSim(this.preset(), this.blockPosition());
			if (this.savedState == null || !this.sim.load(this.savedState, this.savedOrigin)) {
				this.sim.resetAt(this.position(), this.getYRot());
			}
			this.savedState = null;
		}
		return this.sim;
	}

	private void serverTick(ServerLevel level) {
		CarSim sim = this.sim();
		if (this.driver() != null) {
			// The driver's client simulates; poses arrive through acceptDriverState.
			this.wake();
			return;
		}
		if (this.asleep) {
			if (this.tickCount % 10 == 0 && sim.geometryHash(level) != this.sleepHash) {
				this.wake();
			}
			return;
		}
		Vec3 before = sim.position();
		sim.step(level, SUBSTEPS, CarControls.IDLE);
		CarPose pose = sim.pose(1.0F, new CarPose());
		this.applyPose(pose);
		this.broadcast(pose, null);

		boolean resting = pose.has(RlCarNative.FLAG_ON_GROUND) && before.distanceToSqr(pose.position()) < 1.0E-6;
		this.restTicks = resting ? this.restTicks + 1 : 0;
		if (this.restTicks > SLEEP_AFTER_TICKS) {
			this.asleep = true;
			this.sleepHash = sim.geometryHash(level);
		}
	}

	private void wake() {
		this.asleep = false;
		this.restTicks = 0;
	}

	private void applyPose(CarPose pose) {
		this.lastPose = pose;
		this.setPos(pose.x, pose.y, pose.z);
		Vector3f f = pose.forward();
		if (f.x * f.x + f.z * f.z > 1.0E-4F) {
			this.setYRot(Space.yawOfMc(f.x, f.z));
		}
	}

	private void broadcast(CarPose pose, @Nullable ServerPlayer except) {
		CarNet.Pose msg = new CarNet.Pose(this.getId(), pose);
		for (ServerPlayer p : PlayerLookup.tracking(this)) {
			if (p != except) {
				ServerPlayNetworking.send(p, msg);
			}
		}
	}

	/** A player started watching this car: send them where it is right away. */
	public void onStartTracking(ServerPlayer player) {
		if (this.lastPose != null) {
			ServerPlayNetworking.send(player, new CarNet.Pose(this.getId(), this.lastPose));
		}
	}

	/** A state streamed by the driving client. */
	public void acceptDriverState(ServerPlayer player, BlockPos origin, byte[] state) {
		if (this.driver() != player || !this.sim().load(state, origin)) {
			return;
		}
		CarPose pose = this.sim.pose(1.0F, new CarPose());
		this.applyPose(pose);
		this.broadcast(pose, player);
	}

	// ---------------------------------------------------------------------------- riding

	@Override
	public InteractionResult interact(Player player, InteractionHand hand, Vec3 location) {
		InteractionResult result = super.interact(player, hand, location);
		if (result != InteractionResult.PASS) {
			return result;
		}
		if (player.isSecondaryUseActive() || !this.getPassengers().isEmpty()) {
			return InteractionResult.PASS;
		}
		return !this.level().isClientSide() && !player.startRiding(this) ? InteractionResult.PASS : InteractionResult.SUCCESS;
	}

	@Override
	protected boolean canAddPassenger(Entity passenger) {
		return passenger instanceof Player && this.getPassengers().isEmpty();
	}

	@Override
	protected void addPassenger(Entity passenger) {
		super.addPassenger(passenger);
		if (passenger instanceof ServerPlayer player) {
			CarSim sim = this.sim();
			ServerPlayNetworking.send(player, new CarNet.DriverStart(this.getId(), sim.origin().asLong(), sim.save()));
		}
	}

	@Override
	protected void removePassenger(Entity passenger) {
		super.removePassenger(passenger);
		if (!this.level().isClientSide()) {
			// The server takes over from the driver's last state.
			this.wake();
			passenger.resetFallDistance();
		}
	}

	@Override
	protected Vec3 getPassengerAttachmentPoint(Entity passenger, EntityDimensions dimensions, float scale) {
		return Vec3.ZERO;
	}

	@Override
	public boolean isPickable() {
		return !this.isRemoved();
	}

	// ------------------------------------------------------------------------------ client

	@Override
	protected InterpolationHandler createInterpolationHandler() {
		// Client: ignore vanilla position updates; cars are placed by their own pose packets
		// (or by the local driver's simulation).
		return new InterpolationHandler.NoOpInterpolationHandler() {
			@Override
			public boolean interpolateTo(@Nullable PositionPath position, float yRot, float xRot, boolean hasRotation) {
				return CarEntity.this.level().isClientSide();
			}
		};
	}

	/** Client: a pose from the server. Rendering blends from the previous one over one tick. */
	public void receivePose(CarPose pose) {
		this.previousPose = this.targetPose != null ? this.targetPose : pose;
		this.targetPose = pose;
		this.targetTick = this.tickCount;
		if (this.localPose == null) {
			this.setPos(pose.x, pose.y, pose.z);
		}
	}

	/** Client: the pose to draw this frame, or null before the first one arrived. */
	public @Nullable CarPose renderPose(float partialTick) {
		if (this.localPose != null) {
			return this.localPose;
		}
		if (this.targetPose == null || this.previousPose == null) {
			return this.targetPose;
		}
		float t = Math.clamp(this.tickCount - this.targetTick + partialTick, 0.0F, 1.0F);
		return CarPose.lerp(this.previousPose, this.targetPose, t);
	}

	// ------------------------------------------------------------------------- lifecycle

	@Override
	protected Item getDropItem() {
		return RlCar.CAR_ITEM;
	}

	@Override
	public ItemStack getPickResult() {
		return new ItemStack(RlCar.CAR_ITEM);
	}

	@Override
	public void onRemoval(RemovalReason reason) {
		super.onRemoval(reason);
		if (this.sim != null) {
			this.sim.close();
			this.sim = null;
		}
	}

	@Override
	protected void addAdditionalSaveData(ValueOutput output) {
		output.putInt("Preset", this.preset());
		output.putInt("Color", this.color());
		if (this.sim != null) {
			output.putLong("Origin", this.sim.origin().asLong());
			output.putString("State", Base64.getEncoder().encodeToString(this.sim.save()));
		} else if (this.savedState != null) {
			output.putLong("Origin", this.savedOrigin.asLong());
			output.putString("State", Base64.getEncoder().encodeToString(this.savedState));
		}
	}

	@Override
	protected void readAdditionalSaveData(ValueInput input) {
		this.entityData.set(DATA_PRESET, Math.floorMod(input.getIntOr("Preset", 0), RlCarNative.PRESETS.length));
		this.entityData.set(DATA_COLOR, input.getIntOr("Color", BLUE));
		this.savedOrigin = BlockPos.of(input.getLongOr("Origin", BlockPos.ZERO.asLong()));
		this.savedState = input.getString("State").map(s -> {
			try {
				return Base64.getDecoder().decode(s);
			} catch (IllegalArgumentException e) {
				return null;
			}
		}).orElse(null);
	}
}
