#include "sky_world.h"
#include <cmath>
#include <cstring>

namespace sky {

namespace {
inline float dot(const float* a, const float* b) { return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]; }
inline void madd(float* out, const float* a, const float* b, float s) {
	for (int i = 0; i < 3; i++) out[i] = a[i] + b[i] * s;
}
inline float len(const float* a) { return std::sqrt(dot(a, a)); }
inline void cross(float* o, const float* a, const float* b) {
	o[0] = a[1] * b[2] - a[2] * b[1];
	o[1] = a[2] * b[0] - a[0] * b[2];
	o[2] = a[0] * b[1] - a[1] * b[0];
}
inline void sub(float* o, const float* a, const float* b) {
	for (int i = 0; i < 3; i++) o[i] = a[i] - b[i];
}
inline void perp(float* o, const float* v, const float* d) { madd(o, v, d, -dot(v, d)); }

struct Dirs {
	float d[26][3];
	Dirs() {
		int k = 0;
		for (int x = -1; x <= 1; x++)
			for (int y = -1; y <= 1; y++)
				for (int z = -1; z <= 1; z++)
					if (x || y || z) {
						d[k][0] = float(x), d[k][1] = float(y), d[k][2] = float(z);
						k++;
					}
	}
};
const Dirs kDirs;

// Prefer a triangle's own normal over a separating axis this much shallower: keeps a box sliding
// over the seams of a mesh from catching on its inner edges.
constexpr float kFaceBias = 8.0f;

uint32_t pointId(const float* p, uint32_t salt) {
	int q[3] = {int(std::lround(p[0] * 4)), int(std::lround(p[1] * 4)), int(std::lround(p[2] * 4))};
	uint32_t h = 2166136261u ^ salt;
	for (int v : q) {
		h ^= uint32_t(v);
		h *= 16777619u;
	}
	return h;
}

struct Collector {
	ffi::Contact* out;
	uint32_t cap;
	uint32_t n = 0;
	void add(const ffi::Contact& c) {
		for (uint32_t i = 0; i < n; i++)
			if (out[i].surface == c.surface) {
				if (c.depth > out[i].depth) out[i] = c;
				return;
			}
		if (n < cap) out[n++] = c;
	}
};

void sortById(ffi::Contact* out, uint32_t n) {
	std::stable_sort(out, out + n, [](const ffi::Contact& a, const ffi::Contact& b) { return a.surface < b.surface; });
}

bool insideTri(const Tri& t, const float* q, float tol) {
	for (int i = 0; i < 3; i++) {
		float e[3], w[3], c[3];
		sub(e, t.v[(i + 1) % 3], t.v[i]);
		sub(w, q, t.v[i]);
		cross(c, e, w);
		if (dot(c, t.n) < -tol * len(e)) return false;
	}
	return true;
}

bool closestOnTri(const Tri& t, const float* p, float* out) {
	const float *a = t.v[0], *b = t.v[1], *c = t.v[2];
	float ab[3], ac[3], ap[3], bp[3], cp[3];
	sub(ab, b, a), sub(ac, c, a), sub(ap, p, a);
	float d1 = dot(ab, ap), d2 = dot(ac, ap);
	if (d1 <= 0 && d2 <= 0) return std::copy(a, a + 3, out), false;
	sub(bp, p, b);
	float d3 = dot(ab, bp), d4 = dot(ac, bp);
	if (d3 >= 0 && d4 <= d3) return std::copy(b, b + 3, out), false;
	float vc = d1 * d4 - d3 * d2;
	if (vc <= 0 && d1 >= 0 && d3 <= 0) return madd(out, a, ab, d1 / (d1 - d3)), false;
	sub(cp, p, c);
	float d5 = dot(ab, cp), d6 = dot(ac, cp);
	if (d6 >= 0 && d5 <= d6) return std::copy(c, c + 3, out), false;
	float vb = d5 * d2 - d1 * d6;
	if (vb <= 0 && d2 >= 0 && d6 <= 0) return madd(out, a, ac, d2 / (d2 - d6)), false;
	float va = d3 * d6 - d5 * d4;
	if (va <= 0 && (d4 - d3) >= 0 && (d5 - d6) >= 0) {
		float bc[3];
		sub(bc, c, b);
		madd(out, b, bc, (d4 - d3) / ((d4 - d3) + (d5 - d6)));
		return false;
	}
	float denom = 1.0f / (va + vb + vc);
	madd(out, a, ab, vb * denom);
	madd(out, out, ac, vc * denom);
	return true;
}

// Which side of a triangle the body is on: its own side, or where it was last tick if it has just
// crossed the triangle (fast and thin).
float sideOf(const Tri& t, const float* c, const float* last) {
	float rel[3];
	sub(rel, c, t.v[0]);
	float now = dot(t.n, rel);
	if (last[0] < 1e29f) {
		float relLast[3];
		sub(relLast, last, t.v[0]);
		float before = dot(t.n, relLast);
		if ((now < 0) != (before < 0) && std::fabs(before - now) > 1e-4f) {
			float s = before / (before - now), x[3], d[3];
			sub(d, c, last);
			madd(x, last, d, s);
			if (insideTri(t, x, 1.0f)) return before < 0 ? -1.0f : 1.0f;
		}
	}
	return now < 0 ? -1.0f : 1.0f;
}

void corners(const ffi::Obb& b, float v[8][3]) {
	for (int k = 0; k < 8; k++) {
		std::copy(b.center, b.center + 3, v[k]);
		for (int a = 0; a < 3; a++) madd(v[k], v[k], b.axes + 3 * a, ((k >> a) & 1 ? 1.0f : -1.0f) * b.half_extents[a]);
	}
}

void triBoxContact(const Tri& t, float side, const ffi::Obb& b, const float v[8][3], float margin, Collector& out) {
	float nn[3] = {t.n[0] * side, t.n[1] * side, t.n[2] * side};
	const float* ax[3] = {b.axes, b.axes + 3, b.axes + 6};
	const float* h = b.half_extents;
	float cen[3] = {(t.v[0][0] + t.v[1][0] + t.v[2][0]) / 3, (t.v[0][1] + t.v[1][1] + t.v[2][1]) / 3, (t.v[0][2] + t.v[1][2] + t.v[2][2]) / 3};
	float toBox[3];
	sub(toBox, b.center, cen);

	float faceDepth = 0, minDepth = 1e30f, minAxis[3] = {};
	bool minIsFace = false;
	auto test = [&](const float* axis, bool face) {
		float L[3] = {axis[0], axis[1], axis[2]};
		float l = len(L);
		if (l < 1e-4f) return true;
		for (float& x : L) x /= l;
		if (!face && dot(L, toBox) < 0)
			for (float& x : L) x = -x;
		float p0 = dot(t.v[0], L), p1 = dot(t.v[1], L), p2 = dot(t.v[2], L);
		float tmin = std::min({p0, p1, p2}), tmax = std::max({p0, p1, p2});
		float c = dot(b.center, L), r = h[0] * std::fabs(dot(ax[0], L)) + h[1] * std::fabs(dot(ax[1], L)) + h[2] * std::fabs(dot(ax[2], L));
		float depth = tmax - (c - r);
		if (depth < -margin || c + r < tmin - margin) return false;
		if (face) faceDepth = depth;
		if (depth < minDepth) minDepth = depth, std::copy(L, L + 3, minAxis), minIsFace = face;
		return true;
	};
	if (!test(nn, true)) return;
	for (auto* a : ax)
		if (!test(a, false)) return;
	for (int i = 0; i < 3; i++) {
		float e[3];
		sub(e, t.v[(i + 1) % 3], t.v[i]);
		for (auto* a : ax) {
			float L[3];
			cross(L, e, a);
			if (!test(L, false)) return;
		}
	}

	bool preferFace = minIsFace || faceDepth <= minDepth + kFaceBias;
	float bestAny = 1e30f;
	const float* anyV = nullptr;
	auto faceContact = [&](const float* vert, float dist) {
		ffi::Contact ct{};
		madd(ct.point, vert, nn, -dist);
		std::copy(nn, nn + 3, ct.normal);
		ct.depth = -dist;
		ct.surface = side > 0 ? t.idFront : t.idBack;
		out.add(ct);
	};
	if (preferFace) {
		float best = 1e30f;
		const float* bestV = nullptr;
		for (int k = 0; k < 8; k++) {
			float rel[3], q[3];
			sub(rel, v[k], t.v[0]);
			float dist = dot(nn, rel);
			if (dist >= margin) continue;
			if (dist < bestAny) bestAny = dist, anyV = v[k];
			if (dist >= best) continue;
			madd(q, v[k], nn, -dist);
			if (!insideTri(t, q, 1.0f)) continue;
			best = dist, bestV = v[k];
		}
		if (bestV) return faceContact(bestV, best);
	}

	float best = -1e30f;
	ffi::Contact ct{};
	for (int i = 0; i < 3; i++) {
		float rel[3], local[3];
		sub(rel, t.v[i], b.center);
		bool in = true;
		for (int a = 0; a < 3; a++) local[a] = dot(rel, ax[a]), in &= std::fabs(local[a]) < h[a] + margin;
		if (!in) continue;
		int axis = 0;
		float pen = 1e30f;
		for (int a = 0; a < 3; a++)
			if (h[a] - std::fabs(local[a]) < pen) pen = h[a] - std::fabs(local[a]), axis = a;
		if (pen <= best) continue;
		best = pen;
		float s = local[axis] < 0 ? -1.0f : 1.0f;
		for (int k = 0; k < 3; k++) ct.normal[k] = -s * ax[axis][k], ct.point[k] = t.v[i][k];
		ct.depth = pen;
		ct.surface = pointId(t.v[i], 0x56455254u);
	}
	if (best > -margin) {
		out.add(ct);
		return;
	}
	if (preferFace) {
		if (anyV) faceContact(anyV, bestAny);
		return;
	}
	int top = 0;
	for (int i = 1; i < 3; i++)
		if (dot(t.v[i], minAxis) > dot(t.v[top], minAxis)) top = i;
	std::copy(t.v[top], t.v[top] + 3, ct.point);
	std::copy(minAxis, minAxis + 3, ct.normal);
	ct.depth = minDepth;
	ct.surface = pointId(t.v[top], 0x45444745u);
	out.add(ct);
}

void closestOnSegment(const float* a, const float* b, const float* p, float* out) {
	float ab[3], ap[3];
	sub(ab, b, a);
	sub(ap, p, a);
	float l2 = dot(ab, ab);
	float s = l2 > 1e-8f ? std::clamp(dot(ap, ab) / l2, 0.0f, 1.0f) : 0.0f;
	madd(out, a, ab, s);
}

void capsuleBoxContact(const Capsule& cap, const ffi::Obb& b, float margin, Collector& out) {
	const float* ax[3] = {b.axes, b.axes + 3, b.axes + 6};
	float bestD = 1e30f, bestS[3] = {}, bestQ[3] = {};
	constexpr int kSamples = 16;
	for (int i = 0; i <= kSamples; i++) {
		float s[3], ab[3], rel[3], q[3];
		sub(ab, cap.b, cap.a);
		madd(s, cap.a, ab, float(i) / kSamples);
		sub(rel, s, b.center);
		std::copy(b.center, b.center + 3, q);
		for (int a = 0; a < 3; a++) madd(q, q, ax[a], std::clamp(dot(rel, ax[a]), -b.half_extents[a], b.half_extents[a]));
		float d[3];
		sub(d, q, s);
		float dl = len(d);
		if (dl < bestD) bestD = dl, std::copy(s, s + 3, bestS), std::copy(q, q + 3, bestQ);
	}
	if (bestD >= cap.r + margin) return;
	ffi::Contact ct{};
	if (bestD > 1e-3f) {
		for (int k = 0; k < 3; k++) ct.normal[k] = (bestQ[k] - bestS[k]) / bestD;
	} else {
		float rel[3];
		sub(rel, b.center, bestS);
		float rl = len(rel);
		if (rl < 1e-3f) return;
		for (int k = 0; k < 3; k++) ct.normal[k] = rel[k] / rl;
	}
	madd(ct.point, bestS, ct.normal, cap.r);
	ct.depth = cap.r - bestD;
	ct.surface = cap.id;
	out.add(ct);
}

bool rayTri(const Tri& t, const float* o, const float* dir, float maxDist, float& dist, float* n) {
	float e1[3], e2[3], p[3], q[3], s[3];
	sub(e1, t.v[1], t.v[0]);
	sub(e2, t.v[2], t.v[0]);
	cross(p, dir, e2);
	float det = dot(e1, p);
	if (std::fabs(det) < 1e-9f) return false;
	if (t.oneSided && dot(dir, t.n) >= 0) return false;
	float inv = 1.0f / det;
	sub(s, o, t.v[0]);
	float u = dot(s, p) * inv;
	if (u < 0 || u > 1) return false;
	cross(q, s, e1);
	float w = dot(dir, q) * inv;
	if (w < 0 || u + w > 1) return false;
	float d = dot(e2, q) * inv;
	if (d < 0 || d > maxDist) return false;
	dist = d;
	float sgn = dot(dir, t.n) < 0 ? 1.0f : -1.0f;
	for (int i = 0; i < 3; i++) n[i] = t.n[i] * sgn;
	return true;
}

bool raySphere(const float* o, const float* dir, const float* c, float r, float maxDist, float& t) {
	float oc[3];
	sub(oc, o, c);
	float b = dot(dir, oc), cc = dot(oc, oc) - r * r;
	if (cc < 0) return false;
	float h = b * b - cc;
	if (h < 0) return false;
	float d = -b - std::sqrt(h);
	if (d < 0 || d > maxDist) return false;
	t = d;
	return true;
}

bool rayCapsule(const Capsule& cap, const float* o, const float* dir, float maxDist, float& t, float* n) {
	float ba[3], oa[3];
	sub(ba, cap.b, cap.a);
	sub(oa, o, cap.a);
	float baba = dot(ba, ba), bard = dot(ba, dir), baoa = dot(ba, oa), rdoa = dot(dir, oa), oaoa = dot(oa, oa);
	float best = maxDist;
	bool hit = false;
	float a = baba - bard * bard;
	if (baba > 1e-6f && a > 1e-8f) {
		float b = baba * rdoa - baoa * bard, c = baba * oaoa - baoa * baoa - cap.r * cap.r * baba;
		float h = b * b - a * c;
		if (c > 0 && h >= 0) {
			float d = (-b - std::sqrt(h)) / a, y = baoa + d * bard;
			if (d >= 0 && d <= best && y > 0 && y < baba) best = d, hit = true;
		}
	}
	float d;
	for (const float* end : {cap.a, cap.b})
		if (raySphere(o, dir, end, cap.r, best, d)) best = d, hit = true;
	if (!hit) return false;
	t = best;
	float p[3], q[3];
	madd(p, o, dir, t);
	closestOnSegment(cap.a, cap.b, p, q);
	sub(n, p, q);
	float l = len(n);
	for (int i = 0; i < 3; i++) n[i] = l > 1e-6f ? n[i] / l : -dir[i];
	return true;
}
}

uint32_t planeId(const float* n, const float* p) {
	int q[4] = {int(std::lround(n[0] * 32)), int(std::lround(n[1] * 32)), int(std::lround(n[2] * 32)),
	            int(std::lround(dot(n, p) / 8.0f))};
	uint32_t h = 2166136261u;
	for (int v : q) {
		h ^= uint32_t(v);
		h *= 16777619u;
	}
	return h;
}

void Cache::clear() {
	tris.clear();
	capsules.clear();
	cells_.clear();
	stamp_.clear();
	nx_ = ny_ = 0;
	ready = false;
}

void Cache::setRegion(const float* l, const float* h) {
	std::copy(l, l + 3, lo);
	std::copy(h, h + 3, hi);
}

void Cache::addTri(const float* a, const float* b, const float* c, bool oneSided) {
	Tri t{};
	std::copy(a, a + 3, t.v[0]);
	std::copy(b, b + 3, t.v[1]);
	std::copy(c, c + 3, t.v[2]);
	float e1[3], e2[3];
	sub(e1, b, a);
	sub(e2, c, a);
	cross(t.n, e1, e2);
	float l = len(t.n);
	if (!(l > 1e-4f)) return;
	for (float& x : t.n) x /= l;
	for (int i = 0; i < 3; i++) {
		t.lo[i] = std::min({a[i], b[i], c[i]});
		t.hi[i] = std::max({a[i], b[i], c[i]});
	}
	float back[3] = {-t.n[0], -t.n[1], -t.n[2]};
	t.idFront = planeId(t.n, a);
	t.idBack = planeId(back, a);
	t.oneSided = oneSided;
	tris.push_back(t);
}

void Cache::addBox(const float* c, const float (*ax)[3], const float* h) {
	float v[8][3];
	for (int k = 0; k < 8; k++)
		for (int i = 0; i < 3; i++)
			v[k][i] = c[i] + ax[0][i] * h[0] * ((k & 1) ? 1 : -1) + ax[1][i] * h[1] * ((k & 2) ? 1 : -1) + ax[2][i] * h[2] * ((k & 4) ? 1 : -1);
	static const int kFaces[6][4] = {{0, 1, 3, 2}, {4, 5, 7, 6}, {0, 1, 5, 4}, {2, 3, 7, 6}, {0, 2, 6, 4}, {1, 3, 7, 5}};
	for (auto& f : kFaces) {
		float e1[3], e2[3], n[3], mid[3], out[3];
		sub(e1, v[f[1]], v[f[0]]);
		sub(e2, v[f[2]], v[f[0]]);
		cross(n, e1, e2);
		for (int i = 0; i < 3; i++) mid[i] = (v[f[0]][i] + v[f[2]][i]) * 0.5f;
		sub(out, mid, c);
		if (dot(n, out) >= 0) {
			addTri(v[f[0]], v[f[1]], v[f[2]], true);
			addTri(v[f[0]], v[f[2]], v[f[3]], true);
		} else {
			addTri(v[f[0]], v[f[2]], v[f[1]], true);
			addTri(v[f[0]], v[f[3]], v[f[2]], true);
		}
	}
}

void Cache::addCapsule(const float* a, const float* b, float r) {
	Capsule cap{};
	std::copy(a, a + 3, cap.a);
	std::copy(b, b + 3, cap.b);
	cap.r = r;
	for (int i = 0; i < 3; i++) {
		cap.lo[i] = std::min(a[i], b[i]) - r;
		cap.hi[i] = std::max(a[i], b[i]) + r;
	}
	float mid[3] = {(a[0] + b[0]) * 0.5f, (a[1] + b[1]) * 0.5f, (a[2] + b[2]) * 0.5f};
	cap.id = pointId(mid, 0x43415053u);
	capsules.push_back(cap);
}

void Cache::addConvex(const std::vector<std::array<float, 4>>& planes, const float* plo, const float* phi) {
	float ext[3] = {phi[0] - plo[0], phi[1] - plo[1], phi[2] - plo[2]};
	float diag = len(ext) + 1.0f;
	float mid[3] = {(plo[0] + phi[0]) * 0.5f, (plo[1] + phi[1]) * 0.5f, (plo[2] + phi[2]) * 0.5f};
	for (size_t i = 0; i < planes.size(); i++) {
		const auto& pl = planes[i];
		float n[3] = {pl[0], pl[1], pl[2]};
		float nl = len(n);
		if (nl < 1e-6f) continue;
		float dist = (dot(n, mid) + pl[3]) / (nl * nl);
		float o[3];
		madd(o, mid, n, -dist);
		float ref[3] = {std::fabs(n[2]) < 0.9f * nl ? 0.0f : 1.0f, 0.0f, std::fabs(n[2]) < 0.9f * nl ? 1.0f : 0.0f};
		float t1[3], t2[3];
		cross(t1, ref, n);
		float l1 = len(t1);
		for (float& x : t1) x /= l1;
		cross(t2, n, t1);
		float l2 = len(t2);
		for (float& x : t2) x /= l2;
		std::vector<std::array<float, 3>> poly;
		const float s1[4] = {-1, 1, 1, -1}, s2[4] = {-1, -1, 1, 1};
		for (int q = 0; q < 4; q++)
			poly.push_back({o[0] + (t1[0] * s1[q] + t2[0] * s2[q]) * diag, o[1] + (t1[1] * s1[q] + t2[1] * s2[q]) * diag,
			                o[2] + (t1[2] * s1[q] + t2[2] * s2[q]) * diag});
		for (size_t j = 0; j < planes.size() && poly.size() >= 3; j++) {
			if (j == i) continue;
			const auto& cp = planes[j];
			std::vector<std::array<float, 3>> clipped;
			for (size_t v = 0; v < poly.size(); v++) {
				const auto& A = poly[v];
				const auto& B = poly[(v + 1) % poly.size()];
				float da = cp[0] * A[0] + cp[1] * A[1] + cp[2] * A[2] + cp[3];
				float db = cp[0] * B[0] + cp[1] * B[1] + cp[2] * B[2] + cp[3];
				if (da <= 0) clipped.push_back(A);
				if ((da <= 0) != (db <= 0)) {
					float t = da / (da - db);
					clipped.push_back({A[0] + (B[0] - A[0]) * t, A[1] + (B[1] - A[1]) * t, A[2] + (B[2] - A[2]) * t});
				}
			}
			poly.swap(clipped);
		}
		for (size_t v = 1; v + 1 < poly.size(); v++) {
			float e1[3], e2[3], fn[3];
			sub(e1, poly[v].data(), poly[0].data());
			sub(e2, poly[v + 1].data(), poly[0].data());
			cross(fn, e1, e2);
			if (dot(fn, n) >= 0)
				addTri(poly[0].data(), poly[v].data(), poly[v + 1].data(), true);
			else
				addTri(poly[0].data(), poly[v + 1].data(), poly[v].data(), true);
		}
	}
}

void Cache::cellRange(const float* l, const float* h, int& x0, int& y0, int& x1, int& y1) const {
	auto cell = [&](float v, float base, int n) { return std::clamp(int(std::floor((v - base) / kCell)), 0, n - 1); };
	x0 = cell(l[0], lo[0], nx_), x1 = cell(h[0], lo[0], nx_);
	y0 = cell(l[1], lo[1], ny_), y1 = cell(h[1], lo[1], ny_);
}

void Cache::build() {
	nx_ = std::clamp(int(std::ceil((hi[0] - lo[0]) / kCell)), 1, 1024);
	ny_ = std::clamp(int(std::ceil((hi[1] - lo[1]) / kCell)), 1, 1024);
	cells_.assign(size_t(nx_) * ny_, {});
	for (uint32_t i = 0; i < tris.size(); i++) {
		int x0, y0, x1, y1;
		cellRange(tris[i].lo, tris[i].hi, x0, y0, x1, y1);
		for (int y = y0; y <= y1; y++)
			for (int x = x0; x <= x1; x++) cells_[size_t(y) * nx_ + x].push_back(i);
	}
	stamp_.assign(tris.size(), 0);
	query_ = 0;
	ready = true;
}

bool Cache::covers(const float* p, float r) const {
	if (!ready) return false;
	for (int i = 0; i < 3; i++)
		if (p[i] - r < lo[i] || p[i] + r > hi[i]) return false;
	return true;
}

void Cache::translate(const float* d) {
	for (auto& t : tris)
		for (int i = 0; i < 3; i++) {
			for (auto& v : t.v) v[i] += d[i];
			t.lo[i] += d[i], t.hi[i] += d[i];
		}
	for (auto& t : tris) {
		float back[3] = {-t.n[0], -t.n[1], -t.n[2]};
		t.idFront = planeId(t.n, t.v[0]);
		t.idBack = planeId(back, t.v[0]);
	}
	for (auto& c : capsules) {
		for (int i = 0; i < 3; i++) c.a[i] += d[i], c.b[i] += d[i], c.lo[i] += d[i], c.hi[i] += d[i];
		float mid[3] = {(c.a[0] + c.b[0]) * 0.5f, (c.a[1] + c.b[1]) * 0.5f, (c.a[2] + c.b[2]) * 0.5f};
		c.id = pointId(mid, 0x43415053u);
	}
	for (int i = 0; i < 3; i++) lo[i] += d[i], hi[i] += d[i];
}

bool SkyWorld::rayCache(const Cache& c, const float* o, const float* dir, float maxDist, float& t, float* normal) {
	float lo[3], hi[3];
	for (int i = 0; i < 3; i++) {
		float e = o[i] + dir[i] * maxDist;
		lo[i] = std::min(o[i], e), hi[i] = std::max(o[i], e);
	}
	float best = maxDist, n[3];
	bool hit = false;
	c.forTris(lo, hi, [&](const Tri& tri) {
		float d;
		if (rayTri(tri, o, dir, best, d, n)) best = d, std::copy(n, n + 3, normal), hit = true;
	});
	for (auto& cap : c.capsules) {
		if (cap.lo[0] > hi[0] || cap.hi[0] < lo[0] || cap.lo[1] > hi[1] || cap.hi[1] < lo[1] || cap.lo[2] > hi[2] || cap.hi[2] < lo[2]) continue;
		float d;
		if (rayCapsule(cap, o, dir, best, d, n)) best = d, std::copy(n, n + 3, normal), hit = true;
	}
	t = best;
	return hit;
}

bool SkyWorld::probe(const Cache& c, const float* from, const float* to, float* point, float* normal) {
	float d[3];
	sub(d, to, from);
	float l = len(d);
	if (l < 1e-4f) return false;
	for (float& x : d) x /= l;
	float t;
	queries++;
	if (!rayCache(c, from, d, l, t, normal)) return false;
	madd(point, from, d, t);
	return true;
}

void SkyWorld::forget() {
	carRamps_ = {};
	ballRamps_ = {};
	std::fill(lastBox_, lastBox_ + 3, 1e30f);
	std::fill(lastSphere_, lastSphere_ + 3, 1e30f);
}

bool SkyWorld::needsRefresh(RampState& s, const float* center) {
	float d[3] = {center[0] - s.at[0], center[1] - s.at[1], center[2] - s.at[2]};
	return ++s.age >= refreshTicks || len(d) > refreshDistance;
}

void SkyWorld::addHit(RampState& s, const float* point, const float* normal, const float* rayDir) {
	float nl = len(normal);
	if (!(nl > 0.5f)) return;
	float n[3] = {normal[0] / nl, normal[1] / nl, normal[2] / nl};
	if (dot(n, rayDir) > -0.05f) return;
	for (auto& pl : s.planes) {
		float diff[3] = {point[0] - pl.p[0], point[1] - pl.p[1], point[2] - pl.p[2]};
		if (dot(pl.n, n) > 0.9986f && std::fabs(dot(pl.n, diff)) < 3.0f) return;
	}
	Plane pl{};
	for (int i = 0; i < 3; i++) pl.p[i] = point[i], pl.n[i] = n[i];
	pl.id = planeId(pl.n, pl.p);
	s.planes.push_back(pl);
}

void SkyWorld::probeAndAdd(const Cache& c, RampState& s, const float* from, const float* dir, float l) {
	float to[3], p[3], n[3];
	madd(to, from, dir, l);
	if (probe(c, from, to, p, n)) addHit(s, p, n, dir);
}

void SkyWorld::probeForWalls(const Cache& c, RampState& s, const float* center, float reach) {
	for (int k = 0; k < 8; k++) {
		float ang = float(k) * 0.785398163f;
		float dir[3] = {std::cos(ang), std::sin(ang), 0};
		probeAndAdd(c, s, center, dir, reach);
	}
}

void SkyWorld::buildRamps(RampState& s) {
	s.ramps.clear();
	if (!wallRamps) return;
	const float R = rampRadius;
	for (auto& f : s.planes) {
		if (f.n[2] < 0.7f) continue;
		for (auto& w : s.planes) {
			if (std::fabs(w.n[2]) > 0.35f) continue;
			float fw[3], wf[3];
			sub(fw, f.p, w.p);
			sub(wf, w.p, f.p);
			if (dot(w.n, fw) <= 0 || dot(f.n, wf) <= -1) continue;
			Ramp r{};
			cross(r.d, f.n, w.n);
			float dl = len(r.d);
			if (dl < 0.2f) continue;
			for (float& v : r.d) v /= dl;
			float bF = dot(f.n, f.p) + R, bW = dot(w.n, w.p) + R, ff = dot(f.n, f.n), ww = dot(w.n, w.n), fwn = dot(f.n, w.n);
			float det = ff * ww - fwn * fwn;
			for (int i = 0; i < 3; i++) r.x0[i] = ((bF * ww - bW * fwn) * f.n[i] + (bW * ff - bF * fwn) * w.n[i]) / det;
			std::copy(f.n, f.n + 3, r.nFloor);
			std::copy(w.n, w.n + 3, r.nWall);
			std::copy(w.p, w.p + 3, r.wallPoint);
			r.id = (f.id * 31u) ^ (w.id * 0x9E3779B9u) ^ 0x52414D50u;
			s.ramps.push_back(r);
		}
	}
}

bool SkyWorld::wallBehind(const Cache& c, const Ramp& r, const float* q) {
	float to[3], p[3], n[3];
	madd(to, q, r.nWall, -(rampRadius + 30.0f));
	if (!probe(c, q, to, p, n)) return false;
	float off[3];
	sub(off, p, r.wallPoint);
	return std::fabs(dot(r.nWall, off)) < 6.0f && dot(n, r.nWall) > 0.9f;
}

bool SkyWorld::rayRamp(const Ramp& r, const float* o, const float* dir, float maxDist, float& t, float* n) const {
	float rel[3], op[3], dp[3];
	sub(rel, o, r.x0);
	perp(op, rel, r.d);
	perp(dp, dir, r.d);
	float a = dot(dp, dp), b = dot(op, dp), cc = dot(op, op) - rampRadius * rampRadius;
	if (a < 1e-8f) return false;
	if (cc > 0 && dot(op, r.nFloor) <= 0 && dot(op, r.nWall) <= 0) return false;
	float disc = b * b - a * cc;
	if (disc < 0) return false;
	t = (-b + std::sqrt(disc)) / a;
	if (t < 0 || t > maxDist) return false;
	float q[3];
	madd(q, op, dp, t);
	if (dot(q, r.nFloor) > 0 || dot(q, r.nWall) > 0) return false;
	for (int i = 0; i < 3; i++) n[i] = -q[i] / rampRadius;
	return true;
}

uint32_t SkyWorld::raycast(const float* origin, const float* dir, float maxDist, ffi::RayHit* hit) {
	const Cache& c = *boxCache;
	float t, n[3];
	queries++;
	bool got = rayCache(c, origin, dir, maxDist, t, n);
	if (got) {
		hit->distance = t;
		madd(hit->point, origin, dir, t);
		std::copy(n, n + 3, hit->normal);
	}
	for (auto& r : carRamps_.ramps) {
		float rt, rn[3];
		if (!rayRamp(r, origin, dir, got ? hit->distance : maxDist, rt, rn)) continue;
		float q[3];
		madd(q, origin, dir, rt);
		if (!wallBehind(c, r, q)) continue;
		hit->distance = rt;
		std::copy(q, q + 3, hit->point);
		std::copy(rn, rn + 3, hit->normal);
		got = true;
	}
	return got ? 1 : 0;
}

uint32_t SkyWorld::boxContacts(const ffi::Obb& obb, float margin, ffi::Contact* out, uint32_t cap) {
	const Cache& cache = *boxCache;
	const float* c = obb.center;
	const float* ax[3] = {obb.axes, obb.axes + 3, obb.axes + 6};
	const float* he = obb.half_extents;
	queries++;
	if (wallRamps && needsRefresh(carRamps_, c)) {
		carRamps_.planes.clear();
		carRamps_.age = 0;
		std::copy(c, c + 3, carRamps_.at);
		for (auto& d : kDirs.d) {
			float surf[3] = {c[0], c[1], c[2]}, dir[3] = {0, 0, 0};
			for (int a = 0; a < 3; a++) {
				madd(surf, surf, ax[a], d[a] * he[a]);
				madd(dir, dir, ax[a], d[a]);
			}
			float l = len(dir);
			for (float& v : dir) v /= l;
			float to[3], p[3], n[3];
			madd(to, surf, dir, margin + lookahead);
			if (probe(cache, c, to, p, n)) addHit(carRamps_, p, n, dir);
		}
		float down[3] = {0, 0, -1};
		probeAndAdd(cache, carRamps_, c, down, rampRadius + 100.0f);
		probeForWalls(cache, carRamps_, c, rampRadius + 2 * he[0] + 60.0f);
		buildRamps(carRamps_);
	} else if (!wallRamps) {
		carRamps_.ramps.clear();
	}
	Collector col{out, cap};
	for (auto& r : carRamps_.ramps) {
		float best = -1e30f, bestV[3] = {}, bestQ[3] = {};
		for (int k = 0; k < 8; k++) {
			float v[3] = {c[0], c[1], c[2]};
			for (int a = 0; a < 3; a++) madd(v, v, ax[a], ((k >> a) & 1 ? 1.0f : -1.0f) * he[a]);
			float rel[3], q[3];
			sub(rel, v, r.x0);
			perp(q, rel, r.d);
			if (dot(q, r.nFloor) > 0 || dot(q, r.nWall) > 0) continue;
			float depth = len(q) - rampRadius;
			if (depth > best) best = depth, std::copy(v, v + 3, bestV), std::copy(q, q + 3, bestQ);
		}
		if (best <= -margin) continue;
		float ql = len(bestQ);
		if (ql < 1e-3f) continue;
		ffi::Contact ct{};
		for (int i = 0; i < 3; i++) {
			ct.normal[i] = -bestQ[i] / ql;
			ct.point[i] = bestV[i] + ct.normal[i] * best;
		}
		if (!wallBehind(cache, r, ct.point)) continue;
		ct.depth = best;
		ct.surface = r.id;
		col.add(ct);
	}

	float v[8][3];
	corners(obb, v);
	float lo[3], hi[3];
	for (int i = 0; i < 3; i++) {
		float e = std::fabs(ax[0][i]) * he[0] + std::fabs(ax[1][i]) * he[1] + std::fabs(ax[2][i]) * he[2] + std::max(margin, 0.0f) + 1.0f;
		lo[i] = c[i] - e, hi[i] = c[i] + e;
	}
	cache.forTris(lo, hi, [&](const Tri& t) {
		float side = sideOf(t, c, lastBox_);
		if (t.oneSided && side < 0) return;
		triBoxContact(t, side, obb, v, margin, col);
	});
	for (auto& cp : cache.capsules) {
		if (cp.lo[0] > hi[0] || cp.hi[0] < lo[0] || cp.lo[1] > hi[1] || cp.hi[1] < lo[1] || cp.lo[2] > hi[2] || cp.hi[2] < lo[2]) continue;
		capsuleBoxContact(cp, obb, margin, col);
	}
	std::copy(c, c + 3, lastBox_);
	sortById(out, col.n);
	return col.n;
}

uint32_t SkyWorld::sphereContacts(const float* c, float r, float margin, ffi::Contact* out, uint32_t cap) {
	const Cache& cache = *sphereCache;
	queries++;
	if (wallRamps && needsRefresh(ballRamps_, c)) {
		ballRamps_.planes.clear();
		ballRamps_.age = 0;
		std::copy(c, c + 3, ballRamps_.at);
		for (auto& d : kDirs.d) {
			float dir[3] = {d[0], d[1], d[2]};
			float l = len(dir);
			for (float& x : dir) x /= l;
			probeAndAdd(cache, ballRamps_, c, dir, r + margin + sphereLookahead);
		}
		float down[3] = {0, 0, -1};
		probeAndAdd(cache, ballRamps_, c, down, rampRadius + r + 100.0f);
		probeForWalls(cache, ballRamps_, c, rampRadius + r + 60.0f);
		buildRamps(ballRamps_);
	} else if (!wallRamps) {
		ballRamps_.ramps.clear();
	}
	Collector col{out, cap};
	for (auto& rp : ballRamps_.ramps) {
		float rel[3], q[3];
		sub(rel, c, rp.x0);
		perp(q, rel, rp.d);
		if (dot(q, rp.nFloor) > 0 || dot(q, rp.nWall) > 0) continue;
		float ql = len(q), depth = ql + r - rampRadius;
		if (depth <= -margin || ql < 1e-3f) continue;
		ffi::Contact ct{};
		for (int i = 0; i < 3; i++) {
			ct.normal[i] = -q[i] / ql;
			ct.point[i] = c[i] - ct.normal[i] * (r - depth);
		}
		if (!wallBehind(cache, rp, ct.point)) continue;
		ct.depth = depth;
		ct.surface = rp.id;
		col.add(ct);
	}

	float reach = r + std::max(margin, 0.0f) + 1.0f;
	float lo[3] = {c[0] - reach, c[1] - reach, c[2] - reach}, hi[3] = {c[0] + reach, c[1] + reach, c[2] + reach};
	cache.forTris(lo, hi, [&](const Tri& t) {
		float side = sideOf(t, c, lastSphere_);
		if (t.oneSided && side < 0) return;
		float q[3], d[3];
		closestOnTri(t, c, q);
		sub(d, c, q);
		float dl = len(d);
		if (dl >= r + margin) return;
		ffi::Contact ct{};
		if (dl > 1e-3f) {
			for (int i = 0; i < 3; i++) ct.normal[i] = d[i] / dl;
			if (dot(ct.normal, t.n) * side < 0) {
				for (int i = 0; i < 3; i++) ct.normal[i] = t.n[i] * side;
				dl = -dl;
			}
		} else {
			for (int i = 0; i < 3; i++) ct.normal[i] = t.n[i] * side;
		}
		std::copy(q, q + 3, ct.point);
		ct.depth = r - dl;
		ct.surface = planeId(ct.normal, ct.point);
		col.add(ct);
	});
	for (auto& cp : cache.capsules) {
		if (cp.lo[0] > hi[0] || cp.hi[0] < lo[0] || cp.lo[1] > hi[1] || cp.hi[1] < lo[1] || cp.lo[2] > hi[2] || cp.hi[2] < lo[2]) continue;
		float q[3], d[3];
		closestOnSegment(cp.a, cp.b, c, q);
		sub(d, c, q);
		float dl = len(d);
		if (dl >= cp.r + r + margin || dl < 1e-3f) continue;
		ffi::Contact ct{};
		for (int i = 0; i < 3; i++) ct.normal[i] = d[i] / dl;
		madd(ct.point, q, ct.normal, cp.r);
		ct.depth = cp.r + r - dl;
		ct.surface = cp.id;
		col.add(ct);
	}
	std::copy(c, c + 3, lastSphere_);
	sortById(out, col.n);
	return col.n;
}

uint32_t SkyWorld::cbRaycast(void* u, const float* o, const float* d, float m, ffi::RayHit* h) {
	return static_cast<SkyWorld*>(u)->raycast(o, d, m, h);
}
uint32_t SkyWorld::cbBox(void* u, const ffi::Obb* obb, float margin, ffi::Contact* out, uint32_t cap) {
	return static_cast<SkyWorld*>(u)->boxContacts(*obb, margin, out, cap);
}
uint32_t SkyWorld::cbSphere(void* u, const float* c, float r, float margin, ffi::Contact* out, uint32_t cap) {
	return static_cast<SkyWorld*>(u)->sphereContacts(c, r, margin, out, cap);
}

}
