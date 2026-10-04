#pragma once
#include "rlcar_ffi.h"
#include <algorithm>
#include <array>
#include <cstdint>
#include <vector>

// The core's collision from Skyrim geometry copied out of Havok (havok_world.*): triangles and
// capsules near the car or the ball, in Rocket League space. Boxes and convex hulls are stored as
// their outward triangles. Answers the three rlcar callbacks exactly, plus the wall quarter-pipes.
namespace sky {

struct Tri {
	float v[3][3];
	float n[3];
	float lo[3], hi[3];
	uint32_t idFront, idBack;
	bool oneSided;
};

struct Capsule {
	float a[3], b[3];
	float r;
	float lo[3], hi[3];
	uint32_t id;
};

class Cache {
public:
	void clear();
	void setRegion(const float* lo, const float* hi);
	void addTri(const float* a, const float* b, const float* c, bool oneSided);
	void addBox(const float* c, const float (*ax)[3], const float* h);
	void addCapsule(const float* a, const float* b, float r);
	void addConvex(const std::vector<std::array<float, 4>>& planes, const float* lo, const float* hi);
	void build();
	bool covers(const float* p, float r) const;
	void translate(const float* d);

	template <class F>
	void forTris(const float* lo, const float* hi, F&& f) const {
		if (!ready) return;
		int x0, y0, x1, y1;
		cellRange(lo, hi, x0, y0, x1, y1);
		if (++query_ == 0) std::fill(stamp_.begin(), stamp_.end(), 0u), query_ = 1;
		for (int y = y0; y <= y1; y++)
			for (int x = x0; x <= x1; x++)
				for (uint32_t i : cells_[size_t(y) * nx_ + x]) {
					if (stamp_[i] == query_) continue;
					stamp_[i] = query_;
					const Tri& t = tris[i];
					if (t.lo[0] > hi[0] || t.hi[0] < lo[0] || t.lo[1] > hi[1] || t.hi[1] < lo[1] || t.lo[2] > hi[2] || t.hi[2] < lo[2]) continue;
					f(t);
				}
	}

	std::vector<Tri> tris;
	std::vector<Capsule> capsules;
	float lo[3] = {0, 0, 0}, hi[3] = {0, 0, 0};
	bool ready = false;
	static constexpr float kCell = 128.0f;

private:
	void cellRange(const float* lo, const float* hi, int& x0, int& y0, int& x1, int& y1) const;
	int nx_ = 0, ny_ = 0;
	std::vector<std::vector<uint32_t>> cells_;
	mutable std::vector<uint32_t> stamp_;
	mutable uint32_t query_ = 0;
};

class SkyWorld {
public:
	uint32_t raycast(const float* origin, const float* dir, float maxDist, ffi::RayHit* hit);
	uint32_t boxContacts(const ffi::Obb& obb, float margin, ffi::Contact* out, uint32_t cap);
	uint32_t sphereContacts(const float* center, float radius, float margin, ffi::Contact* out, uint32_t cap);

	static uint32_t cbRaycast(void* user, const float* o, const float* d, float max, ffi::RayHit* hit);
	static uint32_t cbBox(void* user, const ffi::Obb* obb, float margin, ffi::Contact* out, uint32_t cap);
	static uint32_t cbSphere(void* user, const float* c, float r, float margin, ffi::Contact* out, uint32_t cap);

	// Closest hit of the segment o + dir * t, t in [0, maxDist], against the cache's geometry.
	static bool rayCache(const Cache& c, const float* o, const float* dir, float maxDist, float& t, float* normal);

	void forget();

	Cache car, ball;
	const Cache* boxCache = &car;
	const Cache* sphereCache = &ball;
	uint64_t queries = 0;

	bool wallRamps = true;
	float rampRadius = 150.0f;
	float lookahead = 25.0f;
	float sphereLookahead = 60.0f;
	int refreshTicks = 4;
	float refreshDistance = 15;

	struct Plane {
		float p[3], n[3];
		uint32_t id;
	};
	struct Ramp {
		float x0[3], d[3], nFloor[3], nWall[3], wallPoint[3];
		uint32_t id;
	};
	const std::vector<Ramp>& ramps() const { return carRamps_.ramps; }

private:
	struct RampState {
		std::vector<Plane> planes;
		std::vector<Ramp> ramps;
		float at[3] = {1e30f, 1e30f, 1e30f};
		int age = 1 << 30;
	};
	RampState carRamps_, ballRamps_;
	float lastBox_[3] = {1e30f, 1e30f, 1e30f};
	float lastSphere_[3] = {1e30f, 1e30f, 1e30f};

	bool probe(const Cache& c, const float* from, const float* to, float* point, float* normal);
	bool needsRefresh(RampState& s, const float* center);
	void addHit(RampState& s, const float* point, const float* normal, const float* rayDir);
	void probeAndAdd(const Cache& c, RampState& s, const float* from, const float* dir, float len);
	void probeForWalls(const Cache& c, RampState& s, const float* center, float reach);
	void buildRamps(RampState& s);
	bool wallBehind(const Cache& c, const Ramp& r, const float* surfacePoint);
	bool rayRamp(const Ramp& r, const float* o, const float* dir, float maxDist, float& t, float* n) const;
};

uint32_t planeId(const float* n, const float* p);

}
