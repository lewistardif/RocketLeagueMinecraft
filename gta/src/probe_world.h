#pragma once
#include "rlcar_ffi.h"
#include <cstdint>
#include <functional>
#include <vector>

struct ProbeHit {
	float point[3];
	float normal[3];
	int entity = 0;
};

using ProbeFn = std::function<bool(const float* from, const float* to, ProbeHit& hit)>;

class ProbeWorld {
public:
	explicit ProbeWorld(ProbeFn probe) : probe_(std::move(probe)) {}

	uint32_t raycast(const float* origin, const float* dir, float maxDist, ffi::RayHit* hit);
	uint32_t boxContacts(const ffi::Obb& obb, float margin, ffi::Contact* out, uint32_t cap);
	uint32_t sphereContacts(const float* center, float radius, float margin, ffi::Contact* out, uint32_t cap);

	static uint32_t cbRaycast(void* user, const float* o, const float* d, float max, ffi::RayHit* hit);
	static uint32_t cbBox(void* user, const ffi::Obb* obb, float margin, ffi::Contact* out, uint32_t cap);
	static uint32_t cbSphere(void* user, const float* c, float r, float margin, ffi::Contact* out, uint32_t cap);

	void forget() { boxCache_ = {}; sphereCache_ = {}; }
	uint64_t probes = 0;

	float lookahead = 25.0f;
	float sphereLookahead = 60.0f;
	int refreshTicks = 4;
	float refreshDistance = 15;

	bool wallRamps = true;
	float rampRadius = 320.0f;

	struct Plane {
		float p[3], n[3];
		uint32_t id;
	};

	struct Ramp {
		float x0[3], d[3], nFloor[3], nWall[3], wallPoint[3];
		uint32_t id;
	};
	const std::vector<Ramp>& ramps() const { return boxCache_.ramps; }

private:
	struct Cache {
		std::vector<Plane> planes;
		std::vector<Ramp> ramps;
		std::vector<ProbeHit> hits;
		float at[3] = {1e30f, 1e30f, 1e30f};
		float last[3] = {1e30f, 1e30f, 1e30f};
		int age = 1 << 30;
	};
	ProbeFn probe_;
	Cache boxCache_, sphereCache_;
	bool needsRefresh(Cache& c, const float* center);
	void addHit(Cache& c, const ProbeHit& h, const float* rayDir);
	void sweep(Cache& c, const float* from, const float* to, const float* moved, float movedLen, const float* const* ax, const float* he, float margin);
	void probeForWalls(Cache& c, const float* center, const float* up, float reach);
	void buildRamps(Cache& c);
	bool wallBehind(const Ramp& r, const float* surfacePoint);
	bool rayRamp(const Ramp& r, const float* o, const float* dir, float maxDist, float& t, float* n) const;
};

uint32_t planeId(const float* n, const float* p);
