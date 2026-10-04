#pragma once
#include "bindings.h"

namespace hidpad {

void start();
bool read(PadState& out);
void stop();

}
