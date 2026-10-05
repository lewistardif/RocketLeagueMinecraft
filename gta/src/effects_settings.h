#pragma once
#include <string>

struct EffectsSettings {
	bool enabled = true;
	std::string asset = "veh_impexp_rocket";
	std::string effect = "veh_rocket_boost";
	std::string fallbackAsset = "core";
	std::string fallbackEffect = "veh_exhaust_afterburner";
	float scale = 1.0f;
	float supersonicScale = 1.6f;
	float rot[3] = {0, 0, 180};
	bool idleGlow = true;
	float glowRange = 2.5f;
	float glowIntensity = 3.0f;
	bool boostSound = true;
	bool engineAudio = true;
};
