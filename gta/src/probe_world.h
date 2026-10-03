// Collision for the core built from segment probes (GTA V's synchronous LOS shape tests).
//
// GTA only answers "what does this segment hit first" synchronously, so:
// * wheel raycasts are one probe each (exact GTA geometry);
// * body (box) and ball (sphere) contacts come from a small set of planes found by probing
//   outwards in 26 directions. Planes are cached and re-probed every few ticks or when the body
//   moves, since the world is static. Each contact is checked with one more probe straight at the
//   surface, so a plane is never extended past the real geometry's edge (driving off a ledge).
// Everything is in Rocket League space (uu). Contacts are sorted by surface id (deterministic).
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

// Probe the segment from -> to; return true and fill `hit` for the first hit.
using ProbeFn = std::function<bool(const float* from, const float* to, ProbeHit& hit)>;

class ProbeWorld {
public:
	explicit ProbeWorld(ProbeFn probe) : probe_(std::move(probe)) {}

	uint32_t raycast(const float* origin, const float* dir, float maxDist, ffi::RayHit* hit);
	uint32_t boxContacts(const ffi::Obb& obb, float margin, ffi::Contact* out, uint32_t cap);
	uint32_t sphereContacts(const float* center, float radius, float margin, ffi::Contact* out, uint32_t cap);

	// C callbacks for rlcar_cbworld_new (user = this).
	static uint32_t cbRaycast(void* user, const float* o, const float* d, float max, ffi::RayHit* hit);
	static uint32_t cbBox(void* user, const ffi::Obb* obb, float margin, ffi::Contact* out, uint32_t cap);
	static uint32_t cbSphere(void* user, const float* c, float r, float margin, ffi::Contact* out, uint32_t cap);

	void forget() { boxCache_ = {}; sphereCache_ = {}; }  // after a teleport / origin shift
	uint64_t probes = 0;  // statistics

	// Tuning (uu / ticks).
	float lookahead = 25.0f;     // how far past the body the outward probes reach
	int refreshTicks = 4;        // re-probe at least this often
	float refreshDistance = 15;  // ... or when the body moved this far

	// Virtual quarter-pipes where the ground meets a steep wall, so GTA's sharp building corners can
	// be driven up like Rocket League's curved arena walls. Collision shape only; physics unchanged.
	bool wallRamps = true;
	float rampRadius = 320.0f;  // uu (the Bevy arena's quarter-pipes are 320 uu)

	struct Plane {
		float p[3], n[3];
		uint32_t id;
	};

	// A quarter-pipe filling the concave corner between a floor and a wall plane: the solid is the
	// part of the corner farther than `radius` from the axis line (point `x0`, direction `d`).
	struct Ramp {
		float x0[3], d[3], nFloor[3], nWall[3], wallPoint[3];
		uint32_t id;
	};
	const std::vector<Ramp>& ramps() const { return boxCache_.ramps; }

private:
	struct Cache {
		std::vector<Plane> planes;
		std::vector<Ramp> ramps;
		std::vector<ProbeHit> hits;  // raw hits of the last refresh (sphere edge contacts)
		float at[3] = {1e30f, 1e30f, 1e30f};
		int age = 1 << 30;
	};
	ProbeFn probe_;
	Cache boxCache_, sphereCache_;
	bool needsRefresh(Cache& c, const float* center);
	void addHit(Cache& c, const ProbeHit& h, const float* rayDir);
	void probeForWalls(Cache& c, const float* center, const float* up, float reach);
	void buildRamps(Cache& c);
	bool wallBehind(const Ramp& r, const float* surfacePoint);
	bool rayRamp(const Ramp& r, const float* o, const float* dir, float maxDist, float& t, float* n) const;
};

uint32_t planeId(const float* n, const float* p);
