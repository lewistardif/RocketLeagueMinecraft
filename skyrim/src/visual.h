#pragma once

// A vanilla model standing in for the car or the ball: a temporary reference placed at the player,
// with its collision off, moved to the core's pose every frame. It is never saved.
namespace visual {

class Prop {
public:
	bool spawn(RE::TESBoundObject* base, RE::TESObjectREFR* at);
	void remove();
	RE::TESObjectREFR* get() const;
	// The model's local origin goes to `origin`, its local axes along rot's columns, scaled.
	void place(RE::PlayerCharacter* player, const RE::NiPoint3& origin, const RE::NiMatrix3& rot, float scale);

	float boundMin[3] = {}, boundMax[3] = {};

private:
	RE::ObjectRefHandle handle_;
	RE::NiAVObject* quiet_ = nullptr;
};

RE::TESBoundObject* lookup(const std::string& plugin, uint32_t id);

}
