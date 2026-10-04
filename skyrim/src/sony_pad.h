#pragma once
#include "bindings.h"
#include <cstddef>
#include <cstdint>

namespace sony {

constexpr uint16_t VENDOR = 0x054C;

inline bool isDualSense(uint16_t pid) { return pid == 0x0CE6 || pid == 0x0DF2; }
inline bool isDualShock4(uint16_t pid) { return pid == 0x05C4 || pid == 0x09CC || pid == 0x0BA0; }
inline bool supported(uint16_t vid, uint16_t pid) { return vid == VENDOR && (isDualSense(pid) || isDualShock4(pid)); }

inline float axis(uint8_t v) {
	float f = (float(v) - 127.5f) / 127.5f;
	return f < -1 ? -1 : (f > 1 ? 1 : f);
}

inline uint32_t dpad(uint8_t nibble) {
	static const uint32_t map[8] = {pad::DUP,
	                                pad::DUP | pad::DRIGHT,
	                                pad::DRIGHT,
	                                pad::DRIGHT | pad::DDOWN,
	                                pad::DDOWN,
	                                pad::DDOWN | pad::DLEFT,
	                                pad::DLEFT,
	                                pad::DLEFT | pad::DUP};
	return nibble < 8 ? map[nibble] : 0;
}

inline void fill(const uint8_t* s, uint8_t faceAndDpad, uint8_t shoulders, uint8_t l2, uint8_t r2, PadState& out) {
	out.connected = true;
	out.lx = axis(s[0]);
	out.ly = -axis(s[1]);
	out.rx = axis(s[2]);
	out.ry = -axis(s[3]);
	out.lt = l2 / 255.0f;
	out.rt = r2 / 255.0f;
	uint32_t b = dpad(faceAndDpad & 0x0F);
	if (faceAndDpad & 0x10) b |= pad::X;
	if (faceAndDpad & 0x20) b |= pad::A;
	if (faceAndDpad & 0x40) b |= pad::B;
	if (faceAndDpad & 0x80) b |= pad::Y;
	if (shoulders & 0x01) b |= pad::LB;
	if (shoulders & 0x02) b |= pad::RB;
	if (shoulders & 0x10) b |= pad::BACK;
	if (shoulders & 0x20) b |= pad::START;
	if (shoulders & 0x40) b |= pad::LS;
	if (shoulders & 0x80) b |= pad::RS;
	out.buttons = b;
}

inline bool parse(uint16_t pid, const uint8_t* r, size_t len, size_t reportSize, PadState& out) {
	if (len < 10) return false;
	if (isDualSense(pid)) {
		if (r[0] == 0x01 && reportSize == 64) {
			fill(r + 1, r[8], r[9], r[5], r[6], out);
			return true;
		}
		if (r[0] == 0x31 && len >= 12) {
			fill(r + 2, r[9], r[10], r[6], r[7], out);
			return true;
		}
		if (r[0] == 0x01) {
			fill(r + 1, r[5], r[6], r[8], r[9], out);
			return true;
		}
		return false;
	}
	if (r[0] == 0x01) {
		fill(r + 1, r[5], r[6], r[8], r[9], out);
		return true;
	}
	if (r[0] == 0x11 && len >= 12) {
		fill(r + 3, r[7], r[8], r[10], r[11], out);
		return true;
	}
	return false;
}

}
