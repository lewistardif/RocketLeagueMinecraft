#pragma once
#include "natives.h"
#include "rlcar_ffi.h"
#include "interact_settings.h"
#include "space.h"
#include <unordered_map>
#include <vector>

struct CarView {
	const space::Frame* frame;
	const float* pose;
	float hitbox[6];
	ffi::Car* car;
	Entity self;
	float lookahead;
};

class Interactions {
public:
	int update(const ffi::Api& api, const CarView& car, const InteractSettings& s);
	void clear();

private:
	std::unordered_map<int, DWORD> cooldown_;
	std::vector<std::pair<int, DWORD>> wrecks_;
	std::vector<int> buf_ = std::vector<int>(512);
};
