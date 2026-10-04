#pragma once
#include <array>
#include <cstdint>
#include <string>
#include <vector>

class Ini;

enum class Action {
	Throttle, Reverse, SteerRight, SteerLeft, PitchUp, PitchDown, YawRight, YawLeft,
	AirRollRight, AirRollLeft, AirRoll, Jump, Boost, Powerslide, RearCamera, ResetCar, ExitCar,
	BecomeCar, ReloadConfig, SpawnBall, BallCam, Count
};

namespace pad {
constexpr uint32_t DUP = 0x0001, DDOWN = 0x0002, DLEFT = 0x0004, DRIGHT = 0x0008, START = 0x0010, BACK = 0x0020, LS = 0x0040,
                   RS = 0x0080, LB = 0x0100, RB = 0x0200, A = 0x1000, B = 0x2000, X = 0x4000, Y = 0x8000, LT = 0x10000,
                   RT = 0x20000;
}

struct PadState {
	bool connected = false;
	uint32_t buttons = 0;
	float lt = 0, rt = 0;
	float lx = 0, ly = 0, rx = 0, ry = 0;
};

struct InputState {
	std::array<bool, 256> keys{};
	PadState pad;
};

struct KeyChord {
	std::vector<int> keys;
	uint32_t padBits = 0;
};

struct DriveInput {
	float throttle = 0, steer = 0, pitch = 0, yaw = 0, roll = 0;
	bool jump = false, boost = false, handbrake = false, rearCamera = false;
	float lookRight = 0, lookUp = 0;
};

class Bindings {
public:
	Bindings();
	void load(const Ini& ini);
	float value(Action a, const InputState& s) const;
	bool held(Action a, const InputState& s) const { return value(a, s) >= 0.5f; }
	DriveInput drive(const InputState& s) const;

	float deadzone = 0.15f, swivelDeadzone = 0.20f;
	bool invertPitch = false;

	static int keyCode(const std::string& name);
	static uint32_t padBits(const std::string& name);
	static const char* name(Action a);

private:
	std::array<std::vector<KeyChord>, size_t(Action::Count)> keys_, pads_;
	void parse(Action a, const std::string& keys, const std::string& pads);
};
