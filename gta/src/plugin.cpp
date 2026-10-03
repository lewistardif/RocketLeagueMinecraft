// RL Car for GTA V: a Script Hook V plugin that drives this repository's Rust core
// (crates/rl_car_core) through its C ABI (crates/rl_car_ffi), like the Minecraft mod.
//
// The core simulates the car at a fixed 120 Hz against GTA's collision (gta_probe.cpp); a stock
// GTA vehicle (or a converted Rocket League model) is drawn at the interpolated pose with GTA's
// own physics frozen. The camera is the core's Rocket League car camera.
#include "natives.h"
#include "ini.h"
#include "probe_world.h"
#include "rlcar_ffi.h"
#include "settings.h"
#include "space.h"

#include <cmath>
#include <cstdarg>
#include <cstdio>
#include <memory>
#include <string>
#include <xinput.h>

bool gtaProbe(const space::Frame& frame, int flags, Entity ignore, const float* from, const float* to, ProbeHit& hit);

namespace {

HMODULE g_module;
std::string g_dir;  // folder of RLCar.asi
std::string g_dataDir;  // <g_dir>\RLCar
ffi::Api g_api{};
bool g_apiOk = false;
std::string g_apiError;
Settings g_settings;

void log(const char* fmt, ...) {
	FILE* f = nullptr;
	if (fopen_s(&f, (g_dataDir + "\\RLCar.log").c_str(), "a") != 0 || !f) return;
	SYSTEMTIME t;
	GetLocalTime(&t);
	fprintf(f, "[%02d:%02d:%02d.%03d] ", t.wHour, t.wMinute, t.wSecond, t.wMilliseconds);
	va_list ap;
	va_start(ap, fmt);
	vfprintf(f, fmt, ap);
	va_end(ap);
	fputc('\n', f);
	fclose(f);
}

// ------------------------------------------------------------------------------------- input

using XInputGetStateFn = DWORD(WINAPI*)(DWORD, XINPUT_STATE*);
XInputGetStateFn g_xinput = nullptr;

void loadXInput() {
	for (const char* dll : {"xinput1_4.dll", "xinput1_3.dll", "xinput9_1_0.dll"}) {
		if (HMODULE m = LoadLibraryA(dll)) {
			g_xinput = reinterpret_cast<XInputGetStateFn>(GetProcAddress(m, "XInputGetState"));
			if (g_xinput) return;
		}
	}
}

bool gameHasFocus() {
	DWORD pid = 0;
	GetWindowThreadProcessId(GetForegroundWindow(), &pid);
	return pid == GetCurrentProcessId();
}

InputState readInput() {
	InputState s;
	if (gameHasFocus()) {
		for (int vk = 1; vk < 256; vk++) s.keys[size_t(vk)] = (GetAsyncKeyState(vk) & 0x8000) != 0;
	}
	XINPUT_STATE xs{};
	if (g_xinput && g_xinput(0, &xs) == ERROR_SUCCESS) {
		auto& g = xs.Gamepad;
		s.pad.connected = true;
		s.pad.buttons = g.wButtons;
		s.pad.lt = g.bLeftTrigger / 255.0f;
		s.pad.rt = g.bRightTrigger / 255.0f;
		auto ax = [](SHORT v) { return v < 0 ? v / 32768.0f : v / 32767.0f; };
		s.pad.lx = ax(g.sThumbLX), s.pad.ly = ax(g.sThumbLY), s.pad.rx = ax(g.sThumbRX), s.pad.ry = ax(g.sThumbRY);
	}
	return s;
}

// Edge detection for one-shot actions.
struct Edges {
	bool prev[size_t(Action::Count)] = {};
	bool now[size_t(Action::Count)] = {};
	void update(const InputState& s) {
		for (size_t i = 0; i < size_t(Action::Count); i++) {
			prev[i] = now[i];
			now[i] = g_settings.bindings.held(Action(i), s);
		}
	}
	bool pressed(Action a) const { return now[size_t(a)] && !prev[size_t(a)]; }
} g_edges;

// ------------------------------------------------------------------------------------ helpers

void drawText(const char* text, float x, float y, float scale, int r = 255, int g = 255, int b = 255, bool centre = false) {
	HUD::SET_TEXT_FONT(4);
	HUD::SET_TEXT_SCALE(0.0f, scale);
	HUD::SET_TEXT_COLOUR(r, g, b, 255);
	HUD::SET_TEXT_OUTLINE();
	HUD::SET_TEXT_CENTRE(centre);
	HUD::BEGIN_TEXT_COMMAND_DISPLAY_TEXT("STRING");
	HUD::ADD_TEXT_COMPONENT_SUBSTRING_PLAYER_NAME(text);
	HUD::END_TEXT_COMMAND_DISPLAY_TEXT(x, y, 0);
}

void notify(const char* text, const char* sender = nullptr) {
	HUD::BEGIN_TEXT_COMMAND_THEFEED_POST("STRING");
	HUD::ADD_TEXT_COMPONENT_SUBSTRING_PLAYER_NAME(text);
	if (sender)
		HUD::END_TEXT_COMMAND_THEFEED_POST_MESSAGETEXT("CHAR_MP_MECHANIC", "CHAR_MP_MECHANIC", FALSE, 1, sender, "RL Car");
	else
		HUD::END_TEXT_COMMAND_THEFEED_POST_TICKER(FALSE, FALSE);
}

void help(const char* text) {
	HUD::BEGIN_TEXT_COMMAND_DISPLAY_HELP("STRING");
	HUD::ADD_TEXT_COMPONENT_SUBSTRING_PLAYER_NAME(text);
	HUD::END_TEXT_COMMAND_DISPLAY_HELP(0, FALSE, FALSE, -1);
}

space::V3 v3(const Vector3& v) { return {v.x, v.y, v.z}; }

bool loadModel(Hash h) {
	if (!STREAMING::IS_MODEL_IN_CDIMAGE(h) || !STREAMING::IS_MODEL_A_VEHICLE(h)) return false;
	STREAMING::REQUEST_MODEL(h);
	for (int i = 0; i < 200 && !STREAMING::HAS_MODEL_LOADED(h); i++) WAIT(0);
	return STREAMING::HAS_MODEL_LOADED(h) != 0;
}

// --------------------------------------------------------------------------------------- car

struct RlCar {
	ffi::Car* car = nullptr;
	ffi::World* world = nullptr;
	void* camera = nullptr;
	std::unique_ptr<ProbeWorld> probe;
	space::Frame frame;
	Vehicle veh = 0;
	Blip blip = 0;
	Cam cam = 0;
	int preset = 0, team = 0;
	float modelLift = 0;  // metres from the physics car origin to the drawn model's origin, along car up
	float pose[ffi::POSE_FLOATS] = {};
	uint32_t flags = 0;
	bool rearView = false;
	bool driving = false;
	DWORD exitingUntil = 0;  // GetTickCount() until the get-out animation is over

	~RlCar() { destroy(); }

	void destroy() {
		if (cam) {
			CAMERA::RENDER_SCRIPT_CAMS(FALSE, FALSE, 0, TRUE, FALSE, 0);
			CAMERA::DESTROY_CAM(cam, FALSE);
			cam = 0;
		}
		if (blip && HUD::DOES_BLIP_EXIST(blip)) HUD::REMOVE_BLIP(&blip);
		if (veh && ENTITY::DOES_ENTITY_EXIST(veh)) {
			Ped me = PLAYER::PLAYER_PED_ID();
			if (PED::IS_PED_IN_VEHICLE(me, veh, FALSE)) TASK::CLEAR_PED_TASKS_IMMEDIATELY(me);
			ENTITY::SET_ENTITY_AS_MISSION_ENTITY(veh, TRUE, TRUE);
			ENTITY::DELETE_ENTITY(&veh);
		}
		if (camera) g_api.camera_free(camera), camera = nullptr;
		if (world) g_api.cbworld_free(world), world = nullptr;
		if (car) g_api.car_free(car), car = nullptr;
	}

	// Creates the car resting on the ground at `at` (GTA metres) facing `heading` (degrees).
	bool spawn(space::V3 at, double heading, int presetIndex, int teamIndex) {
		preset = presetIndex;
		team = teamIndex;
		Hash model = MISC::GET_HASH_KEY(g_settings.fallbackModel.c_str());
		if (!loadModel(model)) {
			notify(("RL Car: unknown vehicle model " + g_settings.fallbackModel + ", using bifta").c_str());
			model = MISC::GET_HASH_KEY("bifta");
			if (!loadModel(model)) return false;
		}
		veh = VEHICLE::CREATE_VEHICLE(model, float(at.x), float(at.y), float(at.z), float(heading), FALSE, TRUE, FALSE);
		STREAMING::SET_MODEL_AS_NO_LONGER_NEEDED(model);
		if (!veh) return false;
		ENTITY::SET_ENTITY_AS_MISSION_ENTITY(veh, TRUE, TRUE);
		ENTITY::FREEZE_ENTITY_POSITION(veh, TRUE);  // the core moves it, not GTA
		ENTITY::SET_ENTITY_INVINCIBLE(veh, TRUE, FALSE);
		ENTITY::SET_ENTITY_PROOFS(veh, TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, TRUE);
		VEHICLE::SET_VEHICLE_CAN_BE_VISIBLY_DAMAGED(veh, FALSE);
		AUDIO::SET_VEHICLE_RADIO_ENABLED(veh, FALSE);
		VEHICLE::SET_VEHICLE_ENGINE_ON(veh, TRUE, TRUE, FALSE);
		if (team == 0)
			VEHICLE::SET_VEHICLE_CUSTOM_PRIMARY_COLOUR(veh, 20, 90, 255);
		else
			VEHICLE::SET_VEHICLE_CUSTOM_PRIMARY_COLOUR(veh, 255, 110, 10);
		VEHICLE::SET_VEHICLE_CUSTOM_SECONDARY_COLOUR(veh, 25, 25, 25);

		// The drawn model's wheels sit on the ground when the physics car rests (origin 17 uu up).
		Vector3 mn{}, mx{};
		MISC::GET_MODEL_DIMENSIONS(model, &mn, &mx);
		frame.scale = g_settings.worldScale;
		modelLift = g_settings.modelOffset != 0 ? float(g_settings.modelOffset) : float(-mn.z - frame.lenToGta(17.0));

		car = g_api.car_new(uint32_t(preset));
		g_api.car_set_config(car, g_settings.sim);
		probe = std::make_unique<ProbeWorld>([this](const float* a, const float* b, ProbeHit& h) {
			return gtaProbe(frame, g_settings.probeFlags, veh, a, b, h);
		});
		applyWorldSettings();
		world = g_api.cbworld_new(probe.get(), &ProbeWorld::cbRaycast, &ProbeWorld::cbBox, &ProbeWorld::cbSphere);
		camera = g_api.camera_new();
		placeAt(at, heading);
		return true;
	}

	// Physics origin at `at` (GTA metres); the car is put a little above, upright, at rest.
	void placeAt(space::V3 at, double heading) {
		frame.origin = at;
		float pos[3] = {0, 0, 40};
		g_api.car_reset(car, pos, space::headingToRlYaw(heading), 0, 0);
		probe->forget();
		g_api.camera_reset(camera);
	}

	// Keeps the core's coordinates small: re-centre the origin on the car when it gets far away.
	void recentre() {
		float* p = pose;
		if (std::fabs(p[0]) < 20000 && std::fabs(p[1]) < 20000 && std::fabs(p[2]) < 20000) return;
		float d[3] = {-p[0], -p[1], -p[2]};
		frame.origin = frame.toGta(p);
		g_api.car_translate(car, d);
		g_api.camera_translate(camera, d);
		probe->forget();
	}

	space::V3 position() const { return frame.toGta(pose); }

	void applyWorldSettings() {
		probe->wallRamps = g_settings.wallRamps;
		probe->rampRadius = g_settings.wallRampRadius;
		probe->forget();
	}

	void applyPose(float alpha) {
		flags = g_api.car_pose(car, alpha, pose);
		space::V3 p = frame.toGta(pose);
		space::V3 up = space::dirToGta(pose + 9);
		space::Quat q = space::carRotToGta(pose + 3);
		ENTITY::SET_ENTITY_COORDS_NO_OFFSET(veh, float(p.x + up.x * modelLift), float(p.y + up.y * modelLift),
		                                    float(p.z + up.z * modelLift), FALSE, FALSE, FALSE);
		ENTITY::SET_ENTITY_QUATERNION(veh, q.x, q.y, q.z, q.w);
	}
};

std::unique_ptr<RlCar> g_car;
int g_menuIndex = 0;
bool g_menuOpen = false;
DWORD g_orderAt = 0;  // GetTickCount() when an ordered car arrives (0 = none pending)

bool loadSettings() {
	Ini ini;
	bool found = ini.loadFile(g_dataDir + "\\RLCar.ini");
	g_settings.load(ini, g_api);
	if (g_car && g_car->car) {
		g_api.car_set_config(g_car->car, g_settings.sim);
		g_car->applyWorldSettings();
	}
	return found;
}

void putPlayerIn(RlCar& c) {
	Ped me = PLAYER::PLAYER_PED_ID();
	PED::SET_PED_INTO_VEHICLE(me, c.veh, -1);
	c.exitingUntil = 0;
	g_api.camera_reset(c.camera);
}

// Spawns (or moves) the car under the player and puts them in it.
void becomeCar() {
	Ped me = PLAYER::PLAYER_PED_ID();
	if (g_car && g_car->car) {
		putPlayerIn(*g_car);
		return;
	}
	space::V3 at = v3(ENTITY::GET_ENTITY_COORDS(me, TRUE));
	float gz = 0;
	if (MISC::GET_GROUND_Z_FOR_3D_COORD(float(at.x), float(at.y), float(at.z) + 1, &gz, FALSE, FALSE)) at.z = gz;
	g_car = std::make_unique<RlCar>();
	if (!g_car->spawn(at, ENTITY::GET_ENTITY_HEADING(me), g_settings.preset, g_settings.team)) {
		g_car.reset();
		notify("RL Car: could not create the car");
		return;
	}
	putPlayerIn(*g_car);
}

// The Mechanic brings the car to the nearest road and marks it on the map.
void orderCar() {
	notify("Your Rocket League car is on its way.", "Mechanic");
	g_orderAt = GetTickCount() + 4000;
}

void deliverOrder() {
	g_orderAt = 0;
	Ped me = PLAYER::PLAYER_PED_ID();
	space::V3 p = v3(ENTITY::GET_ENTITY_COORDS(me, TRUE));
	Vector3 fwd = ENTITY::GET_ENTITY_FORWARD_VECTOR(me);
	Vector3 node{};
	float heading = ENTITY::GET_ENTITY_HEADING(me);
	space::V3 at{p.x + fwd.x * 15, p.y + fwd.y * 15, p.z};
	if (PATH::GET_CLOSEST_VEHICLE_NODE_WITH_HEADING(float(at.x), float(at.y), float(at.z), &node, &heading, 1, 3.0f, 0)) at = v3(node);
	if (g_car) g_car.reset();
	g_car = std::make_unique<RlCar>();
	if (!g_car->spawn(at, heading, g_settings.preset, g_settings.team)) {
		g_car.reset();
		return;
	}
	g_car->blip = HUD::ADD_BLIP_FOR_ENTITY(g_car->veh);
	HUD::SET_BLIP_SPRITE(g_car->blip, 225);  // car
	HUD::SET_BLIP_COLOUR(g_car->blip, g_settings.team == 0 ? 3 : 17);
	notify("Your car is parked nearby. Press F (Y) next to it to get in.", "Mechanic");
}

// ---------------------------------------------------------------------------------------- menu

const char* kMenu[] = {"Become the car", "Order from the Mechanic", "Hitbox", "Team", "Unlimited boost", "Remove the car",
                       "Reload RLCar.ini", "Close"};
constexpr int kMenuItems = int(sizeof(kMenu) / sizeof(kMenu[0]));

void runMenu(const InputState& s) {
	bool up = s.keys[VK_UP] || (s.pad.connected && (s.pad.buttons & pad::DUP));
	bool down = s.keys[VK_DOWN] || (s.pad.connected && (s.pad.buttons & pad::DDOWN));
	bool left = s.keys[VK_LEFT] || (s.pad.connected && (s.pad.buttons & pad::DLEFT));
	bool right = s.keys[VK_RIGHT] || (s.pad.connected && (s.pad.buttons & pad::DRIGHT));
	bool ok = s.keys[VK_RETURN] || (s.pad.connected && (s.pad.buttons & pad::A));
	static bool pu, pd, pl, pr, pok;
	auto edge = [](bool now, bool& prev) { bool e = now && !prev; prev = now; return e; };
	if (edge(up, pu)) g_menuIndex = (g_menuIndex + kMenuItems - 1) % kMenuItems;
	if (edge(down, pd)) g_menuIndex = (g_menuIndex + 1) % kMenuItems;
	int dir = edge(right, pr) ? 1 : edge(left, pl) ? -1 : 0;
	bool select = edge(ok, pok);
	PAD::DISABLE_CONTROL_ACTION(0, 172, TRUE);  // phone up / down
	PAD::DISABLE_CONTROL_ACTION(0, 173, TRUE);
	PAD::DISABLE_CONTROL_ACTION(0, 174, TRUE);
	PAD::DISABLE_CONTROL_ACTION(0, 175, TRUE);
	PAD::DISABLE_CONTROL_ACTION(0, 176, TRUE);
	PAD::DISABLE_CONTROL_ACTION(0, 201, TRUE);

	if (g_menuIndex == 2 && dir) g_settings.preset = (g_settings.preset + 7 + dir) % 7;
	if (g_menuIndex == 3 && dir) g_settings.team ^= 1;
	if (g_menuIndex == 4 && (dir || select)) g_settings.sim[10] = g_settings.sim[10] != 0 ? 0.0f : 1.0f;
	if (select) {
		switch (g_menuIndex) {
			case 0: g_menuOpen = false; if (g_car) g_car.reset(); becomeCar(); break;
			case 1: g_menuOpen = false; orderCar(); break;
			case 5: g_car.reset(); break;
			case 6: notify(loadSettings() ? "RLCar.ini reloaded" : "RLCar.ini not found, using defaults"); break;
			case 7: g_menuOpen = false; break;
		}
	}
	if (g_car && g_car->car && g_menuIndex == 4 && (dir || select)) g_api.car_set_config(g_car->car, g_settings.sim);

	GRAPHICS::DRAW_RECT(0.13f, 0.30f, 0.22f, 0.36f, 0, 0, 0, 170, FALSE);
	drawText("RL CAR", 0.13f, 0.13f, 0.7f, 255, 160, 40, true);
	for (int i = 0; i < kMenuItems; i++) {
		char line[96];
		if (i == 2)
			snprintf(line, sizeof line, "Hitbox: < %s >", Settings::presetName(g_settings.preset));
		else if (i == 3)
			snprintf(line, sizeof line, "Team: < %s >", g_settings.team ? "orange" : "blue");
		else if (i == 4)
			snprintf(line, sizeof line, "Unlimited boost: %s", g_settings.sim[10] != 0 ? "on" : "off");
		else
			snprintf(line, sizeof line, "%s", kMenu[i]);
		bool sel = i == g_menuIndex;
		drawText(line, 0.04f, 0.18f + i * 0.033f, 0.42f, 255, sel ? 200 : 255, sel ? 60 : 255);
	}
}

// -------------------------------------------------------------------------------------- frame

// GTA controls that would otherwise act while the car is driven (vehicle, weapon, phone, exit).
void disableGtaDrivingControls() {
	static const int kControls[] = {59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 80, 85, 86,
	                                 87, 88, 89, 90, 99, 100, 101, 102, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113,
	                                 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129, 130,
	                                 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147,
	                                 148, 149, 150, 151, 152, 153, 154, 155, 0, 26, 27, 79, 345, 346, 347};
	for (int c : kControls) PAD::DISABLE_CONTROL_ACTION(0, c, TRUE);
}

void updateCamera(RlCar& c, float alpha, float dt, const DriveInput& in) {
	if (!c.cam) {
		c.cam = CAMERA::CREATE_CAM("DEFAULT_SCRIPTED_CAMERA", TRUE);
		CAMERA::SET_CAM_NEAR_CLIP(c.cam, 0.05f);
		CAMERA::SET_CAM_ACTIVE(c.cam, TRUE);
		CAMERA::RENDER_SCRIPT_CAMS(TRUE, FALSE, 0, TRUE, FALSE, 0);
	}
	float view[ffi::CAMERA_VIEW_FLOATS];
	uint32_t f = c.rearView ? ffi::CAMERA_REAR_VIEW : 0;
	if (!g_api.camera_update(c.camera, c.car, alpha, dt, g_settings.camera, in.lookRight, in.lookUp, f, view)) return;
	space::V3 p = c.frame.toGta(view);
	space::V3 rot = space::cameraRotToGta(view + 3);
	CAMERA::SET_CAM_COORD(c.cam, float(p.x), float(p.y), float(p.z));
	CAMERA::SET_CAM_ROT(c.cam, float(rot.x), float(rot.y), float(rot.z), 2);
	CAMERA::SET_CAM_FOV(c.cam, view[13]);
	CAMERA::INVALIDATE_IDLE_CAM();
}

void releaseCamera(RlCar& c) {
	if (!c.cam) return;
	CAMERA::RENDER_SCRIPT_CAMS(FALSE, FALSE, 0, TRUE, FALSE, 0);
	CAMERA::DESTROY_CAM(c.cam, FALSE);
	c.cam = 0;
}

void drawHud(const RlCar& c) {
	if (!g_settings.showHud) return;
	char line[64];
	float speedUu = std::fabs(c.pose[39]);
	double kmh = c.frame.lenToGta(speedUu) * 3.6;
	snprintf(line, sizeof line, "%d", int(c.pose[18] + 0.5f));
	drawText(line, 0.93f, 0.80f, 1.0f, 255, 170, 30, true);
	drawText("BOOST", 0.93f, 0.86f, 0.35f, 255, 255, 255, true);
	snprintf(line, sizeof line, "%.0f km/h%s", kmh, (c.flags & ffi::flags::SUPERSONIC) ? "  SUPERSONIC" : "");
	drawText(line, 0.93f, 0.89f, 0.35f, 255, 255, 255, true);
}

void tick() {
	InputState in = readInput();
	g_edges.update(in);
	Ped me = PLAYER::PLAYER_PED_ID();

	if (g_edges.pressed(Action::ReloadConfig)) notify(loadSettings() ? "RLCar.ini reloaded" : "RLCar.ini not found, using defaults");
	if (g_edges.pressed(Action::Menu)) g_menuOpen = !g_menuOpen;
	if (g_menuOpen) runMenu(in);
	if (g_orderAt && GetTickCount() >= g_orderAt) deliverOrder();

	bool inCar = g_car && g_car->veh && PED::IS_PED_IN_VEHICLE(me, g_car->veh, FALSE);
	if (inCar && GetTickCount() < g_car->exitingUntil) inCar = false;
	if (!inCar && g_edges.pressed(Action::BecomeCar)) becomeCar();

	if (!g_car || !g_car->car) return;
	RlCar& c = *g_car;
	inCar = PED::IS_PED_IN_VEHICLE(me, c.veh, FALSE) && GetTickCount() >= c.exitingUntil;

	// Getting in: F / Y next to the parked car.
	if (!inCar && !PED::IS_PED_IN_ANY_VEHICLE(me, TRUE) && GetTickCount() >= c.exitingUntil) {
		space::V3 p = v3(ENTITY::GET_ENTITY_COORDS(me, TRUE)), q = c.position();
		double d2 = (p.x - q.x) * (p.x - q.x) + (p.y - q.y) * (p.y - q.y) + (p.z - q.z) * (p.z - q.z);
		if (d2 < 36) {
			PAD::DISABLE_CONTROL_ACTION(0, 23, TRUE);  // GTA's own enter vehicle
			help("Press ~INPUT_ENTER~ to drive the Rocket League car.");
			if (PAD::IS_DISABLED_CONTROL_JUST_PRESSED(0, 23)) {
				putPlayerIn(c);
				inCar = true;
			}
		}
	}
	if (inCar && c.blip) HUD::REMOVE_BLIP(&c.blip), c.blip = 0;

	DriveInput drive{};
	if (inCar && !g_menuOpen) {
		disableGtaDrivingControls();
		drive = g_settings.bindings.drive(in);
		if (g_edges.pressed(Action::ExitCar)) {
			TASK::TASK_LEAVE_VEHICLE(me, c.veh, 0);
			c.exitingUntil = GetTickCount() + 2500;
			releaseCamera(c);
			inCar = false;
			drive = {};
		}
		if (g_edges.pressed(Action::ResetCar)) {
			space::V3 p = c.position();
			c.placeAt({p.x, p.y, p.z}, space::headingOf(space::dirToGta(c.pose + 3)));
		}
		if (g_settings.rearCameraToggle) {
			if (g_edges.pressed(Action::RearCamera)) c.rearView = !c.rearView;
		} else {
			c.rearView = drive.rearCamera;
		}
	}
	c.driving = inCar;

	float dt = MISC::GET_FRAME_TIME();
	if (dt > 0 && !HUD::IS_PAUSE_MENU_ACTIVE()) {
		uint32_t b = (drive.jump ? ffi::buttons::JUMP : 0) | (drive.boost ? ffi::buttons::BOOST : 0) |
		             (drive.handbrake ? ffi::buttons::HANDBRAKE : 0);
		g_api.car_advance_cb(c.car, c.world, dt, drive.throttle, drive.steer, drive.pitch, drive.yaw, drive.roll, b);
	}
	float alpha = g_api.car_alpha(c.car);
	c.applyPose(alpha);
	c.recentre();

	if (inCar) {
		updateCamera(c, alpha, dt, drive);
		drawHud(c);
	} else {
		releaseCamera(c);
	}
}

void scriptMain() {
	log("RL Car starting (folder %s)", g_dataDir.c_str());
	loadXInput();
	g_apiOk = ffi::load(g_dataDir + "\\rl_car_ffi.dll", g_api, g_apiError);
	if (!g_apiOk) {
		log("rl_car_ffi: %s", g_apiError.c_str());
		for (;;) {
			drawText(("RL Car: " + g_apiError).c_str(), 0.02f, 0.02f, 0.4f, 255, 80, 80);
			WAIT(0);
		}
	}
	bool found = loadSettings();
	log("rl_car_ffi ABI %u loaded; RLCar.ini %s; scale %.2f", g_api.abi_version(), found ? "read" : "missing (defaults)",
	    g_settings.worldScale);
	for (;;) {
		tick();
		WAIT(0);
	}
}

}  // namespace

BOOL APIENTRY DllMain(HMODULE module, DWORD reason, LPVOID) {
	if (reason == DLL_PROCESS_ATTACH) {
		g_module = module;
		char path[MAX_PATH];
		GetModuleFileNameA(module, path, MAX_PATH);
		g_dir = path;
		g_dir = g_dir.substr(0, g_dir.find_last_of("\\/"));
		g_dataDir = g_dir + "\\RLCar";
		scriptRegister(module, scriptMain);
	} else if (reason == DLL_PROCESS_DETACH) {
		scriptUnregister(module);
	}
	return TRUE;
}
