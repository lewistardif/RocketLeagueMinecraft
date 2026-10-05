#include "effects.h"

bool Effects::ensureAsset(const EffectsSettings& s) {
	if (state_ == 2) return true;
	if (state_ == 3) return false;
	if (state_ == 0) {
		STREAMING::REQUEST_NAMED_PTFX_ASSET(s.asset.c_str());
		requestedAt_ = GetTickCount();
		state_ = 1;
	}
	if (STREAMING::HAS_NAMED_PTFX_ASSET_LOADED(s.asset.c_str())) {
		asset_ = s.asset.c_str(), effect_ = s.effect.c_str();
		state_ = 2;
		return true;
	}
	STREAMING::REQUEST_NAMED_PTFX_ASSET(s.fallbackAsset.c_str());
	if (GetTickCount() - requestedAt_ > 3000 && STREAMING::HAS_NAMED_PTFX_ASSET_LOADED(s.fallbackAsset.c_str())) {
		asset_ = s.fallbackAsset.c_str(), effect_ = s.fallbackEffect.c_str();
		state_ = 2;
		return true;
	}
	if (GetTickCount() - requestedAt_ > 10000) state_ = 3;
	return false;
}

void Effects::stop(Vehicle veh) {
	if (fx_) GRAPHICS::STOP_PARTICLE_FX_LOOPED(fx_, FALSE), fx_ = 0;
	if (sound_ && veh && ENTITY::DOES_ENTITY_EXIST(veh)) AUDIO::SET_VEHICLE_BOOST_ACTIVE(veh, FALSE);
	sound_ = false;
}

void Effects::update(Vehicle veh, bool boosting, bool supersonic, const float* ex, const EffectsSettings& s) {
	if (!s.enabled || !veh) {
		stop(veh);
		return;
	}
	if (s.idleGlow) {
		Vector3 p = ENTITY::GET_OFFSET_FROM_ENTITY_IN_WORLD_COORDS(veh, ex[0], ex[1], ex[2]);
		float boostGlow = boosting ? 3.0f : 1.0f;
		GRAPHICS::DRAW_LIGHT_WITH_RANGE(p.x, p.y, p.z, 255, 120, 30, s.glowRange * boostGlow, s.glowIntensity * boostGlow);
	}
	if (s.boostSound && boosting != sound_) {
		AUDIO::SET_VEHICLE_BOOST_ACTIVE(veh, boosting ? TRUE : FALSE);
		sound_ = boosting;
	}
	if (!boosting) {
		if (fx_) GRAPHICS::STOP_PARTICLE_FX_LOOPED(fx_, FALSE), fx_ = 0;
		return;
	}
	if (!ensureAsset(s)) return;
	if (!fx_) {
		GRAPHICS::USE_PARTICLE_FX_ASSET(asset_);
		fx_ = GRAPHICS::START_PARTICLE_FX_LOOPED_ON_ENTITY(effect_, veh, ex[0], ex[1], ex[2], s.rot[0], s.rot[1], s.rot[2],
		                                                    supersonic ? s.scale * s.supersonicScale : s.scale, FALSE, FALSE, FALSE);
		fxBig_ = supersonic;
	} else if (fxBig_ != supersonic) {
		GRAPHICS::SET_PARTICLE_FX_LOOPED_SCALE(fx_, supersonic ? s.scale * s.supersonicScale : s.scale);
		fxBig_ = supersonic;
	}
}
