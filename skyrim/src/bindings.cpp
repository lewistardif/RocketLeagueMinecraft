#include "bindings.h"
#include "ini.h"
#include <algorithm>
#include <cctype>
#include <cmath>
#include <map>
#include <sstream>

static const char* kNames[] = {"Throttle", "Reverse", "SteerRight", "SteerLeft", "PitchUp", "PitchDown", "YawRight",
                               "YawLeft", "AirRollRight", "AirRollLeft", "AirRoll", "Jump", "Boost", "Powerslide",
                               "RearCamera", "ResetCar", "ExitCar", "BecomeCar", "ReloadConfig", "SpawnBall", "BallCam"};
static_assert(sizeof(kNames) / sizeof(kNames[0]) == size_t(Action::Count), "one name per action");

struct Default {
	Action a;
	const char* keys;
	const char* pad;
};
static const Default kDefaults[] = {
	{Action::Throttle, "W", "RT"},          {Action::Reverse, "S", "LT"},
	{Action::SteerRight, "D", ""},          {Action::SteerLeft, "A", ""},
	{Action::PitchUp, "S", ""},             {Action::PitchDown, "W", ""},
	{Action::YawRight, "D", ""},            {Action::YawLeft, "A", ""},
	{Action::AirRollRight, "E", "RB"},      {Action::AirRollLeft, "Q", "LB"},
	{Action::AirRoll, "LCTRL", "X"},        {Action::Jump, "SPACE", "A"},
	{Action::Boost, "LSHIFT", "B"},         {Action::Powerslide, "LCTRL", "X"},
	{Action::RearCamera, "MMB", "RS"},      {Action::ResetCar, "R", "Y"},
	{Action::ExitCar, "", "BACK"},          {Action::BecomeCar, "F7", "LS+RS"},
	{Action::ReloadConfig, "F10", ""},      {Action::SpawnBall, "B", "DLEFT"},
	{Action::BallCam, "C", "DDOWN"},
};

static std::string upper(std::string s) {
	std::transform(s.begin(), s.end(), s.begin(), [](unsigned char c) { return char(std::toupper(c)); });
	return s;
}

static std::vector<std::string> split(const std::string& s, char sep) {
	std::vector<std::string> out;
	std::stringstream ss(s);
	std::string t;
	while (std::getline(ss, t, sep)) {
		size_t a = t.find_first_not_of(" \t"), b = t.find_last_not_of(" \t");
		if (a != std::string::npos) out.push_back(t.substr(a, b - a + 1));
	}
	return out;
}

int Bindings::keyCode(const std::string& raw) {
	std::string n = upper(raw);
	if (n.size() == 1 && ((n[0] >= 'A' && n[0] <= 'Z') || (n[0] >= '0' && n[0] <= '9'))) return n[0];
	if (n.size() >= 2 && n[0] == 'F' && std::isdigit((unsigned char)n[1])) {
		int f = std::atoi(n.c_str() + 1);
		if (f >= 1 && f <= 24) return 0x70 + f - 1;
	}
	if (n.size() == 4 && n.rfind("NUM", 0) == 0 && std::isdigit((unsigned char)n[3])) return 0x60 + (n[3] - '0');
	static const std::map<std::string, int> m = {
		{"SPACE", 0x20},  {"LSHIFT", 0xA0}, {"RSHIFT", 0xA1}, {"SHIFT", 0x10}, {"LCTRL", 0xA2}, {"RCTRL", 0xA3},
		{"CTRL", 0x11},   {"LALT", 0xA4},   {"RALT", 0xA5},   {"ALT", 0x12},   {"TAB", 0x09},   {"ENTER", 0x0D},
		{"ESC", 0x1B},    {"BACKSPACE", 0x08}, {"CAPSLOCK", 0x14}, {"UP", 0x26}, {"DOWN", 0x28}, {"LEFT", 0x25},
		{"RIGHT", 0x27},  {"INSERT", 0x2D}, {"DELETE", 0x2E}, {"HOME", 0x24},  {"END", 0x23},   {"PAGEUP", 0x21},
		{"PAGEDOWN", 0x22}, {"LMB", 0x01},  {"RMB", 0x02},    {"MMB", 0x04},   {"MB4", 0x05},   {"MB5", 0x06},
		{"COMMA", 0xBC},  {"PERIOD", 0xBE}, {"SLASH", 0xBF},  {"SEMICOLON", 0xBA}, {"QUOTE", 0xDE}, {"TILDE", 0xC0},
	};
	auto it = m.find(n);
	return it == m.end() ? -1 : it->second;
}

uint32_t Bindings::padBits(const std::string& raw) {
	static const std::map<std::string, uint32_t> m = {
		{"A", pad::A},        {"CROSS", pad::A},    {"B", pad::B},        {"CIRCLE", pad::B},     {"X", pad::X},
		{"SQUARE", pad::X},   {"Y", pad::Y},        {"TRIANGLE", pad::Y}, {"LB", pad::LB},        {"L1", pad::LB},
		{"RB", pad::RB},      {"R1", pad::RB},      {"LT", pad::LT},      {"L2", pad::LT},        {"RT", pad::RT},
		{"R2", pad::RT},      {"LS", pad::LS},      {"L3", pad::LS},      {"RS", pad::RS},        {"R3", pad::RS},
		{"BACK", pad::BACK},  {"VIEW", pad::BACK},  {"SELECT", pad::BACK}, {"SHARE", pad::BACK},  {"START", pad::START},
		{"MENU", pad::START}, {"OPTIONS", pad::START}, {"DUP", pad::DUP}, {"DDOWN", pad::DDOWN},  {"DLEFT", pad::DLEFT},
		{"DRIGHT", pad::DRIGHT},
	};
	uint32_t bits = 0;
	for (auto& part : split(upper(raw), '+')) {
		auto it = m.find(part);
		if (it == m.end()) return 0;
		bits |= it->second;
	}
	return bits;
}

const char* Bindings::name(Action a) { return kNames[size_t(a)]; }

Bindings::Bindings() {
	for (auto& d : kDefaults) parse(d.a, d.keys, d.pad);
}

void Bindings::parse(Action a, const std::string& keys, const std::string& pads) {
	auto& k = keys_[size_t(a)];
	auto& p = pads_[size_t(a)];
	k.clear();
	p.clear();
	for (auto& alt : split(keys, ',')) {
		KeyChord c;
		bool ok = true;
		for (auto& part : split(alt, '+')) {
			int code = keyCode(part);
			ok &= code >= 0;
			c.keys.push_back(code);
		}
		if (ok && !c.keys.empty()) k.push_back(c);
	}
	for (auto& alt : split(pads, ',')) {
		KeyChord c;
		c.padBits = padBits(alt);
		if (c.padBits) p.push_back(c);
	}
}

void Bindings::load(const Ini& ini) {
	for (auto& d : kDefaults) {
		std::string n = name(d.a);
		parse(d.a, ini.str("Controls", n, d.keys), ini.str("Gamepad", n, d.pad));
	}
	deadzone = float(ini.num("Gamepad", "Deadzone", deadzone));
	swivelDeadzone = float(ini.num("Gamepad", "SwivelDeadzone", swivelDeadzone));
	invertPitch = ini.flag("Controls", "InvertPitch", invertPitch);
}

float Bindings::value(Action a, const InputState& s) const {
	float v = 0;
	for (auto& c : keys_[size_t(a)]) {
		bool all = true;
		for (int k : c.keys) all &= k >= 0 && k < 256 && s.keys[size_t(k)];
		if (all) v = 1;
	}
	if (s.pad.connected) {
		for (auto& c : pads_[size_t(a)]) {
			uint32_t buttons = c.padBits & 0xFFFF;
			if ((s.pad.buttons & buttons) != buttons) continue;
			float t = 1;
			if (c.padBits & pad::LT) t = std::min(t, s.pad.lt);
			if (c.padBits & pad::RT) t = std::min(t, s.pad.rt);
			v = std::max(v, t);
		}
	}
	return v;
}

static float stick(float v, float dz) {
	float a = std::fabs(v);
	if (a <= dz) return 0;
	return std::copysign(std::min(1.0f, (a - dz) / (1 - dz)), v);
}

DriveInput Bindings::drive(const InputState& s) const {
	DriveInput d;
	auto v = [&](Action a) { return value(a, s); };
	float lx = s.pad.connected ? stick(s.pad.lx, deadzone) : 0;
	float ly = s.pad.connected ? stick(s.pad.ly, deadzone) : 0;
	auto axis = [](float keys, float stickV) { return std::fabs(stickV) > std::fabs(keys) ? stickV : keys; };

	d.throttle = std::clamp(v(Action::Throttle) - v(Action::Reverse), -1.0f, 1.0f);
	d.steer = axis(v(Action::SteerRight) - v(Action::SteerLeft), lx);
	float pitch = axis(v(Action::PitchUp) - v(Action::PitchDown), -ly);
	d.pitch = invertPitch ? -pitch : pitch;
	float yaw = axis(v(Action::YawRight) - v(Action::YawLeft), lx);
	float roll = v(Action::AirRollRight) - v(Action::AirRollLeft);
	if (held(Action::AirRoll, s)) {
		roll += yaw;
		yaw = 0;
	}
	d.yaw = yaw;
	d.roll = std::clamp(roll, -1.0f, 1.0f);
	d.jump = held(Action::Jump, s);
	d.boost = held(Action::Boost, s);
	d.handbrake = held(Action::Powerslide, s);
	d.rearCamera = held(Action::RearCamera, s);
	if (s.pad.connected) {
		d.lookRight = stick(s.pad.rx, swivelDeadzone);
		d.lookUp = stick(s.pad.ry, swivelDeadzone);
	}
	return d;
}
