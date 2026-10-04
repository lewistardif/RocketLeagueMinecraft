#include "input.h"
#include "hid_pad.h"
#include <atomic>
#include <xinput.h>

namespace input {

namespace {

std::atomic<bool> g_driving{false};

using XInputGetStateFn = DWORD(WINAPI*)(DWORD, XINPUT_STATE*);
XInputGetStateFn g_xinput = nullptr;

void loadXInput() {
	for (const char* dll : {"xinput1_4.dll", "xinput1_3.dll", "xinput9_1_0.dll"}) {
		if (HMODULE m = LoadLibraryA(dll)) {
			g_xinput = reinterpret_cast<XInputGetStateFn>(GetProcAddress(m, "XInputGetState"));
			if (g_xinput) return;
		}
	}
}

bool gameHasFocus() {
	DWORD pid = 0;
	GetWindowThreadProcessId(GetForegroundWindow(), &pid);
	return pid == GetCurrentProcessId();
}

constexpr uint32_t kDikEscape = 0x01, kDikConsole = 0x29;
constexpr uint32_t kPadStart = 0x0010;

bool menuKey(const RE::InputEvent* e) {
	if (e->GetEventType() != RE::INPUT_EVENT_TYPE::kButton) return false;
	auto code = static_cast<const RE::ButtonEvent*>(e)->GetIDCode();
	if (e->GetDevice() == RE::INPUT_DEVICE::kKeyboard) return code == kDikEscape || code == kDikConsole;
	if (e->GetDevice() == RE::INPUT_DEVICE::kGamepad) return code == kPadStart;
	return false;
}

struct MenuControlsHook {
	static RE::BSEventNotifyControl thunk(RE::MenuControls* self, RE::InputEvent* const* events, RE::BSTEventSource<RE::InputEvent*>* source) {
		if (!events || !*events || !g_driving || skyrimMenuOpen()) return func(self, events, source);
		std::vector<RE::InputEvent*> keep, all, next;
		for (auto* e = *events; e; e = e->next) {
			all.push_back(e);
			next.push_back(e->next);
			if (menuKey(e)) keep.push_back(e);
		}
		if (keep.empty()) return RE::BSEventNotifyControl::kContinue;
		for (size_t i = 0; i < keep.size(); i++) keep[i]->next = i + 1 < keep.size() ? keep[i + 1] : nullptr;
		RE::InputEvent* head = keep.front();
		auto result = func(self, &head, source);
		for (size_t i = 0; i < all.size(); i++) all[i]->next = next[i];
		return result;
	}
	static inline REL::Relocation<decltype(thunk)> func;
};

struct PlayerControlsHook {
	static RE::BSEventNotifyControl thunk(RE::PlayerControls* self, RE::InputEvent* const* events, RE::BSTEventSource<RE::InputEvent*>* source) {
		if (!g_driving || skyrimMenuOpen()) return func(self, events, source);
		return RE::BSEventNotifyControl::kContinue;
	}
	static inline REL::Relocation<decltype(thunk)> func;
};

}

void install() {
	loadXInput();
	hidpad::start();
	REL::Relocation<std::uintptr_t> menuVtbl{RE::VTABLE_MenuControls[0]};
	MenuControlsHook::func = menuVtbl.write_vfunc(0x1, MenuControlsHook::thunk);
	REL::Relocation<std::uintptr_t> playerVtbl{RE::VTABLE_PlayerControls[0]};
	PlayerControlsHook::func = playerVtbl.write_vfunc(0x1, PlayerControlsHook::thunk);
}

void setDriving(bool on) { g_driving = on; }

InputState read() {
	InputState s;
	if (gameHasFocus())
		for (int vk = 1; vk < 256; vk++) s.keys[size_t(vk)] = (GetAsyncKeyState(vk) & 0x8000) != 0;
	XINPUT_STATE xs{};
	if (g_xinput && g_xinput(0, &xs) == ERROR_SUCCESS) {
		auto& g = xs.Gamepad;
		s.pad.connected = true;
		s.pad.buttons = g.wButtons;
		s.pad.lt = g.bLeftTrigger / 255.0f;
		s.pad.rt = g.bRightTrigger / 255.0f;
		auto ax = [](SHORT v) { return v < 0 ? v / 32768.0f : v / 32767.0f; };
		s.pad.lx = ax(g.sThumbLX), s.pad.ly = ax(g.sThumbLY), s.pad.rx = ax(g.sThumbRX), s.pad.ry = ax(g.sThumbRY);
	} else {
		hidpad::read(s.pad);
	}
	return s;
}

bool skyrimMenuOpen() {
	auto* ui = RE::UI::GetSingleton();
	if (!ui) return false;
	if (ui->IsMenuOpen(RE::LoadingMenu::MENU_NAME)) return true;
	for (const auto& menu : ui->menuStack)
		if (menu && menu->menuFlags.any(RE::UI_MENU_FLAGS::kPausesGame, RE::UI_MENU_FLAGS::kUsesCursor)) return true;
	return false;
}

}
