# Third-party notices

`rl_car_core` is new Rust code; no source files were copied. Its *model*, though, closely follows
the algorithms of the projects below, closely enough to reproduce their trajectories almost
exactly. That makes it at least arguably a derivative work, so their notices are kept here as their
licenses ask. Both licenses allow this kind of reuse, including commercially.

None of the projects below ship with this repository's runtime. RocketSim and Bullet are fetched
only to build the validation oracle (`oracle/build.sh`).

The optional car-model extractor (`tools/rl_assets/`) drives two external tools that you download
yourself; neither is included here: UModel / UE Viewer by Konstantin Nosov (MIT,
https://github.com/gildor2/UEViewer) and RL-UPKSuite by Martinii89
(https://github.com/Martinii89/RL-UPKSuite), whose `Core.dll` the `rldecrypt` wrapper links against.
The extracted Rocket League assets belong to Psyonix / Epic Games and are never part of this repository.

---

## RocketSim — https://github.com/ZealanL/RocketSim (MIT)

Source of the car model, the constants and the car/world contact behaviour (commit `c2baacb8`).

```
MIT License

Copyright (c) 2022 ZealanL

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Bullet Physics 3.24 (as modified by RocketSim) — https://github.com/bulletphysics/bullet3 (zlib)

Behaviour reproduced in `rl_car_core` (in re-written form): the sequential-impulse contact solver
with split impulse, the persistent contact manifold, the exponential-map transform integration,
the quaternion/matrix conversions, the box-shape margin rule, and `resolveSingleBilateral` /
`resolveSingleCollision`.

```
Bullet Continuous Collision Detection and Physics Library
Copyright (c) 2003-2006 Erwin Coumans  https://bulletphysics.org

This software is provided 'as-is', without any express or implied warranty.
In no event will the authors be held liable for any damages arising from the use of this software.
Permission is granted to anyone to use this software for any purpose,
including commercial applications, and to alter it and redistribute it freely,
subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim that you wrote the
   original software. If you use this software in a product, an acknowledgment in the product
   documentation would be appreciated but is not required.
2. Altered source versions must be plainly marked as such, and must not be misrepresented as being
   the original software.
3. This notice may not be removed or altered from any source distribution.
```

## SkyCraft — https://github.com/chasmlol/SkyCraft (MIT)

The Skyrim port (`skyrim/`) takes its Skyrim-side plumbing from SkyCraft's SKSE plugin: walking the
loaded Havok world for its triangles and transforms with fault-guarded shape reads
(`skyrim/src/havok_world.cpp`, from `Collision.cpp`), the player puppet, the `PlayerCamera::Update`
call-site hook and first-person translation hook (`puppet.cpp`, `camera.cpp`, from `Game.cpp`), the
input hooks (`input.cpp`, from `Input.cpp`), the convex-hull clipping (`sky_world.cpp`) and the CMake
setup. Those parts were adapted, not copied verbatim.

```
MIT License

Copyright (c) 2026 chasmlol

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## CommonLibSSE-NG — https://github.com/alandtse/CommonLibVR/tree/ng (GPL-3.0-or-later with exceptions)

`skyrim/extern/CommonLibSSE-NG` is a git submodule pinned to release 10.0.0 (`d61bca4`), the commit
SkyCraft builds with. It is not part of this repository's sources and keeps its own license
(`COPYING.txt`, `EXCEPTIONS.md`). Since 10.0.0 it is GPL-3.0-or-later with a Modding Exception and a
GPL-3.0 Linking Exception; earlier releases were MIT. `RLCar.dll` links it statically, so **a built
`RLCar.dll` is a combined work distributed under GPL-3.0-or-later**; this repository's own code
stays MIT (which is GPL-compatible), and its source is the corresponding source together with the
submodule. Its vcpkg dependencies (spdlog, {fmt}, DirectXTK, DirectXMath, xbyak, SimpleIni,
nlohmann-json, toml11, rapidcsv) are MIT or BSD-licensed and are fetched at build time.

## Not used

- **RLUtilities** (GPL-3.0): consulted only to check its license; **no code or code structure was
  taken from it**. RocketSim's dodge model credits RLUtilities' public documentation. The half-flip in
  `maneuvers.rs` is a plain input recipe written from how the maneuver is played.
- **Rocket League** assets, binaries and arena meshes: not used, not included, not required.
