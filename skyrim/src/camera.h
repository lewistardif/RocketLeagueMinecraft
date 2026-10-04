#pragma once
#include "space.h"

// Skyrim's camera at the Rocket League camera. Skyrim stays in its first-person state; its camera
// root is moved and turned after PlayerCamera::Update (hooked at its call sites) and the
// first-person state reports our eye as its translation.
namespace camera {

void install();
void set(RE::PlayerCharacter* player, const RE::NiPoint3& eye, const space::CameraBasis& basis, float skyrimFov);
void release();

}
