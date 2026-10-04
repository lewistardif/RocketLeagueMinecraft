#pragma once
#include "bindings.h"
#include "interact_settings.h"
#include "rlcar_ffi.h"
#include <cstdint>
#include <string>

class Ini;

// A form in a plugin: "Skyrim.esm|0x01C0C0" (the id without the load-order byte).
struct FormRef {
	std::string plugin = "Skyrim.esm";
	uint32_t id = 0;

	static bool parse(const std::string& text, FormRef& out);
};

struct Settings {
	int preset = 0;
	FormRef carForm{"Skyrim.esm", 0x01C0C0};
	float modelYaw = 0;
	float modelOffset = 0;
	bool debugLog = false;
	double worldScale = 0;
	float sim[ffi::SIM_CONFIG_FLOATS] = {};
	float camera[ffi::CAMERA_SETTINGS_FLOATS] = {};
	bool rearCameraToggle = false;
	bool wallRamps = true;
	float wallRampRadius = 150;
	float cacheRadius = 2500;
	float cacheRefresh = 0.5f;
	bool ballEnabled = true;
	FormRef ballForm{"Skyrim.esm", 0x0C8868};
	bool ballCamOnSpawn = true;
	float ball[ffi::BALL_CONFIG_FLOATS] = {};
	InteractSettings interact;
	std::string becomeCarKey = "F7";
	Bindings bindings;

	void load(const Ini& ini, const ffi::Api& api);
	static int presetIndex(const std::string& name);
	static const char* presetName(int index);
	static int cameraPresetIndex(const std::string& name);
};
