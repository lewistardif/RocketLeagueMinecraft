package dev.rlcar.client;

import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import dev.rlcar.entity.BallEntity;
import dev.rlcar.physics.BallPose;
import dev.rlcar.physics.Space;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.entity.EntityRenderer;
import net.minecraft.client.renderer.entity.EntityRendererProvider;
import net.minecraft.client.renderer.rendertype.RenderType;
import net.minecraft.client.renderer.rendertype.RenderTypes;
import net.minecraft.client.renderer.state.level.CameraRenderState;
import net.minecraft.client.renderer.texture.OverlayTexture;
import net.minecraft.resources.Identifier;
import net.minecraft.util.Mth;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import org.joml.Quaternionf;
import org.joml.Vector3fc;

/**
 * Draws the ball at its simulated pose: the real Rocket League ball when it was extracted
 * ({@link RlModels#ball()}), with a port of its material ({@link RlShading#submitBall}) when its
 * textures were extracted too, otherwise a plain panelled sphere. Its rotation is integrated from
 * the simulated spin (the core tracks the ball's spin, not its orientation). Around it, the
 * game's ball markers: the reticle on the ground, the location line and the far-away outline
 * ({@link BallMarker}).
 */
public class BallRenderer extends EntityRenderer<BallEntity, BallRenderState> {
	private static final RenderType SPHERE = RenderTypes.entitySolid(Identifier.withDefaultNamespace("textures/block/white_concrete.png"));
	private static final int RINGS = 12;
	private static final int SEGMENTS = 24;

	public BallRenderer(EntityRendererProvider.Context context) {
		super(context);
		this.shadowRadius = BallEntity.RADIUS * 0.9F;
	}

	@Override
	public BallRenderState createRenderState() {
		return new BallRenderState();
	}

	@Override
	public void extractRenderState(BallEntity ball, BallRenderState state, float partialTicks) {
		super.extractRenderState(ball, state, partialTicks);
		BallPose pose = ball.renderPose(partialTicks);
		state.pose = pose;
		if (pose != null) {
			// Draw at the simulated centre, not the entity's tick-interpolated position.
			state.x = pose.x;
			state.y = pose.y;
			state.z = pose.z;
			float dt = Math.max(0, state.ageInTicks - ball.rotationAge) / 20.0F;
			ball.rotationAge = state.ageInTicks;
			Vec3 w = pose.spin;
			double speed = w.length();
			if (speed > 1.0E-4 && dt > 0) {
				new Quaternionf().rotationAxis((float) (speed * dt), (float) (w.x / speed), (float) (w.y / speed), (float) (w.z / speed))
					.mul(ball.rotation, ball.rotation).normalize();
			}
			// RL Y is Minecraft Z; the kickoff spot is the centre of the field.
			Vector3fc kickoff = ball.kickoff();
			state.fieldY = Float.isNaN(kickoff.z()) ? 0.0F : (float) ((pose.z - kickoff.z()) * Space.UU_PER_BLOCK / 1024.0);
			BallMarker.extract(ball, state);
		} else {
			state.markers = false;
		}
		state.rotation.set(ball.rotation);
	}

	@Override
	protected AABB getBoundingBoxForCulling(BallEntity ball, float partialTicks) {
		// Keep drawing while the markers on the ground below are in view.
		AABB box = super.getBoundingBoxForCulling(ball, partialTicks);
		return box.expandTowards(0, -BallMarker.reach(), 0).inflate(BallMarker.radius());
	}

	@Override
	public void submit(BallRenderState state, PoseStack poseStack, SubmitNodeCollector collector, CameraRenderState camera) {
		if (state.pose == null) {
			return;
		}
		BallMarker.submit(collector, poseStack, state, camera);
		poseStack.pushPose();
		poseStack.rotate(state.rotation);
		RlModels.Model model = RlModels.ball();
		if (model != null) {
			if (!RlShading.submitBall(collector, poseStack, model, state.lightCoords, state.fieldY)) {
				CarRenderer.submitModel(collector, poseStack, model, state.lightCoords);
			}
		} else {
			collector.submitCustomGeometry(poseStack, SPHERE, (p, buffer) -> drawSphere(p, buffer, state.lightCoords));
		}
		poseStack.popPose();
		super.submit(state, poseStack, collector, camera);
	}

	/** A sphere of the ball's radius in light and dark grey panels. */
	private static void drawSphere(PoseStack.Pose p, VertexConsumer b, int light) {
		float r = BallEntity.RADIUS;
		for (int i = 0; i < RINGS; i++) {
			float t0 = Mth.PI * i / RINGS;
			float t1 = Mth.PI * (i + 1) / RINGS;
			for (int j = 0; j < SEGMENTS; j++) {
				float p0 = Mth.TWO_PI * j / SEGMENTS;
				float p1 = Mth.TWO_PI * (j + 1) / SEGMENTS;
				int argb = (i / 2 + j / 2) % 2 == 0 ? 0xFFD8DCE0 : 0xFF454B52;
				// Counter-clockwise from outside.
				vertex(b, p, r, t0, p0, argb, light);
				vertex(b, p, r, t0, p1, argb, light);
				vertex(b, p, r, t1, p1, argb, light);
				vertex(b, p, r, t1, p0, argb, light);
			}
		}
	}

	private static void vertex(VertexConsumer b, PoseStack.Pose p, float r, float theta, float phi, int argb, int light) {
		float nx = Mth.sin(theta) * Mth.cos(phi);
		float ny = Mth.cos(theta);
		float nz = Mth.sin(theta) * Mth.sin(phi);
		b.addVertex(p, nx * r, ny * r, nz * r).setColor(argb).setUv(0.5F, 0.5F).setOverlay(OverlayTexture.NO_OVERLAY).setLight(light).setNormal(p, nx, ny, nz);
	}
}
