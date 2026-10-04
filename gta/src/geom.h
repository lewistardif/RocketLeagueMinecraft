#pragma once
#include <cmath>

namespace geom {

struct Box {
	float c[3];
	float ax[3][3];
	float h[3];
};

inline float dot3(const float* a, const float* b) { return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]; }

inline bool overlap(const Box& a, const Box& b) {
	float t[3] = {b.c[0] - a.c[0], b.c[1] - a.c[1], b.c[2] - a.c[2]};
	float r[3][3], ar[3][3];
	for (int i = 0; i < 3; i++)
		for (int j = 0; j < 3; j++) r[i][j] = dot3(a.ax[i], b.ax[j]), ar[i][j] = std::fabs(r[i][j]) + 1e-6f;
	float ta[3] = {dot3(t, a.ax[0]), dot3(t, a.ax[1]), dot3(t, a.ax[2])};
	for (int i = 0; i < 3; i++) {
		float rb = b.h[0] * ar[i][0] + b.h[1] * ar[i][1] + b.h[2] * ar[i][2];
		if (std::fabs(ta[i]) > a.h[i] + rb) return false;
	}
	for (int j = 0; j < 3; j++) {
		float ra = a.h[0] * ar[0][j] + a.h[1] * ar[1][j] + a.h[2] * ar[2][j];
		if (std::fabs(ta[0] * r[0][j] + ta[1] * r[1][j] + ta[2] * r[2][j]) > ra + b.h[j]) return false;
	}
	for (int i = 0; i < 3; i++) {
		int i1 = (i + 1) % 3, i2 = (i + 2) % 3;
		for (int j = 0; j < 3; j++) {
			int j1 = (j + 1) % 3, j2 = (j + 2) % 3;
			float ra = a.h[i1] * ar[i2][j] + a.h[i2] * ar[i1][j];
			float rb = b.h[j1] * ar[i][j2] + b.h[j2] * ar[i][j1];
			if (std::fabs(ta[i2] * r[i1][j] - ta[i1] * r[i2][j]) > ra + rb) return false;
		}
	}
	return true;
}

inline void closestLocal(const Box& a, const float* p, float* local) {
	float d[3] = {p[0] - a.c[0], p[1] - a.c[1], p[2] - a.c[2]};
	for (int i = 0; i < 3; i++) {
		float v = dot3(d, a.ax[i]);
		local[i] = v < -a.h[i] ? -a.h[i] : (v > a.h[i] ? a.h[i] : v);
	}
}

}
