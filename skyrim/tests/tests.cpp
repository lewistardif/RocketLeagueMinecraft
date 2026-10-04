#include "../src/bindings.h"
#include "../src/ini.h"
#include "../src/rlcar_ffi.h"
#include "../src/settings.h"
#include "../src/sky_world.h"
#include "../src/space.h"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>
#include <windows.h>

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

static void* rawSym(const char* name) { return reinterpret_cast<void*>(GetProcAddress(GetModuleHandleA("rl_car_ffi.dll"), name)); }

static void quad(sky::Cache& c, float x0, float y0, float x1, float y1, float z) {
	float a[3] = {x0, y0, z}, b[3] = {x1, y0, z}, cc[3] = {x1, y1, z}, d[3] = {x0, y1, z};
	c.addTri(a, b, cc, false);
	c.addTri(a, cc, d, false);
}

static void finish(sky::Cache& c, float x0, float y0, float z0, float x1, float y1, float z1) {
	float lo[3] = {x0, y0, z0}, hi[3] = {x1, y1, z1};
	c.setRegion(lo, hi);
	c.build();
}

static const float kAxes[3][3] = {{1, 0, 0}, {0, 1, 0}, {0, 0, 1}};

static void testSpace() {
	space::Frame f;
	f.origin = {1000, -2000, 300};
	f.scale = 2.0;
	f.unitsPerMetre = 70;
	float rl[3] = {400, 100, -40}, back[3];
	space::V3 s = f.toSky(rl);
	CHECK_NEAR(s.x, 1560, 1e-9);
	CHECK_NEAR(s.y, -2140, 1e-9);
	CHECK_NEAR(s.z, 244, 1e-9);
	f.toRl(s, back);
	for (int i = 0; i < 3; i++) CHECK_NEAR(back[i], rl[i], 1e-3);
	CHECK_NEAR(f.lenToSky(100), 140, 1e-9);

	for (double h : {0.0, 0.7, 1.5708, 3.0, -2.2}) {
		float y = space::headingToRlYaw(h);
		float fwd[3] = {std::cos(y), std::sin(y), 0};
		space::V3 sf = space::dirToSky(fwd);
		CHECK_NEAR(sf.x, std::sin(h), 1e-5);
		CHECK_NEAR(sf.y, std::cos(h), 1e-5);
		double diff = std::remainder(space::headingOf(sf) - h, 2 * space::kPi);
		CHECK_NEAR(diff, 0, 1e-5);
	}

	// Facing east (RL +X): the model's +Y goes east, +X (right) south, +Z up, a proper rotation.
	float cols[9] = {1, 0, 0, 0, 1, 0, 0, 0, 1};
	double m[3][3];
	space::carRotToSky(cols, m);
	CHECK_NEAR(m[0][1], 1, 1e-9);
	CHECK_NEAR(m[1][0], -1, 1e-9);
	CHECK_NEAR(m[2][2], 1, 1e-9);
	double det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) +
	             m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
	CHECK_NEAR(det, 1, 1e-9);

	// Steering right in Rocket League (yaw +) turns clockwise seen from above in Skyrim (heading +).
	float yaw = 0.3f;
	float turned[3] = {std::cos(yaw), std::sin(yaw), 0};
	CHECK(space::headingOf(space::dirToSky(turned)) > space::headingOf(space::dirToSky(cols)));

	float cam[9] = {0.8f, 0, -0.6f, 0, 1, 0, 0.6f, 0, 0.8f};
	space::CameraBasis b = space::cameraToSky(cam);
	CHECK(b.pitch > 0.6 && b.pitch < 0.7);
	CHECK_NEAR(space::verticalFovToSkyrim(2 * std::atan(0.75) * 180 / space::kPi), 90, 1e-6);

	space::Quat q = space::spin({}, space::spinToSky(cols + 6), 0.5);
	double r[3][3];
	space::quatToMatrix(q, r);
	CHECK_NEAR(r[0][0], std::cos(0.5), 1e-5);
	CHECK_NEAR(r[1][0], -std::sin(0.5), 1e-5);
}

static void testIniAndSettings() {
	FormRef f;
	CHECK(FormRef::parse("Skyrim.esm|0x01C0C0", f) && f.plugin == "Skyrim.esm" && f.id == 0x1C0C0);
	CHECK(FormRef::parse(" Dawnguard.esm | 00ABCD ", f) && f.plugin == "Dawnguard.esm" && f.id == 0xABCD);
	CHECK(FormRef::parse("0C8868", f) && f.plugin == "Skyrim.esm" && f.id == 0xC8868);
	CHECK(!FormRef::parse("Skyrim.esm|zz", f));
	CHECK(!FormRef::parse("Skyrim.esm|0x1000000", f));

	Ini ini;
	ini.loadText("[General]\nPreset = Dominus\nCarForm = Update.esm|0x123\n[Scale]\nWorldScale = 1.5\n[Car]\nMaxSpeed = 1800\n"
	             "UnlimitedBoost = off\n[camera]\nPreset=Custom\nfov=105\n[World]\nWallRamps = 0\nCacheRadius = 99999\n");
	Settings s;
	s.load(ini, api);
	CHECK(s.preset == 1);
	CHECK(s.carForm.plugin == "Update.esm" && s.carForm.id == 0x123);
	CHECK_NEAR(s.worldScale, 1.5, 0);
	CHECK_NEAR(s.sim[14], 1800, 0);
	CHECK(s.sim[10] == 0);
	CHECK_NEAR(s.camera[0], 105, 0);
	CHECK(!s.wallRamps);
	CHECK_NEAR(s.cacheRadius, 10000, 0);
	Ini empty;
	empty.loadText("");
	s.load(empty, api);
	CHECK(s.worldScale == 0 && s.carForm.id == 0x1C0C0 && s.sim[10] == 1 && s.wallRamps);
}

static void testBindings() {
	Bindings b;
	InputState in;
	in.keys['W'] = true;
	in.keys[0xA0] = true;
	DriveInput d = b.drive(in);
	CHECK_NEAR(d.throttle, 1, 0);
	CHECK(d.boost && !d.jump);
	CHECK(Bindings::keyCode("F7") == 0x76);
	InputState t;
	t.keys[0x76] = true;
	CHECK(b.held(Action::BecomeCar, t));
	CHECK(!b.held(Action::ExitCar, t));
	InputState p;
	p.pad.connected = true;
	p.pad.buttons = pad::LS | pad::RS;
	CHECK(b.held(Action::BecomeCar, p));
	p.pad.buttons = pad::BACK;
	CHECK(b.held(Action::ExitCar, p));
	Ini ini;
	ini.loadText("[Controls]\nBecomeCar = K\n[Gamepad]\nBecomeCar = START\n");
	b.load(ini);
	CHECK(!b.held(Action::BecomeCar, t));
	InputState k;
	k.keys['K'] = true;
	CHECK(b.held(Action::BecomeCar, k));
}

static void testContacts() {
	sky::Cache c;
	quad(c, -1000, -1000, 0, 1000, 0);
	quad(c, 0, -1000, 1000, 1000, 0);
	float w0[3] = {300, -1000, 0}, w1[3] = {300, 1000, 0}, w2[3] = {300, 1000, 500}, w3[3] = {300, -1000, 500};
	c.addTri(w0, w1, w2, false);
	c.addTri(w0, w2, w3, false);
	float cap0[3] = {-400, 0, 0}, cap1[3] = {-400, 0, 400};
	c.addCapsule(cap0, cap1, 20);
	finish(c, -1000, -1000, -100, 1000, 1000, 600);
	CHECK(c.tris.size() == 6);

	sky::SkyWorld w;
	w.wallRamps = false;
	w.car = c;
	w.ball = c;
	ffi::Contact out[16];

	// A box straddling the seam of two coplanar triangles, 2 uu into the floor: one contact.
	ffi::Obb obb{};
	obb.center[0] = 0, obb.center[2] = 16;
	std::memcpy(obb.axes, kAxes, sizeof kAxes);
	obb.half_extents[0] = 59, obb.half_extents[1] = 42, obb.half_extents[2] = 18;
	uint32_t n = w.boxContacts(obb, 1.0f, out, 16);
	CHECK(n == 1);
	if (n == 1) {
		CHECK_NEAR(out[0].normal[2], 1, 1e-6);
		CHECK_NEAR(out[0].depth, 2, 1e-4);
		CHECK_NEAR(out[0].point[2], 0, 1e-4);
	}
	// Against the wall too: the floor plus the wall, normal -x.
	obb.center[0] = 245;
	n = w.boxContacts(obb, 1.0f, out, 16);
	CHECK(n == 2);
	bool wall = false;
	for (uint32_t i = 0; i < n; i++) {
		if (out[i].normal[0] >= -0.99f) continue;
		wall = true;
		CHECK_NEAR(out[i].depth, 4, 1e-3);
	}
	CHECK(wall);
	// Raised above the margin: nothing.
	obb.center[0] = -100, obb.center[2] = 20;
	CHECK(w.boxContacts(obb, 1.0f, out, 16) == 0);
	// The capsule (a post) beside the box.
	obb.center[0] = -400 + 59 + 15, obb.center[2] = 100;
	n = w.boxContacts(obb, 1.0f, out, 16);
	CHECK(n == 1);
	if (n == 1) {
		CHECK(out[0].normal[0] > 0.99f);
		CHECK_NEAR(out[0].depth, 5, 0.5);
	}

	// A tilted box: its deepest corner, not its centre.
	obb.center[0] = -600, obb.center[2] = 40;
	float a = 0.5f, ca = std::cos(a), sa = std::sin(a);
	float tilted[9] = {ca, 0, sa, 0, 1, 0, -sa, 0, ca};
	std::memcpy(obb.axes, tilted, sizeof tilted);
	n = w.boxContacts(obb, 1.0f, out, 16);
	float lowest = 40 - (59 * sa + 18 * ca);
	CHECK(n == 1);
	if (n == 1) CHECK_NEAR(out[0].depth, -lowest, 1e-3);

	// A sphere on the seam of the two floor triangles, and in the corner with the wall.
	float sc[3] = {0, 0, 90};
	n = w.sphereContacts(sc, 91.25f, 1.0f, out, 16);
	CHECK(n == 1);
	if (n == 1) CHECK_NEAR(out[0].depth, 1.25, 1e-4);
	float sc2[3] = {210, 300, 90};
	n = w.sphereContacts(sc2, 91.25f, 1.0f, out, 16);
	CHECK(n == 2);

	// Rays: the floor from above, the wall from both sides, the capsule.
	float o[3] = {-200, 0, 100}, down[3] = {0, 0, -1}, t, nrm[3];
	CHECK(sky::SkyWorld::rayCache(c, o, down, 500, t, nrm));
	CHECK_NEAR(t, 100, 1e-4);
	CHECK_NEAR(nrm[2], 1, 1e-6);
	float east[3] = {1, 0, 0}, west[3] = {-1, 0, 0}, o2[3] = {420, 0, 100};
	CHECK(sky::SkyWorld::rayCache(c, o, east, 1000, t, nrm) && std::fabs(t - 500) < 1e-3 && nrm[0] < -0.99f);
	CHECK(sky::SkyWorld::rayCache(c, o2, west, 150, t, nrm) && std::fabs(t - 120) < 1e-3 && nrm[0] > 0.99f);
	float o3[3] = {-700, 0, 200};
	CHECK(sky::SkyWorld::rayCache(c, o3, east, 1000, t, nrm) && std::fabs(t - 280) < 1e-3 && nrm[0] < -0.99f);
	CHECK(!sky::SkyWorld::rayCache(c, o, down, 50, t, nrm));

	// One-sided (a box's faces): seen from inside, nothing.
	sky::Cache solid;
	float bc[3] = {0, 0, 0}, bh[3] = {100, 100, 100};
	solid.addBox(bc, kAxes, bh);
	finish(solid, -200, -200, -200, 200, 200, 200);
	float inside[3] = {0, 0, 0}, outside[3] = {0, 0, 300};
	CHECK(!sky::SkyWorld::rayCache(solid, inside, down, 500, t, nrm));
	CHECK(sky::SkyWorld::rayCache(solid, outside, down, 500, t, nrm) && std::fabs(t - 200) < 1e-3 && nrm[2] > 0.99f);
}

static void testFloorMatchesReference() {
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

	sky::SkyWorld w;
	w.wallRamps = false;
	quad(w.car, -5000, -5000, 600, 5000, 0);
	float wc[3] = {1100, 0, 2000}, wh[3] = {500, 5000, 3000};
	w.car.addBox(wc, kAxes, wh);
	finish(w.car, -5000, -5000, -1000, 1600, 5000, 5000);
	ffi::World* cw = api.cbworld_new(&w, &sky::SkyWorld::cbRaycast, &sky::SkyWorld::cbBox, &sky::SkyWorld::cbSphere);
	ffi::Car *a = api.car_new(0), *b = api.car_new(0);
	float pos[3] = {-1500, 0, 50};
	for (auto* c : {a, b}) api.car_reset(c, pos, 0, 0, 0);
	float pa[ffi::POSE_FLOATS], pb[ffi::POSE_FLOATS];
	double maxErr = 0;
	float maxX = -1e9f;
	for (int t = 0; t < 600; t++) {
		uint32_t btn = t > 60 && t < 300 ? ffi::buttons::BOOST : 0;
		float thr = t > 60 ? 1.0f : 0.0f;
		step(a, bw, 1, thr, 0, 0, 0, 0, btn);
		api.car_step_cb(b, cw, 1, thr, 0, 0, 0, 0, btn);
		api.car_pose(a, 1, pa);
		api.car_pose(b, 1, pb);
		double dy = std::fabs(pa[1]) - std::fabs(pb[1]);
		double e = std::sqrt(std::pow(pa[0] - pb[0], 2) + dy * dy + std::pow(pa[2] - pb[2], 2));
		maxErr = std::max(maxErr, e);
		maxX = std::max(maxX, pb[0]);
		if (t == 59) CHECK_NEAR(pb[2], 17.0, 0.1);
	}
	std::printf("  triangle floor and box wall vs BoxWorld: max position error %.3f uu, closest approach x %.1f uu, %llu queries\n", maxErr,
	            maxX, static_cast<unsigned long long>(w.queries));
	CHECK(maxX > 500 && maxX < 600);
	CHECK(maxErr < 0.5);
	api.car_free(a);
	api.car_free(b);
	api.cbworld_free(cw);
}

// A heightfield-like floor (128 uu squares, alternating diagonals) is as smooth as a flat one.
static void testTerrainSeams() {
	sky::SkyWorld w;
	w.wallRamps = false;
	for (float x = -6000; x < 6000; x += 128)
		for (float y = -1024; y < 1024; y += 128) {
			float a[3] = {x, y, 0}, b[3] = {x + 128, y, 0}, c[3] = {x + 128, y + 128, 0}, d[3] = {x, y + 128, 0};
			bool flip = int((x + y) / 128) & 1;
			w.car.addTri(a, b, flip ? d : c, true);
			w.car.addTri(flip ? b : a, c, d, true);
		}
	finish(w.car, -6000, -1024, -100, 6000, 1024, 100);
	ffi::World* cw = api.cbworld_new(&w, &sky::SkyWorld::cbRaycast, &sky::SkyWorld::cbBox, &sky::SkyWorld::cbSphere);
	ffi::Car* car = api.car_new(0);
	float pos[3] = {-5000, 64, 30};
	api.car_reset(car, pos, 0.05f, 0, 0);
	api.car_set_unlimited_boost(car, 1);
	float pose[ffi::POSE_FLOATS], minZ = 1e9f, maxZ = -1e9f, maxVz = 0, maxVy = 0, maxSpeed = 0;
	for (int t = 0; t < 600; t++) {
		uint32_t f;
		api.car_step_cb(car, cw, 1, 1.0f, 0, 0, 0, 0, t > 30 ? ffi::buttons::BOOST : 0);
		f = api.car_pose(car, 1, pose);
		if (t < 60) continue;
		CHECK(f & ffi::flags::ON_GROUND);
		minZ = std::min(minZ, pose[2]), maxZ = std::max(maxZ, pose[2]);
		maxVz = std::max(maxVz, std::fabs(pose[14]));
		maxSpeed = std::max(maxSpeed, std::sqrt(pose[12] * pose[12] + pose[13] * pose[13]));
	}
	(void)maxVy;
	std::printf("  128 uu terrain grid at up to %.0f uu/s: height %.2f..%.2f uu, |vz| max %.2f uu/s\n", maxSpeed, minZ, maxZ, maxVz);
	CHECK(maxSpeed > 2200);
	CHECK(maxZ - minZ < 0.5f);
	CHECK(maxVz < 5);
	api.car_free(car);
	api.cbworld_free(cw);
}

static void testWallRampClimb() {
	sky::SkyWorld w;
	quad(w.car, -5000, -5000, 600, 5000, 0);
	float a[3] = {600, -5000, 0}, b[3] = {600, 5000, 0}, c[3] = {600, 5000, 5000}, d[3] = {600, -5000, 5000};
	w.car.addTri(a, c, b, true);
	w.car.addTri(a, d, c, true);
	finish(w.car, -5000, -5000, -100, 1600, 5000, 5000);
	ffi::World* cw = api.cbworld_new(&w, &sky::SkyWorld::cbRaycast, &sky::SkyWorld::cbBox, &sky::SkyWorld::cbSphere);
	ffi::Car* car = api.car_new(0);
	float pos[3] = {-1500, 0, 50};
	api.car_reset(car, pos, 0, 0, 0);
	api.car_step_cb(car, cw, 60, 0, 0, 0, 0, 0, 0);
	float pose[ffi::POSE_FLOATS], peak = 0, maxX = -1e9f, minUpX = 1;
	for (int t = 0; t < 360; t++) {
		api.car_step_cb(car, cw, 1, 1.0f, 0, 0, 0, 0, ffi::buttons::BOOST);
		uint32_t f = api.car_pose(car, 1, pose);
		peak = std::max(peak, pose[2]);
		maxX = std::max(maxX, pose[0]);
		if (f & ffi::flags::ON_GROUND) minUpX = std::min(minUpX, pose[9]);
	}
	std::printf("  wall ramp: peak height %.0f uu, closest x %.1f, up.x on the wall %.2f, %zu ramps\n", peak, maxX, minUpX, w.ramps().size());
	CHECK(peak > 1000);
	CHECK(maxX < 600);
	CHECK(minUpX < -0.9f);
	api.car_free(car);
	api.cbworld_free(cw);
}

static void testNoTunnelling() {
	sky::SkyWorld w;
	w.wallRamps = false;
	float fc[3] = {0, 0, -2}, fh[3] = {20000, 20000, 2}, wc[3] = {1502, 0, 10000}, wh[3] = {2, 20000, 10000};
	w.car.addBox(fc, kAxes, fh);
	w.car.addBox(wc, kAxes, wh);
	finish(w.car, -20000, -20000, -100, 20000, 20000, 20000);
	ffi::World* cw = api.cbworld_new(&w, &sky::SkyWorld::cbRaycast, &sky::SkyWorld::cbBox, &sky::SkyWorld::cbSphere);
	ffi::Car* car = api.car_new(0);
	float pose[ffi::POSE_FLOATS];
	float worstZ = 1e9f, fall = 0;
	for (float roll : {0.0f, 1.5708f, 3.14159f}) {
		float pos[3] = {-5000, 0, 6000};
		api.car_reset(car, pos, 0, 0.3f, roll);
		w.forget();
		for (int t = 0; t < 900; t++) {
			api.car_step_cb(car, cw, 1, 0, 0, 0, 0, 0, 0);
			api.car_pose(car, 1, pose);
			worstZ = std::min(worstZ, pose[2]);
			fall = std::min(fall, pose[14]);
		}
	}
	float pos[3] = {-12000, 0, 30};
	api.car_reset(car, pos, 0, 0, 0);
	w.forget();
	api.car_set_unlimited_boost(car, 1);
	float maxX = -1e9f, maxSpeed = 0;
	for (int t = 0; t < 1200; t++) {
		api.car_step_cb(car, cw, 1, 1.0f, 0, 0, 0, 0, ffi::buttons::BOOST);
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
	api.cbworld_free(cw);
}

// A wedge given as a convex hull's planes (n.p + d <= 0 inside), like Havok's convex vertices shape.
static void testConvexRamp() {
	sky::SkyWorld w;
	w.wallRamps = false;
	quad(w.car, -5000, -5000, 5000, 5000, 0);
	float s = 1.0f / std::sqrt(1.0f + 0.09f);
	std::vector<std::array<float, 4>> planes = {
		{0, 0, -1, 0}, {-0.3f * s, 0, s, 0}, {1, 0, 0, -1000}, {0, 1, 0, -500}, {0, -1, 0, -500}};
	float lo[3] = {0, -500, 0}, hi[3] = {1000, 500, 300};
	w.car.addConvex(planes, lo, hi);
	finish(w.car, -5000, -5000, -100, 5000, 5000, 3000);
	CHECK(w.car.tris.size() >= 2 + 8);
	float o[3] = {500, 0, 1000}, down[3] = {0, 0, -1}, t, n[3];
	CHECK(sky::SkyWorld::rayCache(w.car, o, down, 2000, t, n) && std::fabs(t - 850) < 1e-2 && n[2] > 0.9f && n[0] < -0.2f);

	ffi::World* cw = api.cbworld_new(&w, &sky::SkyWorld::cbRaycast, &sky::SkyWorld::cbBox, &sky::SkyWorld::cbSphere);
	ffi::Car* car = api.car_new(0);
	float pos[3] = {-2500, 0, 20};
	api.car_reset(car, pos, 0, 0, 0);
	api.car_set_unlimited_boost(car, 1);
	float pose[ffi::POSE_FLOATS], peak = 0, worst = 1e9f;
	for (int t2 = 0; t2 < 360; t2++) {
		api.car_step_cb(car, cw, 1, 1.0f, 0, 0, 0, 0, ffi::buttons::BOOST);
		api.car_pose(car, 1, pose);
		peak = std::max(peak, pose[2]);
		if (pose[0] > 50 && pose[0] < 950 && std::fabs(pose[1]) < 400) worst = std::min(worst, pose[2] - 0.3f * pose[0]);
	}
	std::printf("  convex wedge ramp: peak height %.0f uu, lowest %.1f uu above the slope\n", peak, worst);
	CHECK(peak > 350);
	CHECK(worst > 0);
	api.car_free(car);
	api.cbworld_free(cw);
}

static void testBallOnTriangles() {
	sky::SkyWorld w;
	w.wallRamps = false;
	quad(w.car, -4000, -4000, 0, 4000, 0);
	quad(w.car, 0, -4000, 4000, 4000, 0);
	float cap0[3] = {1500, 0, 0}, cap1[3] = {1500, 0, 1000};
	w.car.addCapsule(cap0, cap1, 40);
	finish(w.car, -4000, -4000, -100, 4000, 4000, 2000);
	w.sphereCache = &w.car;
	ffi::World* cw = api.cbworld_new(&w, &sky::SkyWorld::cbRaycast, &sky::SkyWorld::cbBox, &sky::SkyWorld::cbSphere);
	ffi::Car* car = api.car_new(0);
	ffi::Ball* ball = api.ball_new();
	float cpos[3] = {-3000, -3000, 20};
	api.car_reset(car, cpos, 0, 0, 0);
	float bpos[3] = {0, 0, 600}, bvel[3] = {0, 0, -1};
	api.ball_reset(ball, bpos, bvel, nullptr);
	float bp[ffi::BALL_POSE_FLOATS], minZ = 1e9f;
	for (int t = 0; t < 600; t++) {
		api.scene_advance_cb(car, ball, cw, 1.0 / 120.0, 0, 0, 0, 0, 0, 0);
		api.ball_pose(ball, 1, bp);
		minZ = std::min(minZ, bp[2]);
	}
	std::printf("  ball dropped on the seam: lowest %.2f uu, rests at %.2f uu\n", minZ, bp[2]);
	CHECK(minZ > 85);
	CHECK_NEAR(bp[2], 93.15, 0.5);
	// Rolled into a post: it rides up it on its spin, never into it.
	float bpos2[3] = {1000, 30, 92}, bvel2[3] = {1500, 0, 0};
	api.ball_reset(ball, bpos2, bvel2, nullptr);
	float closest = 1e9f;
	for (int t = 0; t < 240; t++) {
		api.scene_advance_cb(car, ball, cw, 1.0 / 120.0, 0, 0, 0, 0, 0, 0);
		api.ball_pose(ball, 1, bp);
		if (bp[2] < 1000) closest = std::min(closest, std::hypot(bp[0] - 1500, bp[1]));
	}
	std::printf("  ball rolled into a post: closest centre %.1f uu from its axis (touching at %.2f)\n", closest, 40 + 91.25);
	CHECK(closest > 40 + 91.25 - 1500.0f / 120.0f);
	api.ball_free(ball);
	api.car_free(car);
	api.cbworld_free(cw);
}

// The ball against a wall of triangles bounces exactly as against the same wall made of boxes.
static void testBallMatchesReference() {
	using WorldNew = void* (*)();
	using SetBoxes = uint32_t (*)(void*, const float*, uint32_t);
	using SceneStep = void (*)(ffi::Car*, ffi::Ball*, const void*, uint32_t, float, float, float, float, float, uint32_t);
	auto worldNew = reinterpret_cast<WorldNew>(rawSym("rlcar_world_new"));
	auto setBoxes = reinterpret_cast<SetBoxes>(rawSym("rlcar_world_set_boxes"));
	auto sceneStep = reinterpret_cast<SceneStep>(rawSym("rlcar_scene_step"));
	auto sceneStepCb = reinterpret_cast<SceneStep>(rawSym("rlcar_scene_step_cb"));
	CHECK(worldNew && setBoxes && sceneStep && sceneStepCb);
	if (!worldNew || !sceneStepCb) return;
	void* bw = worldNew();
	float boxes[] = {-5000, -5000, -1000, 1500, 5000, 0, 1500, -5000, -1000, 2500, 5000, 5000};
	setBoxes(bw, boxes, 2);
	sky::SkyWorld w;
	w.wallRamps = false;
	quad(w.car, -5000, -5000, 1500, 5000, 0);
	float a[3] = {1500, -5000, -1000}, b[3] = {1500, 5000, -1000}, c[3] = {1500, 5000, 5000}, d[3] = {1500, -5000, 5000};
	w.car.addTri(a, c, b, false);
	w.car.addTri(a, d, c, false);
	finish(w.car, -5000, -5000, -1000, 2500, 5000, 5000);
	w.sphereCache = &w.car;
	ffi::World* cw = api.cbworld_new(&w, &sky::SkyWorld::cbRaycast, &sky::SkyWorld::cbBox, &sky::SkyWorld::cbSphere);
	ffi::Car *ca = api.car_new(0), *cb = api.car_new(0);
	ffi::Ball *ba = api.ball_new(), *bb = api.ball_new();
	float cpos[3] = {-3000, -3000, 17}, bpos[3] = {0, 30, 400}, bvel[3] = {1600, 200, 300};
	for (auto* c2 : {ca, cb}) api.car_reset(c2, cpos, 0, 0, 0);
	for (auto* b2 : {ba, bb}) api.ball_reset(b2, bpos, bvel, nullptr);
	float pa[ffi::BALL_POSE_FLOATS], pb[ffi::BALL_POSE_FLOATS];
	double maxErr = 0;
	float maxX = -1e9f;
	for (int t = 0; t < 360; t++) {
		sceneStep(ca, ba, bw, 1, 0, 0, 0, 0, 0, 0);
		sceneStepCb(cb, bb, cw, 1, 0, 0, 0, 0, 0, 0);
		api.ball_pose(ba, 1, pa);
		api.ball_pose(bb, 1, pb);
		maxErr = std::max(maxErr, std::sqrt(std::pow(pa[0] - pb[0], 2) + std::pow(pa[1] - pb[1], 2) + std::pow(pa[2] - pb[2], 2)));
		maxX = std::max(maxX, pb[0]);
	}
	std::printf("  ball off a triangle wall vs BoxWorld: max position error %.3f uu, closest x %.1f\n", maxErr, maxX);
	CHECK(maxX > 1350 && maxX < 1500);
	CHECK(maxErr < 0.5);
	for (auto* b2 : {ba, bb}) api.ball_free(b2);
	for (auto* c2 : {ca, cb}) api.car_free(c2);
	api.cbworld_free(cw);
}

int main(int argc, char** argv) {
	std::string lib = argc > 1 ? argv[1] : "rl_car_ffi.dll";
	std::string err;
	if (!ffi::load(lib, api, err)) {
		std::printf("cannot load %s: %s\n", lib.c_str(), err.c_str());
		return 2;
	}
	testSpace();
	testIniAndSettings();
	testBindings();
	testContacts();
	testFloorMatchesReference();
	testTerrainSeams();
	testWallRampClimb();
	testNoTunnelling();
	testConvexRamp();
	testBallOnTriangles();
	testBallMatchesReference();
	std::printf("%d checks, %d failures\n", g_checks, g_failures);
	return g_failures ? 1 : 0;
}
