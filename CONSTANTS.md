# Constants and their sources

Every number in `crates/rl_car_core/src/consts.rs` and `config.rs` is a fact about the game. Values
were taken from **RocketSim** (pinned commit
[`c2baacb`](https://github.com/ZealanL/RocketSim/tree/c2baacb8f4b441dd8505e63c2aeb5a1679b60b02), MIT) and
cross-checked against the **[RLBot wiki: Useful Game Values](https://github.com/RLBot/RLBot/wiki/Useful-Game-Values)**.
Every value was also checked end to end: the core reproduces RocketSim trajectories to within
hundredths of a uu (see [`validation/REPORT.md`](validation/REPORT.md)).

File references (`RLConst.h:12`, etc.) are relative to RocketSim's `src/` directory. The Bullet ones
(`libsrc/...`) point to RocketSim's modified copy of Bullet 3.24.

Units: **uu** = Unreal unit (≈ 1 cm). **bt** = the internal Bullet scale the game simulates in, where
1 bt = 50 uu (`BulletLink.h:12,15`).

## Spec values that were checked

The task listed some values to verify. Results:

| Value given | Verified value | Source | Notes |
|---|---|---|---|
| gravity ~650 uu/s² | **650** | `RLConst.h:12`, RLBot wiki | |
| max speed w/o boost ~1410 | **1410** | `RLConst.h:447` (drive torque curve hits 0 at 1410), RLBot wiki | The validation measured 1409.9 |
| boost accel ~991.67 | **991.667 ground / 1058.333 air** | `RLConst.h:59-60`, RLBot wiki | Air boost is stronger; the spec only listed the ground value |
| supersonic / max 2300 | **2300 max, 2200 supersonic** | `RLConst.h:53,69`, RLBot wiki | Supersonic is kept down to 2100 for up to 1 s (`:73,76`) |
| boost use ~33.3/s | **100/3 per s**, tank 0–100 | `RLConst.h:56-57`, RLBot wiki | Minimum boost duration 0.1 s (`:58`); spawn amount 33.3 (`:61`) |
| flip window ~1.25 s | **1.25 s after the jump ends** | `RLConst.h:99`, `Car.cpp:698-704` | The timer counts from when the jump *stops* (button released or 0.2 s reached), not from takeoff |
| max angular velocity ~5.5 rad/s | **5.5** | `RLConst.h:66`, RLBot wiki | Clamped after every tick |

## World and body

| Constant | Value | Source |
|---|---|---|
| Physics tick | 120 Hz | RocketSim `Arena` default tick rate; RLBot |
| Gravity | (0, 0, −650) uu/s² | `RLConst.h:12` |
| Car mass | 180 (bt mass units) | `RLConst.h:28`; RLBot wiki "Car mass: 180" |
| Car speed cap | 2300 uu/s | `RLConst.h:53`; `Car.cpp:190-203` (clamp) |
| Angular speed cap | 5.5 rad/s | `RLConst.h:66`; `Car.cpp:198` |
| Car–world friction / restitution | 0.3 / 0.3 | `RLConst.h:37-38`; overridden per contact in `Arena.cpp` (`_BtCallback_OnCarWorldCollision`) |
| Car spawn rest height | 17 uu | `RLConst.h:141` (RLBot wiki rest elevation: Octane 17.01) |
| Inertia | solid box of the effective hitbox, mass 180 | `Car.cpp:218-222` (`btBoxShape::calculateLocalInertia`) |
| Gyroscopic forces | disabled | `Car.cpp:237-238` |

## Throttle, brake, steering, powerslide

| Constant | Value | Source |
|---|---|---|
| Throttle torque | 180 × 400 | `RLConst.h:84` (→ 1600 uu/s² at standstill) |
| Brake torque | 180 × (14.25 + 1/3) | `RLConst.h:85` (→ 3500 uu/s²; RLBot wiki "braking −3500") |
| Coasting brake factor | 0.15 | `RLConst.h:88` (→ 525 uu/s²; RLBot wiki "coasting −525") |
| Full-stop speed | 25 uu/s | `RLConst.h:87` |
| Braking no-throttle threshold | 0.01 uu/s | `RLConst.h:89` |
| Throttle deadzone | 0.001 | `RLConst.h:90` |
| Air throttle accel | 200/3 uu/s² (forward and reverse) | `RLConst.h:92`; RLBot wiki "≈66.667". **Known difference:** RLBot's Jumping Physics page gives 33.334 uu/s² for *reverse* air throttle. RocketSim (and so this core) uses 66.667 both ways. Not changed, because the oracle defines correctness here |
| Drive torque vs speed curve | (0,1) (1400,0.1) (1410,0) | `RLConst.h:447-453` |
| Max steer angle vs speed | (0,0.53356) (500,0.31930) (1000,0.18203) (1500,0.10570) (1750,0.08507) (3000,0.03454) | `RLConst.h:418-427` |
| Steer curve (three-wheel) | (0,0.342473) (2300,0.034837) | `RLConst.h:429-434` |
| Powerslide steer angle | (0,0.39235) (2500,0.12610) | `RLConst.h:438-443` |
| Powerslide rise / fall rate | 5 / 2 per s | `RLConst.h:81-82` |
| Lateral friction vs slip | (0,1) (1,0.2) | `RLConst.h:463-468` |
| Lateral friction vs slip (three-wheel) | (0,0.30) (1,0.25) | `RLConst.h:470-475` |
| Longitudinal friction curve | empty → 1 | `RLConst.h:477-481` |
| Handbrake lateral factor | 0.1 | `RLConst.h:483-487` |
| Handbrake longitudinal factor | (0,0.5) (1,0.9) | `RLConst.h:489-494` |
| Non-sticky friction vs normal.z | (0,0.1) (0.7075,0.5) (1,1) | `RLConst.h:455-461` |
| Tire friction scale | mass / 3 | `btVehicleRL.cpp:308` |
| Side friction damping | 0.2 | Bullet `resolveSingleBilateral`, `libsrc/.../btContactConstraint.cpp:141` |
| Rolling (brake) friction gain | 113.73963 | `btVehicleRL.cpp:362` |
| Sticky force | −650 × mass × (0.5 + (1 − \|n.z\|) if throttling or > 25 uu/s); 0.5 → 0 for three-wheel cars | `Car.cpp:485-496` |

## Suspension

| Constant | Value | Source |
|---|---|---|
| Stiffness | 500 | `RLConst.h:161` |
| Damping compression / relaxation | 25 / 40 | `RLConst.h:162-163` |
| Force scale front / back | 35.75 / 54.265 | `RLConst.h:158-159` |
| Max travel | 12 uu | `RLConst.h:164` |
| Raycast subtraction | 0.05 bt | `RLConst.h:165`; used in `btVehicleRL.cpp:126,182` |
| Bottom-out push-back | Bullet `resolveSingleCollision`, erp 0.2, split across 4 wheels | `btVehicleRL.cpp:181-197` |
| Wheel radii, rest lengths, attachment points | per preset | `Sim/Car/CarConfig/CarConfig.cpp:42-84` |

## Jump, double jump, dodge

| Constant | Value | Source |
|---|---|---|
| Jump impulse | 875/3 ≈ 291.667 uu/s | `RLConst.h:95`; [RLBot Jumping Physics](https://github.com/RLBot/RLBot/wiki/Jumping-Physics): "292" |
| Jump hold accel | 4375/3 ≈ 1458.33 uu/s² | `RLConst.h:94`; RLBot Jumping Physics: "292 over 0.2 s" (= 1460) |
| Jump min / max hold time | 0.025 / 0.2 s | `RLConst.h:96,98` |
| Jump accel scale before min time | 0.62 | `Car.cpp:585` |
| Jump reset time pad | 1/40 s | `RLConst.h:97` |
| Double jump impulse | 875/3 uu/s | `RLConst.h:95`; `Car.cpp:775`; RLBot wiki "≈291.667" |
| Double-jump / dodge window | 1.25 s after the jump ends | `RLConst.h:99`; RLBot Jumping Physics: "between 1.25 and 1.45 seconds" after takeoff, depending on hold time |
| Dodge deadzone | \|pitch\|+\|yaw\|+\|roll\| ≥ 0.5 | `CarConfig.h:37` |
| Dodge initial velocity | 500 uu/s | `RLConst.h:109` |
| Dodge speed scaling forward / side / backward | 1.0 / 1.9 / 2.5 | `RLConst.h:112-114` |
| Backward dodge X scale | 16/15 | `RLConst.h:115` |
| Flip torque X (roll) / Y (pitch) | 260 / 224 rad/s² | `RLConst.h:110-111` |
| Flip torque time | 0.65 s | `RLConst.h:105` |
| Flip pitch-lock extra time | 0.3 s | `RLConst.h:108` |
| Flip Z damping | ×0.65 per tick from 0.15 s while falling (or until 0.21 s) | `RLConst.h:102-104`; `Car.cpp:784-790` |

## Aerial control

| Constant | Value | Source |
|---|---|---|
| Torque (pitch, yaw, roll) | (130, 95, 400) | `RLConst.h:150` |
| Damping (pitch, yaw, roll) | (30, 20, 50) | `RLConst.h:151` |
| Torque unit scale | 2π / 65536 × 1000 | `RLConst.h:124` |
| → max angular accel pitch / yaw / roll | 12.46 / 9.11 / 38.35 rad/s² | derived; RLBot wiki: 12.46 / 9.11 / 38.34 |
| Auto-flip impulse / torque / time | 200 / 50 / 0.4 s | `RLConst.h:126-128` |
| Auto-flip thresholds | normal.z > √½, \|roll\| > 2.8 | `RLConst.h:129-130` |
| Auto-roll force / torque | 100 / 80 | `RLConst.h:132-133` |

## Boost

| Constant | Value | Source |
|---|---|---|
| Ground / air accel | 2975/3 / 3175/3 uu/s² | `RLConst.h:59-60`; RLBot wiki 991.666 / 1058.333 |
| Use rate | 100/3 per s | `RLConst.h:57`; RLBot wiki 33.3 |
| Minimum boost time | 0.1 s | `RLConst.h:58` |
| Spawn amount | 100/3 | `RLConst.h:61` |
| Recharge (mutator, off by default) | 10/s after 0.25 s | `RLConst.h:63-64` |

## Hitbox presets

Full sizes and offsets (uu) from `Sim/Car/CarConfig/CarConfig.cpp:20-40`:

| Preset | Size (L × W × H) | Offset (x, y, z) | Wheel radius F / B |
|---|---|---|---|
| Octane | 120.507 × 86.6994 × 38.6591 | 13.8757, 0, 20.755 | 12.5 / 15 |
| Dominus | 130.427 × 85.7799 × 33.8 | 9.0, 0, 15.75 | 12 / 13.5 |
| Plank (Batmobile) | 131.32 × 87.1704 × 31.8944 | 9.00857, 0, 12.0942 | 12.5 / 17 |
| Breakout | 133.992 × 83.021 × 32.8 | 12.5, 0, 11.75 | 13.5 / 15 |
| Hybrid (Venom) | 129.519 × 84.6879 × 36.6591 | 13.8757, 0, 20.755 | 12.5 / 15 |
| Merc | 123.22 × 79.2103 × 44.1591 | 11.3757, 0, 21.505 | 15 / 15 |
| Psyclops (three-wheel) | 120.641 × 86.8334 × 38.7931 | 13.8757, 0, 15.0 | 12.5 / 15 |

**Discrepancy with commonly quoted hitboxes.** RocketSim notes (`CarConfig.cpp:9-18`) that the sizes
the game reports through `GetLocalCollisionExtent()` (the numbers community tables are built from,
e.g. HalfwayDead's car-body spreadsheet linked from the RLBot wiki) are slightly larger than the ones
used in simulation. Its values are the ones that reproduce the game's inertia tensor. We follow
RocketSim because it is the oracle. The RLBot "Useful Game Values" page itself lists only rest
elevations (Octane 17.01, Dominus 17.05, …), which match the 17 uu rest height here.

**Effective box.** Bullet builds the box as `half − 0.04 bt` and then lowers the margin to 10% of the
smallest half-extent without adding the difference back. The box that actually collides is therefore
slightly smaller: −0.07 uu per side for the Octane, up to −0.36 uu for the Breakout. This also changes
the inertia tensor and the contact threshold (`CarConfig::effective_half_extents_bt`). It came from
Bullet's `btBoxShape` constructor and was confirmed against the oracle. The validation error dropped
roughly 100× once it was modelled.

## Contact solver (Bullet settings as configured by RocketSim)

| Setting | Value | Source |
|---|---|---|
| Iterations | 10 | Bullet `btContactSolverInfo` default |
| Split-impulse ERP (`erp2`) | 0.8 | `Arena.cpp:475` |
| Split-impulse threshold | 1e30 (always split) | `Arena.cpp:474` |
| Split-impulse turn ERP | 0.1 | Bullet default |
| Restitution velocity threshold | 0.2 bt/s | Bullet default |
| Warm-starting factor | 0.85 | Bullet default |
| Friction directions | 1, velocity-aligned | Bullet default solver mode |
| Contact breaking threshold | 0.02 × (box bounding radius + offset length) bt (≈ 2.03 uu Octane) | `btCollisionDispatcher.cpp:76-80`, `btCollisionShape.cpp:147-156` |
| Points per manifold | 4; one new point per surface per tick; never merged | `btPersistentManifold.cpp` (RocketSim's `getCacheEntry` returns −1) |
| Speculative contacts | approaching velocity removed, no gap closure | `btSequentialImpulseConstraintSolver.cpp`, "ROCKETSIM CHANGE" in `setupContactConstraint` |
