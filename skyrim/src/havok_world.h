#pragma once
#include "sky_world.h"
#include "space.h"

// Copies the loaded Havok world's collision near the car into a sky::Cache, in Rocket League space.
// Main thread, under the world's read lock. Havok shape layouts are partly reverse-engineered
// (SkyCraft's Collision.cpp): every shape read is fault-guarded and a bad one is skipped.
namespace havok {

struct Query {
	const space::Frame* frame = nullptr;
	float lo[3] = {}, hi[3] = {};
	const RE::TESObjectREFR* ignore[2] = {};
};

struct Stats {
	int bodies = 0, tris = 0, boxes = 0, capsules = 0, convexes = 0, fallbacks = 0, faults = 0;
	double ms = 0;
};

bool gather(RE::TESObjectCELL* cell, const Query& q, sky::Cache& out, Stats& stats);

}
