package dev.rlcar.net;

import dev.rlcar.RlCar;
import dev.rlcar.entity.CarEntity;
import dev.rlcar.physics.CarPose;
import net.fabricmc.fabric.api.networking.v1.PayloadTypeRegistry;
import net.fabricmc.fabric.api.networking.v1.ServerPlayNetworking;
import net.minecraft.core.BlockPos;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.resources.Identifier;

/**
 * Networking. The player driving a car simulates it on their own client (so steering, jumps and
 * dodges respond within the frame) and streams its full state to the server, which relays the
 * pose to everyone else. Cars nobody drives are simulated by the server.
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

	public static void init() {
		PayloadTypeRegistry.serverboundPlay().register(DriveState.TYPE, DriveState.CODEC);
		PayloadTypeRegistry.serverboundPlay().register(ExitCar.TYPE, ExitCar.CODEC);
		PayloadTypeRegistry.clientboundPlay().register(DriverStart.TYPE, DriverStart.CODEC);
		PayloadTypeRegistry.clientboundPlay().register(Pose.TYPE, Pose.CODEC);

		ServerPlayNetworking.registerGlobalReceiver(DriveState.TYPE, (payload, context) -> {
			if (context.player().level().getEntity(payload.entityId()) instanceof CarEntity car) {
				car.acceptDriverState(context.player(), BlockPos.of(payload.origin()), payload.state());
			}
		});
		ServerPlayNetworking.registerGlobalReceiver(ExitCar.TYPE, (payload, context) -> {
			if (context.player().getVehicle() instanceof CarEntity) {
				context.player().stopRiding();
			}
		});
	}
}
