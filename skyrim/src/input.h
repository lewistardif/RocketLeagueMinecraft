#pragma once
#include "bindings.h"

// Keyboard (while Skyrim has focus), XInput and PlayStation pads, read like the GTA port. While the
// car is driven Skyrim's own player controls get nothing and its menus only Esc, the console and
// the gamepad's Start.
namespace input {

void install();
void setDriving(bool on);
InputState read();
bool skyrimMenuOpen();

}
