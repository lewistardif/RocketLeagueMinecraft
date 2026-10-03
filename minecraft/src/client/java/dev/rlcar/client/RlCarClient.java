package dev.rlcar.client;

import dev.rlcar.RlCar;
import dev.rlcar.entity.CarEntity;
import dev.rlcar.net.CarNet;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayConnectionEvents;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayNetworking;
import net.fabricmc.fabric.api.client.rendering.v1.EntityRendererRegistry;
import net.fabricmc.fabric.api.client.rendering.v1.hud.HudElementRegistry;

public final class RlCarClient implements ClientModInitializer {
	@Override
	public void onInitializeClient() {
		CarKeys.register();
		PadBinds.load();
		EntityRendererRegistry.register(RlCar.CAR, CarRenderer::new);
		HudElementRegistry.addLast(RlCar.id("car_hud"), CarHud::draw);

		ClientPlayNetworking.registerGlobalReceiver(CarNet.DriverStart.TYPE, (payload, context) -> ClientDriving.onDriverStart(payload));
		ClientPlayNetworking.registerGlobalReceiver(CarNet.Pose.TYPE, (payload, context) -> {
			if (context.client().level != null && context.client().level.getEntity(payload.entityId()) instanceof CarEntity car) {
				car.receivePose(payload.pose());
			}
		});
		ClientPlayConnectionEvents.DISCONNECT.register((handler, client) -> ClientDriving.stop());
		ClientTickEvents.END_CLIENT_TICK.register(ClientDriving::tick);
	}
}
