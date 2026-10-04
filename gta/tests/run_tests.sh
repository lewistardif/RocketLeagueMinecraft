#!/usr/bin/env bash
set -e
cd "$(dirname "$0")/../.."
cargo build -p rl_car_ffi --release
case "$(uname -s)" in
	MINGW*|MSYS*|CYGWIN*) LIB=target/release/rl_car_ffi.dll; EXTRA= ;;
	Darwin) LIB=target/release/librl_car_ffi.dylib; EXTRA= ;;
	*) LIB=target/release/librl_car_ffi.so; EXTRA=-ldl ;;
esac
g++ -std=c++20 -O2 -Wall -o target/rlcar_tests gta/tests/tests.cpp gta/src/ini.cpp gta/src/bindings.cpp \
	gta/src/settings.cpp gta/src/probe_world.cpp gta/src/rlcar_ffi.cpp gta/src/model.cpp $EXTRA
target/rlcar_tests "$LIB"
