#include "weapons.h"

const char* Weapons::name() const { return current_ == 0 ? "MACHINE GUN" : "MISSILES"; }

void Weapons::update(Vehicle veh, Ped owner, bool fire, bool next, const float* gl, const float* gr, float dt, const WeaponsSettings& s) {
	if (!s.enabled || !veh) return;
	if (next) current_ ^= 1;
	cooldown_ -= dt;
	if (!fire || cooldown_ > 0) return;
	const std::string& weapon = current_ == 0 ? s.gun : s.missile;
	Hash h = MISC::GET_HASH_KEY(weapon.c_str());
	if (!WEAPON::HAS_WEAPON_ASSET_LOADED(h)) {
		WEAPON::REQUEST_WEAPON_ASSET(h, 31, 0);
		return;
	}
	const float* at = side_ ? gr : gl;
	side_ = !side_;
	Vector3 from = ENTITY::GET_OFFSET_FROM_ENTITY_IN_WORLD_COORDS(veh, at[0], at[1], at[2]);
	Vector3 to = ENTITY::GET_OFFSET_FROM_ENTITY_IN_WORLD_COORDS(veh, at[0], at[1] + 300.0f, at[2]);
	MISC::SHOOT_SINGLE_BULLET_BETWEEN_COORDS_IGNORE_ENTITY(from.x, from.y, from.z, to.x, to.y, to.z, current_ == 0 ? s.gunDamage : s.missileDamage,
	                                                       TRUE, h, owner, TRUE, FALSE, -1.0f, veh, 0);
	cooldown_ = current_ == 0 ? s.gunInterval : s.missileInterval;
}
