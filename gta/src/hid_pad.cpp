#include "hid_pad.h"
#include "sony_pad.h"
#include <windows.h>
#include <atomic>
#include <mutex>
#include <setupapi.h>
#include <string>
#include <thread>
#include <vector>
extern "C" {
#include <hidsdi.h>
}

namespace hidpad {
namespace {

std::mutex g_lock;
PadState g_state;
DWORD g_lastReport = 0;
std::atomic<bool> g_run{false};
std::thread g_thread;

HANDLE openPad(uint16_t& pid, DWORD& reportLen) {
	GUID guid;
	HidD_GetHidGuid(&guid);
	HDEVINFO set = SetupDiGetClassDevsA(&guid, nullptr, nullptr, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
	if (set == INVALID_HANDLE_VALUE) return INVALID_HANDLE_VALUE;
	HANDLE found = INVALID_HANDLE_VALUE;
	SP_DEVICE_INTERFACE_DATA iface{};
	iface.cbSize = sizeof iface;
	for (DWORD i = 0; found == INVALID_HANDLE_VALUE && SetupDiEnumDeviceInterfaces(set, nullptr, &guid, i, &iface); i++) {
		DWORD need = 0;
		SetupDiGetDeviceInterfaceDetailA(set, &iface, nullptr, 0, &need, nullptr);
		if (!need) continue;
		std::vector<char> buf(need);
		auto* detail = reinterpret_cast<SP_DEVICE_INTERFACE_DETAIL_DATA_A*>(buf.data());
		detail->cbSize = sizeof(SP_DEVICE_INTERFACE_DETAIL_DATA_A);
		if (!SetupDiGetDeviceInterfaceDetailA(set, &iface, detail, need, nullptr, nullptr)) continue;
		HANDLE h = CreateFileA(detail->DevicePath, GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_EXISTING,
		                       FILE_FLAG_OVERLAPPED, nullptr);
		if (h == INVALID_HANDLE_VALUE)
			h = CreateFileA(detail->DevicePath, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_EXISTING, FILE_FLAG_OVERLAPPED,
			                nullptr);
		if (h == INVALID_HANDLE_VALUE) continue;
		HIDD_ATTRIBUTES attr{};
		attr.Size = sizeof attr;
		PHIDP_PREPARSED_DATA pre = nullptr;
		HIDP_CAPS caps{};
		bool ok = HidD_GetAttributes(h, &attr) && sony::supported(attr.VendorID, attr.ProductID) && HidD_GetPreparsedData(h, &pre);
		if (ok) {
			ok = HidP_GetCaps(pre, &caps) == HIDP_STATUS_SUCCESS && caps.UsagePage == 0x01 && caps.Usage == 0x05;
			HidD_FreePreparsedData(pre);
		}
		if (ok) {
			pid = attr.ProductID;
			reportLen = caps.InputReportByteLength;
			found = h;
		} else {
			CloseHandle(h);
		}
	}
	SetupDiDestroyDeviceInfoList(set);
	return found;
}

void run() {
	while (g_run) {
		uint16_t pid = 0;
		DWORD len = 0;
		HANDLE h = openPad(pid, len);
		if (h == INVALID_HANDLE_VALUE) {
			for (int i = 0; i < 20 && g_run; i++) Sleep(100);
			continue;
		}
		std::vector<uint8_t> report(len < 64 ? 64 : len);
		OVERLAPPED ov{};
		ov.hEvent = CreateEventA(nullptr, TRUE, FALSE, nullptr);
		while (g_run) {
			ResetEvent(ov.hEvent);
			DWORD got = 0;
			if (!ReadFile(h, report.data(), DWORD(report.size()), nullptr, &ov) && GetLastError() != ERROR_IO_PENDING) break;
			if (WaitForSingleObject(ov.hEvent, 500) != WAIT_OBJECT_0) {
				CancelIo(h);
				continue;
			}
			if (!GetOverlappedResult(h, &ov, &got, FALSE)) break;
			PadState s;
			if (sony::parse(pid, report.data(), got, len, s)) {
				std::lock_guard<std::mutex> lk(g_lock);
				g_state = s;
				g_lastReport = GetTickCount();
			}
		}
		CancelIo(h);
		CloseHandle(ov.hEvent);
		CloseHandle(h);
	}
}

}

void start() {
	if (g_run.exchange(true)) return;
	g_thread = std::thread(run);
}

bool read(PadState& out) {
	std::lock_guard<std::mutex> lk(g_lock);
	if (!g_lastReport || GetTickCount() - g_lastReport > 1000) return false;
	out = g_state;
	return true;
}

void stop() {
	if (!g_run.exchange(false)) return;
	if (g_thread.joinable()) g_thread.join();
}

}
