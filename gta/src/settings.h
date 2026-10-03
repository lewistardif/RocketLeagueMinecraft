#pragma once
#include "bindings.h"
#include "rlcar_ffi.h"
#include <string>

class Ini;

struct Settings {
	int preset = 0;
	int team = 0;
	std::string fallbackModel = "bifta";
	double modelOffset = 0;
	bool showHud = true;
	bool debugLog = false;
	double worldScale = 2.5;
	float sim[ffi::SIM_CONFIG_FLOATS] = {};
	float camera[ffi::CAMERA_SETTINGS_FLOATS] = {};
	bool rearCameraToggle = false;
	int probeFlags = 1 | 2 | 16;
	bool wallRamps = true;
	float wallRampRadius = 320;
	bool ballEnabled = true;
	std::string ballModel = "stt_prop_stunt_soccer_ball";
	float ball[ffi::BALL_CONFIG_FLOATS] = {};
	bool ballCamOnSpawn = true;
	Bindings bindings;

	void load(const Ini& ini, const ffi::Api& api);
	static int presetIndex(const std::string& name);
	static const char* presetName(int index);
	static int cameraPresetIndex(const std::string& name);
};
