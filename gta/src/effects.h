#pragma once
#include "effects_settings.h"
#include "natives.h"

class Effects {
public:
	void update(Vehicle veh, bool boosting, bool supersonic, const float* exhaustLocal, const EffectsSettings& s);
	void stop(Vehicle veh);

private:
	int fx_ = 0;
	bool fxBig_ = false;
	bool sound_ = false;
	int state_ = 0;
	const char* asset_ = nullptr;
	const char* effect_ = nullptr;
	DWORD requestedAt_ = 0;
	bool ensureAsset(const EffectsSettings& s);
};
