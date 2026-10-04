package dev.rlcar.client;

import com.mojang.blaze3d.vertex.PoseStack;
import org.joml.Vector3f;

/**
 * A model part's vertices put through a pose once per draw, for the indexed meshes ({@link
 * RlModels.Part}): a car body has some 20 000 vertices but is written as over 100 000 (each
 * triangle a quad), and {@code VertexConsumer.addVertex(Pose, ...)} / {@code setNormal(Pose, ...)}
 * transform and allocate a vector for every one of them, which at a few hundred frames a second
 * keeps the garbage collector busy. Same arithmetic as those methods, so the vertices are
 * identical; the results go to scratch arrays reused by every draw (render thread only).
 */
final class PosedMesh {
	private static final Vector3f TMP = new Vector3f();
	private static float[] positions = new float[0];
	private static float[] normals = new float[0];

	private PosedMesh() {
	}

	/** {@code pos} (x, y, z per vertex) through the pose's matrix; valid until the next call. */
	static float[] positions(PoseStack.Pose pose, float[] pos) {
		if (positions.length < pos.length) {
			positions = new float[pos.length];
		}
		float[] out = positions;
		for (int i = 0; i + 2 < pos.length; i += 3) {
			pose.pose().transformPosition(pos[i], pos[i + 1], pos[i + 2], TMP);
			out[i] = TMP.x;
			out[i + 1] = TMP.y;
			out[i + 2] = TMP.z;
		}
		return out;
	}

	/** {@code nrm} (x, y, z per vertex) through the pose's normal matrix; valid until the next call. */
	static float[] normals(PoseStack.Pose pose, float[] nrm) {
		if (normals.length < nrm.length) {
			normals = new float[nrm.length];
		}
		float[] out = normals;
		for (int i = 0; i + 2 < nrm.length; i += 3) {
			pose.transformNormal(nrm[i], nrm[i + 1], nrm[i + 2], TMP);
			out[i] = TMP.x;
			out[i + 1] = TMP.y;
			out[i + 2] = TMP.z;
		}
		return out;
	}
}
