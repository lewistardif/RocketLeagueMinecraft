#include "settings.h"
#include "ini.h"
#include <algorithm>
#include <cctype>

static const char* kPresets[] = {"octane", "dominus", "plank", "breakout", "hybrid", "merc", "psyclops"};
static const char* kCameraPresets[] = {"default", "balanced", "wide", "custom", "legacy", "modern"};

static std::string lower(std::string s) {
	std::transform(s.begin(), s.end(), s.begin(), [](unsigned char c) { return char(std::tolower(c)); });
	return s;
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
	team = lower(ini.str("General", "Team", "blue")) == "orange" ? 1 : 0;
	fallbackModel = ini.str("General", "FallbackModel", "bifta");
	modelOffset = ini.num("General", "ModelOffset", 0);
	showHud = ini.flag("General", "ShowHud", true);
	debugLog = ini.flag("General", "DebugLog", false);
	worldScale = std::clamp(ini.num("Scale", "WorldScale", 2.5), 0.1, 20.0);

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
	probeFlags = int(ini.num("World", "ProbeFlags", probeFlags));
	wallRamps = ini.flag("World", "WallRamps", true);
	wallRampRadius = std::clamp(ini.numf("World", "WallRampRadius", 320), 50.0f, 2000.0f);
	ballEnabled = ini.flag("Ball", "Enabled", true);
	ballModel = ini.str("Ball", "Model", "stt_prop_stunt_soccer_ball");
	ballCamOnSpawn = ini.flag("Ball", "BallCamOnSpawn", true);
	api.default_ball_config(ball);
	static const char* kBallKeys[ffi::BALL_CONFIG_FLOATS] = {"Radius", "Mass", "Drag", "WorldFriction", "WorldRestitution",
	                                                          "MaxSpeed", "MaxSpin", "CarFriction", "CarRestitution", "HitForce"};
	for (int i = 0; i < ffi::BALL_CONFIG_FLOATS; i++) ball[i] = ini.numf("Ball", kBallKeys[i], ball[i]);
	interact.enabled = ini.flag("Interaction", "Enabled", true);
	interact.demolish = ini.flag("Interaction", "Demolish", true);
	interact.hitPeds = ini.flag("Interaction", "HitPeds", true);
	interact.bumpForce = ini.numf("Interaction", "BumpForce", 1.0f);
	interact.pedForce = ini.numf("Interaction", "PedForce", 1.0f);
	effects.enabled = ini.flag("Effects", "Enabled", true);
	effects.asset = ini.str("Effects", "BoostAsset", effects.asset);
	effects.effect = ini.str("Effects", "BoostEffect", effects.effect);
	effects.fallbackAsset = ini.str("Effects", "FallbackAsset", effects.fallbackAsset);
	effects.fallbackEffect = ini.str("Effects", "FallbackEffect", effects.fallbackEffect);
	effects.scale = ini.numf("Effects", "BoostScale", effects.scale);
	effects.supersonicScale = ini.numf("Effects", "SupersonicScale", effects.supersonicScale);
	effects.rot[0] = ini.numf("Effects", "BoostRotX", effects.rot[0]);
	effects.rot[1] = ini.numf("Effects", "BoostRotY", effects.rot[1]);
	effects.rot[2] = ini.numf("Effects", "BoostRotZ", effects.rot[2]);
	effects.idleGlow = ini.flag("Effects", "IdleGlow", effects.idleGlow);
	effects.glowRange = ini.numf("Effects", "GlowRange", effects.glowRange);
	effects.glowIntensity = ini.numf("Effects", "GlowIntensity", effects.glowIntensity);
	effects.boostSound = ini.flag("Effects", "BoostSound", effects.boostSound);
	effects.engineAudio = ini.flag("Effects", "EngineAudio", effects.engineAudio);
	weapons.enabled = ini.flag("Weapons", "Enabled", true);
	weapons.gun = ini.str("Weapons", "Gun", weapons.gun);
	weapons.missile = ini.str("Weapons", "Missile", weapons.missile);
	weapons.gunInterval = ini.numf("Weapons", "GunInterval", weapons.gunInterval);
	weapons.missileInterval = ini.numf("Weapons", "MissileInterval", weapons.missileInterval);
	weapons.gunDamage = int(ini.num("Weapons", "GunDamage", weapons.gunDamage));
	weapons.missileDamage = int(ini.num("Weapons", "MissileDamage", weapons.missileDamage));
	bindings.load(ini);
}
