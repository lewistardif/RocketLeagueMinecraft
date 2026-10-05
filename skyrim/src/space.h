#pragma once
#include <cmath>

// Rocket League <-> Skyrim. Both are Z up. Rocket League is left-handed (X forward, Y right), Skyrim
// right-handed (X east, Y north), so Y flips. 1 uu = 1 cm; Skyrim has ~70 units per metre (Havok's
// world scale, bhkWorld::GetWorldScaleInverse()).
namespace space {

constexpr double kUnitsPerMetre = 69.99125;
constexpr double kPi = 3.14159265358979323846;

struct V3 {
	double x = 0, y = 0, z = 0;
};

struct Quat {
	float x = 0, y = 0, z = 0, w = 1;
};

struct Frame {
	V3 origin;
	double scale = 1.0;
	double unitsPerMetre = kUnitsPerMetre;

	double k() const { return scale * unitsPerMetre / 100.0; }

	V3 toSky(const float* rl) const { return {origin.x + rl[0] * k(), origin.y - rl[1] * k(), origin.z + rl[2] * k()}; }
	void toRl(V3 s, float* out) const {
		out[0] = float((s.x - origin.x) / k());
		out[1] = float(-(s.y - origin.y) / k());
		out[2] = float((s.z - origin.z) / k());
	}
	double lenToSky(double uu) const { return uu * k(); }
	double lenToRl(double units) const { return units / k(); }
};

inline V3 dirToSky(const float* d) { return {d[0], -d[1], d[2]}; }
inline void dirToRl(V3 s, float* out) {
	out[0] = float(s.x);
	out[1] = float(-s.y);
	out[2] = float(s.z);
}

// Skyrim heading (angle.z): radians clockwise from north, forward = (sin h, cos h).
inline float headingToRlYaw(double heading) { return float(std::atan2(-std::cos(heading), std::sin(heading))); }
inline double headingOf(V3 d) { return std::atan2(d.x, d.y); }

// Rotation of a Skyrim model (local X right, Y forward, Z up) from the car's axes (forward, right,
// up columns, Rocket League). m[row][col], column j is local axis j in Skyrim space.
inline void carRotToSky(const float* cols, double m[3][3]) {
	V3 f = dirToSky(cols), r = dirToSky(cols + 3), u = dirToSky(cols + 6);
	const V3 c[3] = {r, f, u};
	for (int j = 0; j < 3; j++) m[0][j] = c[j].x, m[1][j] = c[j].y, m[2][j] = c[j].z;
}

inline V3 spinToSky(const float* w) { return {-w[0], w[1], -w[2]}; }

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

inline void quatToMatrix(Quat q, double m[3][3]) {
	double x = q.x, y = q.y, z = q.z, w = q.w;
	m[0][0] = 1 - 2 * (y * y + z * z), m[0][1] = 2 * (x * y - z * w), m[0][2] = 2 * (x * z + y * w);
	m[1][0] = 2 * (x * y + z * w), m[1][1] = 1 - 2 * (x * x + z * z), m[1][2] = 2 * (y * z - x * w);
	m[2][0] = 2 * (x * z - y * w), m[2][1] = 2 * (y * z + x * w), m[2][2] = 1 - 2 * (x * x + y * y);
}

// The camera's view direction, up and right in Skyrim space from its forward, right, up columns.
struct CameraBasis {
	V3 forward, up, right;
	double heading = 0, pitch = 0;  // Skyrim angles: pitch positive looks down
};

inline CameraBasis cameraToSky(const float* cols) {
	CameraBasis b;
	b.forward = dirToSky(cols);
	b.right = dirToSky(cols + 3);
	b.up = dirToSky(cols + 6);
	double fz = b.forward.z < -1 ? -1 : (b.forward.z > 1 ? 1 : b.forward.z);
	b.heading = headingOf(b.forward);
	b.pitch = -std::asin(fz);
	return b;
}

// Skyrim's FOV setting is the horizontal FOV of a 4:3 view.
inline double verticalFovToSkyrim(double verticalDeg) {
	double half = verticalDeg * 0.5 * kPi / 180.0;
	return 2.0 * std::atan(std::tan(half) * 4.0 / 3.0) * 180.0 / kPi;
}

}
