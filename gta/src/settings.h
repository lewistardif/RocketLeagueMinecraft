// Everything in RLCar.ini, with Rocket League's values as defaults.
#pragma once
#include "bindings.h"
#include "rlcar_ffi.h"
#include <string>

class Ini;

struct Settings {
	int preset = 0;  // index into the core's HitboxPreset::ALL
	int team = 0;    // 0 blue, 1 orange
	std::string fallbackModel = "bifta";
	double modelOffset = 0;  // metres, 0 = automatic
	bool showHud = true;
	bool debugLog = false;
	double worldScale = 2.5;
	float sim[ffi::SIM_CONFIG_FLOATS] = {};
	float camera[ffi::CAMERA_SETTINGS_FLOATS] = {};
	bool rearCameraToggle = false;
	int probeFlags = 1 | 2 | 16;
	bool wallRamps = true;
	float wallRampRadius = 320;  // uu
	Bindings bindings;

	// `api` supplies Rocket League's defaults for the physics and camera values.
	void load(const Ini& ini, const ffi::Api& api);
	static int presetIndex(const std::string& name);  // -1 if unknown
	static const char* presetName(int index);
	static int cameraPresetIndex(const std::string& name);
};
