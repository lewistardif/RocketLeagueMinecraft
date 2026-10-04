#include "interact.h"
#include "geom.h"

namespace {

constexpr float kPi = 3.14159265f;

geom::Box carBox(const CarView& c, float grow) {
	geom::Box b{};
	const float* p = c.pose;
	for (int i = 0; i < 3; i++)
		for (int k = 0; k < 3; k++) b.ax[i][k] = p[3 + i * 3 + k];
	for (int k = 0; k < 3; k++) {
		b.c[k] = p[k] + b.ax[0][k] * c.hitbox[3] + b.ax[1][k] * c.hitbox[4] + b.ax[2][k] * c.hitbox[5];
		b.h[k] = c.hitbox[k] * 0.5f + grow;
	}
	return b;
}

// An actor as an upright box around its bound radius and height, facing its heading.
geom::Box actorBox(const space::Frame& f, RE::Actor* a) {
	auto pos = a->GetPosition();
	float r = std::clamp(a->GetBoundRadius(), 10.0f, 400.0f), h = std::clamp(a->GetHeight(), 30.0f, 2000.0f);
	float heading = a->GetAngleZ();
	geom::Box b{};
	f.toRl({pos.x, pos.y, pos.z + h * 0.5}, b.c);
	space::dirToRl({std::cos(heading), -std::sin(heading), 0}, b.ax[0]);
	space::dirToRl({std::sin(heading), std::cos(heading), 0}, b.ax[1]);
	space::dirToRl({0, 0, 1}, b.ax[2]);
	b.h[0] = b.h[1] = float(f.lenToRl(r));
	b.h[2] = float(f.lenToRl(h * 0.5));
	return b;
}

// Every simulated body of the actor's ragdoll gets the velocity (Skyrim units per second).
bool flingRagdoll(RE::Actor* actor, const RE::NiPoint3& velocity) {
	auto* root = actor->Get3D();
	auto* cell = actor->GetParentCell();
	auto* world = cell ? cell->GetbhkWorld() : nullptr;
	if (!root || !world) return false;
	const float k = RE::bhkWorld::GetWorldScale();
	int bodies = 0;
	RE::BSWriteLockGuard lock(world->worldLock);
	RE::BSVisit::TraverseScenegraphCollision(root, [&](RE::bhkNiCollisionObject* collision) {
		auto* rigid = collision->body ? netimmerse_cast<RE::bhkRigidBody*>(collision->body.get()) : nullptr;
		auto* body = rigid ? rigid->GetRigidBody() : nullptr;
		if (body) {
			using M = RE::hkpMotion::MotionType;
			auto type = body->motion.type.get();
			if (type != M::kFixed && type != M::kKeyframed && type != M::kCharacter && type != M::kInvalid) {
				float m = body->motion.GetMass();
				body->ApplyLinearImpulse(RE::hkVector4(velocity.x * k * m, velocity.y * k * m, velocity.z * k * m, 0.0f));
				bodies++;
			}
		}
		return RE::BSVisit::BSVisitControl::kContinue;
	});
	return bodies > 0;
}

int gameSetting(const char* name, int def) {
	auto* settings = RE::GameSettingCollection::GetSingleton();
	auto* s = settings ? settings->GetSetting(name) : nullptr;
	return s ? s->GetInteger() : def;
}

void reportCrime(RE::Actor* victim, bool murder) {
	auto* faction = victim->GetCrimeFaction();
	if (!faction) return;
	faction->ModCrimeGold(murder ? gameSetting("iCrimeGoldMurder", 1000) : gameSetting("iCrimeGoldAttack", 40), true);
}

void stagger(RE::Actor* actor, const RE::NiPoint3& push, float magnitude) {
	float heading = std::atan2(push.x, push.y);
	float dir = (heading - actor->GetAngleZ()) / (2.0f * kPi) + 0.5f;
	dir -= std::floor(dir);
	actor->SetGraphVariableFloat("staggerDirection", dir);
	actor->SetGraphVariableFloat("staggerMagnitude", std::clamp(magnitude, 0.1f, 1.0f));
	actor->NotifyAnimationGraph("staggerStart");
}

}

void Interactions::clear() {
	cooldown_.clear();
	flings_.clear();
}

void Interactions::updateFlings(float delta) {
	for (auto it = flings_.begin(); it != flings_.end();) {
		auto actor = it->actor.get();
		it->timeLeft -= delta;
		bool done = !actor || it->timeLeft <= 0;
		if (actor && flingRagdoll(actor.get(), it->velocity)) done = true;
		it = done ? flings_.erase(it) : it + 1;
	}
}

Interactions::Hits Interactions::update(const ffi::Api& api, RE::PlayerCharacter* player, const CarView& car, const InteractSettings& s, float delta) {
	Hits hits;
	clock_ += delta;
	updateFlings(delta);
	if (!s.enabled || !s.hitActors) return hits;
	auto* lists = RE::ProcessLists::GetSingleton();
	if (!lists) return hits;
	const space::Frame& f = *car.frame;
	geom::Box me = carBox(car, car.lookahead);
	geom::Box exact = carBox(car, 0);
	const float* myVel = car.pose + 12;
	for (auto& handle : lists->highActorHandles) {
		auto ptr = handle.get();
		RE::Actor* a = ptr.get();
		if (!a || a == player || a->IsDead() || a->IsDisabled() || !a->Is3DLoaded() || a->IsGhost()) continue;
		auto cd = cooldown_.find(a->GetFormID());
		if (cd != cooldown_.end() && clock_ < cd->second) continue;
		geom::Box other = actorBox(f, a);
		float dx = other.c[0] - me.c[0], dy = other.c[1] - me.c[1], dz = other.c[2] - me.c[2];
		if (dx * dx + dy * dy + dz * dz > 3000.0f * 3000.0f || !geom::overlap(me, other)) continue;
		float local[3];
		geom::closestLocal(exact, other.c, local);
		float contactX = local[0] + car.hitbox[3];
		float cw[3], nrm[3];
		for (int k = 0; k < 3; k++) cw[k] = exact.c[k] + exact.ax[0][k] * local[0] + exact.ax[1][k] * local[1] + exact.ax[2][k] * local[2];
		for (int k = 0; k < 3; k++) nrm[k] = other.c[k] - cw[k];
		float nl = std::sqrt(nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]);
		if (nl < 1e-3f) {
			float vl = std::sqrt(myVel[0] * myVel[0] + myVel[1] * myVel[1] + myVel[2] * myVel[2]);
			if (vl < 1e-3f) continue;
			for (int k = 0; k < 3; k++) nrm[k] = myVel[k] / vl;
		} else {
			for (float& v : nrm) v /= nl;
		}
		RE::NiPoint3 sv;
		a->GetLinearVelocity(sv);
		float rv[3], up[3] = {other.ax[2][0], other.ax[2][1], other.ax[2][2]};
		space::dirToRl({sv.x / f.k(), sv.y / f.k(), sv.z / f.k()}, rv);
		float vn = 0;
		for (int k = 0; k < 3; k++) vn += (myVel[k] - rv[k]) * nrm[k];
		bool canDemolish = s.demolish && !a->IsEssential() && (s.demolishFollowers || !a->IsPlayerTeammate());
		float bumpDv[3] = {};
		uint32_t r = api.car_bump(car.car, other.c, rv, 1u, up, contactX, s.bumpForce, canDemolish ? 1u : 0u, bumpDv);
		if (r == 0 && vn < s.minImpactSpeed) continue;
		float share = (1.0f + s.restitution) * s.carMass / (s.carMass + s.actorMass);
		float dv[3];
		for (int k = 0; k < 3; k++) dv[k] = nrm[k] * std::max(vn, 0.0f) * share;
		dv[2] += std::max(vn, 0.0f) * s.lift;
		if (r != 0)
			for (int k = 0; k < 3; k++) dv[k] += bumpDv[k];
		cooldown_[a->GetFormID()] = clock_ + 0.25f;
		space::V3 g = space::dirToSky(dv);
		double scale = f.k() * s.actorForce;
		RE::NiPoint3 velocity{float(g.x * scale), float(g.y * scale), float(g.z * scale)};
		RE::NiPoint3 push{float(g.x), float(g.y), 0.0f};
		float speed = float(std::sqrt(g.x * g.x + g.y * g.y + g.z * g.z) * scale);
		if (r == 2) {
			float health = a->AsActorValueOwner()->GetActorValue(RE::ActorValue::kHealth);
			a->KillImpl(player, health + 1.0f, true, true);
			if (s.crime) reportCrime(a, true);
			hits.demolished++;
		} else {
			if (s.crime && !a->IsHostileToActor(player)) reportCrime(a, false);
			if (!a->IsPlayerTeammate() && !a->IsInCombat()) a->StartCombat(player);
			hits.bumped++;
		}
		if (s.ragdoll || r == 2) {
			if (r != 2)
				if (auto* process = a->GetActorRuntimeData().currentProcess) {
					auto at = a->GetPosition();
					float back = std::max(1.0f, std::sqrt(push.x * push.x + push.y * push.y));
					RE::NiPoint3 from{at.x - push.x / back * 50.0f, at.y - push.y / back * 50.0f, at.z};
					process->KnockExplosion(a, from, std::clamp(speed / 300.0f, 1.0f, 20.0f));
				}
			flings_.push_back({a->GetHandle(), velocity, 0.75f});
		} else {
			stagger(a, push, speed / 1000.0f);
		}
		logger::info("{} {} ({:08X}) at {:.0f} uu/s, thrown at {:.0f} units/s", r == 2 ? "demolished" : "bumped", a->GetDisplayFullName(),
		             a->GetFormID(), vn, speed);
	}
	return hits;
}
