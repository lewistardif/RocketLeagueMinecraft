#pragma once
#include <cmath>
#include <cstdint>
#include <string>
#include <vector>

namespace rlm {

struct Mesh {
	std::vector<float> pos;
	std::vector<uint32_t> idx;
	std::vector<std::vector<uint8_t>> colours;
	uint32_t triangles() const { return uint32_t(idx.size() / 3); }
	uint32_t vertices() const { return uint32_t(pos.size() / 3); }
};

bool load(const std::string& path, Mesh& out, std::string& err);

struct Models {
	std::vector<Mesh> body, wheel, ball;
	float anchors[4][3] = {};
	bool hasAnchors = false;
	bool ok() const { return !body.empty() && !wheel.empty(); }
	bool load(const std::string& folder, const std::string& preset, std::string& log);
	static const Mesh* pick(const std::vector<Mesh>& lods, int level) {
		if (lods.empty()) return nullptr;
		if (level < 0) level = 0;
		if (level >= int(lods.size())) level = int(lods.size()) - 1;
		return &lods[size_t(level)];
	}
};

struct Xform {
	double a[3][3] = {{1, 0, 0}, {0, 1, 0}, {0, 0, 1}};
	double t[3] = {0, 0, 0};
};

struct Light {
	double sun[3] = {0.35, 0.25, 0.9};
	double ambient = 0.45;
	double diffuse = 0.6;
	double brightness = 1.0;
};

inline double det3(const double m[3][3]) {
	return m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) +
	       m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
}

inline void mul(const double a[3][3], const double b[3][3], double out[3][3]) {
	double r[3][3];
	for (int i = 0; i < 3; i++)
		for (int j = 0; j < 3; j++) r[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
	for (int i = 0; i < 3; i++)
		for (int j = 0; j < 3; j++) out[i][j] = r[i][j];
}

template <class Emit>
int draw(const Mesh& m, int colourSet, const Xform& x, const double cam[3], const Light& light, std::vector<double>& scratch, Emit emit) {
	if (m.colours.empty()) return 0;
	const std::vector<uint8_t>& col = m.colours[size_t(colourSet) < m.colours.size() ? size_t(colourSet) : 0];
	uint32_t nv = m.vertices();
	scratch.resize(size_t(nv) * 3);
	for (uint32_t v = 0; v < nv; v++) {
		const float* p = &m.pos[size_t(v) * 3];
		for (int i = 0; i < 3; i++) scratch[size_t(v) * 3 + i] = x.a[i][0] * p[0] + x.a[i][1] * p[1] + x.a[i][2] * p[2] + x.t[i];
	}
	double sign = det3(x.a) < 0 ? -1.0 : 1.0;
	double sl = std::sqrt(light.sun[0] * light.sun[0] + light.sun[1] * light.sun[1] + light.sun[2] * light.sun[2]);
	double sun[3] = {light.sun[0] / sl, light.sun[1] / sl, light.sun[2] / sl};
	int drawn = 0;
	uint32_t nt = m.triangles();
	for (uint32_t t = 0; t < nt; t++) {
		const double* a = &scratch[size_t(m.idx[t * 3]) * 3];
		const double* b = &scratch[size_t(m.idx[t * 3 + 1]) * 3];
		const double* c = &scratch[size_t(m.idx[t * 3 + 2]) * 3];
		double e1[3] = {b[0] - a[0], b[1] - a[1], b[2] - a[2]}, e2[3] = {c[0] - a[0], c[1] - a[1], c[2] - a[2]};
		double n[3] = {(e1[1] * e2[2] - e1[2] * e2[1]) * sign, (e1[2] * e2[0] - e1[0] * e2[2]) * sign, (e1[0] * e2[1] - e1[1] * e2[0]) * sign};
		double toCam[3] = {cam[0] - a[0], cam[1] - a[1], cam[2] - a[2]};
		if (n[0] * toCam[0] + n[1] * toCam[1] + n[2] * toCam[2] <= 0) continue;
		double nl = std::sqrt(n[0] * n[0] + n[1] * n[1] + n[2] * n[2]);
		if (nl <= 0) continue;
		const uint8_t* c4 = &col[size_t(t) * 4];
		double shade = light.ambient + light.diffuse * std::fmax(0.0, (n[0] * sun[0] + n[1] * sun[1] + n[2] * sun[2]) / nl);
		double g = c4[3] / 255.0;
		double s = (shade * (1.0 - g) + g) * light.brightness;
		int rgb[3];
		for (int i = 0; i < 3; i++) {
			double v = c4[i] * s;
			rgb[i] = v > 255 ? 255 : v < 0 ? 0 : int(v);
		}
		emit(a, b, c, rgb);
		drawn++;
	}
	return drawn;
}

}
