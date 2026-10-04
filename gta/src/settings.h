#pragma once
#include "bindings.h"
#include "effects_settings.h"
#include "interact_settings.h"
#include "weapons_settings.h"
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
	double worldScale = 0;
	bool showHitbox = false;
	float sim[ffi::SIM_CONFIG_FLOATS] = {};
	float camera[ffi::CAMERA_SETTINGS_FLOATS] = {};
	bool rearCameraToggle = false;
	int probeFlags = 1 | 2 | 16;
	bool wallRamps = true;
	float wallRampRadius = 150;
	bool ballEnabled = true;
	std::string ballModel = "stt_prop_stunt_soccer_ball";
	float ball[ffi::BALL_CONFIG_FLOATS] = {};
	bool ballCamOnSpawn = true;
	bool models = true;
	std::string modelFolder = "models";
	int modelDetail = 1;
	float modelLodDistance = 15.0f;
	float modelMaxDistance = 300.0f;
	int modelWinding = 0;
	float modelAmbient = 0.45f;
	float modelDiffuse = 0.6f;
	float modelBrightness = 1.0f;
	InteractSettings interact;
	EffectsSettings effects;
	WeaponsSettings weapons;
	Bindings bindings;

	void load(const Ini& ini, const ffi::Api& api);
	static int presetIndex(const std::string& name);
	static const char* presetName(int index);
	static int cameraPresetIndex(const std::string& name);
};
