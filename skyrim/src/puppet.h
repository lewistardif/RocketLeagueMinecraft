#pragma once

// The Dragonborn while they are the car: hidden (Skyrim's first-person state with its meshes off),
// their capsule parked under the car so the world keeps loading around it, no momentum or fall
// damage of their own. end() hands them back where the car is.
namespace puppet {

void begin(RE::PlayerCharacter* player);
void hold(RE::PlayerCharacter* player, const RE::NiPoint3& feet);
void end(RE::PlayerCharacter* player, const RE::NiPoint3& feet, float heading);
bool active();

}
