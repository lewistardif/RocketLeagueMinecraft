# Third-party notices

`rl_car_core` is new Rust code; no source files were copied. Its *model*, though, closely follows
the algorithms of the projects below, closely enough to reproduce their trajectories almost
exactly. That makes it at least arguably a derivative work, so their notices are kept here as their
licenses ask. Both licenses allow this kind of reuse, including commercially.

None of the projects below ship with this repository's runtime. RocketSim and Bullet are fetched
only to build the validation oracle (`oracle/build.sh`).

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

## Not used

- **RLUtilities** (GPL-3.0): consulted only to check its license; **no code or code structure was
  taken from it**. RocketSim's dodge model credits RLUtilities' public documentation. The half-flip in
  `maneuvers.rs` is a plain input recipe written from how the maneuver is played.
- **Rocket League** assets, binaries and arena meshes: not used, not included, not required.
