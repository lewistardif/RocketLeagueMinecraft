#pragma once

// The state machine: on foot <-> the Rocket League car. Everything runs on the main thread, from
// the player's per-frame update.
namespace plugin {

void install();
void onLoading(const char* why);
void update(RE::PlayerCharacter* player, float delta);
bool driving();

}
