#include "interact.h"
#include "geom.h"
#include <cmath>

namespace {

geom::Box carBox(const CarView& c, float grow) {
	geom::Box b{};
	const float* p = c.pose;
	for (int i = 0; i < 3; i++)
		for (int k = 0; k < 3; k++) b.ax[i][k] = p[3 + i * 3 + k];
	for (int k = 0; k < 3; k++) {
		b.c[k] = p[k] + b.ax[0][k] * c.hitbox[3] + b.ax[1][k] * c.hitbox[4] + b.ax[2][k] * c.hitbox[5];
		b.h[k] = c.hitbox[k] * 0.5f + grow;
	}
	return b;
}

void toRlDir(const Vector3& v, float* out) { space::dirToRl({v.x, v.y, v.z}, out); }

geom::Box entityBox(const space::Frame& f, Entity e, bool ped) {
	geom::Box b{};
	Vector3 fwd{}, right{}, up{}, pos{};
	ENTITY::GET_ENTITY_MATRIX(e, &fwd, &right, &up, &pos);
	float mn[3] = {-0.35f, -0.35f, -1.0f}, mx[3] = {0.35f, 0.35f, 0.9f};
	if (!ped) {
		Vector3 a{}, c{};
		MISC::GET_MODEL_DIMENSIONS(ENTITY::GET_ENTITY_MODEL(e), &a, &c);
		mn[0] = a.x, mn[1] = a.y, mn[2] = a.z, mx[0] = c.x, mx[1] = c.y, mx[2] = c.z;
	}
	float axR[3], axF[3], axU[3];
	toRlDir(right, axR);
	toRlDir(fwd, axF);
	toRlDir(up, axU);
	float centreLocal[3] = {(mn[0] + mx[0]) * 0.5f, (mn[1] + mx[1]) * 0.5f, (mn[2] + mx[2]) * 0.5f};
	space::V3 cg{pos.x + right.x * centreLocal[0] + fwd.x * centreLocal[1] + up.x * centreLocal[2],
	             pos.y + right.y * centreLocal[0] + fwd.y * centreLocal[1] + up.y * centreLocal[2],
	             pos.z + right.z * centreLocal[0] + fwd.z * centreLocal[1] + up.z * centreLocal[2]};
	f.toRl(cg, b.c);
	for (int k = 0; k < 3; k++) b.ax[0][k] = axR[k], b.ax[1][k] = axF[k], b.ax[2][k] = axU[k];
	for (int i = 0; i < 3; i++) b.h[i] = float(f.lenToRl((mx[i] - mn[i]) * 0.5));
	return b;
}

}

void Interactions::clear() {
	cooldown_.clear();
}

int Interactions::update(const ffi::Api& api, const CarView& car, const InteractSettings& s) {
	if (!s.enabled) return 0;
	DWORD now = GetTickCount();
	const space::Frame& f = *car.frame;
	geom::Box me = carBox(car, car.lookahead);
	geom::Box exact = carBox(car, 0);
	const float* myVel = car.pose + 12;
	int hits = 0;
	for (int pass = 0; pass < 2; pass++) {
		bool peds = pass == 1;
		if (peds && !s.hitPeds) break;
		int n = peds ? worldGetAllPeds(buf_.data(), int(buf_.size())) : worldGetAllVehicles(buf_.data(), int(buf_.size()));
		for (int i = 0; i < n; i++) {
			Entity e = buf_[size_t(i)];
			if (e == car.self || !ENTITY::DOES_ENTITY_EXIST(e)) continue;
			if (peds && (PED::IS_PED_A_PLAYER(e) || PED::IS_PED_IN_ANY_VEHICLE(e, FALSE))) continue;
			if (!peds && PED::IS_PED_IN_VEHICLE(PLAYER::PLAYER_PED_ID(), e, FALSE)) continue;
			auto cd = cooldown_.find(e);
			if (cd != cooldown_.end() && now < cd->second) continue;
			Vector3 gp = ENTITY::GET_ENTITY_COORDS(e, TRUE);
			float rp[3];
			f.toRl({gp.x, gp.y, gp.z}, rp);
			float dx = rp[0] - me.c[0], dy = rp[1] - me.c[1], dz = rp[2] - me.c[2];
			if (dx * dx + dy * dy + dz * dz > 3000.0f * 3000.0f) continue;
			geom::Box other = entityBox(f, e, peds);
			if (!geom::overlap(me, other)) continue;
			float local[3];
			geom::closestLocal(exact, other.c, local);
			float contactX = local[0] + car.hitbox[3];
			float cw[3], nrm[3];
			for (int k = 0; k < 3; k++) cw[k] = exact.c[k] + exact.ax[0][k] * local[0] + exact.ax[1][k] * local[1] + exact.ax[2][k] * local[2];
			for (int k = 0; k < 3; k++) nrm[k] = other.c[k] - cw[k];
			if (!peds) nrm[2] *= 0.25f;
			float nl = std::sqrt(nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]);
			if (nl < 1e-3f) {
				float vl = std::sqrt(myVel[0] * myVel[0] + myVel[1] * myVel[1] + myVel[2] * myVel[2]);
				if (vl < 1e-3f) continue;
				for (int k = 0; k < 3; k++) nrm[k] = myVel[k] / vl;
			} else {
				for (float& v : nrm) v /= nl;
			}
			Vector3 gv = ENTITY::GET_ENTITY_VELOCITY(e);
			float rv[3], up[3] = {other.ax[2][0], other.ax[2][1], other.ax[2][2]};
			space::dirToRl({gv.x / f.k(), gv.y / f.k(), gv.z / f.k()}, rv);
			float vn = 0;
			for (int k = 0; k < 3; k++) vn += (myVel[k] - rv[k]) * nrm[k];
			bool ground = peds ? true : VEHICLE::IS_VEHICLE_ON_ALL_WHEELS(e) != 0;
			float bumpDv[3] = {};
			uint32_t r = api.car_bump(car.car, other.c, rv, ground ? 1u : 0u, up, contactX, s.bumpForce, s.demolish ? 1u : 0u, bumpDv);
			if (r == 0 && vn < s.minImpactSpeed) continue;
			float victimMass = peds ? s.pedMass : s.vehicleMass;
			float share = (1.0f + s.restitution) * s.carMass / (s.carMass + victimMass);
			float dv[3];
			for (int k = 0; k < 3; k++) dv[k] = nrm[k] * std::max(vn, 0.0f) * share;
			if (peds) dv[2] += std::max(vn, 0.0f) * s.pedLift;
			if (r != 0)
				for (int k = 0; k < 3; k++) dv[k] += bumpDv[k];
			cooldown_[e] = now + 250;
			hits++;
			space::V3 g = space::dirToGta(dv);
			double scale = f.k() * (peds ? s.pedForce : 1.0);
			if (peds) PED::SET_PED_TO_RAGDOLL(e, 3000, 3000, 0, FALSE, FALSE, FALSE);
			ENTITY::SET_ENTITY_VELOCITY(e, float(gv.x + g.x * scale), float(gv.y + g.y * scale), float(gv.z + g.z * scale));
			if (r == 2) {
				if (peds) {
					FIRE::ADD_EXPLOSION(gp.x, gp.y, gp.z, 7, 0.0f, TRUE, FALSE, 0.4f, TRUE);
					ENTITY::SET_ENTITY_HEALTH(e, 0, 0, 0);
				} else {
					VEHICLE::EXPLODE_VEHICLE(e, TRUE, FALSE);
				}
			}
		}
	}
	return hits;
}
