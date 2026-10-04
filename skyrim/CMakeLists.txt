cmake_minimum_required(VERSION 3.25)
project(RLCarSkyrim VERSION 0.1.0 LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
# CommonLibSSE-NG needs the DLL runtime (the GTA project links the static one: keep them apart).
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>DLL")
add_compile_definitions(WIN32_LEAN_AND_MEAN NOMINMAX _CRT_SECURE_NO_WARNINGS)

option(RLCAR_TESTS_ONLY "Build only the game-free tests (no CommonLibSSE-NG, no vcpkg)" OFF)

set(CORE_SOURCES src/ini.cpp src/bindings.cpp src/settings.cpp src/sky_world.cpp src/rlcar_ffi.cpp)

add_executable(tests tests/tests.cpp ${CORE_SOURCES})
target_include_directories(tests PRIVATE src)
if(MSVC)
	target_compile_options(tests PRIVATE /W3 /utf-8 /permissive-)
endif()

set(COMMONLIB_DIR "${CMAKE_CURRENT_SOURCE_DIR}/extern/CommonLibSSE-NG")
if(RLCAR_TESTS_ONLY OR NOT EXISTS "${COMMONLIB_DIR}/CMakeLists.txt")
	message(WARNING "CommonLibSSE-NG not used: building the tests only (git submodule update --init --recursive).")
	return()
endif()

set(BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(ENABLE_SKYRIM_VR OFF CACHE BOOL "" FORCE)
set(ENABLE_SKYRIM_SE ON CACHE BOOL "" FORCE)
set(ENABLE_SKYRIM_AE ON CACHE BOOL "" FORCE)
add_subdirectory("${COMMONLIB_DIR}" CommonLibSSE EXCLUDE_FROM_ALL)
if(NOT COMMAND add_commonlibsse_plugin)
	include("${COMMONLIB_DIR}/cmake/CommonLibSSE.cmake")
endif()

add_commonlibsse_plugin(
	RLCar
	NAME "RLCar"
	AUTHOR "RocketLeagueMinecraft"
	USE_ADDRESS_LIBRARY
	SOURCES
		src/main.cpp src/plugin.cpp src/havok_world.cpp src/puppet.cpp src/camera.cpp src/input.cpp src/visual.cpp src/interact.cpp
		src/hid_pad.cpp ${CORE_SOURCES}
)
target_include_directories(RLCar PRIVATE src)
target_precompile_headers(RLCar PRIVATE src/PCH.h)
target_link_libraries(RLCar PRIVATE setupapi hid)
if(MSVC)
	target_compile_options(RLCar PRIVATE /W3 /utf-8 /permissive- /Zc:preprocessor /MP)
	target_link_options(RLCar PRIVATE /DEBUG:FULL)
endif()
