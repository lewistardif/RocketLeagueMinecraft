#pragma once

struct InteractSettings {
	bool enabled = true;
	bool demolish = true;
	bool hitPeds = true;
	float bumpForce = 1.0f;
	float pedForce = 1.0f;
	float carMass = 180.0f;
	float vehicleMass = 180.0f;
	float pedMass = 60.0f;
	float restitution = 0.1f;
	float pedLift = 0.25f;
	float minImpactSpeed = 150.0f;
};
