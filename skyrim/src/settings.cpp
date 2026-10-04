#include "settings.h"
#include "ini.h"
#include <algorithm>
#include <cctype>
#include <cstdlib>

static const char* kPresets[] = {"octane", "dominus", "plank", "breakout", "hybrid", "merc", "psyclops"};
static const char* kCameraPresets[] = {"default", "balanced", "wide", "custom", "legacy", "modern"};

static std::string lower(std::string s) {
	std::transform(s.begin(), s.end(), s.begin(), [](unsigned char c) { return char(std::tolower(c)); });
	return s;
}

static std::string trim(const std::string& s) {
	size_t a = s.find_first_not_of(" \t"), b = s.find_last_not_of(" \t");
	return a == std::string::npos ? "" : s.substr(a, b - a + 1);
}

bool FormRef::parse(const std::string& text, FormRef& out) {
	size_t bar = text.find('|');
	std::string plugin = bar == std::string::npos ? "Skyrim.esm" : trim(text.substr(0, bar));
	std::string id = trim(bar == std::string::npos ? text : text.substr(bar + 1));
	if (plugin.empty() || id.empty()) return false;
	char* end = nullptr;
	unsigned long v = std::strtoul(id.c_str(), &end, 16);
	if (!end || *end != '\0' || v == 0 || v > 0xFFFFFF) return false;
	out.plugin = plugin;
	out.id = uint32_t(v);
	return true;
}

int Settings::presetIndex(const std::string& name) {
	for (int i = 0; i < 7; i++)
		if (lower(name) == kPresets[i]) return i;
	return -1;
}

const char* Settings::presetName(int i) { return (i >= 0 && i < 7) ? kPresets[i] : "octane"; }

int Settings::cameraPresetIndex(const std::string& name) {
	for (int i = 0; i < 6; i++)
		if (lower(name) == kCameraPresets[i]) return i;
	return -1;
}

void Settings::load(const Ini& ini, const ffi::Api& api) {
	int p = presetIndex(ini.str("General", "Preset", "octane"));
	preset = p < 0 ? 0 : p;
	carForm = {"Skyrim.esm", 0x01C0C0};
	FormRef::parse(ini.str("General", "CarForm", ""), carForm);
	modelYaw = ini.numf("General", "ModelYaw", 0);
	modelOffset = ini.numf("General", "ModelOffset", 0);
	debugLog = ini.flag("General", "DebugLog", false);
	std::string ws = lower(ini.str("Scale", "WorldScale", "auto"));
	worldScale = ws == "auto" ? 0.0 : ini.num("Scale", "WorldScale", 0);
	worldScale = worldScale <= 0 ? 0.0 : std::clamp(worldScale, 0.1, 20.0);

	api.default_config(sim);
	static const char* kSimKeys[ffi::SIM_CONFIG_FLOATS] = {
		"Gravity", "BoostAccelGround", "BoostAccelAir", "BoostUsedPerSecond", "JumpAccel", "JumpImpulse", "WorldFriction",
		"WorldRestitution", "UnlimitedFlips", "UnlimitedDoubleJumps", "UnlimitedBoost", "BoostRecharge",
		"BoostRechargePerSecond", "BoostRechargeDelay", "MaxSpeed"};
	sim[10] = 1;
	for (int i = 0; i < ffi::SIM_CONFIG_FLOATS; i++) {
		bool isFlag = (i >= 8 && i <= 11);
		sim[i] = isFlag ? (ini.flag("Car", kSimKeys[i], sim[i] != 0) ? 1.0f : 0.0f) : ini.numf("Car", kSimKeys[i], sim[i]);
	}

	int cp = cameraPresetIndex(ini.str("Camera", "Preset", "Default"));
	api.camera_preset(cp < 0 ? 0 : uint32_t(cp), camera);
	static const char* kCamKeys[ffi::CAMERA_SETTINGS_FLOATS] = {"FOV", "Height", "Angle", "Distance", "Stiffness",
	                                                             "SwivelSpeed", "TransitionSpeed", "InvertSwivelPitch"};
	if (cp < 0 || cp == 3) {
		for (int i = 0; i < ffi::CAMERA_SETTINGS_FLOATS; i++) camera[i] = ini.numf("Camera", kCamKeys[i], camera[i]);
	}
	rearCameraToggle = ini.flag("Camera", "RearCameraToggle", false);
	wallRamps = ini.flag("World", "WallRamps", true);
	wallRampRadius = std::clamp(ini.numf("World", "WallRampRadius", 150), 50.0f, 2000.0f);
	cacheRadius = std::clamp(ini.numf("World", "CacheRadius", 2500), 800.0f, 10000.0f);
	cacheRefresh = std::clamp(ini.numf("World", "CacheRefresh", 0.5f), 0.05f, 5.0f);
	ballEnabled = ini.flag("Ball", "Enabled", true);
	ballForm = {"Skyrim.esm", 0x0C8868};
	FormRef::parse(ini.str("Ball", "Form", ""), ballForm);
	ballCamOnSpawn = ini.flag("Ball", "BallCamOnSpawn", true);
	api.default_ball_config(ball);
	static const char* kBallKeys[ffi::BALL_CONFIG_FLOATS] = {"Radius", "Mass", "Drag", "WorldFriction", "WorldRestitution",
	                                                          "MaxSpeed", "MaxSpin", "CarFriction", "CarRestitution", "HitForce"};
	for (int i = 0; i < ffi::BALL_CONFIG_FLOATS; i++) ball[i] = ini.numf("Ball", kBallKeys[i], ball[i]);
	interact.enabled = ini.flag("Interaction", "Enabled", true);
	interact.demolish = ini.flag("Interaction", "Demolish", true);
	interact.hitActors = ini.flag("Interaction", "HitActors", true);
	interact.ragdoll = ini.flag("Interaction", "Ragdoll", true);
	interact.crime = ini.flag("Interaction", "Crime", true);
	interact.demolishFollowers = ini.flag("Interaction", "DemolishFollowers", false);
	interact.bumpForce = ini.numf("Interaction", "BumpForce", 1.0f);
	interact.actorForce = ini.numf("Interaction", "ActorForce", 1.0f);
	interact.actorMass = std::max(1.0f, ini.numf("Interaction", "ActorMass", 60.0f));
	interact.restitution = ini.numf("Interaction", "Restitution", 0.1f);
	interact.lift = ini.numf("Interaction", "Lift", 0.25f);
	interact.minImpactSpeed = ini.numf("Interaction", "MinImpactSpeed", 150.0f);
	becomeCarKey = ini.str("Controls", "BecomeCar", "F7");
	bindings.load(ini);
}
