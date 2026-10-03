#include "probe_world.h"
#include <algorithm>
#include <cmath>

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

void sortById(ffi::Contact* out, uint32_t n) {
	std::stable_sort(out, out + n, [](const ffi::Contact& a, const ffi::Contact& b) { return a.surface < b.surface; });
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

void ProbeWorld::probeForWalls(Cache& c, const float* center, const float* up, float reach) {
	float ref[3] = {1, 0, 0};
	if (std::fabs(dot(ref, up)) > 0.9f) ref[0] = 0, ref[1] = 1;
	float a[3], b[3];
	perp(a, ref, up);
	float l = len(a);
	for (float& v : a) v /= l;
	cross(b, up, a);
	for (int k = 0; k < 8; k++) {
		float ang = float(k) * 0.785398163f, dir[3], to[3];
		for (int i = 0; i < 3; i++) dir[i] = a[i] * std::cos(ang) + b[i] * std::sin(ang);
		madd(to, center, dir, reach);
		ProbeHit h;
		probes++;
		if (probe_(center, to, h)) addHit(c, h, dir);
	}
}

void ProbeWorld::buildRamps(Cache& c) {
	c.ramps.clear();
	if (!wallRamps) return;
	const float R = rampRadius;
	for (auto& f : c.planes) {
		if (f.n[2] < 0.7f) continue;
		for (auto& w : c.planes) {
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
			c.ramps.push_back(r);
		}
	}
}

bool ProbeWorld::wallBehind(const Ramp& r, const float* q) {
	float to[3];
	madd(to, q, r.nWall, -(rampRadius + 30.0f));
	ProbeHit h;
	probes++;
	if (!probe_(q, to, h)) return false;
	float off[3];
	sub(off, h.point, r.wallPoint);
	return std::fabs(dot(r.nWall, off)) < 6.0f && dot(h.normal, r.nWall) > 0.9f;
}

bool ProbeWorld::rayRamp(const Ramp& r, const float* o, const float* dir, float maxDist, float& t, float* n) const {
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

uint32_t ProbeWorld::raycast(const float* origin, const float* dir, float maxDist, ffi::RayHit* hit) {
	float to[3];
	madd(to, origin, dir, maxDist);
	ProbeHit h;
	probes++;
	bool got = false;
	if (probe_(origin, to, h)) {
		float d[3] = {h.point[0] - origin[0], h.point[1] - origin[1], h.point[2] - origin[2]};
		float dist = dot(d, dir);
		if (dist >= 0 && dist <= maxDist) {
			hit->distance = dist;
			for (int i = 0; i < 3; i++) hit->point[i] = h.point[i], hit->normal[i] = h.normal[i];
			got = true;
		}
	}
	for (auto& r : boxCache_.ramps) {
		float t, n[3];
		if (!rayRamp(r, origin, dir, got ? hit->distance : maxDist, t, n)) continue;
		float q[3];
		madd(q, origin, dir, t);
		if (!wallBehind(r, q)) continue;
		hit->distance = t;
		std::copy(q, q + 3, hit->point);
		std::copy(n, n + 3, hit->normal);
		got = true;
	}
	return got ? 1 : 0;
}

bool ProbeWorld::needsRefresh(Cache& c, const float* center) {
	float d[3] = {center[0] - c.at[0], center[1] - c.at[1], center[2] - c.at[2]};
	return ++c.age >= refreshTicks || len(d) > refreshDistance;
}

void ProbeWorld::addHit(Cache& c, const ProbeHit& h, const float* rayDir) {
	float nl = len(h.normal);
	if (!(nl > 0.5f)) return;
	float n[3] = {h.normal[0] / nl, h.normal[1] / nl, h.normal[2] / nl};
	if (dot(n, rayDir) > -0.05f) return;
	c.hits.push_back(h);
	for (auto& pl : c.planes) {
		float diff[3] = {h.point[0] - pl.p[0], h.point[1] - pl.p[1], h.point[2] - pl.p[2]};
		if (dot(pl.n, n) > 0.9986f && std::fabs(dot(pl.n, diff)) < 3.0f) return;
	}
	Plane pl{};
	for (int i = 0; i < 3; i++) pl.p[i] = h.point[i], pl.n[i] = n[i];
	pl.id = planeId(pl.n, pl.p);
	c.planes.push_back(pl);
}

uint32_t ProbeWorld::boxContacts(const ffi::Obb& obb, float margin, ffi::Contact* out, uint32_t cap) {
	const float* c = obb.center;
	const float* ax[3] = {obb.axes, obb.axes + 3, obb.axes + 6};
	const float* he = obb.half_extents;
	if (needsRefresh(boxCache_, c)) {
		boxCache_.planes.clear();
		boxCache_.hits.clear();
		boxCache_.age = 0;
		std::copy(c, c + 3, boxCache_.at);
		for (auto& d : kDirs.d) {
			float surf[3] = {c[0], c[1], c[2]}, dir[3] = {0, 0, 0};
			for (int a = 0; a < 3; a++) {
				madd(surf, surf, ax[a], d[a] * he[a]);
				madd(dir, dir, ax[a], d[a]);
			}
			float l = len(dir);
			for (float& v : dir) v /= l;
			float to[3];
			madd(to, surf, dir, margin + lookahead);
			ProbeHit h;
			probes++;
			if (probe_(c, to, h)) addHit(boxCache_, h, dir);
		}
		if (wallRamps) {
			float down[3] = {0, 0, -1}, to[3];
			madd(to, c, down, rampRadius + 100.0f);
			ProbeHit h;
			probes++;
			if (probe_(c, to, h)) addHit(boxCache_, h, down);
			float up[3] = {0, 0, 1};
			probeForWalls(boxCache_, c, up, rampRadius + 2 * he[0] + 60.0f);
		}
		buildRamps(boxCache_);
	}
	uint32_t n = 0;
	for (auto& r : boxCache_.ramps) {
		if (n >= cap) break;
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
		if (!wallBehind(r, ct.point)) continue;
		ct.depth = best;
		ct.surface = r.id;
		out[n++] = ct;
	}
	for (auto& pl : boxCache_.planes) {
		if (n >= cap) break;
		float deepest[3] = {c[0], c[1], c[2]};
		for (int a = 0; a < 3; a++) madd(deepest, deepest, ax[a], (dot(ax[a], pl.n) >= 0 ? -1.0f : 1.0f) * he[a]);
		float rel[3] = {deepest[0] - pl.p[0], deepest[1] - pl.p[1], deepest[2] - pl.p[2]};
		float dist = dot(pl.n, rel);
		if (dist >= margin) continue;
		float from[3], to[3];
		madd(from, deepest, pl.n, std::max(margin, 0.0f) + 15.0f);
		madd(to, deepest, pl.n, -(std::max(-dist, 0.0f) + 15.0f));
		ProbeHit h;
		probes++;
		if (!probe_(from, to, h)) continue;
		float off[3] = {h.point[0] - pl.p[0], h.point[1] - pl.p[1], h.point[2] - pl.p[2]};
		if (std::fabs(dot(pl.n, off)) > 4.0f || dot(h.normal, pl.n) < 0.9f) continue;
		ffi::Contact& ct = out[n++];
		madd(ct.point, deepest, pl.n, -dist);
		std::copy(pl.n, pl.n + 3, ct.normal);
		ct.depth = -dist;
		ct.surface = pl.id;
	}
	sortById(out, n);
	return n;
}

uint32_t ProbeWorld::sphereContacts(const float* c, float r, float margin, ffi::Contact* out, uint32_t cap) {
	if (needsRefresh(sphereCache_, c)) {
		sphereCache_.planes.clear();
		sphereCache_.hits.clear();
		sphereCache_.age = 0;
		std::copy(c, c + 3, sphereCache_.at);
		for (auto& d : kDirs.d) {
			float dir[3] = {d[0], d[1], d[2]};
			float l = len(dir);
			for (float& v : dir) v /= l;
			float to[3];
			madd(to, c, dir, r + margin + sphereLookahead);
			ProbeHit h;
			probes++;
			if (probe_(c, to, h)) addHit(sphereCache_, h, dir);
		}
		if (wallRamps) {
			float up[3] = {0, 0, 1}, down[3] = {0, 0, -1}, to[3];
			madd(to, c, down, rampRadius + r + 100.0f);
			ProbeHit h;
			probes++;
			if (probe_(c, to, h)) addHit(sphereCache_, h, down);
			probeForWalls(sphereCache_, c, up, rampRadius + r + 60.0f);
		}
		buildRamps(sphereCache_);
	}
	uint32_t n = 0;
	for (auto& rp : sphereCache_.ramps) {
		if (n >= cap) break;
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
		if (!wallBehind(rp, ct.point)) continue;
		ct.depth = depth;
		ct.surface = rp.id;
		out[n++] = ct;
	}
	for (auto& pl : sphereCache_.planes) {
		if (n >= cap) break;
		float rel[3] = {c[0] - pl.p[0], c[1] - pl.p[1], c[2] - pl.p[2]};
		float centerDist = dot(pl.n, rel);
		float dist = centerDist - r;
		if (dist >= margin) continue;
		float foot[3];
		madd(foot, c, pl.n, -centerDist);
		float from[3], to[3];
		madd(from, c, pl.n, 0.0f);
		madd(to, c, pl.n, -(centerDist + 10.0f));
		ProbeHit h;
		probes++;
		ffi::Contact ct{};
		ct.surface = pl.id;
		bool face = probe_(from, to, h);
		if (face) {
			float off[3] = {h.point[0] - pl.p[0], h.point[1] - pl.p[1], h.point[2] - pl.p[2]};
			face = std::fabs(dot(pl.n, off)) < 4.0f && dot(h.normal, pl.n) > 0.9f;
		}
		if (face) {
			std::copy(foot, foot + 3, ct.point);
			std::copy(pl.n, pl.n + 3, ct.normal);
			ct.depth = -dist;
		} else {
			const ProbeHit* best = nullptr;
			float bestD = 1e30f;
			for (auto& ph : sphereCache_.hits) {
				float v[3] = {c[0] - ph.point[0], c[1] - ph.point[1], c[2] - ph.point[2]};
				float dd = len(v);
				if (dd < bestD) bestD = dd, best = &ph;
			}
			if (!best || bestD - r >= margin || bestD < 1e-3f) continue;
			for (int i = 0; i < 3; i++) ct.point[i] = best->point[i], ct.normal[i] = (c[i] - best->point[i]) / bestD;
			ct.depth = r - bestD;
			ct.surface = planeId(ct.normal, ct.point);
		}
		out[n++] = ct;
	}
	sortById(out, n);
	return n;
}

uint32_t ProbeWorld::cbRaycast(void* u, const float* o, const float* d, float m, ffi::RayHit* h) {
	return static_cast<ProbeWorld*>(u)->raycast(o, d, m, h);
}
uint32_t ProbeWorld::cbBox(void* u, const ffi::Obb* obb, float margin, ffi::Contact* out, uint32_t cap) {
	return static_cast<ProbeWorld*>(u)->boxContacts(*obb, margin, out, cap);
}
uint32_t ProbeWorld::cbSphere(void* u, const float* c, float r, float margin, ffi::Contact* out, uint32_t cap) {
	return static_cast<ProbeWorld*>(u)->sphereContacts(c, r, margin, out, cap);
}
