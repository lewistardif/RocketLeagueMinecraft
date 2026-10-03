#pragma once
#include <windows.h>
#include <cstdint>
#include <cstring>
#include <type_traits>
#include "main.h"

typedef int Any;
typedef unsigned int Hash;
typedef int Entity;
typedef int Ped;
typedef int Vehicle;
typedef int Object;
typedef int Player;
typedef int Cam;
typedef int Pickup;
typedef int Blip;
typedef int FireId;
typedef int Interior;

#pragma pack(push, 1)
struct Vector3 {
	float x; DWORD _px;
	float y; DWORD _py;
	float z; DWORD _pz;
};
#pragma pack(pop)
static_assert(sizeof(Vector3) == 24, "script Vector3 is 24 bytes");

template <typename T> inline void nativePushT(T v) {
	static_assert(sizeof(T) <= 8, "native arguments are 64-bit slots");
	UINT64 u = 0;
	std::memcpy(&u, &v, sizeof(T));
	nativePush64(u);
}

template <typename R, typename... A> inline R invoke(UINT64 hash, A... args) {
	nativeInit(hash);
	(nativePushT(args), ...);
	if constexpr (std::is_void_v<R>) {
		nativeCall();
	} else {
		return *reinterpret_cast<R*>(nativeCall());
	}
}
