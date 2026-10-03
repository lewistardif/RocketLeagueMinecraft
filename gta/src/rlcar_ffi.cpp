#include "rlcar_ffi.h"
#include <type_traits>
#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif

namespace ffi {

bool load(const std::string& path, Api& api, std::string& error) {
#ifdef _WIN32
	HMODULE lib = LoadLibraryA(path.c_str());
	auto sym = [&](const char* n) { return reinterpret_cast<void*>(GetProcAddress(lib, n)); };
#else
	void* lib = dlopen(path.c_str(), RTLD_NOW);
	auto sym = [&](const char* n) { return dlsym(lib, n); };
#endif
	if (!lib) {
		error = "cannot load " + path;
		return false;
	}
	bool ok = true;
	auto get = [&](auto& fn, const char* name) {
		fn = reinterpret_cast<std::remove_reference_t<decltype(fn)>>(sym(name));
		if (!fn) {
			error += std::string(error.empty() ? "missing symbols:" : "") + " " + name;
			ok = false;
		}
	};
	get(api.abi_version, "rlcar_abi_version");
	get(api.cbworld_new, "rlcar_cbworld_new");
	get(api.cbworld_free, "rlcar_cbworld_free");
	get(api.car_new, "rlcar_car_new");
	get(api.car_free, "rlcar_car_free");
	get(api.car_reset, "rlcar_car_reset");
	get(api.car_translate, "rlcar_car_translate");
	get(api.car_step_cb, "rlcar_car_step_cb");
	get(api.car_advance_cb, "rlcar_car_advance_cb");
	get(api.car_alpha, "rlcar_car_alpha");
	get(api.car_pose, "rlcar_car_pose");
	get(api.car_preset, "rlcar_car_preset");
	get(api.car_set_unlimited_boost, "rlcar_car_set_unlimited_boost");
	get(api.car_set_config, "rlcar_car_set_config");
	get(api.car_config, "rlcar_car_config");
	get(api.default_config, "rlcar_default_config");
	get(api.preset_hitbox, "rlcar_preset_hitbox");
	get(api.camera_new, "rlcar_camera_new");
	get(api.camera_free, "rlcar_camera_free");
	get(api.camera_reset, "rlcar_camera_reset");
	get(api.camera_translate, "rlcar_camera_translate");
	get(api.camera_update, "rlcar_camera_update");
	get(api.camera_preset, "rlcar_camera_preset");
	if (ok && api.abi_version() != ABI_VERSION) {
		error = "rl_car_ffi ABI " + std::to_string(api.abi_version()) + ", plugin expects " + std::to_string(ABI_VERSION) +
		        "; rebuild with gta\\build.bat";
		ok = false;
	}
	return ok;
}

}  // namespace ffi
