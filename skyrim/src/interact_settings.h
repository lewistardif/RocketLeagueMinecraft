#pragma once

struct InteractSettings {
	bool enabled = true;
	bool demolish = true;
	bool hitActors = true;
	bool ragdoll = true;
	bool crime = true;
	bool demolishFollowers = false;
	float bumpForce = 1.0f;
	float actorForce = 1.0f;
	float carMass = 180.0f;
	float actorMass = 60.0f;
	float restitution = 0.1f;
	float lift = 0.25f;
	float minImpactSpeed = 150.0f;
};
