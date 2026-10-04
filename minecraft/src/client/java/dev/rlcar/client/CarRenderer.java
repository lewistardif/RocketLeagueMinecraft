package dev.rlcar.client;

import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import dev.rlcar.entity.CarEntity;
import dev.rlcar.physics.CarPose;
import dev.rlcar.physics.RlCarNative;
import dev.rlcar.physics.Space;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.culling.Frustum;
import net.minecraft.client.renderer.entity.EntityRenderer;
import net.minecraft.client.renderer.entity.EntityRendererProvider;
import net.minecraft.client.renderer.rendertype.RenderType;
import net.minecraft.client.renderer.rendertype.RenderTypes;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import net.minecraft.client.renderer.texture.OverlayTexture;
import net.minecraft.resources.Identifier;
import net.minecraft.util.Mth;
import net.minecraft.world.phys.AABB;
import org.joml.Quaternionf;

/**
 * Draws a car at its simulated pose: the real Rocket League body and wheels when the extracted
 * models are available ({@link RlModels}), otherwise a plain box car sized from the hitbox.
 *
 * <p>Model space is +X forward, +Y up, +Z right, origin at the centre of mass. Wheels hang at the
 * simulated suspension length, steer, and roll with the car's speed, like in the Bevy demo.
 */
public class CarRenderer extends EntityRenderer<CarEntity, CarRenderState> {
	private static final RenderType BOX = RenderTypes.entitySolid(Identifier.withDefaultNamespace("textures/block/white_concrete.png"));
	private static final float[][] HITBOX = new float[RlCarNative.PRESETS.length][];

	public CarRenderer(EntityRendererProvider.Context context) {
		super(context);
		this.shadowRadius = 0.6F;
	}

	@Override
	public CarRenderState createRenderState() {
		return new CarRenderState();
	}

	@Override
	public void extractRenderState(CarEntity car, CarRenderState state, float partialTicks) {
		super.extractRenderState(car, state, partialTicks);
		CarPose pose = car.renderPose(partialTicks);
		state.pose = pose;
		state.preset = car.preset();
		state.color = car.color();
		System.arraycopy(hitbox(car.preset()), 0, state.hitbox, 0, 6);
		if (pose != null) {
			// Draw at the simulated position, not the entity's tick-interpolated one.
			state.x = pose.x;
			state.y = pose.y;
			state.z = pose.z;
			float dt = Math.max(0, state.ageInTicks - car.wheelSpinAge) / 20.0F;
			car.wheelSpinAge = state.ageInTicks;
			float radius = Math.max(0.05F, pose.wheelRadius[0]);
			car.wheelSpin = (car.wheelSpin + pose.forwardSpeed / Space.UU_PER_BLOCK / radius * dt) % Mth.TWO_PI;
		}
		state.wheelSpin = car.wheelSpin;
		if (pose != null) {
			boolean real = RlModels.body(state.preset, state.color == CarEntity.ORANGE) != null && RlModels.wheel() != null && RlModels.anchors(state.preset) != null;
			RlBoost.extract(car, pose, state, real);
		} else {
			state.boostCones = null;
			state.simpleFlame = false;
			state.smokeCount = 0;
		}
	}

	@Override
	public boolean shouldRender(CarEntity car, Frustum frustum, double camX, double camY, double camZ, float partialTicks) {
		// Demolished: gone until it respawns.
		return !car.demolished() && super.shouldRender(car, frustum, camX, camY, camZ, partialTicks);
	}

	@Override
	protected AABB getBoundingBoxForCulling(CarEntity car, float partialTicks) {
		// Keep drawing while the boost smoke trails behind (it is part of this renderer).
		return super.getBoundingBoxForCulling(car, partialTicks).inflate(RlBoost.smokeRadius(car));
	}

	private static float[] hitbox(int preset) {
		float[] h = HITBOX[preset];
		if (h == null) {
			h = RlCarNative.presetHitbox(preset);
			for (int i = 0; i < h.length; i++) {
				h[i] /= Space.UU_PER_BLOCK;
			}
			HITBOX[preset] = h;
		}
		return h;
	}

	@Override
	public void submit(CarRenderState state, PoseStack poseStack, SubmitNodeCollector collector, CameraRenderState camera) {
		CarPose pose = state.pose;
		if (pose == null) {
			return;
		}
		// Smoke first, world-aligned around the car origin.
		RlBoost.submitSmoke(collector, poseStack, state, camera);
		poseStack.pushPose();
		poseStack.rotate(pose.rotation);
		if (state.boostCones != null) {
			RlBoost.submitCones(collector, poseStack, state.boostCones);
		}
		if (state.simpleFlame) {
			float[] h = state.hitbox;
			RlBoost.submitSimpleFlame(collector, poseStack, h[3] - h[0] / 2, h[5] - h[2] * 0.1F, h[4]);
		}
		RlModels.Model body = RlModels.body(state.preset, state.color == CarEntity.ORANGE);
		RlModels.Model wheel = RlModels.wheel();
		float[][] anchors = RlModels.anchors(state.preset);
		if (body != null && wheel != null && anchors != null) {
			submitModel(collector, poseStack, body, state.lightCoords);
			for (int i = 0; i < 4; i++) {
				boolean front = i < 2;
				boolean left = i % 2 == 1;
				// The model's own hub positions fore/aft and sideways; the height comes from the suspension.
				float[] hub = anchors[(front ? 0 : 2) + (left ? 0 : 1)];
				poseStack.pushPose();
				poseStack.translate(hub[0], pose.wheels[i * 3 + 1], hub[2]);
				poseStack.rotate(new Quaternionf().rotationY(-pose.steer[i]).rotateZ(-state.wheelSpin));
				float s = pose.wheelRadius[i] / RlModels.WHEEL_MESH_RADIUS;
				poseStack.scale(s, s, s);
				if (left) {
					// The mesh's outer face is +Z; turn left wheels around so it faces outwards.
					poseStack.rotate(new Quaternionf().rotationY(Mth.PI));
				}
				submitModel(collector, poseStack, wheel, state.lightCoords);
				poseStack.popPose();
			}
		} else {
			collector.submitCustomGeometry(poseStack, BOX, (p, buffer) -> drawBoxCar(state, pose, p, buffer));
		}
		poseStack.popPose();
		super.submit(state, poseStack, collector, camera);
	}

	static void submitModel(SubmitNodeCollector collector, PoseStack poseStack, RlModels.Model model, int light) {
		for (RlModels.Part part : model.parts()) {
			collector.submitCustomGeometry(poseStack, RenderTypes.entityCutout(part.texture()), (p, b) -> {
				float[] pos = part.positions();
				float[] nrm = part.normals();
				float[] uv = part.uvs();
				int[] tri = part.triangles();
				for (int t = 0; t + 2 < tri.length; t += 3) {
					// Entity render types draw quads: a triangle is a quad with its last vertex repeated.
					for (int k = 0; k < 4; k++) {
						int v = tri[t + Math.min(k, 2)];
						b.addVertex(p, pos[v * 3], pos[v * 3 + 1], pos[v * 3 + 2])
							.setColor(-1)
							.setUv(uv[v * 2], uv[v * 2 + 1])
							.setOverlay(OverlayTexture.NO_OVERLAY)
							.setLight(light)
							.setNormal(p, nrm[v * 3], nrm[v * 3 + 1], nrm[v * 3 + 2]);
					}
				}
			});
		}
	}

	// --------------------------------------------------------------- fallback box car

	private static void drawBoxCar(CarRenderState state, CarPose pose, PoseStack.Pose p, VertexConsumer b) {
		float[] h = state.hitbox;
		float len = h[0], wid = h[1], hgt = h[2];
		float cx = h[3], cy = h[5];
		int light = state.lightCoords;
		float x0 = cx - len / 2, x1 = cx + len / 2;
		float y0 = cy - hgt / 2, y1 = cy + hgt / 2;
		float hw = wid / 2;
		box(b, p, x0, y0, -hw, x1, y0 + hgt * 0.55F, hw, state.color, light);
		box(b, p, cx - len * 0.30F, y0 + hgt * 0.55F, -hw * 0.82F, cx + len * 0.12F, y1 + hgt * 0.15F, hw * 0.82F, 0x1A222C, light);
		for (int i = 0; i < 4; i++) {
			float r = pose.wheelRadius[i];
			float wx = pose.wheels[i * 3], wy = pose.wheels[i * 3 + 1], wz = pose.wheels[i * 3 + 2];
			float zc = (wz < 0 ? -1 : 1) * Math.max(Math.abs(wz), hw - 0.03F);
			box(b, p, wx - r, wy - r, zc - 0.05F, wx + r, wy + r, zc + 0.05F, 0x1C1C1E, light);
		}
	}

	/** An axis-aligned box in the current pose, faces wound counter-clockwise from outside. */
	private static void box(VertexConsumer b, PoseStack.Pose p, float x0, float y0, float z0, float x1, float y1, float z1, int rgb, int light) {
		quad(b, p, x1, y0, z0, x1, y1, z0, x1, y1, z1, x1, y0, z1, 1, 0, 0, rgb, light);
		quad(b, p, x0, y0, z1, x0, y1, z1, x0, y1, z0, x0, y0, z0, -1, 0, 0, rgb, light);
		quad(b, p, x0, y1, z0, x0, y1, z1, x1, y1, z1, x1, y1, z0, 0, 1, 0, rgb, light);
		quad(b, p, x0, y0, z0, x1, y0, z0, x1, y0, z1, x0, y0, z1, 0, -1, 0, rgb, light);
		quad(b, p, x0, y0, z1, x1, y0, z1, x1, y1, z1, x0, y1, z1, 0, 0, 1, rgb, light);
		quad(b, p, x0, y0, z0, x0, y1, z0, x1, y1, z0, x1, y0, z0, 0, 0, -1, rgb, light);
	}

	private static void quad(
		VertexConsumer b, PoseStack.Pose p,
		float ax, float ay, float az, float bx, float by, float bz, float cx, float cy, float cz, float dx, float dy, float dz,
		float nx, float ny, float nz, int rgb, int light
	) {
		int argb = 0xFF000000 | rgb;
		b.addVertex(p, ax, ay, az).setColor(argb).setUv(0, 1).setOverlay(OverlayTexture.NO_OVERLAY).setLight(light).setNormal(p, nx, ny, nz);
		b.addVertex(p, bx, by, bz).setColor(argb).setUv(1, 1).setOverlay(OverlayTexture.NO_OVERLAY).setLight(light).setNormal(p, nx, ny, nz);
		b.addVertex(p, cx, cy, cz).setColor(argb).setUv(1, 0).setOverlay(OverlayTexture.NO_OVERLAY).setLight(light).setNormal(p, nx, ny, nz);
		b.addVertex(p, dx, dy, dz).setColor(argb).setUv(0, 0).setOverlay(OverlayTexture.NO_OVERLAY).setLight(light).setNormal(p, nx, ny, nz);
	}
}
