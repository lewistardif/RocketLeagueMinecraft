#!/usr/bin/env bash
# Builds the RocketSim oracle (reference simulator for the validation harness).
# Requires: git, g++ (C++20). Fetches RocketSim at a pinned commit into oracle/RocketSim.
set -euo pipefail
cd "$(dirname "$0")"

ROCKETSIM_COMMIT=c2baacb8f4b441dd8505e63c2aeb5a1679b60b02

if [ ! -d RocketSim ]; then
  git clone https://github.com/ZealanL/RocketSim RocketSim
fi
git -C RocketSim fetch --depth 1 origin "$ROCKETSIM_COMMIT" 2>/dev/null || true
git -C RocketSim checkout -q "$ROCKETSIM_COMMIT"

mkdir -p build/obj
CXX=${CXX:-g++}
FLAGS="-O2 -std=c++20 -w -D_USE_MATH_DEFINES -IRocketSim/src"

# Compile each translation unit once (incremental), then link.
objs=()
while IFS= read -r src; do
  obj="build/obj/$(echo "$src" | tr '/' '_' ).o"
  if [ ! -f "$obj" ] || [ "$src" -nt "$obj" ]; then
    $CXX $FLAGS -c "$src" -o "$obj" &
  fi
  objs+=("$obj")
  # limit parallel jobs
  while [ "$(jobs -r | wc -l)" -ge 8 ]; do sleep 0.2; done
done < <(find RocketSim/src RocketSim/libsrc -name '*.cpp')
wait

$CXX $FLAGS -c oracle.cpp -o build/obj/oracle.o
$CXX -o build/rocketsim_oracle build/obj/oracle.o "${objs[@]}" -static
echo "built oracle/build/rocketsim_oracle"
