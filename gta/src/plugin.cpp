#include "natives.h"
#include "ini.h"
#include "probe_world.h"
#include "rlcar_ffi.h"
#include "effects.h"
#include "hid_pad.h"
#include "interact.h"
#include "model.h"
#include "weapons.h"
#include "settings.h"
#include "space.h"

#include <cmath>
#include <cstdarg>
#include <cstdio>
#include <memory>
#include <string>
#include <vector>
#include <xinput.h>

bool gtaProbe(const space::Frame& frame, int flags, Entity ignore, const float* from, const float* to, ProbeHit& hit);

namespace {

HMODULE g_module;
std::string g_dir;
std::string g_dataDir;
ffi::Api g_api{};
bool g_apiOk = false;
std::string g_apiError;
Settings g_settings;
rlm::Models g_models;
bool g_modelsOk = false;
std::vector<double> g_scratch;

void drawPoly(const double* a, const double* b, const double* c, const int* rgb) {
	if (g_settings.modelWinding != 2)
		GRAPHICS::DRAW_POLY(float(a[0]), float(a[1]), float(a[2]), float(b[0]), float(b[1]), float(b[2]), float(c[0]), float(c[1]), float(c[2]), rgb[0], rgb[1], rgb[2], 255);
	if (g_settings.modelWinding != 1)
		GRAPHICS::DRAW_POLY(float(a[0]), float(a[1]), float(a[2]), float(c[0]), float(c[1]), float(c[2]), float(b[0]), float(b[1]), float(b[2]), rgb[0], rgb[1], rgb[2], 255);
}

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
	} else {
		hidpad::read(s.pad);
	}
	return s;
}

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

bool loadAny(Hash h) {
	if (!STREAMING::IS_MODEL_IN_CDIMAGE(h) || !STREAMING::IS_MODEL_VALID(h)) return false;
	STREAMING::REQUEST_MODEL(h);
	for (int i = 0; i < 200 && !STREAMING::HAS_MODEL_LOADED(h); i++) WAIT(0);
	return STREAMING::HAS_MODEL_LOADED(h) != 0;
}

bool loadModel(Hash h) {
	if (!STREAMING::IS_MODEL_IN_CDIMAGE(h) || !STREAMING::IS_MODEL_A_VEHICLE(h)) return false;
	STREAMING::REQUEST_MODEL(h);
	for (int i = 0; i < 200 && !STREAMING::HAS_MODEL_LOADED(h); i++) WAIT(0);
	return STREAMING::HAS_MODEL_LOADED(h) != 0;
}

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
	float modelLift = 0;
	float pose[ffi::POSE_FLOATS] = {};
	uint32_t flags = 0;
	bool rearView = false;
	bool driving = false;
	DWORD exitingUntil = 0;
	ffi::Ball* ball = nullptr;
	Object ballProp = 0;
	float ballPropLift = 0;
	float ballPose[ffi::BALL_POSE_FLOATS] = {};
	space::Quat ballRot;
	bool ballCam = false;
	Interactions interactions;
	Effects effects;
	Weapons weapons;
	float hitbox[6] = {};
	float wheelSpin[4] = {}, wheelRate[4] = {};
	bool modelHidden = false;

	~RlCar() { destroy(); }

	void hideGtaModels(bool hide) {
		if (veh && ENTITY::DOES_ENTITY_EXIST(veh) && hide != modelHidden) ENTITY::SET_ENTITY_VISIBLE(veh, hide ? FALSE : TRUE, FALSE);
		modelHidden = hide;
		if (ballProp && ENTITY::DOES_ENTITY_EXIST(ballProp)) ENTITY::SET_ENTITY_VISIBLE(ballProp, hide && !g_models.ball.empty() ? FALSE : TRUE, FALSE);
		if (hide && driving) NETWORK::SET_ENTITY_LOCALLY_INVISIBLE(PLAYER::PLAYER_PED_ID());
	}

	void drawModels(float dt) {
		bool use = g_settings.models && g_modelsOk;
		hideGtaModels(use);
		if (!use) return;
		Vector3 cg = CAMERA::GET_FINAL_RENDERED_CAM_COORD();
		double cam[3] = {cg.x, cg.y, cg.z};
		rlm::Light light;
		light.ambient = g_settings.modelAmbient;
		light.diffuse = g_settings.modelDiffuse;
		light.brightness = g_settings.modelBrightness;
		double k = frame.k();
		const double d[3] = {1, -1, 1};
		double base[3][3];
		for (int i = 0; i < 3; i++)
			for (int j = 0; j < 3; j++) base[i][j] = d[i] * pose[3 + 3 * j + i] * k;
		space::V3 o = frame.toGta(pose);
		double dist = std::sqrt((o.x - cam[0]) * (o.x - cam[0]) + (o.y - cam[1]) * (o.y - cam[1]) + (o.z - cam[2]) * (o.z - cam[2]));
		bool onGround = (flags & ffi::flags::ON_GROUND) != 0;
		if (dist < g_settings.modelMaxDistance) {
			int lod = g_settings.modelDetail + int(dist / std::max(1.0f, g_settings.modelLodDistance));
			if (const rlm::Mesh* body = rlm::Models::pick(g_models.body, lod)) {
				rlm::Xform x;
				for (int i = 0; i < 3; i++)
					for (int j = 0; j < 3; j++) x.a[i][j] = base[i][j];
				x.t[0] = o.x, x.t[1] = o.y, x.t[2] = o.z;
				rlm::draw(*body, team, x, cam, light, g_scratch, drawPoly);
			}
			const rlm::Mesh* wheel = rlm::Models::pick(g_models.wheel, lod);
			for (int w = 0; wheel && w < 4; w++) {
				const float* wc = pose + 19 + 3 * w;
				float local[3] = {wc[0], wc[1], wc[2]};
				bool left = wc[1] < 0, front = wc[0] > 0;
				if (g_models.hasAnchors) {
					const float* an = g_models.anchors[(front ? 0 : 2) + (left ? 0 : 1)];
					local[0] = an[0], local[1] = an[1];
				}
				double radius = std::max(1.0f, pose[31 + w]);
				wheelRate[w] = onGround ? float(pose[39] / radius) : float(wheelRate[w] * std::exp(-0.5 * dt));
				wheelSpin[w] = float(std::fmod(wheelSpin[w] + wheelRate[w] * dt, 6.283185307));
				double cs = std::cos(wheelSpin[w]), sn = std::sin(wheelSpin[w]);
				double st = pose[35 + w], cst = std::cos(st), snt = std::sin(st);
				double spin[3][3] = {{cs, 0, sn}, {0, 1, 0}, {-sn, 0, cs}};
				double steer[3][3] = {{cst, -snt, 0}, {snt, cst, 0}, {0, 0, 1}};
				double mirror[3][3] = {{left ? -radius : radius, 0, 0}, {0, left ? -radius : radius, 0}, {0, 0, radius}};
				double m[3][3];
				rlm::mul(spin, mirror, m);
				rlm::mul(steer, m, m);
				rlm::Xform x;
				rlm::mul(base, m, x.a);
				float wp[3];
				for (int i = 0; i < 3; i++) wp[i] = pose[i] + pose[3 + i] * local[0] + pose[6 + i] * local[1] + pose[9 + i] * local[2];
				space::V3 g = frame.toGta(wp);
				x.t[0] = g.x, x.t[1] = g.y, x.t[2] = g.z;
				rlm::draw(*wheel, 0, x, cam, light, g_scratch, drawPoly);
			}
		}
		if (!ball || g_models.ball.empty()) return;
		space::V3 b = frame.toGta(ballPose);
		double bd = std::sqrt((b.x - cam[0]) * (b.x - cam[0]) + (b.y - cam[1]) * (b.y - cam[1]) + (b.z - cam[2]) * (b.z - cam[2]));
		if (bd > g_settings.modelMaxDistance) return;
		int lod = g_settings.modelDetail + int(bd / std::max(1.0f, g_settings.modelLodDistance * 1.5f));
		const rlm::Mesh* mesh = rlm::Models::pick(g_models.ball, lod);
		double qx = ballRot.x, qy = ballRot.y, qz = ballRot.z, qw = ballRot.w;
		double q[3][3] = {{1 - 2 * (qy * qy + qz * qz), 2 * (qx * qy - qz * qw), 2 * (qx * qz + qy * qw)},
		                  {2 * (qx * qy + qz * qw), 1 - 2 * (qx * qx + qz * qz), 2 * (qy * qz - qx * qw)},
		                  {2 * (qx * qz - qy * qw), 2 * (qy * qz + qx * qw), 1 - 2 * (qx * qx + qy * qy)}};
		double r = g_settings.ball[0] * k;
		double sc[3][3] = {{r, 0, 0}, {0, -r, 0}, {0, 0, r}};
		rlm::Xform x;
		rlm::mul(q, sc, x.a);
		x.t[0] = b.x, x.t[1] = b.y, x.t[2] = b.z;
		rlm::draw(*mesh, 0, x, cam, light, g_scratch, drawPoly);
	}

	void removeBall() {
		if (ballProp && ENTITY::DOES_ENTITY_EXIST(ballProp)) {
			ENTITY::SET_ENTITY_AS_MISSION_ENTITY(ballProp, TRUE, TRUE);
			ENTITY::DELETE_ENTITY(&ballProp);
		}
		ballProp = 0;
		if (ball) g_api.ball_free(ball), ball = nullptr;
		ballCam = false;
	}

	bool spawnBall() {
		if (!ball) ball = g_api.ball_new();
		g_api.ball_set_config(ball, g_settings.ball);
		if (!ballProp || !ENTITY::DOES_ENTITY_EXIST(ballProp)) {
			Hash model = 0;
			for (const std::string& name : {g_settings.ballModel, std::string("prop_beachball_02")}) {
				Hash h = MISC::GET_HASH_KEY(name.c_str());
				if (loadAny(h)) {
					model = h;
					log("ball model %s", name.c_str());
					break;
				}
			}
			if (!model) {
				log("no ball model could be loaded");
				removeBall();
				return false;
			}
			space::V3 p = position();
			ballProp = OBJECT::CREATE_OBJECT_NO_OFFSET(model, float(p.x), float(p.y), float(p.z) + 5, FALSE, TRUE, FALSE, 0);
			Vector3 mn{}, mx{};
			MISC::GET_MODEL_DIMENSIONS(model, &mn, &mx);
			STREAMING::SET_MODEL_AS_NO_LONGER_NEEDED(model);
			if (!ballProp) {
				removeBall();
				return false;
			}
			ballPropLift = -(mn.z + mx.z) * 0.5f;
			ENTITY::SET_ENTITY_AS_MISSION_ENTITY(ballProp, TRUE, TRUE);
			ENTITY::FREEZE_ENTITY_POSITION(ballProp, TRUE);
			ENTITY::SET_ENTITY_COLLISION(ballProp, FALSE, FALSE);
		}
		float fx = pose[3], fy = pose[4], fl = std::sqrt(fx * fx + fy * fy);
		if (fl < 1e-3f) fx = 1, fy = 0, fl = 1;
		float pos[3] = {pose[0] + fx / fl * 800, pose[1] + fy / fl * 800, pose[2] + 250};
		float vel[3] = {0, 0, -1};
		g_api.ball_reset(ball, pos, vel, nullptr);
		ballRot = {};
		ballCam = g_settings.ballCamOnSpawn;
		return true;
	}

	void applyBallPose(float alpha, float dt) {
		if (!ball || !ballProp) return;
		g_api.ball_pose(ball, alpha, ballPose);
		ballRot = space::spin(ballRot, space::spinToGta(ballPose + 6), dt);
		space::V3 p = frame.toGta(ballPose);
		ENTITY::SET_ENTITY_COORDS_NO_OFFSET(ballProp, float(p.x), float(p.y), float(p.z) + ballPropLift, FALSE, FALSE, FALSE);
		ENTITY::SET_ENTITY_QUATERNION(ballProp, ballRot.x, ballRot.y, ballRot.z, ballRot.w);
		float dx = ballPose[0] - pose[0], dy = ballPose[1] - pose[1], dz = ballPose[2] - pose[2];
		if (dx * dx + dy * dy + dz * dz > 60000.0f * 60000.0f) removeBall();
	}

	void destroy() {
		effects.stop(veh);
		removeBall();
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
		setLive(g_settings.effects.engineAudio);
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

		Vector3 mn{}, mx{};
		MISC::GET_MODEL_DIMENSIONS(model, &mn, &mx);
		g_api.preset_hitbox(uint32_t(preset), hitbox);
		frame.scale = g_settings.worldScale > 0 ? g_settings.worldScale : std::max(0.5, double(mx.y - mn.y) / (hitbox[0] / 100.0));
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
		log("car spawned: model %s (%.2f x %.2f x %.2f m), preset %d, team %d, scale %.2f, at %.1f %.1f %.1f, model lift %.2f m",
		    g_settings.fallbackModel.c_str(), mx.x - mn.x, mx.y - mn.y, mx.z - mn.z, preset, team, frame.scale, at.x, at.y, at.z, modelLift);
		return true;
	}

	void placeAt(space::V3 at, double heading) {
		frame.origin = at;
		float pos[3] = {0, 0, 40};
		g_api.car_reset(car, pos, space::headingToRlYaw(heading), 0, 0);
		probe->forget();
		g_api.camera_reset(camera);
	}

	void recentre() {
		float* p = pose;
		if (std::fabs(p[0]) < 20000 && std::fabs(p[1]) < 20000 && std::fabs(p[2]) < 20000) return;
		float d[3] = {-p[0], -p[1], -p[2]};
		frame.origin = frame.toGta(p);
		g_api.car_translate(car, d);
		g_api.camera_translate(camera, d);
		if (ball) g_api.ball_translate(ball, d);
		probe->forget();
	}

	space::V3 position() const { return frame.toGta(pose); }

	void applyWorldSettings() {
		probe->wallRamps = g_settings.wallRamps;
		probe->rampRadius = g_settings.wallRampRadius;
		probe->forget();
	}

	bool live = false;

	void drawHitbox() const {
		space::V3 c[8];
		for (int i = 0; i < 8; i++) {
			float p[3];
			for (int k = 0; k < 3; k++) {
				float lx = hitbox[3] + ((i & 1) ? 0.5f : -0.5f) * hitbox[0];
				float ly = hitbox[4] + ((i & 2) ? 0.5f : -0.5f) * hitbox[1];
				float lz = hitbox[5] + ((i & 4) ? 0.5f : -0.5f) * hitbox[2];
				p[k] = pose[k] + pose[3 + k] * lx + pose[6 + k] * ly + pose[9 + k] * lz;
			}
			c[i] = frame.toGta(p);
		}
		static const int e[12][2] = {{0, 1}, {2, 3}, {4, 5}, {6, 7}, {0, 2}, {1, 3}, {4, 6}, {5, 7}, {0, 4}, {1, 5}, {2, 6}, {3, 7}};
		for (auto& ed : e) {
			const space::V3 &a = c[ed[0]], &b = c[ed[1]];
			GRAPHICS::DRAW_LINE(float(a.x), float(a.y), float(a.z), float(b.x), float(b.y), float(b.z), 40, 255, 80, 255);
		}
		if (!ball) return;
		space::V3 bc = frame.toGta(ballPose);
		double r = frame.lenToGta(g_settings.ball[0]);
		for (int plane = 0; plane < 3; plane++) {
			for (int j = 0; j < 32; j++) {
				double a0 = j * 6.2831853 / 32, a1 = (j + 1) * 6.2831853 / 32;
				auto pt = [&](double a) {
					double u = std::cos(a) * r, v = std::sin(a) * r;
					return plane == 0 ? space::V3{bc.x + u, bc.y + v, bc.z} : plane == 1 ? space::V3{bc.x + u, bc.y, bc.z + v} : space::V3{bc.x, bc.y + u, bc.z + v};
				};
				space::V3 a = pt(a0), b = pt(a1);
				GRAPHICS::DRAW_LINE(float(a.x), float(a.y), float(a.z), float(b.x), float(b.y), float(b.z), 40, 200, 255, 255);
			}
		}
	}

	void setLive(bool on) {
		live = on;
		ENTITY::FREEZE_ENTITY_POSITION(veh, on ? FALSE : TRUE);
		ENTITY::SET_ENTITY_HAS_GRAVITY(veh, on ? FALSE : TRUE);
		VEHICLE::SET_VEHICLE_GRAVITY(veh, on ? FALSE : TRUE);
		ENTITY::SET_ENTITY_COLLISION(veh, FALSE, FALSE);
	}

	void applyPose(float alpha) {
		flags = g_api.car_pose(car, alpha, pose);
		space::V3 p = frame.toGta(pose);
		space::V3 up = space::dirToGta(pose + 9);
		space::Quat q = space::carRotToGta(pose + 3);
		ENTITY::SET_ENTITY_COORDS_NO_OFFSET(veh, float(p.x + up.x * modelLift), float(p.y + up.y * modelLift),
		                                    float(p.z + up.z * modelLift), FALSE, FALSE, FALSE);
		ENTITY::SET_ENTITY_QUATERNION(veh, q.x, q.y, q.z, q.w);
		float k = float(frame.k());
		if (live) {
			space::V3 v = space::dirToGta(pose + 12);
			ENTITY::SET_ENTITY_VELOCITY(veh, float(v.x * k), float(v.y * k), float(v.z * k));
		}
		float ex[3] = {0, (hitbox[3] - hitbox[0] * 0.5f) * k, hitbox[5] * k - modelLift};
		effects.update(veh, (flags & ffi::flags::BOOSTING) != 0, (flags & ffi::flags::SUPERSONIC) != 0, ex, g_settings.effects);
	}
};

std::unique_ptr<RlCar> g_car;
int g_menuIndex = 0;
bool g_menuOpen = false;
DWORD g_orderAt = 0;

bool loadSettings() {
	Ini ini;
	bool found = ini.loadFile(g_dataDir + "\\RLCar.ini");
	g_settings.load(ini, g_api);
	{
		std::string mlog;
		g_modelsOk = g_models.load(g_dataDir + "\\" + g_settings.modelFolder, Settings::presetName(g_settings.preset), mlog);
		log("models %s: %s%s", g_settings.modelFolder.c_str(), mlog.c_str(), g_modelsOk ? "drawing the real car" : "using the GTA models");
	}
	if (g_car && g_car->car) {
		g_api.car_set_config(g_car->car, g_settings.sim);
		g_car->applyWorldSettings();
		if (g_car->live != g_settings.effects.engineAudio) g_car->setLive(g_settings.effects.engineAudio);
		if (g_car->ball) g_api.ball_set_config(g_car->ball, g_settings.ball);
	}
	return found;
}

void putPlayerIn(RlCar& c) {
	Ped me = PLAYER::PLAYER_PED_ID();
	PED::SET_PED_INTO_VEHICLE(me, c.veh, -1);
	c.exitingUntil = 0;
	g_api.camera_reset(c.camera);
}

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
	HUD::SET_BLIP_SPRITE(g_car->blip, 225);
	HUD::SET_BLIP_COLOUR(g_car->blip, g_settings.team == 0 ? 3 : 17);
	notify("Your car is parked nearby. Press F (Y) next to it to get in.", "Mechanic");
}

const char* kMenu[] = {"Become the car", "Order from the Mechanic", "Hitbox", "Team", "Unlimited boost", "Spawn the ball",
                       "Remove the ball", "Remove the car", "Reload RLCar.ini", "Hitbox overlay", "Close"};
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
	PAD::DISABLE_CONTROL_ACTION(0, 172, TRUE);
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
			case 5:
				if (g_car && g_car->car && g_settings.ballEnabled && !g_car->spawnBall()) notify("RL Car: could not create the ball");
				break;
			case 6: if (g_car) g_car->removeBall(); break;
			case 7: g_car.reset(); break;
			case 8: notify(loadSettings() ? "RLCar.ini reloaded" : "RLCar.ini not found, using defaults"); break;
			case 9: g_settings.showHitbox = !g_settings.showHitbox; break;
			case 10: g_menuOpen = false; break;
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
		else if (i == 9)
			snprintf(line, sizeof line, "Hitbox overlay: %s", g_settings.showHitbox ? "on" : "off");
		else
			snprintf(line, sizeof line, "%s", kMenu[i]);
		bool sel = i == g_menuIndex;
		drawText(line, 0.04f, 0.18f + i * 0.033f, 0.42f, 255, sel ? 200 : 255, sel ? 60 : 255);
	}
}

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
	uint32_t f = (c.rearView ? ffi::CAMERA_REAR_VIEW : 0) | (c.ballCam && c.ball ? ffi::CAMERA_BALL_CAM : 0);
	if (!g_api.camera_update_ball(c.camera, c.car, c.ball, alpha, dt, g_settings.camera, in.lookRight, in.lookUp, f, view)) return;
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
	if (c.ball) drawText(c.ballCam ? "BALL CAM" : "CAR CAM", 0.93f, 0.77f, 0.35f, 255, 255, 255, true);
	if (g_settings.weapons.enabled) drawText(c.weapons.name(), 0.93f, 0.74f, 0.35f, 255, 200, 120, true);
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

	if (!inCar && !PED::IS_PED_IN_ANY_VEHICLE(me, TRUE) && GetTickCount() >= c.exitingUntil) {
		space::V3 p = v3(ENTITY::GET_ENTITY_COORDS(me, TRUE)), q = c.position();
		double d2 = (p.x - q.x) * (p.x - q.x) + (p.y - q.y) * (p.y - q.y) + (p.z - q.z) * (p.z - q.z);
		if (d2 < 36) {
			PAD::DISABLE_CONTROL_ACTION(0, 23, TRUE);
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
		if (g_edges.pressed(Action::SpawnBall) && g_settings.ballEnabled && !c.spawnBall()) notify("RL Car: could not create the ball");
		if (g_edges.pressed(Action::BallCam) && c.ball) c.ballCam = !c.ballCam;
		{
			float k = float(c.frame.k());
			float fx = (c.hitbox[3] + c.hitbox[0] * 0.5f + 15.0f) * k, sy = c.hitbox[1] * 0.4f * k, z = c.hitbox[5] * k - c.modelLift;
			float gl[3] = {-sy, fx, z}, gr[3] = {sy, fx, z};
			c.weapons.update(c.veh, me, g_edges.now[size_t(Action::FireWeapon)], g_edges.pressed(Action::NextWeapon), gl, gr,
			                 MISC::GET_FRAME_TIME(), g_settings.weapons);
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
		float speed = std::sqrt(c.pose[12] * c.pose[12] + c.pose[13] * c.pose[13] + c.pose[14] * c.pose[14]);
		CarView view{&c.frame, c.pose, {}, c.car, c.veh, speed * dt + 10.0f};
		std::copy(c.hitbox, c.hitbox + 6, view.hitbox);
		if (int hits = c.interactions.update(g_api, view, g_settings.interact)) log("hit %d GTA entities at %.0f uu/s", hits, speed);
		{
			space::V3 p = c.frame.toGta(c.pose);
			space::V3 v = space::dirToGta(c.pose + 12);
			double k = c.frame.k();
			STREAMING::REQUEST_COLLISION_AT_COORD(float(p.x), float(p.y), float(p.z));
			STREAMING::REQUEST_COLLISION_AT_COORD(float(p.x + v.x * k * 0.5), float(p.y + v.y * k * 0.5), float(p.z + v.z * k * 0.5));
		}
		uint32_t b = (drive.jump ? ffi::buttons::JUMP : 0) | (drive.boost ? ffi::buttons::BOOST : 0) |
		             (drive.handbrake ? ffi::buttons::HANDBRAKE : 0);
		if (c.ball)
			g_api.scene_advance_cb(c.car, c.ball, c.world, dt, drive.throttle, drive.steer, drive.pitch, drive.yaw, drive.roll, b);
		else
			g_api.car_advance_cb(c.car, c.world, dt, drive.throttle, drive.steer, drive.pitch, drive.yaw, drive.roll, b);
	}
	float alpha = g_api.car_alpha(c.car);
	c.applyPose(alpha);
	c.applyBallPose(alpha, HUD::IS_PAUSE_MENU_ACTIVE() ? 0.0f : dt);
	c.drawModels(HUD::IS_PAUSE_MENU_ACTIVE() ? 0.0f : dt);
	if (g_settings.showHitbox) c.drawHitbox();
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
	hidpad::start();
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

}

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
