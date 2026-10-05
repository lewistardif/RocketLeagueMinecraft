#pragma once
#include <cmath>

namespace space {

struct V3 {
	double x = 0, y = 0, z = 0;
};

struct Quat {
	float x = 0, y = 0, z = 0, w = 1;
};

struct Frame {
	V3 origin;
	double scale = 1.0;

	double k() const { return scale / 100.0; }

	V3 toGta(const float* rl) const { return {origin.x + rl[0] * k(), origin.y - rl[1] * k(), origin.z + rl[2] * k()}; }
	void toRl(V3 g, float* out) const {
		out[0] = float((g.x - origin.x) / k());
		out[1] = float(-(g.y - origin.y) / k());
		out[2] = float((g.z - origin.z) / k());
	}
	double lenToGta(double uu) const { return uu * k(); }
	double lenToRl(double m) const { return m / k(); }
};

inline V3 dirToGta(const float* d) { return {d[0], -d[1], d[2]}; }
inline void dirToRl(V3 g, float* out) {
	out[0] = float(g.x);
	out[1] = float(-g.y);
	out[2] = float(g.z);
}

inline Quat carRotToGta(const float* cols) {
	V3 f = dirToGta(cols), r = dirToGta(cols + 3), u = dirToGta(cols + 6);
	double m00 = r.x, m01 = f.x, m02 = u.x;
	double m10 = r.y, m11 = f.y, m12 = u.y;
	double m20 = r.z, m21 = f.z, m22 = u.z;
	Quat q;
	double tr = m00 + m11 + m22;
	if (tr > 0) {
		double s = std::sqrt(tr + 1.0) * 2;
		q.w = float(0.25 * s);
		q.x = float((m21 - m12) / s);
		q.y = float((m02 - m20) / s);
		q.z = float((m10 - m01) / s);
	} else if (m00 > m11 && m00 > m22) {
		double s = std::sqrt(1.0 + m00 - m11 - m22) * 2;
		q.w = float((m21 - m12) / s);
		q.x = float(0.25 * s);
		q.y = float((m01 + m10) / s);
		q.z = float((m02 + m20) / s);
	} else if (m11 > m22) {
		double s = std::sqrt(1.0 + m11 - m00 - m22) * 2;
		q.w = float((m02 - m20) / s);
		q.x = float((m01 + m10) / s);
		q.y = float(0.25 * s);
		q.z = float((m12 + m21) / s);
	} else {
		double s = std::sqrt(1.0 + m22 - m00 - m11) * 2;
		q.w = float((m10 - m01) / s);
		q.x = float((m02 + m20) / s);
		q.y = float((m12 + m21) / s);
		q.z = float(0.25 * s);
	}
	return q;
}

constexpr double kPi = 3.14159265358979323846;

inline float headingToRlYaw(double headingDeg) {
	double h = headingDeg * kPi / 180.0;
	return float(std::atan2(-std::cos(h), -std::sin(h)));
}

inline V3 spinToGta(const float* w) { return {-w[0], w[1], -w[2]}; }

inline Quat spin(Quat q, V3 w, double dt) {
	double ang = std::sqrt(w.x * w.x + w.y * w.y + w.z * w.z) * dt;
	if (ang < 1e-9) return q;
	double s = std::sin(ang * 0.5) / (ang / dt), c = std::cos(ang * 0.5);
	double ax = w.x * s, ay = w.y * s, az = w.z * s;
	double x = c * q.x + ax * q.w + ay * q.z - az * q.y;
	double y = c * q.y - ax * q.z + ay * q.w + az * q.x;
	double z = c * q.z + ax * q.y - ay * q.x + az * q.w;
	double ww = c * q.w - ax * q.x - ay * q.y - az * q.z;
	double n = std::sqrt(x * x + y * y + z * z + ww * ww);
	return {float(x / n), float(y / n), float(z / n), float(ww / n)};
}

inline double headingOf(V3 d) { return std::atan2(-d.x, d.y) * 180.0 / kPi; }

inline V3 cameraRotToGta(const float* cols) {
	V3 f = dirToGta(cols), u = dirToGta(cols + 6);
	double pitch = std::asin(f.z < -1 ? -1 : f.z > 1 ? 1 : f.z) * 180.0 / kPi;
	double yaw = headingOf(f);
	V3 lr{f.y, -f.x, 0};
	double n = std::sqrt(lr.x * lr.x + lr.y * lr.y);
	double roll = 0;
	if (n > 1e-6) {
		lr = {lr.x / n, lr.y / n, 0};
		V3 lu{lr.y * f.z - 0 * f.y, 0 * f.x - lr.x * f.z, lr.x * f.y - lr.y * f.x};
		double dr = u.x * lr.x + u.y * lr.y + u.z * lr.z;
		double du = u.x * lu.x + u.y * lu.y + u.z * lu.z;
		roll = std::atan2(dr, du) * 180.0 / kPi;
	}
	return {pitch, roll, yaw};
}

}
