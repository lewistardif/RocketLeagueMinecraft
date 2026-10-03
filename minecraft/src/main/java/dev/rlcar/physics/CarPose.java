package dev.rlcar.physics;

import io.netty.buffer.ByteBuf;
import net.minecraft.core.BlockPos;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.world.phys.Vec3;
import org.joml.Quaternionf;
import org.joml.Vector3f;

/**
 * Everything needed to draw a car, in Minecraft space: where it is, how it is rotated (model
 * local +X forward, +Y up, +Z right), where its wheels are and what it is doing.
 */
public final class CarPose {
	/** Car origin (centre of mass), absolute block coordinates. */
	public double x;
	public double y;
	public double z;
	public final Quaternionf rotation = new Quaternionf();
	/** Wheel centres in model space (blocks), FR, FL, BR, BL: x, y, z per wheel. */
	public final float[] wheels = new float[12];
	public final float[] wheelRadius = new float[4];
	/** Steer angle per wheel (radians, positive = right). */
	public final float[] steer = new float[4];
	public int flags;
	/** 0..100 */
	public float boost;
	/** Forward speed, uu/s (negative when reversing). */
	public float forwardSpeed;

	public static final StreamCodec<ByteBuf, CarPose> STREAM_CODEC = StreamCodec.of(CarPose::write, CarPose::read);

	public Vec3 position() {
		return new Vec3(this.x, this.y, this.z);
	}

	public boolean has(int flag) {
		return (this.flags & flag) != 0;
	}

	/** Model-space +X (car forward) in world space. */
	public Vector3f forward() {
		return this.rotation.transform(new Vector3f(1, 0, 0));
	}

	public Vector3f up() {
		return this.rotation.transform(new Vector3f(0, 1, 0));
	}

	/** Fills this pose from {@code rl} (the native pose buffer) for a car whose origin is {@code origin}. */
	public CarPose setFromNative(BlockPos origin, float[] rl, int flags) {
		Vec3 p = Space.toMc(origin, rl[0], rl[1], rl[2]);
		this.x = p.x;
		this.y = p.y;
		this.z = p.z;
		this.rotation.set(Space.rotationToMc(rl, 3));
		for (int i = 0; i < 4; i++) {
			int w = 19 + i * 3;
			// Car-local RL (forward, right, up) -> model (forward, up, right).
			this.wheels[i * 3] = rl[w] / Space.UU_PER_BLOCK;
			this.wheels[i * 3 + 1] = rl[w + 2] / Space.UU_PER_BLOCK;
			this.wheels[i * 3 + 2] = rl[w + 1] / Space.UU_PER_BLOCK;
			this.wheelRadius[i] = rl[31 + i] / Space.UU_PER_BLOCK;
			this.steer[i] = rl[35 + i];
		}
		this.boost = rl[18];
		this.forwardSpeed = rl[39];
		this.flags = flags;
		return this;
	}

	public CarPose copy() {
		CarPose c = new CarPose();
		c.set(this);
		return c;
	}

	public void set(CarPose o) {
		this.x = o.x;
		this.y = o.y;
		this.z = o.z;
		this.rotation.set(o.rotation);
		System.arraycopy(o.wheels, 0, this.wheels, 0, 12);
		System.arraycopy(o.wheelRadius, 0, this.wheelRadius, 0, 4);
		System.arraycopy(o.steer, 0, this.steer, 0, 4);
		this.flags = o.flags;
		this.boost = o.boost;
		this.forwardSpeed = o.forwardSpeed;
	}

	/** {@code a} blended towards {@code b} by {@code t} (0..1); flags and boost come from {@code b}. */
	public static CarPose lerp(CarPose a, CarPose b, float t) {
		CarPose out = b.copy();
		out.x = a.x + (b.x - a.x) * t;
		out.y = a.y + (b.y - a.y) * t;
		out.z = a.z + (b.z - a.z) * t;
		a.rotation.slerp(b.rotation, t, out.rotation);
		for (int i = 0; i < 12; i++) {
			out.wheels[i] = a.wheels[i] + (b.wheels[i] - a.wheels[i]) * t;
		}
		for (int i = 0; i < 4; i++) {
			out.steer[i] = a.steer[i] + (b.steer[i] - a.steer[i]) * t;
		}
		out.forwardSpeed = a.forwardSpeed + (b.forwardSpeed - a.forwardSpeed) * t;
		return out;
	}

	private static void write(ByteBuf buf, CarPose p) {
		buf.writeDouble(p.x);
		buf.writeDouble(p.y);
		buf.writeDouble(p.z);
		buf.writeFloat(p.rotation.x);
		buf.writeFloat(p.rotation.y);
		buf.writeFloat(p.rotation.z);
		buf.writeFloat(p.rotation.w);
		for (float f : p.wheels) {
			buf.writeFloat(f);
		}
		for (float f : p.wheelRadius) {
			buf.writeFloat(f);
		}
		for (float f : p.steer) {
			buf.writeFloat(f);
		}
		buf.writeInt(p.flags);
		buf.writeFloat(p.boost);
		buf.writeFloat(p.forwardSpeed);
	}

	private static CarPose read(ByteBuf buf) {
		CarPose p = new CarPose();
		p.x = buf.readDouble();
		p.y = buf.readDouble();
		p.z = buf.readDouble();
		p.rotation.set(buf.readFloat(), buf.readFloat(), buf.readFloat(), buf.readFloat());
		for (int i = 0; i < 12; i++) {
			p.wheels[i] = buf.readFloat();
		}
		for (int i = 0; i < 4; i++) {
			p.wheelRadius[i] = buf.readFloat();
		}
		for (int i = 0; i < 4; i++) {
			p.steer[i] = buf.readFloat();
		}
		p.flags = buf.readInt();
		p.boost = buf.readFloat();
		p.forwardSpeed = buf.readFloat();
		return p;
	}
}
