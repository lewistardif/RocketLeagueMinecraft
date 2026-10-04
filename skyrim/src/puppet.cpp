#include "puppet.h"

namespace puppet {

namespace {

bool g_active = false;
bool g_wasThirdPerson = false;
std::vector<RE::NiPointer<RE::BSGeometry>> g_hidden;

// Only the first-person model's meshes are hidden, never its nodes (Skyrim's camera and animation
// use those), and exactly those meshes are shown again afterwards.
void hideFirstPerson(RE::PlayerCharacter* player, bool hide) {
	if (!hide) {
		for (auto& mesh : g_hidden)
			if (mesh && mesh->GetAppCulled()) mesh->SetAppCulled(false);
		g_hidden.clear();
		return;
	}
	auto* root = player->Get3D(true);
	if (!root) return;
	RE::BSVisit::TraverseScenegraphGeometries(root, [](RE::BSGeometry* mesh) {
		if (!mesh->GetAppCulled()) {
			mesh->SetAppCulled(true);
			g_hidden.emplace_back(mesh);
		}
		return RE::BSVisit::BSVisitControl::kContinue;
	});
}

void still(RE::PlayerCharacter* player, float z) {
	auto* controller = player->GetCharController();
	if (!controller) return;
	controller->SetLinearVelocityImpl(RE::hkVector4(0.0f, 0.0f, 0.0f, 0.0f));
	controller->fallStartHeight = z;
	controller->fallTime = 0.0f;
}

}

void begin(RE::PlayerCharacter* player) {
	auto* camera = RE::PlayerCamera::GetSingleton();
	g_wasThirdPerson = camera && camera->IsInThirdPerson();
	if (camera && !camera->IsInFirstPerson()) camera->ForceFirstPerson();
	if (player->AsActorState()->GetWeaponState() == RE::WEAPON_STATE::kDrawn) player->DrawWeaponMagicHands(false);
	g_active = true;
	logger::info("puppet on ({} person before)", g_wasThirdPerson ? "third" : "first");
}

void hold(RE::PlayerCharacter* player, const RE::NiPoint3& feet) {
	if (!g_active) return;
	if (auto* camera = RE::PlayerCamera::GetSingleton(); camera && !camera->IsInFirstPerson()) camera->ForceFirstPerson();
	hideFirstPerson(player, true);
	player->SetPosition(feet, true);
	still(player, feet.z);
}

void end(RE::PlayerCharacter* player, const RE::NiPoint3& feet, float heading) {
	if (!g_active) return;
	g_active = false;
	hideFirstPerson(player, false);
	player->SetPosition(feet, true);
	player->data.angle.x = 0;
	player->data.angle.z = heading;
	still(player, feet.z);
	if (auto* camera = RE::PlayerCamera::GetSingleton(); camera && g_wasThirdPerson) camera->ForceThirdPerson();
	logger::info("puppet off at {:.0f} {:.0f} {:.0f}", feet.x, feet.y, feet.z);
}

bool active() { return g_active; }

}
