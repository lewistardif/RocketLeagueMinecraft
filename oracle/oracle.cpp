// RocketSim oracle: runs a scenario file through RocketSim and writes the car trajectory as CSV.
//
// This program is the *reference* side of the validation harness. It links against an
// unmodified RocketSim checkout (MIT, https://github.com/ZealanL/RocketSim) and is never part of
// the shipped physics core.
//
// Usage: rocketsim_oracle <scenario.txt> <out.csv>
//
// Scenario format (one directive per line, '#' comments):
//   preset <octane|dominus|plank|breakout|hybrid|merc|psyclops>
//   ticks <n>
//   plane <px> <py> <pz> <nx> <ny> <nz>          (static half-space, normal into free space; repeatable)
//   pos|vel|angvel <x> <y> <z>
//   rot <fx> <fy> <fz> <rx> <ry> <rz> <ux> <uy> <uz>   (forward, right, up columns)
//   boost <amount>
//   ctrl <from_tick> <throttle> <steer> <pitch> <yaw> <roll> <jump> <boost> <handbrake>
//
// The world is RocketSim's THE_VOID game mode (no arena meshes, which are game assets) plus the
// listed Bullet static planes. The ball is removed from the world.

#include "RocketSim.h"
#include "../libsrc/bullet3-3.24/BulletCollision/CollisionShapes/btStaticPlaneShape.h"

#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>

using namespace RocketSim;

struct CtrlSeg {
	int from;
	CarControls c;
};

static const CarConfig& PresetConfig(const std::string& name) {
	if (name == "dominus") return CAR_CONFIG_DOMINUS;
	if (name == "plank") return CAR_CONFIG_PLANK;
	if (name == "breakout") return CAR_CONFIG_BREAKOUT;
	if (name == "hybrid") return CAR_CONFIG_HYBRID;
	if (name == "merc") return CAR_CONFIG_MERC;
	if (name == "psyclops") return CAR_CONFIG_PSYCLOPS;
	return CAR_CONFIG_OCTANE;
}

static void WriteRow(FILE* f, int tick, CarState s, Ball* ball) {
	fprintf(f, "%d", tick);
	auto v = [&](const Vec& x) { fprintf(f, ",%.9g,%.9g,%.9g", x.x, x.y, x.z); };
	v(s.pos); v(s.vel); v(s.angVel);
	v(s.rotMat.forward); v(s.rotMat.right); v(s.rotMat.up);
	fprintf(f, ",%.9g,%d,%d,%d,%d,%d,%d,%d,%d", s.boost, (int)s.isOnGround, (int)s.hasJumped,
		(int)s.hasDoubleJumped, (int)s.hasFlipped,
		(int)s.wheelsWithContact[0], (int)s.wheelsWithContact[1], (int)s.wheelsWithContact[2], (int)s.wheelsWithContact[3]);
	if (ball) {
		BallState b = ball->GetState();
		v(b.pos); v(b.vel); v(b.angVel);
	}
	fprintf(f, "\n");
}

int main(int argc, char** argv) {
	if (argc < 3) {
		fprintf(stderr, "usage: %s <scenario.txt> <out.csv>\n", argv[0]);
		return 1;
	}

	std::string preset = "octane";
	int ticks = 120;
	std::vector<std::pair<Vec, Vec>> planes;
	CarState init = CarState();
	std::vector<CtrlSeg> ctrls;
	bool hasBall = false, hasCar = true;
	BallState ballInit = BallState();

	std::ifstream in(argv[1]);
	if (!in) {
		fprintf(stderr, "cannot open %s\n", argv[1]);
		return 1;
	}
	std::string line;
	while (std::getline(in, line)) {
		std::istringstream ss(line);
		std::string key;
		if (!(ss >> key) || key[0] == '#') continue;
		if (key == "preset") ss >> preset;
		else if (key == "ticks") ss >> ticks;
		else if (key == "plane") {
			Vec p, n;
			ss >> p.x >> p.y >> p.z >> n.x >> n.y >> n.z;
			planes.push_back({ p, n });
		}
		else if (key == "pos") ss >> init.pos.x >> init.pos.y >> init.pos.z;
		else if (key == "vel") ss >> init.vel.x >> init.vel.y >> init.vel.z;
		else if (key == "angvel") ss >> init.angVel.x >> init.angVel.y >> init.angVel.z;
		else if (key == "rot") {
			RotMat& r = init.rotMat;
			ss >> r.forward.x >> r.forward.y >> r.forward.z >> r.right.x >> r.right.y >> r.right.z >> r.up.x >> r.up.y >> r.up.z;
		}
		else if (key == "boost") ss >> init.boost;
		else if (key == "ctrl") {
			CtrlSeg seg;
			int jump, boost, hb;
			ss >> seg.from >> seg.c.throttle >> seg.c.steer >> seg.c.pitch >> seg.c.yaw >> seg.c.roll >> jump >> boost >> hb;
			seg.c.jump = jump; seg.c.boost = boost; seg.c.handbrake = hb;
			ctrls.push_back(seg);
		}
		else if (key == "ball") {
			hasBall = true;
			ss >> ballInit.pos.x >> ballInit.pos.y >> ballInit.pos.z >> ballInit.vel.x >> ballInit.vel.y >> ballInit.vel.z
				>> ballInit.angVel.x >> ballInit.angVel.y >> ballInit.angVel.z;
		}
		else if (key == "nocar") hasCar = false;
		else if (key == "name") {}
		else {
			fprintf(stderr, "unknown directive '%s'\n", key.c_str());
			return 1;
		}
	}

	RocketSim::Init("__no_meshes__", true);

	ArenaConfig cfg = ArenaConfig();
	cfg.useCustomBroadphase = false;
	Arena* arena = Arena::Create(GameMode::THE_VOID, cfg, 120);

	// No ball in the oracle world.
	if (hasBall)
		arena->ball->SetState(ballInit);
	else
		arena->_bulletWorld.removeRigidBody(&arena->ball->_rigidBody);

	for (auto& pl : planes) {
		auto* shape = new btStaticPlaneShape(btVector3(pl.second.x, pl.second.y, pl.second.z), 0);
		arena->_AddStaticCollisionShape(shape, btVector3(pl.first.x, pl.first.y, pl.first.z) * UU_TO_BT);
	}

	// Bullet only sets the solver timestep inside the first step; before that it holds its 1/60 s
	// default, which the suspension "extra pushback" reads during the first tick's pre-update.
	// Use the steady-state value so tick 1 behaves like any other tick.
	arena->_bulletWorld.getSolverInfo().m_timeStep = 1 / 120.f;

	Car* car = nullptr;
	if (hasCar) {
		car = arena->AddCar(Team::BLUE, PresetConfig(preset));
		car->SetState(init);
	}

	FILE* out = fopen(argv[2], "w");
	if (!out) {
		fprintf(stderr, "cannot write %s\n", argv[2]);
		return 1;
	}
	auto row = [&](int tick) {
		WriteRow(out, tick, car ? car->GetState() : CarState(), hasBall ? arena->ball : nullptr);
	};
	fprintf(out, "tick,px,py,pz,vx,vy,vz,wx,wy,wz,fx,fy,fz,rx,ry,rz,ux,uy,uz,boost,on_ground,has_jumped,has_double_jumped,has_flipped,c0,c1,c2,c3%s\n",
		hasBall ? ",bpx,bpy,bpz,bvx,bvy,bvz,bwx,bwy,bwz" : "");
	row(0);

	bool debug = getenv("ORACLE_DEBUG") != nullptr;
	size_t seg = 0;
	for (int t = 0; t < ticks; t++) {
		while (seg + 1 < ctrls.size() && ctrls[seg + 1].from <= t) seg++;
		if (car)
			car->controls = (!ctrls.empty() && ctrls[seg].from <= t) ? ctrls[seg].c : CarControls();
		arena->Step(1);
		row(t + 1);
		if (debug) {
			auto* d = arena->_bulletWorld.getDispatcher();
			for (int i = 0; i < d->getNumManifolds(); i++) {
				auto* m = d->getManifoldByIndexInternal(i);
				fprintf(stderr, "tick %d manifold %d (%d,%d) thr=%.6f n=%d:", t + 1, i, m->getBody0()->getUserIndex(), m->getBody1()->getUserIndex(), m->getContactBreakingThreshold(), m->getNumContacts());
				for (int j = 0; j < m->getNumContacts(); j++) {
					auto& cp = m->getContactPoint(j);
					fprintf(stderr, " [d=%.5f imp=%.4f life=%d A=(%.4f,%.4f,%.4f) lA=(%.4f,%.4f,%.4f)]", cp.getDistance(), cp.m_appliedImpulse, cp.getLifeTime(), cp.getPositionWorldOnA().x()*50, cp.getPositionWorldOnA().y()*50, cp.getPositionWorldOnA().z()*50, cp.m_localPointA.x()*50, cp.m_localPointA.y()*50, cp.m_localPointA.z()*50);
				}
				fprintf(stderr, "\n");
			}
		}
	}
	fclose(out);
	delete arena;
	return 0;
}
