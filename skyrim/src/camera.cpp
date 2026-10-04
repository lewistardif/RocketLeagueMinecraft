#include "camera.h"

namespace camera {

namespace {

constexpr float kRadToDeg = 57.2957795f;

RE::NiPoint3 g_eye;
bool g_eyeValid = false;
RE::NiMatrix3 g_ideal;        // what we render with
RE::NiMatrix3 g_idealNoRoll;  // what Skyrim makes of the angles we give the player
bool g_idealValid = false;
// Skyrim's camera-root axis convention, learnt before we take its rotation over: for each column of
// its matrix, which of (+/-) forward / up / right it is.
std::array<std::array<int, 6>, 3> g_votes{};
int g_samples = 0;
std::array<int, 3> g_axisMap{0, 1, 2};
bool g_validated = false, g_rejected = false;
bool g_fovSaved = false;
float g_savedFov = 0;

RE::NiPoint3 col(const RE::NiMatrix3& m, int c) { return {m.entry[0][c], m.entry[1][c], m.entry[2][c]}; }

float angleDeg(RE::NiPoint3 a, RE::NiPoint3 b) {
	float la = a.Length(), lb = b.Length();
	if (la < 1e-6f || lb < 1e-6f) return 180.0f;
	return std::acos(std::clamp(a.Dot(b) / (la * lb), -1.0f, 1.0f)) * kRadToDeg;
}

// NiCamera convention: column 0 = view direction, 1 = up, 2 = right.
RE::NiMatrix3 fromBasis(const RE::NiPoint3& f, const RE::NiPoint3& u, const RE::NiPoint3& r) {
	RE::NiMatrix3 m;
	const RE::NiPoint3 c[3] = {f, u, r};
	for (int j = 0; j < 3; j++) m.entry[0][j] = c[j].x, m.entry[1][j] = c[j].y, m.entry[2][j] = c[j].z;
	return m;
}

RE::NiMatrix3 fromAngles(float heading, float pitch) {
	float sh = std::sin(heading), ch = std::cos(heading), sp = std::sin(pitch), cp = std::cos(pitch);
	RE::NiPoint3 f{sh * cp, ch * cp, -sp};
	RE::NiPoint3 r{ch, -sh, 0.0f};
	return fromBasis(f, r.Cross(f), r);
}

void applyRotation(RE::NiAVObject* root) {
	if (!g_validated) return;
	const RE::NiPoint3 f = col(g_ideal, 0), u = col(g_ideal, 1), r = col(g_ideal, 2);
	const std::array<RE::NiPoint3, 6> cand{f, f * -1.0f, u, u * -1.0f, r, r * -1.0f};
	RE::NiMatrix3 m;
	for (int c = 0; c < 3; c++) {
		const auto& v = cand[g_axisMap[c]];
		m.entry[0][c] = v.x, m.entry[1][c] = v.y, m.entry[2][c] = v.z;
	}
	root->local.rotate = m;
	root->world.rotate = m;
}

void pin(RE::PlayerCamera* camera) {
	auto* root = camera->cameraRoot.get();
	if (!root->parent || root->parent->world.translate.Length() < 0.001f) root->local.translate = g_eye;
	root->world.translate = g_eye;
	camera->GetRuntimeData2().pos = g_eye;
	if (auto* sky = RE::Sky::GetSingleton(); sky && sky->root) {
		sky->root->local.translate = g_eye;
		sky->root->world.translate = g_eye;
	}
}

void vote(const RE::NiMatrix3& R) {
	if (g_validated || g_rejected) return;
	const RE::NiPoint3 f = col(g_idealNoRoll, 0), u = col(g_idealNoRoll, 1), r = col(g_idealNoRoll, 2);
	const std::array<RE::NiPoint3, 6> cand{f, f * -1.0f, u, u * -1.0f, r, r * -1.0f};
	for (int c = 0; c < 3; c++) {
		int best = -1;
		float bestAngle = 1e9f;
		for (int k = 0; k < 6; k++) {
			float a = angleDeg(col(R, c), cand[k]);
			if (a < bestAngle) bestAngle = a, best = k;
		}
		if (bestAngle < 6.0f) g_votes[c][best]++;
	}
	if (++g_samples < 240) return;
	bool ok = true;
	std::array<bool, 3> used{};
	for (int c = 0; c < 3; c++) {
		auto it = std::ranges::max_element(g_votes[c]);
		int k = int(it - g_votes[c].begin());
		ok &= *it > g_samples * 6 / 10 && !used[k / 2];
		used[k / 2] = true;
		g_axisMap[c] = k;
	}
	static constexpr const char* kNames[6] = {"+forward", "-forward", "+up", "-up", "+right", "-right"};
	g_validated = ok;
	g_rejected = !ok;
	logger::info("camera root axes: {} {} {} -> {}", kNames[g_axisMap[0]], kNames[g_axisMap[1]], kNames[g_axisMap[2]],
	             ok ? "the car camera now rolls too" : "inconsistent; heading and pitch only");
}

struct PlayerCameraUpdateHook {
	static void thunk(RE::PlayerCamera* self) {
		func(self);
		if (!g_eyeValid || !self->cameraRoot) return;
		auto* root = self->cameraRoot.get();
		if (g_idealValid) {
			vote(root->world.rotate);
			applyRotation(root);
		}
		pin(self);
		RE::NiUpdateData update{};
		root->UpdateDownwardPass(update, 0);
	}
	static inline REL::Relocation<decltype(thunk)> func;
};

struct FirstPersonTranslationHook {
	static void thunk(RE::TESCameraState* self, RE::NiPoint3& out) {
		func(self, out);
		if (g_eyeValid) out = g_eye;
	}
	static inline REL::Relocation<decltype(thunk)> func;
};

}

void install() {
	// PlayerCamera::Update is called directly, not through the vtable: hook each `call` to it.
	const auto target = REL::Relocation<std::uintptr_t>{RELOCATION_ID(49852, 50784)}.address();
	const auto text = REL::Module::get().segment(REL::Segment::textx);
	const auto base = text.address();
	const auto* code = reinterpret_cast<const uint8_t*>(base);
	std::vector<std::uintptr_t> sites;
	for (size_t i = 0; i + 5 <= text.size(); i++) {
		if (code[i] != 0xE8) continue;
		int32_t rel;
		std::memcpy(&rel, code + i + 1, 4);
		if (base + i + 5 + static_cast<std::intptr_t>(rel) == target) sites.push_back(base + i);
	}
	auto& trampoline = SKSE::GetTrampoline();
	for (auto site : sites) PlayerCameraUpdateHook::func = trampoline.write_call<5>(site, PlayerCameraUpdateHook::thunk);
	logger::info("PlayerCamera::Update: hooked {} call site(s)", sites.size());

	REL::Relocation<std::uintptr_t> fpVtbl{RE::VTABLE_FirstPersonState[0]};
	FirstPersonTranslationHook::func = fpVtbl.write_vfunc(0x5, FirstPersonTranslationHook::thunk);
}

void set(RE::PlayerCharacter* player, const RE::NiPoint3& eye, const space::CameraBasis& b, float skyrimFov) {
	auto p = [](space::V3 v) { return RE::NiPoint3{float(v.x), float(v.y), float(v.z)}; };
	g_eye = eye;
	g_eyeValid = true;
	g_ideal = fromBasis(p(b.forward), p(b.up), p(b.right));
	g_idealNoRoll = fromAngles(float(b.heading), float(b.pitch));
	g_idealValid = true;
	player->data.angle.z = float(b.heading);
	player->data.angle.x = float(b.pitch);
	auto* camera = RE::PlayerCamera::GetSingleton();
	if (!camera) return;
	auto& data = camera->GetRuntimeData2();
	if (!g_fovSaved) {
		g_savedFov = data.worldFOV;
		g_fovSaved = true;
		logger::info("camera FOV {:.1f} -> {:.1f} (Rocket League's)", g_savedFov, skyrimFov);
	}
	data.worldFOV = skyrimFov;
	if (!camera->cameraRoot) return;
	auto* root = camera->cameraRoot.get();
	applyRotation(root);
	pin(camera);
	RE::NiUpdateData update{};
	root->UpdateDownwardPass(update, 0);
}

void release() {
	g_eyeValid = false;
	g_idealValid = false;
	auto* camera = RE::PlayerCamera::GetSingleton();
	if (g_fovSaved && camera) camera->GetRuntimeData2().worldFOV = g_savedFov;
	g_fovSaved = false;
}

}
