package dev.rlcar.net;

import dev.rlcar.RlCar;
import dev.rlcar.entity.BallEntity;
import dev.rlcar.entity.CarEntity;
import dev.rlcar.physics.BallPose;
import dev.rlcar.physics.CarPose;
import dev.rlcar.physics.RlCarNative;
import io.netty.buffer.ByteBuf;
import net.fabricmc.fabric.api.networking.v1.PayloadTypeRegistry;
import net.fabricmc.fabric.api.networking.v1.ServerPlayNetworking;
import net.minecraft.core.BlockPos;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.resources.Identifier;
import net.minecraft.world.phys.Vec3;

/**
 * Networking. The player driving a car simulates it on their own client (so steering, jumps and
 * dodges respond within the frame) and streams its full state to the server, which relays the
 * pose to everyone else. Cars nobody drives are simulated by the server.
 *
 * <p>Balls work the same way: the server lends a ball to the player whose car is nearest
 * ({@link BallOwn}); their client steps it together with their car and streams its state back
 * ({@link BallState}) until the server takes it back ({@link BallRelease}). Otherwise the server
 * simulates it. Either way the server relays its pose ({@link BallPoseMsg}).
 */
public final class CarNet {
	private CarNet() {
	}

	private static <T extends CustomPacketPayload> CustomPacketPayload.Type<T> payloadType(String path) {
		return new CustomPacketPayload.Type<>(Identifier.fromNamespaceAndPath(RlCar.MOD_ID, path));
	}

	/** Driver -> server, every client tick: the car's full simulation state. */
	public record DriveState(int entityId, long origin, byte[] state) implements CustomPacketPayload {
		public static final Type<DriveState> TYPE = payloadType("drive_state");
		public static final StreamCodec<RegistryFriendlyByteBuf, DriveState> CODEC = StreamCodec.composite(
			ByteBufCodecs.VAR_INT, DriveState::entityId,
			ByteBufCodecs.LONG, DriveState::origin,
			ByteBufCodecs.BYTE_ARRAY, DriveState::state,
			DriveState::new
		);

		@Override
		public Type<? extends CustomPacketPayload> type() {
			return TYPE;
		}
	}

	/** Driver -> server: get out. */
	public record ExitCar() implements CustomPacketPayload {
		public static final Type<ExitCar> TYPE = payloadType("exit_car");
		public static final StreamCodec<RegistryFriendlyByteBuf, ExitCar> CODEC = StreamCodec.unit(new ExitCar());

		@Override
		public Type<? extends CustomPacketPayload> type() {
			return TYPE;
		}
	}

	/** Server -> new driver: the car's current state, to continue simulating from. */
	public record DriverStart(int entityId, long origin, byte[] state) implements CustomPacketPayload {
		public static final Type<DriverStart> TYPE = payloadType("driver_start");
		public static final StreamCodec<RegistryFriendlyByteBuf, DriverStart> CODEC = StreamCodec.composite(
			ByteBufCodecs.VAR_INT, DriverStart::entityId,
			ByteBufCodecs.LONG, DriverStart::origin,
			ByteBufCodecs.BYTE_ARRAY, DriverStart::state,
			DriverStart::new
		);

		@Override
		public Type<? extends CustomPacketPayload> type() {
			return TYPE;
		}
	}

	/** Server -> watching clients: where a car is and what it is doing. */
	public record Pose(int entityId, CarPose pose) implements CustomPacketPayload {
		public static final Type<Pose> TYPE = payloadType("pose");
		public static final StreamCodec<RegistryFriendlyByteBuf, Pose> CODEC = StreamCodec.composite(
			ByteBufCodecs.VAR_INT, Pose::entityId,
			CarPose.STREAM_CODEC, Pose::pose,
			Pose::new
		);

		@Override
		public Type<? extends CustomPacketPayload> type() {
			return TYPE;
		}
	}

	/** A ball's native state: position, velocity, angular velocity (RL space, relative to an origin). */
	private static final StreamCodec<ByteBuf, float[]> BALL_STATE = StreamCodec.of(
		(buf, a) -> {
			for (int i = 0; i < RlCarNative.BALL_POSE_FLOATS; i++) {
				buf.writeFloat(a[i]);
			}
		},
		buf -> {
			float[] a = new float[RlCarNative.BALL_POSE_FLOATS];
			for (int i = 0; i < a.length; i++) {
				a[i] = buf.readFloat();
			}
			return a;
		}
	);

	/** Server -> player: simulate this ball together with your car, starting from this state. */
	public record BallOwn(int entityId, long origin, float[] state) implements CustomPacketPayload {
		public static final Type<BallOwn> TYPE = payloadType("ball_own");
		public static final StreamCodec<RegistryFriendlyByteBuf, BallOwn> CODEC = StreamCodec.composite(
			ByteBufCodecs.VAR_INT, BallOwn::entityId,
			ByteBufCodecs.LONG, BallOwn::origin,
			BALL_STATE, BallOwn::state,
			BallOwn::new
		);

		@Override
		public Type<? extends CustomPacketPayload> type() {
			return TYPE;
		}
	}

	/** Server -> player: stop simulating this ball (the server continues from your last state). */
	public record BallRelease(int entityId) implements CustomPacketPayload {
		public static final Type<BallRelease> TYPE = payloadType("ball_release");
		public static final StreamCodec<RegistryFriendlyByteBuf, BallRelease> CODEC = StreamCodec.composite(
			ByteBufCodecs.VAR_INT, BallRelease::entityId,
			BallRelease::new
		);

		@Override
		public Type<? extends CustomPacketPayload> type() {
			return TYPE;
		}
	}

	/** Ball owner -> server, every client tick: the ball's state. */
	public record BallState(int entityId, long origin, float[] state) implements CustomPacketPayload {
		public static final Type<BallState> TYPE = payloadType("ball_state");
		public static final StreamCodec<RegistryFriendlyByteBuf, BallState> CODEC = StreamCodec.composite(
			ByteBufCodecs.VAR_INT, BallState::entityId,
			ByteBufCodecs.LONG, BallState::origin,
			BALL_STATE, BallState::state,
			BallState::new
		);

		@Override
		public Type<? extends CustomPacketPayload> type() {
			return TYPE;
		}
	}

	/** Server -> watching clients: where a ball is and how it moves. */
	public record BallPoseMsg(int entityId, BallPose pose) implements CustomPacketPayload {
		public static final Type<BallPoseMsg> TYPE = payloadType("ball_pose");
		public static final StreamCodec<RegistryFriendlyByteBuf, BallPoseMsg> CODEC = StreamCodec.composite(
			ByteBufCodecs.VAR_INT, BallPoseMsg::entityId,
			BallPose.STREAM_CODEC, BallPoseMsg::pose,
			BallPoseMsg::new
		);

		@Override
		public Type<? extends CustomPacketPayload> type() {
			return TYPE;
		}
	}

	/** Server -> driver: another car bumped yours; add this velocity (Minecraft axes, blocks/s). */
	public record Bumped(int entityId, Vec3 velocity) implements CustomPacketPayload {
		public static final Type<Bumped> TYPE = payloadType("bumped");
		public static final StreamCodec<RegistryFriendlyByteBuf, Bumped> CODEC = StreamCodec.composite(
			ByteBufCodecs.VAR_INT, Bumped::entityId,
			Vec3.STREAM_CODEC, Bumped::velocity,
			Bumped::new
		);

		@Override
		public Type<? extends CustomPacketPayload> type() {
			return TYPE;
		}
	}

	public static void init() {
		PayloadTypeRegistry.serverboundPlay().register(DriveState.TYPE, DriveState.CODEC);
		PayloadTypeRegistry.serverboundPlay().register(ExitCar.TYPE, ExitCar.CODEC);
		PayloadTypeRegistry.clientboundPlay().register(DriverStart.TYPE, DriverStart.CODEC);
		PayloadTypeRegistry.clientboundPlay().register(Pose.TYPE, Pose.CODEC);
		PayloadTypeRegistry.serverboundPlay().register(BallState.TYPE, BallState.CODEC);
		PayloadTypeRegistry.clientboundPlay().register(BallOwn.TYPE, BallOwn.CODEC);
		PayloadTypeRegistry.clientboundPlay().register(BallRelease.TYPE, BallRelease.CODEC);
		PayloadTypeRegistry.clientboundPlay().register(BallPoseMsg.TYPE, BallPoseMsg.CODEC);
		PayloadTypeRegistry.clientboundPlay().register(Bumped.TYPE, Bumped.CODEC);

		ServerPlayNetworking.registerGlobalReceiver(DriveState.TYPE, (payload, context) -> {
			if (context.player().level().getEntity(payload.entityId()) instanceof CarEntity car) {
				car.acceptDriverState(context.player(), BlockPos.of(payload.origin()), payload.state());
			}
		});
		ServerPlayNetworking.registerGlobalReceiver(BallState.TYPE, (payload, context) -> {
			if (context.player().level().getEntity(payload.entityId()) instanceof BallEntity ball) {
				ball.acceptOwnerState(context.player(), BlockPos.of(payload.origin()), payload.state());
			}
		});
		ServerPlayNetworking.registerGlobalReceiver(ExitCar.TYPE, (payload, context) -> {
			if (context.player().getVehicle() instanceof CarEntity) {
				context.player().stopRiding();
			}
		});
	}
}
