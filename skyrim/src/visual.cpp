#include "visual.h"

namespace visual {

RE::TESBoundObject* lookup(const std::string& plugin, uint32_t id) {
	auto* data = RE::TESDataHandler::GetSingleton();
	auto* form = data ? data->LookupForm(id, plugin) : nullptr;
	return form ? form->As<RE::TESBoundObject>() : nullptr;
}

bool Prop::spawn(RE::TESBoundObject* base, RE::TESObjectREFR* at) {
	remove();
	if (!base || !at) return false;
	auto ref = at->PlaceObjectAtMe(base, false);
	if (!ref) return false;
	ref->SetTemporary();
	ref->SetActivationBlocked(true);
	ref->SetCollision(false);
	handle_ = ref->GetHandle();
	quiet_ = nullptr;
	const auto& b = base->boundData;
	boundMin[0] = b.boundMin.x, boundMin[1] = b.boundMin.y, boundMin[2] = b.boundMin.z;
	boundMax[0] = b.boundMax.x, boundMax[1] = b.boundMax.y, boundMax[2] = b.boundMax.z;
	return true;
}

void Prop::remove() {
	if (auto ref = handle_.get()) {
		ref->Disable();
		ref->SetDelete(true);
	}
	handle_.reset();
	quiet_ = nullptr;
}

RE::TESObjectREFR* Prop::get() const { return handle_.get().get(); }

void Prop::place(RE::PlayerCharacter* player, const RE::NiPoint3& origin, const RE::NiMatrix3& rot, float scale) {
	auto ref = handle_.get();
	if (!ref) return;
	// Keep it in the player's cell (the player rides along under the car), or it unloads with its own.
	auto* cell = player->GetParentCell();
	if (cell && ref->GetParentCell() != cell) ref->MoveTo(player);
	ref->SetPosition(origin);
	auto* node = ref->Get3D();
	if (!node) return;
	if (node != quiet_) {
		// Its Havok body stays in the world but collides with nothing and is moved, not simulated.
		node->SetMotionType(RE::hkpMotion::MotionType::kKeyframed, true, false, false);
		node->SetCollisionLayer(RE::COL_LAYER::kNonCollidable);
		quiet_ = node;
	}
	node->local.translate = origin;
	node->local.rotate = rot;
	node->local.scale = scale;
	RE::NiUpdateData update{};
	node->Update(update);
}

}
