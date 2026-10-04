#include "plugin.h"
#include "camera.h"
#include "havok_world.h"
#include "ini.h"
#include "input.h"
#include "interact.h"
#include "puppet.h"
#include "rlcar_ffi.h"
#include "settings.h"
#include "sky_world.h"
#include "space.h"
#include "visual.h"

namespace plugin {

namespace {

std::string g_dir;
ffi::Api g_api{};
bool g_apiOk = false;
Settings g_settings;
uint32_t g_worldId = 0;

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

void notify(const std::string& text) { RE::SendHUDMessage::ShowHUDMessage(("RL Car: " + text).c_str()); }

RE::NiPoint3 ni(space::V3 v) { return {float(v.x), float(v.y), float(v.z)}; }

RE::NiMatrix3 niMatrix(const double m[3][3]) {
	RE::NiMatrix3 r;
	for (int i = 0; i < 3; i++)
		for (int j = 0; j < 3; j++) r.entry[i][j] = float(m[i][j]);
	return r;
}

uint32_t worldIdOf(RE::PlayerCharacter* player) {
	auto* cell = player->GetParentCell();
	if (!cell) return 0;
	auto* ws = player->GetWorldspace();
	return cell->IsInteriorCell() || !ws ? cell->GetFormID() : ws->GetFormID();
}

struct RlCar {
	ffi::Car* car = nullptr;
	ffi::World* world = nullptr;
	void* camera = nullptr;
	std::unique_ptr<sky::SkyWorld> sky;
	space::Frame frame;
	float pose[ffi::POSE_FLOATS] = {};
	uint32_t flags = 0;
	float hitbox[6] = {};
	bool rearView = false;
	visual::Prop body;
	RE::TESBoundObject* base = nullptr;
	float modelScale = 1, modelLength = 1;
	float cacheAge = 0;
	havok::Stats stats;
	int gathers = 0, outran = 0;
	double gatherMs = 0, gatherMaxMs = 0;
	float logTimer = 0;
	RE::NiPoint3 heldFeet;
	bool held = false;
	ffi::Ball* ball = nullptr;
	visual::Prop ballProp;
	float ballPose[ffi::BALL_POSE_FLOATS] = {};
	space::Quat ballRot;
	float ballModelRadius = 1, ballCacheAge = 0;
	bool ballCam = false;
	havok::Stats ballStats;
	Interactions interactions;

	~RlCar() { destroy(); }

	void destroy() {
		removeBall();
		body.remove();
		if (camera) g_api.camera_free(camera), camera = nullptr;
		if (world) g_api.cbworld_free(world), world = nullptr;
		if (car) g_api.car_free(car), car = nullptr;
	}

	space::V3 position() const { return frame.toSky(pose); }

	bool spawn(RE::PlayerCharacter* player) {
		base = visual::lookup(g_settings.carForm.plugin, g_settings.carForm.id);
		if (!base) {
			logger::warn("CarForm {}|{:06X} not found; using Skyrim.esm|01C0C0", g_settings.carForm.plugin, g_settings.carForm.id);
			notify("CarForm not found, using the hand cart");
			base = visual::lookup("Skyrim.esm", 0x01C0C0);
		}
		if (!base || !body.spawn(base, player)) {
			logger::error("could not place the car's model");
			return false;
		}
		g_api.preset_hitbox(uint32_t(g_settings.preset), hitbox);
		float yaw = g_settings.modelYaw * float(space::kPi / 180.0);
		float ex = body.boundMax[0] - body.boundMin[0], ey = body.boundMax[1] - body.boundMin[1];
		modelLength = std::max(10.0f, std::fabs(std::sin(yaw)) * ex + std::fabs(std::cos(yaw)) * ey);
		frame.unitsPerMetre = RE::bhkWorld::GetWorldScaleInverse();
		frame.scale = g_settings.worldScale > 0 ? g_settings.worldScale
		                                         : std::clamp(double(modelLength) / (hitbox[0] * frame.unitsPerMetre / 100.0), 0.5, 5.0);
		modelScale = float(hitbox[0] * frame.k() / modelLength);

		car = g_api.car_new(uint32_t(g_settings.preset));
		g_api.car_set_config(car, g_settings.sim);
		sky = std::make_unique<sky::SkyWorld>();
		sky->wallRamps = g_settings.wallRamps;
		sky->rampRadius = g_settings.wallRampRadius;
		world = g_api.cbworld_new(sky.get(), &sky::SkyWorld::cbRaycast, &sky::SkyWorld::cbBox, &sky::SkyWorld::cbSphere);
		camera = g_api.camera_new();
		auto at = player->GetPosition();
		startAt(player, player->data.angle.z);
		logger::info("car: {} {:08X} ({:.0f} x {:.0f} x {:.0f} units), preset {}, scale {:.2f} ({:.1f} units/uu), model scale {:.2f}, at {:.0f} {:.0f} {:.0f}",
		             base->GetName(), base->GetFormID(), ex, ey, body.boundMax[2] - body.boundMin[2], Settings::presetName(g_settings.preset),
		             frame.scale, frame.k(), modelScale, at.x, at.y, at.z);
		return true;
	}

	// The car upright where the player is, the collision copied again from scratch.
	void startAt(RE::PlayerCharacter* player, float heading) {
		removeBall();
		interactions.clear();
		auto at = player->GetPosition();
		frame.origin = {at.x, at.y, at.z};
		float pos[3] = {0, 0, 20};
		g_api.car_reset(car, pos, space::headingToRlYaw(heading), 0, 0);
		g_api.camera_reset(camera);
		g_api.car_pose(car, 1, pose);
		sky->forget();
		sky->car.clear();
		held = false;
		gather(player);
	}

	void gather(RE::PlayerCharacter* player) {
		havok::Query q;
		q.frame = &frame;
		const float R = g_settings.cacheRadius;
		const float* v = pose + 12;
		for (int i = 0; i < 3; i++) {
			float centre = pose[i] + std::clamp(v[i] * 0.25f, -R * 0.3f, R * 0.3f);
			q.lo[i] = centre - R, q.hi[i] = centre + R;
		}
		q.ignore[0] = body.get();
		q.ignore[1] = ballProp.get();
		havok::gather(player->GetParentCell(), q, sky->car, stats);
		sky->boxCache = &sky->car;
		cacheAge = 0;
		gathers++;
		gatherMs += stats.ms;
		gatherMaxMs = std::max(gatherMaxMs, stats.ms);
		if (gathers <= 3 || (stats.ms > 15 && gathers % 20 == 0))
			logger::info("collision cache: {} bodies, {} triangles ({} boxes, {} capsules, {} convex, {} as bounding boxes) in {:.1f} ms{}",
			             stats.bodies, stats.tris, stats.boxes, stats.capsules, stats.convexes, stats.fallbacks, stats.ms,
			             stats.faults ? fmt::format(", {} faulted shapes skipped", stats.faults) : "");
	}

	void refresh(RE::PlayerCharacter* player, float delta) {
		cacheAge += delta;
		const float* v = pose + 12;
		float ahead[3] = {pose[0] + v[0] * (delta + 0.1f), pose[1] + v[1] * (delta + 0.1f), pose[2] + v[2] * (delta + 0.1f)};
		if (!sky->car.covers(pose, 150.0f) && outran++ < 20)
			logger::warn("the car outran its collision copy at {:.0f} uu/s (frame {:.0f} ms); copying again",
			             std::sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]), delta * 1000);
		bool edge = !sky->car.covers(ahead, g_settings.cacheRadius * 0.5f);
		if (edge || cacheAge >= g_settings.cacheRefresh) gather(player);
	}

	void removeBall() {
		ballProp.remove();
		if (ball) g_api.ball_free(ball), ball = nullptr;
		ballCam = false;
	}

	bool spawnBall(RE::PlayerCharacter* player) {
		auto* ballBase = visual::lookup(g_settings.ballForm.plugin, g_settings.ballForm.id);
		if (!ballBase) {
			logger::warn("Ball Form {}|{:06X} not found; using Skyrim.esm|0C8868", g_settings.ballForm.plugin, g_settings.ballForm.id);
			ballBase = visual::lookup("Skyrim.esm", 0x0C8868);
		}
		if (!ballProp.get() && (!ballBase || !ballProp.spawn(ballBase, player))) {
			logger::error("could not place the ball's model");
			return false;
		}
		ballModelRadius = 1;
		for (int i = 0; i < 3; i++) ballModelRadius = std::max(ballModelRadius, (ballProp.boundMax[i] - ballProp.boundMin[i]) * 0.5f);
		if (!ball) ball = g_api.ball_new();
		g_api.ball_set_config(ball, g_settings.ball);
		float fx = pose[3], fy = pose[4], fl = std::sqrt(fx * fx + fy * fy);
		if (fl < 1e-3f) fx = 1, fy = 0, fl = 1;
		float pos[3] = {pose[0] + fx / fl * 800, pose[1] + fy / fl * 800, pose[2] + 250};
		float vel[3] = {0, 0, -1};
		g_api.ball_reset(ball, pos, vel, nullptr);
		g_api.ball_pose(ball, 1, ballPose);
		ballRot = {};
		ballCam = g_settings.ballCamOnSpawn;
		ballCacheAge = 1e9f;
		logger::info("ball: {} {:08X}, radius {:.2f} uu = {:.0f} units, model scale {:.2f}", ballBase ? ballBase->GetName() : "?",
		             ballBase ? ballBase->GetFormID() : 0, g_settings.ball[0], frame.lenToSky(g_settings.ball[0]),
		             frame.lenToSky(g_settings.ball[0]) / ballModelRadius);
		return true;
	}

	// The ball collides with the car's copy while it's inside it, else with its own copy around it.
	void ballCollision(RE::PlayerCharacter* player, float delta) {
		const float r = g_settings.ball[0];
		ballCacheAge += delta;
		if (sky->car.covers(ballPose, r + 300.0f)) {
			sky->sphereCache = &sky->car;
			return;
		}
		if (!sky->ball.covers(ballPose, r + 300.0f) || ballCacheAge >= g_settings.cacheRefresh) {
			havok::Query q;
			q.frame = &frame;
			const float R = std::max(r * 4, 1500.0f);
			for (int i = 0; i < 3; i++) {
				float centre = ballPose[i] + std::clamp(ballPose[3 + i] * 0.25f, -R * 0.3f, R * 0.3f);
				q.lo[i] = centre - R, q.hi[i] = centre + R;
			}
			q.ignore[0] = body.get();
			q.ignore[1] = ballProp.get();
			havok::gather(player->GetParentCell(), q, sky->ball, ballStats);
			ballCacheAge = 0;
		}
		sky->sphereCache = &sky->ball;
	}

	void applyBallPose(RE::PlayerCharacter* player, float alpha, float dt) {
		if (!ball) return;
		g_api.ball_pose(ball, alpha, ballPose);
		ballRot = space::spin(ballRot, space::spinToSky(ballPose + 6), dt);
		double m[3][3];
		space::quatToMatrix(ballRot, m);
		float s = float(frame.lenToSky(g_settings.ball[0]) / ballModelRadius);
		double c[3] = {(ballProp.boundMin[0] + ballProp.boundMax[0]) * 0.5 * s, (ballProp.boundMin[1] + ballProp.boundMax[1]) * 0.5 * s,
		               (ballProp.boundMin[2] + ballProp.boundMax[2]) * 0.5 * s};
		space::V3 p = frame.toSky(ballPose);
		RE::NiPoint3 origin{float(p.x - (m[0][0] * c[0] + m[0][1] * c[1] + m[0][2] * c[2])), float(p.y - (m[1][0] * c[0] + m[1][1] * c[1] + m[1][2] * c[2])),
		                    float(p.z - (m[2][0] * c[0] + m[2][1] * c[1] + m[2][2] * c[2]))};
		ballProp.place(player, origin, niMatrix(m), s);
		float dx = ballPose[0] - pose[0], dy = ballPose[1] - pose[1], dz = ballPose[2] - pose[2];
		if (dx * dx + dy * dy + dz * dz > 15000.0f * 15000.0f || ballPose[2] - pose[2] < -20000.0f) {
			logger::info("the ball is lost ({:.0f} uu away); removed", std::sqrt(dx * dx + dy * dy + dz * dz));
			notify("the ball is lost");
			removeBall();
		}
	}

	void reset() {
		float pos[3] = {pose[0], pose[1], pose[2] + 20};
		g_api.car_reset(car, pos, std::atan2(pose[4], pose[3]), 0, 0);
		g_api.camera_reset(camera);
		sky->forget();
	}

	void recentre(RE::PlayerCharacter* player) {
		if (std::fabs(pose[0]) < 20000 && std::fabs(pose[1]) < 20000 && std::fabs(pose[2]) < 20000) return;
		float d[3] = {-pose[0], -pose[1], -pose[2]};
		frame.origin = frame.toSky(pose);
		g_api.car_translate(car, d);
		g_api.camera_translate(camera, d);
		if (ball) {
			g_api.ball_translate(ball, d);
			for (int i = 0; i < 3; i++) ballPose[i] += d[i];
		}
		sky->car.translate(d);
		sky->ball.translate(d);
		sky->forget();
		for (int i = 0; i < 3; i++) pose[i] += d[i];
		gather(player);
	}

	void applyPose(RE::PlayerCharacter* player, float alpha) {
		flags = g_api.car_pose(car, alpha, pose);
		double m[3][3];
		space::carRotToSky(pose + 3, m);
		double yaw = g_settings.modelYaw * space::kPi / 180.0, cy = std::cos(yaw), sy = std::sin(yaw);
		double rz[3][3] = {{cy, -sy, 0}, {sy, cy, 0}, {0, 0, 1}}, mm[3][3];
		for (int i = 0; i < 3; i++)
			for (int j = 0; j < 3; j++) mm[i][j] = m[i][0] * rz[0][j] + m[i][1] * rz[1][j] + m[i][2] * rz[2][j];
		// The model's footprint centre on the hitbox centre, its bottom on the ground under the car.
		float anchorRl[3];
		for (int i = 0; i < 3; i++) anchorRl[i] = pose[i] + pose[3 + i] * hitbox[3] + pose[6 + i] * hitbox[4] + pose[9 + i] * -17.0f;
		space::V3 w = frame.toSky(anchorRl);
		space::V3 up = space::dirToSky(pose + 9);
		double a[3] = {(body.boundMin[0] + body.boundMax[0]) * 0.5 * modelScale, (body.boundMin[1] + body.boundMax[1]) * 0.5 * modelScale,
		               body.boundMin[2] * modelScale};
		RE::NiPoint3 origin{float(w.x + up.x * g_settings.modelOffset - (mm[0][0] * a[0] + mm[0][1] * a[1] + mm[0][2] * a[2])),
		                    float(w.y + up.y * g_settings.modelOffset - (mm[1][0] * a[0] + mm[1][1] * a[1] + mm[1][2] * a[2])),
		                    float(w.z + up.z * g_settings.modelOffset - (mm[2][0] * a[0] + mm[2][1] * a[1] + mm[2][2] * a[2]))};
		if (!body.get() && base) {
			logger::info("the car's model was unloaded; placing it again");
			body.spawn(base, player);
		}
		body.place(player, origin, niMatrix(mm), modelScale);
		space::V3 c = position();
		heldFeet = {float(c.x), float(c.y), float(c.z - frame.lenToSky(17.0))};
		held = true;
		puppet::hold(player, heldFeet);
	}

	// Skyrim moved the player itself (a script, fast travel inside the worldspace).
	bool teleported(RE::PlayerCharacter* player) const { return held && player->GetPosition().GetDistance(heldFeet) > 300.0f; }

	void updateCamera(RE::PlayerCharacter* player, float alpha, float dt, const DriveInput& in) {
		float view[ffi::CAMERA_VIEW_FLOATS];
		uint32_t f = (rearView ? ffi::CAMERA_REAR_VIEW : 0) | (ballCam && ball ? ffi::CAMERA_BALL_CAM : 0);
		if (!g_api.camera_update_ball(camera, car, ball, alpha, dt, g_settings.camera, in.lookRight, in.lookUp, f, view)) return;
		camera::set(player, ni(frame.toSky(view)), space::cameraToSky(view + 3), float(space::verticalFovToSkyrim(view[13])));
	}

	// Where the Dragonborn stands when they get out: on the ground under the car.
	RE::NiPoint3 dropPoint() const {
		float down[3] = {0, 0, -1}, t, n[3];
		float from[3] = {pose[0], pose[1], pose[2] + 40};
		space::V3 c = position();
		if (sky && sky::SkyWorld::rayCache(sky->car, from, down, 5000, t, n)) {
			float hit[3] = {from[0], from[1], from[2] - t};
			c = frame.toSky(hit);
			return {float(c.x), float(c.y), float(c.z + 5)};
		}
		return {float(c.x), float(c.y), float(c.z)};
	}
};

std::unique_ptr<RlCar> g_car;

bool loadSettings() {
	Ini ini;
	bool found = ini.loadFile(g_dir + "\\RLCar.ini");
	g_settings.load(ini, g_api);
	if (g_car && g_car->car) {
		g_api.car_set_config(g_car->car, g_settings.sim);
		if (g_car->ball) g_api.ball_set_config(g_car->ball, g_settings.ball);
		g_car->sky->wallRamps = g_settings.wallRamps;
		g_car->sky->rampRadius = g_settings.wallRampRadius;
		g_car->sky->forget();
	}
	return found;
}

void becomeCar(RE::PlayerCharacter* player) {
	if (player->IsDead() || player->IsOnMount() || player->AsActorState()->GetSitSleepState() != RE::SIT_SLEEP_STATE::kNormal) {
		notify("not now");
		return;
	}
	g_car = std::make_unique<RlCar>();
	if (!g_car->spawn(player)) {
		g_car.reset();
		notify("could not create the car (see RLCar.log)");
		return;
	}
	g_worldId = worldIdOf(player);
	puppet::begin(player);
	input::setDriving(true);
	notify("you are the car. " + g_settings.becomeCarKey + " again to get out");
}

void leaveCar(RE::PlayerCharacter* player, const char* why) {
	if (!g_car) return;
	RE::NiPoint3 at = g_car->dropPoint();
	float heading = float(space::headingOf(space::dirToSky(g_car->pose + 3)));
	logger::info("left the car ({}) at {:.0f} {:.0f} {:.0f}; {} collision gathers, {:.1f} ms average, {:.1f} ms worst", why, at.x, at.y, at.z,
	             g_car->gathers, g_car->gathers ? g_car->gatherMs / g_car->gathers : 0.0, g_car->gatherMaxMs);
	camera::release();
	input::setDriving(false);
	puppet::end(player, at, heading);
	g_car.reset();
}

struct PlayerUpdateHook {
	static void thunk(RE::PlayerCharacter* self, float delta) {
		func(self, delta);
		try {
			update(self, delta);
		} catch (const std::exception& e) {
			logger::error("per-frame update: {}", e.what());
		}
	}
	static inline REL::Relocation<decltype(thunk)> func;
};

}

void install() {
	char path[MAX_PATH];
	HMODULE self = nullptr;
	GetModuleHandleExA(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, reinterpret_cast<LPCSTR>(&install), &self);
	GetModuleFileNameA(self, path, MAX_PATH);
	std::string dll = path;
	g_dir = dll.substr(0, dll.find_last_of("\\/")) + "\\RLCar";
	std::string error;
	g_apiOk = ffi::load(g_dir + "\\rl_car_ffi.dll", g_api, error);
	if (!g_apiOk) {
		logger::error("rl_car_ffi: {} (expected in {})", error, g_dir);
		RE::DebugMessageBox(("RL Car: " + error).c_str());
		return;
	}
	bool found = loadSettings();
	logger::info("rl_car_ffi ABI {} loaded from {}; RLCar.ini {}", g_api.abi_version(), g_dir, found ? "read" : "missing (defaults)");
	REL::Relocation<std::uintptr_t> playerVtbl{RE::VTABLE_PlayerCharacter[0]};
	PlayerUpdateHook::func = playerVtbl.write_vfunc(0xAD, PlayerUpdateHook::thunk);
	camera::install();
	input::install();
	logger::info("hooks installed; {} becomes the car", g_settings.becomeCarKey);
}

void onLoading(const char* why) {
	if (auto* player = RE::PlayerCharacter::GetSingleton(); player && g_car) leaveCar(player, why);
	g_worldId = 0;
}

bool driving() { return g_car != nullptr; }

void update(RE::PlayerCharacter* player, float delta) {
	if (!g_apiOk) return;
	InputState in = input::read();
	bool menu = input::skyrimMenuOpen();
	if (menu) in = {};
	g_edges.update(in);

	if (g_edges.pressed(Action::ReloadConfig)) notify(loadSettings() ? "RLCar.ini reloaded" : "RLCar.ini not found, using defaults");
	if (!g_car) {
		if (!menu && g_edges.pressed(Action::BecomeCar)) becomeCar(player);
		return;
	}
	RlCar& c = *g_car;
	if (player->IsDead()) return leaveCar(player, "the Dragonborn died");
	auto* ui = RE::UI::GetSingleton();
	if (!player->GetParentCell() || !player->Is3DLoaded() || (ui && ui->IsMenuOpen(RE::LoadingMenu::MENU_NAME))) return;
	if (uint32_t id = worldIdOf(player); id != g_worldId || c.teleported(player)) {
		auto p = player->GetPosition();
		logger::info("{} ({:08X}): the car starts again at {:.0f} {:.0f} {:.0f}", id != g_worldId ? "new cell or worldspace" : "Skyrim moved the player",
		             id, p.x, p.y, p.z);
		g_worldId = id;
		c.startAt(player, player->data.angle.z);
	}
	if (!menu && (g_edges.pressed(Action::BecomeCar) || g_edges.pressed(Action::ExitCar))) return leaveCar(player, "key");

	DriveInput drive{};
	if (!menu) {
		drive = g_settings.bindings.drive(in);
		if (g_edges.pressed(Action::ResetCar)) c.reset();
		c.rearView = g_settings.rearCameraToggle ? (c.rearView != g_edges.pressed(Action::RearCamera)) : drive.rearCamera;
		if (g_edges.pressed(Action::SpawnBall) && g_settings.ballEnabled && !c.spawnBall(player)) notify("could not create the ball");
		if (g_edges.pressed(Action::BallCam) && c.ball) c.ballCam = !c.ballCam;
	}

	float dt = std::min(delta, 0.1f);
	if (dt > 0 && !menu) {
		c.refresh(player, dt);
		uint32_t b = (drive.jump ? ffi::buttons::JUMP : 0) | (drive.boost ? ffi::buttons::BOOST : 0) | (drive.handbrake ? ffi::buttons::HANDBRAKE : 0);
		if (c.ball) {
			c.ballCollision(player, dt);
			g_api.scene_advance_cb(c.car, c.ball, c.world, dt, drive.throttle, drive.steer, drive.pitch, drive.yaw, drive.roll, b);
		} else {
			g_api.car_advance_cb(c.car, c.world, dt, drive.throttle, drive.steer, drive.pitch, drive.yaw, drive.roll, b);
		}
		float speed = std::sqrt(c.pose[12] * c.pose[12] + c.pose[13] * c.pose[13] + c.pose[14] * c.pose[14]);
		CarView view{&c.frame, c.pose, {}, c.car, speed * dt + 10.0f};
		std::copy(c.hitbox, c.hitbox + 6, view.hitbox);
		c.interactions.update(g_api, player, view, g_settings.interact, dt);
	}
	float alpha = g_api.car_alpha(c.car);
	c.applyPose(player, alpha);
	c.applyBallPose(player, alpha, menu ? 0.0f : dt);
	c.updateCamera(player, alpha, menu ? 0.0f : dt, drive);
	c.recentre(player);

	if (g_settings.debugLog && (c.logTimer -= dt) <= 0) {
		c.logTimer = 2.0f;
		space::V3 p = c.position();
		logger::info("car at {:.0f} {:.0f} {:.0f}, {:.0f} uu/s, {}{}, boost {:.0f}, cache {} tris, {} queries", p.x, p.y, p.z,
		             std::sqrt(c.pose[12] * c.pose[12] + c.pose[13] * c.pose[13] + c.pose[14] * c.pose[14]),
		             (c.flags & ffi::flags::ON_GROUND) ? "on the ground" : "in the air", (c.flags & ffi::flags::SUPERSONIC) ? ", supersonic" : "",
		             c.pose[18], c.stats.tris, c.sky->queries);
	}
}

}
