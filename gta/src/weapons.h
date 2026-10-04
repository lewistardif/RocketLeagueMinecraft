#pragma once
#include "natives.h"
#include "weapons_settings.h"

class Weapons {
public:
	void update(Vehicle veh, Ped owner, bool fire, bool next, const float* gunLeft, const float* gunRight, float dt, const WeaponsSettings& s);
	const char* name() const;

private:
	int current_ = 0;
	float cooldown_ = 0;
	bool side_ = false;
};
