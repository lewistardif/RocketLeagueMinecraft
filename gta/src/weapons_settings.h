#pragma once
#include <string>

struct WeaponsSettings {
	bool enabled = true;
	std::string gun = "VEHICLE_WEAPON_PLAYER_LAZER";
	std::string missile = "WEAPON_VEHICLE_ROCKET";
	float gunInterval = 0.08f;
	float missileInterval = 0.6f;
	int gunDamage = 60;
	int missileDamage = 250;
};
