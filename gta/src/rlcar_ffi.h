#pragma once
#include <cstdint>
#include <string>

namespace ffi {

constexpr uint32_t ABI_VERSION = 5;
constexpr int POSE_FLOATS = 40;
constexpr int CAMERA_SETTINGS_FLOATS = 8;
constexpr int CAMERA_VIEW_FLOATS = 17;
constexpr int SIM_CONFIG_FLOATS = 15;
constexpr int BALL_CONFIG_FLOATS = 10;
constexpr int BALL_POSE_FLOATS = 9;

namespace flags {
constexpr uint32_t ON_GROUND = 1u << 0, BOOSTING = 1u << 1, SUPERSONIC = 1u << 2, HAS_FLIP_OR_JUMP = 1u << 3,
                   FLIPPING = 1u << 4, JUMPING = 1u << 5, THROTTLING = 1u << 6, WHEEL_CONTACT_SHIFT = 8;
}
namespace buttons {
constexpr uint32_t JUMP = 1u << 0, BOOST = 1u << 1, HANDBRAKE = 1u << 2;
}
constexpr uint32_t CAMERA_REAR_VIEW = 1u << 0;
constexpr uint32_t CAMERA_BALL_CAM = 1u << 1;

struct RayHit {
	float distance;
	float point[3];
	float normal[3];
};
struct Contact {
	float point[3];
	float normal[3];
	float depth;
	uint32_t surface;
};
struct Obb {
	float center[3];
	float axes[9];
	float half_extents[3];
};

struct Car;
struct World;
struct Ball;
using RaycastFn = uint32_t (*)(void* user, const float* origin, const float* dir, float maxDist, RayHit* hit);
using BoxContactsFn = uint32_t (*)(void* user, const Obb* obb, float margin, Contact* out, uint32_t cap);
using SphereContactsFn = uint32_t (*)(void* user, const float* center, float radius, float margin, Contact* out, uint32_t cap);

struct Api {
	uint32_t (*abi_version)();
	World* (*cbworld_new)(void*, RaycastFn, BoxContactsFn, SphereContactsFn);
	void (*cbworld_free)(World*);
	Car* (*car_new)(uint32_t preset);
	void (*car_free)(Car*);
	void (*car_reset)(Car*, const float* pos, float yaw, float pitch, float roll);
	void (*car_translate)(Car*, const float* delta);
	void (*car_step_cb)(Car*, const World*, uint32_t ticks, float, float, float, float, float, uint32_t);
	uint32_t (*car_advance_cb)(Car*, const World*, double dt, float, float, float, float, float, uint32_t);
	float (*car_alpha)(const Car*);
	uint32_t (*car_pose)(const Car*, float alpha, float* out);
	uint32_t (*car_preset)(const Car*);
	void (*car_set_unlimited_boost)(Car*, uint32_t);
	void (*car_set_config)(Car*, const float*);
	void (*car_config)(const Car*, float*);
	void (*default_config)(float*);
	void (*preset_hitbox)(uint32_t preset, float* out);
	void* (*camera_new)();
	void (*camera_free)(void*);
	void (*camera_reset)(void*);
	void (*camera_translate)(void*, const float*);
	uint32_t (*camera_update)(void*, const Car*, float alpha, float dt, const float* settings, float lookRight, float lookUp,
	                          uint32_t flags, float* out);
	uint32_t (*camera_preset)(uint32_t index, float* out);
	Ball* (*ball_new)();
	void (*ball_free)(Ball*);
	void (*ball_reset)(Ball*, const float* pos, const float* vel, const float* angVel);
	void (*ball_translate)(Ball*, const float* delta);
	void (*default_ball_config)(float*);
	void (*ball_config)(const Ball*, float*);
	void (*ball_set_config)(Ball*, const float*);
	uint32_t (*ball_pose)(const Ball*, float alpha, float* out);
	uint32_t (*car_ball_touch)(Car*, float* out);
	uint32_t (*scene_advance_cb)(Car*, Ball*, const World*, double dt, float, float, float, float, float, uint32_t);
	uint32_t (*camera_update_ball)(void*, const Car*, const Ball*, float alpha, float dt, const float* settings, float lookRight,
	                               float lookUp, uint32_t flags, float* out);
};

bool load(const std::string& path, Api& api, std::string& error);

}
