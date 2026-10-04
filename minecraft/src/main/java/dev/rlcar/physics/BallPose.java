package dev.rlcar.physics;

import io.netty.buffer.ByteBuf;
import net.minecraft.core.BlockPos;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.world.phys.Vec3;

/** Where a ball is and how it moves, in Minecraft space: centre (absolute blocks), velocity (blocks/s), spin (rad/s). */
public final class BallPose {
	public double x;
	public double y;
	public double z;
	public Vec3 velocity = Vec3.ZERO;
	public Vec3 spin = Vec3.ZERO;

	public static final StreamCodec<ByteBuf, BallPose> STREAM_CODEC = StreamCodec.of(BallPose::write, BallPose::read);

	public Vec3 position() {
		return new Vec3(this.x, this.y, this.z);
	}

	/** Fills this pose from a native ball state ({@link RlCarNative#BALL_POSE_FLOATS} floats) relative to {@code origin}. */
	public BallPose setFromNative(BlockPos origin, float[] rl) {
		Vec3 p = Space.toMc(origin, rl[0], rl[1], rl[2]);
		this.x = p.x;
		this.y = p.y;
		this.z = p.z;
		this.velocity = Space.dirToMc(rl[3], rl[4], rl[5], Space.UU_PER_BLOCK);
		this.spin = Space.angularToMc(rl[6], rl[7], rl[8]);
		return this;
	}

	public BallPose copy() {
		BallPose c = new BallPose();
		c.x = this.x;
		c.y = this.y;
		c.z = this.z;
		c.velocity = this.velocity;
		c.spin = this.spin;
		return c;
	}

	/** {@code a} blended towards {@code b} by {@code t} (0..1). */
	public static BallPose lerp(BallPose a, BallPose b, float t) {
		BallPose out = b.copy();
		out.x = a.x + (b.x - a.x) * t;
		out.y = a.y + (b.y - a.y) * t;
		out.z = a.z + (b.z - a.z) * t;
		out.velocity = a.velocity.lerp(b.velocity, t);
		out.spin = a.spin.lerp(b.spin, t);
		return out;
	}

	private static void write(ByteBuf buf, BallPose p) {
		buf.writeDouble(p.x);
		buf.writeDouble(p.y);
		buf.writeDouble(p.z);
		for (Vec3 v : new Vec3[] {p.velocity, p.spin}) {
			buf.writeFloat((float) v.x);
			buf.writeFloat((float) v.y);
			buf.writeFloat((float) v.z);
		}
	}

	private static BallPose read(ByteBuf buf) {
		BallPose p = new BallPose();
		p.x = buf.readDouble();
		p.y = buf.readDouble();
		p.z = buf.readDouble();
		p.velocity = new Vec3(buf.readFloat(), buf.readFloat(), buf.readFloat());
		p.spin = new Vec3(buf.readFloat(), buf.readFloat(), buf.readFloat());
		return p;
	}
}
