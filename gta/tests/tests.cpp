#include "../src/bindings.h"
#include "../src/geom.h"
#include "../src/ini.h"
#include "../src/probe_world.h"
#include "../src/model.h"
#include "../src/rlcar_ffi.h"
#include "../src/settings.h"
#include "../src/space.h"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <cstdlib>
#include <string>
#include <vector>
#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif

static int g_failures = 0, g_checks = 0;
#define CHECK(cond)                                                              \
	do {                                                                         \
		g_checks++;                                                              \
		if (!(cond)) {                                                           \
			g_failures++;                                                        \
			std::printf("FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond);          \
		}                                                                        \
	} while (0)
#define CHECK_NEAR(a, b, eps) CHECK(std::fabs(double(a) - double(b)) <= (eps))

static ffi::Api api;
static std::string g_lib;

static void* rawSym(const char* name) {
#ifdef _WIN32
	return reinterpret_cast<void*>(GetProcAddress(GetModuleHandleA("rl_car_ffi.dll"), name));
#else
	static void* h = dlopen(g_lib.c_str(), RTLD_NOW);
	return dlsym(h, name);
#endif
}

static void testSpace() {
	space::Frame f;
	f.origin = {100, -200, 30};
	f.scale = 2.5;
	float rl[3] = {400, 100, -40}, back[3];
	space::V3 g = f.toGta(rl);
	CHECK_NEAR(g.x, 110, 1e-9);
	CHECK_NEAR(g.y, -202.5, 1e-9);
	CHECK_NEAR(g.z, 29, 1e-9);
	f.toRl(g, back);
	for (int i = 0; i < 3; i++) CHECK_NEAR(back[i], rl[i], 1e-3);

	for (double h : {0.0, 45.0, 90.0, 180.0, -135.0}) {
		float y = space::headingToRlYaw(h);
		float fwd[3] = {std::cos(y), std::sin(y), 0};
		space::V3 gf = space::dirToGta(fwd);
		double back = space::headingOf(gf);
		double diff = std::fmod(back - h + 540.0, 360.0) - 180.0;
		CHECK_NEAR(diff, 0, 1e-4);
	}

	float cols[9] = {1, 0, 0, 0, 1, 0, 0, 0, 1};
	space::Quat q = space::carRotToGta(cols);
	auto rot = [&](double vx, double vy, double vz, double* o) {
		double x = q.x, y = q.y, z = q.z, w = q.w;
		double tx = 2 * (y * vz - z * vy), ty = 2 * (z * vx - x * vz), tz = 2 * (x * vy - y * vx);
		o[0] = vx + w * tx + (y * tz - z * ty);
		o[1] = vy + w * ty + (z * tx - x * tz);
		o[2] = vz + w * tz + (x * ty - y * tx);
	};
	double o[3];
	rot(0, 1, 0, o);
	CHECK_NEAR(o[0], 1, 1e-6);
	CHECK_NEAR(o[1], 0, 1e-6);
	rot(1, 0, 0, o);
	CHECK_NEAR(o[1], -1, 1e-6);
	rot(0, 0, 1, o);
	CHECK_NEAR(o[2], 1, 1e-6);

	float cam[9] = {1, 0, 0, 0, 1, 0, 0, 0, 1};
	space::V3 r = space::cameraRotToGta(cam);
	CHECK_NEAR(r.x, 0, 1e-6);
	CHECK_NEAR(r.y, 0, 1e-6);
	CHECK_NEAR(r.z, -90, 1e-6);
}

static bool planeProbe(const float* a, const float* b, ProbeHit& h) {
	struct P {
		float n[3], d;
	} planes[] = {{{0, 0, 1}, 0}, {{-1, 0, 0}, -600}};
	float best = 2;
	for (auto& p : planes) {
		float da = p.n[0] * a[0] + p.n[1] * a[1] + p.n[2] * a[2] - p.d;
		float db = p.n[0] * b[0] + p.n[1] * b[1] + p.n[2] * b[2] - p.d;
		if (da < 0 || db >= 0) continue;
		float t = da / (da - db);
		if (t < best) {
			best = t;
			for (int i = 0; i < 3; i++) h.point[i] = a[i] + (b[i] - a[i]) * t, h.normal[i] = p.n[i];
		}
	}
	return best <= 1;
}

static void testYawSign() {
	ProbeWorld pw(planeProbe);
	pw.wallRamps = false;
	ffi::World* w = api.cbworld_new(&pw, &ProbeWorld::cbRaycast, &ProbeWorld::cbBox, &ProbeWorld::cbSphere);
	ffi::Car* car = api.car_new(0);
	float pos[3] = {0, 0, 17};
	api.car_reset(car, pos, space::headingToRlYaw(0), 0, 0);
	api.car_step_cb(car, w, 60, 0, 0, 0, 0, 0, 0);
	api.car_step_cb(car, w, 120, 1.0f, 1.0f, 0, 0, 0, 0);
	float pose[ffi::POSE_FLOATS];
	api.car_pose(car, 1, pose);
	space::V3 f = space::dirToGta(pose + 3);
	CHECK(f.x > 0.3);
	CHECK(space::headingOf(f) < -15);
	space::V3 p = space::Frame{}.toGta(pose);
	CHECK(p.x > 0);
	api.car_free(car);
	api.cbworld_free(w);
}

static void testProbeWorldMatchesReference() {
	using WorldNew = void* (*)();
	using SetBoxes = uint32_t (*)(void*, const float*, uint32_t);
	using Step = void (*)(ffi::Car*, const void*, uint32_t, float, float, float, float, float, uint32_t);
	auto worldNew = reinterpret_cast<WorldNew>(rawSym("rlcar_world_new"));
	auto setBoxes = reinterpret_cast<SetBoxes>(rawSym("rlcar_world_set_boxes"));
	auto step = reinterpret_cast<Step>(rawSym("rlcar_car_step"));
	CHECK(worldNew && setBoxes && step);
	if (!worldNew) return;
	void* bw = worldNew();
	float boxes[] = {-5000, -5000, -1000, 600, 5000, 0, 600, -5000, -1000, 1600, 5000, 5000};
	setBoxes(bw, boxes, 2);

	ProbeWorld pw(planeProbe);
	pw.wallRamps = false;
	ffi::World* cw = api.cbworld_new(&pw, &ProbeWorld::cbRaycast, &ProbeWorld::cbBox, &ProbeWorld::cbSphere);
	ffi::Car *a = api.car_new(0), *b = api.car_new(0);
	float pos[3] = {-1500, 0, 50};
	for (auto* c : {a, b}) api.car_reset(c, pos, 0, 0, 0);
	float pa[ffi::POSE_FLOATS], pb[ffi::POSE_FLOATS];
	double maxErr = 0;
	float maxZ = 0;
	for (int t = 0; t < 600; t++) {
		uint32_t btn = t > 60 && t < 300 ? ffi::buttons::BOOST : 0;
		float thr = t > 60 ? 1.0f : 0.0f;
		step(a, bw, 1, thr, 0, 0, 0, 0, btn);
		api.car_step_cb(b, cw, 1, thr, 0, 0, 0, 0, btn);
		api.car_pose(a, 1, pa);
		api.car_pose(b, 1, pb);
		double dy = std::fabs(pa[1]) - std::fabs(pb[1]);
		double e = std::sqrt(std::pow(pa[0] - pb[0], 2) + dy * dy + std::pow(pa[2] - pb[2], 2));
		if (e > maxErr) maxErr = e;
		if (pb[0] > maxZ) maxZ = pb[0];
		if (t == 59) CHECK_NEAR(pb[2], 17.0, 0.1);
	}
	std::printf("  probe world vs BoxWorld: max position error %.3f uu, closest approach x %.1f uu, %llu probes\n",
	            maxErr, maxZ, static_cast<unsigned long long>(pw.probes));
	CHECK(maxZ > 500 && maxZ < 600);
	CHECK(maxErr < 0.5);
	api.car_free(a);
	api.car_free(b);
	api.cbworld_free(cw);
}

static const float kSlabs[][6] = {{-20000, -20000, -4, 20000, 20000, 0}, {1500, -20000, -4, 1504, 20000, 20000}};

static bool slabProbe(const float* a, const float* b, ProbeHit& h) {
	float best = 2;
	for (auto& s : kSlabs) {
		float t0 = 0, t1 = 1;
		int axis = -1;
		float sign = 0;
		bool ok = true;
		for (int i = 0; i < 3 && ok; i++) {
			float d = b[i] - a[i];
			if (std::fabs(d) < 1e-9f) {
				if (a[i] < s[i] || a[i] > s[i + 3]) ok = false;
				continue;
			}
			float ta = (s[i] - a[i]) / d, tb = (s[i + 3] - a[i]) / d;
			float sg = -1;
			if (ta > tb) std::swap(ta, tb), sg = 1;
			if (ta > t0) t0 = ta, axis = i, sign = sg;
			t1 = std::min(t1, tb);
			if (t0 > t1) ok = false;
		}
		if (!ok || axis < 0 || t0 >= best) continue;
		best = t0;
		for (int i = 0; i < 3; i++) h.point[i] = a[i] + (b[i] - a[i]) * t0, h.normal[i] = 0;
		h.normal[axis] = sign;
	}
	return best <= 1;
}

static void testNoTunnelling() {
	ProbeWorld pw(slabProbe);
	pw.wallRamps = false;
	ffi::World* w = api.cbworld_new(&pw, &ProbeWorld::cbRaycast, &ProbeWorld::cbBox, &ProbeWorld::cbSphere);
	ffi::Car* car = api.car_new(0);
	float pose[ffi::POSE_FLOATS];
	float worstZ = 1e9f, fall = 0;
	for (float roll : {0.0f, 1.5708f, 3.14159f}) {
		float pos[3] = {-5000, 0, 6000};
		api.car_reset(car, pos, 0, 0.3f, roll);
		for (int t = 0; t < 900; t++) {
			api.car_step_cb(car, w, 1, 0, 0, 0, 0, 0, 0);
			api.car_pose(car, 1, pose);
			worstZ = std::min(worstZ, pose[2]);
			fall = std::min(fall, pose[14]);
		}
	}
	float pos[3] = {-12000, 0, 30};
	api.car_reset(car, pos, 0, 0, 0);
	api.car_set_unlimited_boost(car, 1);
	float maxX = -1e9f, maxSpeed = 0;
	for (int t = 0; t < 1200; t++) {
		api.car_step_cb(car, w, 1, 1.0f, 0, 0, 0, 0, ffi::buttons::BOOST);
		api.car_pose(car, 1, pose);
		maxX = std::max(maxX, pose[0]);
		if (pose[0] < 1000) maxSpeed = std::max(maxSpeed, std::sqrt(pose[12] * pose[12] + pose[13] * pose[13]));
	}
	std::printf("  thin slabs: dropped at %.0f uu/s upright, sideways and upside down, lowest z %.1f; hit a 4 uu wall at %.0f uu/s, closest x %.1f\n",
	            -fall, worstZ, maxSpeed, maxX);
	CHECK(-fall > 2000);
	CHECK(worstZ > -20);
	CHECK(maxSpeed > 2200);
	CHECK(maxX < 1500);
	api.car_free(car);
	api.cbworld_free(w);
}

static void testModelLoadAndCull() {
	std::string path = "rlm_test_cube.rlm";
	{
		FILE* f = std::fopen(path.c_str(), "wb");
		const float v[] = {-1, -1, -1, 1, -1, -1, 1, 1, -1, -1, 1, -1, -1, -1, 1, 1, -1, 1, 1, 1, 1, -1, 1, 1};
		const uint32_t t[] = {0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 2, 3, 7, 2, 7, 6, 1, 2, 6, 1, 6, 5, 0, 4, 7, 0, 7, 3};
		uint32_t hdr[3] = {8, 12, 2};
		std::fwrite("RLM1", 1, 4, f);
		std::fwrite(hdr, 4, 3, f);
		std::fwrite(v, 4, 24, f);
		std::fwrite(t, 4, 36, f);
		std::vector<uint8_t> c(12 * 4 * 2, 200);
		for (int i = 0; i < 12; i++) c[size_t(i) * 4 + 3] = 0;
		std::fwrite(c.data(), 1, c.size(), f);
		std::fclose(f);
	}
	rlm::Mesh m;
	std::string err;
	CHECK(rlm::load(path, m, err));
	CHECK(m.triangles() == 12 && m.colours.size() == 2);
	double cam[3] = {10, 0, 0};
	rlm::Light light;
	std::vector<double> scratch;
	rlm::Xform x;
	int n = 0;
	rlm::draw(m, 1, x, cam, light, scratch, [&](const double*, const double*, const double*, const int*) { n++; });
	CHECK(n == 2);
	rlm::Xform mirrored;
	mirrored.a[1][1] = -1;
	int nm = 0;
	rlm::draw(m, 0, mirrored, cam, light, scratch, [&](const double* a, const double* b, const double* c, const int*) {
		if (a[0] > 0.99 && b[0] > 0.99 && c[0] > 0.99) nm++;
	});
	CHECK(nm == 2);
	double diag[3] = {10, 10, 10};
	int corner = 0;
	rlm::draw(m, 0, x, diag, light, scratch, [&](const double*, const double*, const double*, const int*) { corner++; });
	CHECK(corner == 6);
	std::remove(path.c_str());
	CHECK(!rlm::load(path, m, err));
	if (const char* dir = std::getenv("RLM_DIR")) {
		rlm::Models models;
		std::string log;
		bool ok = models.load(dir, "octane", log);
		std::printf("  models in %s: %s\n", dir, log.c_str());
		CHECK(ok && models.hasAnchors);
		CHECK(models.anchors[0][0] > 0 && models.anchors[0][1] < 0 && models.anchors[3][0] < 0 && models.anchors[3][1] > 0);
		float mn[3] = {1e9f, 1e9f, 1e9f}, mx[3] = {-1e9f, -1e9f, -1e9f};
		const rlm::Mesh& b = models.body[0];
		for (size_t i = 0; i < b.pos.size(); i++) mn[i % 3] = std::min(mn[i % 3], b.pos[i]), mx[i % 3] = std::max(mx[i % 3], b.pos[i]);
		std::printf("  octane body %u tris, extent x %.1f..%.1f y %.1f..%.1f z %.1f..%.1f uu\n", b.triangles(), mn[0], mx[0], mn[1], mx[1], mn[2], mx[2]);
		CHECK(mx[0] - mn[0] > 110 && mx[0] - mn[0] < 150);
	}
}

static void testWallRampClimb() {
	ProbeWorld pw(planeProbe);
	ffi::World* w = api.cbworld_new(&pw, &ProbeWorld::cbRaycast, &ProbeWorld::cbBox, &ProbeWorld::cbSphere);
	ffi::Car* car = api.car_new(0);
	float pos[3] = {-1500, 0, 50};
	api.car_reset(car, pos, 0, 0, 0);
	api.car_step_cb(car, w, 60, 0, 0, 0, 0, 0, 0);
	float pose[ffi::POSE_FLOATS], peak = 0, maxX = -1e9f, minUpX = 1;
	for (int t = 0; t < 360; t++) {
		api.car_step_cb(car, w, 1, 1.0f, 0, 0, 0, 0, ffi::buttons::BOOST);
		uint32_t f = api.car_pose(car, 1, pose);
		peak = std::max(peak, pose[2]);
		maxX = std::max(maxX, pose[0]);
		if (f & ffi::flags::ON_GROUND) minUpX = std::min(minUpX, pose[9]);
	}
	std::printf("  wall ramp: peak height %.0f uu, closest x %.1f, up.x on the wall %.2f, %zu ramps\n", peak, maxX, minUpX,
	            pw.ramps().size());
	CHECK(peak > 1000);
	CHECK(maxX < 600);
	CHECK(minUpX < -0.9f);
	api.car_free(car);
	api.cbworld_free(w);
}

static void testIniAndSettings() {
	Ini ini;
	ini.loadText("; c\n[Car]\nMaxSpeed = 1800  ; faster cap\nBoostAccelGround = 991.6667\nUnlimitedBoost = off\n"
	             "[camera]\nPreset=Custom\nfov=105\n[Gamepad]\nJump = CROSS\nBoost=\n");
	CHECK(ini.has("car", "maxspeed"));
	CHECK_NEAR(ini.num("Car", "MaxSpeed", 0), 1800, 0);
	Settings s;
	s.load(ini, api);
	CHECK_NEAR(s.sim[14], 1800, 0);
	float def[ffi::SIM_CONFIG_FLOATS];
	api.default_config(def);
	CHECK(s.sim[1] == def[1]);
	CHECK(s.sim[10] == 0);
	CHECK_NEAR(s.camera[0], 105, 0);
	CHECK(Settings::presetIndex("Psyclops") == 6);

	InputState in;
	in.pad.connected = true;
	in.pad.buttons = pad::A;
	CHECK(s.bindings.held(Action::Jump, in));
	in.pad.buttons = pad::B;
	CHECK(!s.bindings.held(Action::Boost, in));

	Ini shipped;
	if (shipped.loadFile("gta/RLCar.ini") || shipped.loadFile("../RLCar.ini") || shipped.loadFile("RLCar.ini")) {
		Settings d;
		d.load(shipped, api);
		for (int i = 0; i < ffi::SIM_CONFIG_FLOATS; i++)
			if (i != 10) CHECK(d.sim[i] == def[i]);
		CHECK(d.sim[10] == 1);
		CHECK(d.worldScale == 0);
	} else {
		std::printf("  (RLCar.ini not found from the working directory; shipped-defaults check skipped)\n");
	}
}

static void testBindings() {
	Bindings b;
	InputState s;
	s.keys[size_t(Bindings::keyCode("W"))] = true;
	DriveInput d = b.drive(s);
	CHECK_NEAR(d.throttle, 1, 0);
	CHECK_NEAR(d.pitch, -1, 0);
	s = {};
	s.pad.connected = true;
	s.pad.rt = 0.4f;
	s.pad.lx = 1.0f;
	s.pad.ly = 1.0f;
	d = b.drive(s);
	CHECK_NEAR(d.throttle, 0.4, 1e-6);
	CHECK_NEAR(d.steer, 1, 1e-6);
	CHECK_NEAR(d.yaw, 1, 1e-6);
	CHECK_NEAR(d.pitch, -1, 1e-6);
	s.pad.buttons = pad::X;
	d = b.drive(s);
	CHECK_NEAR(d.yaw, 0, 0);
	CHECK_NEAR(d.roll, 1, 1e-6);
	CHECK(d.handbrake);
	s = {};
	s.pad.connected = true;
	s.pad.buttons = pad::LB;
	CHECK_NEAR(b.drive(s).roll, -1, 0);
	s.pad.buttons = pad::LS;
	CHECK(!b.held(Action::BecomeCar, s));
	s.pad.buttons = pad::LS | pad::RS;
	CHECK(b.held(Action::BecomeCar, s));
	CHECK(Bindings::padBits("square") == pad::X && Bindings::padBits("LS+RS") == (pad::LS | pad::RS));
	CHECK(Bindings::keyCode("F10") == 0x79 && Bindings::keyCode("nope") == -1);
}

static void testGeom() {
	geom::Box a{{0, 0, 0}, {{1, 0, 0}, {0, 1, 0}, {0, 0, 1}}, {60, 40, 18}};
	geom::Box b = a;
	b.c[0] = 130;
	b.h[0] = 80;
	CHECK(geom::overlap(a, b));
	b.c[0] = 141;
	CHECK(!geom::overlap(a, b));
	float s = std::sqrt(0.5f);
	geom::Box r{{0, 105, 0}, {{s, s, 0}, {-s, s, 0}, {0, 0, 1}}, {50, 50, 50}};
	CHECK(geom::overlap(a, r));
	r.c[1] = 112 + 40;
	CHECK(!geom::overlap(a, r));
	float p[3] = {500, 10, -100}, local[3];
	geom::closestLocal(a, p, local);
	CHECK_NEAR(local[0], 60, 1e-4);
	CHECK_NEAR(local[1], 10, 1e-4);
	CHECK_NEAR(local[2], -18, 1e-4);
}

static void testBallAndBump() {
	ffi::Ball* ball = api.ball_new();
	float cfg[ffi::BALL_CONFIG_FLOATS];
	api.default_ball_config(cfg);
	CHECK(cfg[0] == 91.25f && cfg[1] == 30.0f);
	float pos[3] = {0, 0, 500}, vel[3] = {0, 0, -1}, pose[ffi::BALL_POSE_FLOATS];
	api.ball_reset(ball, pos, vel, nullptr);
	ffi::Car* car = api.car_new(0);
	float cpos[3] = {-3000, 0, 17};
	api.car_reset(car, cpos, 0, 0, 0);
	for (int i = 0; i < 120; i++) api.scene_advance_cb(car, ball, nullptr, 1.0 / 120.0, 0, 0, 0, 0, 0, 0);
	api.ball_pose(ball, 1.0f, pose);
	CHECK(pose[2] < 200 && pose[5] < -600);
	float vpos[3] = {-2850, 0, 17}, vvel[3] = {0, 0, 0}, up[3] = {0, 0, 1}, dv[3] = {};
	CHECK(api.car_bump(car, vpos, vvel, 1, up, 70, 1, 1, dv) == 0);
	api.ball_free(ball);
	api.car_free(car);
}

int main(int argc, char** argv) {
	g_lib = argc > 1 ? argv[1] : "rl_car_ffi.dll";
	std::string err;
	if (!ffi::load(g_lib, api, err)) {
		std::printf("cannot load %s: %s\n", g_lib.c_str(), err.c_str());
		return 2;
	}
	testSpace();
	testYawSign();
	testIniAndSettings();
	testBindings();
	testProbeWorldMatchesReference();
	testWallRampClimb();
	testNoTunnelling();
	testModelLoadAndCull();
	testGeom();
	testBallAndBump();
	std::printf("%d checks, %d failures\n", g_checks, g_failures);
	return g_failures ? 1 : 0;
}
