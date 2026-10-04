#include "plugin.h"

namespace {

void setupLog() {
	auto dir = SKSE::log::log_directory();
	if (!dir) return;
	auto sink = std::make_shared<spdlog::sinks::basic_file_sink_mt>((*dir / "RLCar.log").string(), true);
	auto log = std::make_shared<spdlog::logger>("global", std::move(sink));
	log->set_level(spdlog::level::info);
	log->flush_on(spdlog::level::info);
	spdlog::set_default_logger(std::move(log));
	spdlog::set_pattern("[%H:%M:%S.%e] [%l] %v");
}

void onMessage(SKSE::MessagingInterface::Message* msg) {
	switch (msg->type) {
		case SKSE::MessagingInterface::kDataLoaded: plugin::install(); break;
		case SKSE::MessagingInterface::kPreLoadGame: plugin::onLoading("loading a save"); break;
		case SKSE::MessagingInterface::kPostLoadGame:
		case SKSE::MessagingInterface::kNewGame: plugin::onLoading("game loaded"); break;
		default: break;
	}
}

}

SKSEPluginLoad(const SKSE::LoadInterface* skse) {
	SKSE::Init(skse, {.trampoline = true, .trampolineSize = 256});
	setupLog();
	logger::info("RL Car loading (runtime {})", skse->RuntimeVersion().string());
	SKSE::GetMessagingInterface()->RegisterListener(onMessage);
	return true;
}
