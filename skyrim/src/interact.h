#pragma once
#include "interact_settings.h"
#include "rlcar_ffi.h"
#include "space.h"
#include <unordered_map>
#include <vector>

struct CarView {
	const space::Frame* frame;
	const float* pose;
	float hitbox[6];
	ffi::Car* car;
	float lookahead;
};

// Bumps and demolitions of Skyrim's actors, with the GTA port's rule: an overlap pushes them away
// like a Rocket League car of their mass would; the front bumper adds Rocket League's bump, and at
// supersonic speed demolishes (kills, with the crime that comes with it).
class Interactions {
public:
	struct Hits {
		int bumped = 0, demolished = 0;
	};
	Hits update(const ffi::Api& api, RE::PlayerCharacter* player, const CarView& car, const InteractSettings& s, float delta);
	void clear();

private:
	struct Fling {
		RE::ActorHandle actor;
		RE::NiPoint3 velocity;  // Skyrim units per second
		float timeLeft;
	};
	std::unordered_map<RE::FormID, float> cooldown_;
	std::vector<Fling> flings_;
	float clock_ = 0;
	void updateFlings(float delta);
};
