# Rocket League car physics: an engine-agnostic rewrite (Rust + Bevy)

> **Unofficial fan project.** Not affiliated with, endorsed by, sponsored by, or connected to
> Psyonix or Epic Games. "Rocket League" is a trademark of Psyonix. No Rocket League assets, code or
> binaries are included or needed; the game is only a reference. The demo can optionally show the real
> car models, extracted locally from your own copy of the game (see below); those files are git-ignored
> and must never be committed or redistributed.

This repository reimplements Rocket League's **car** physics (driving, powerslide, the wall/ceiling
sticky force, boost, jumps, double jumps, dodges/flips, flip cancels, aerial control, suspension,
the six hitbox presets, car-vs-world collision) as:

1. **`crates/rl_car_core`**: a deterministic physics core with **zero dependencies** and no engine
   or renderer inside. The whole model is one pure function:
   `step(&CarState, &Controls, &dyn CollisionWorld, dt) -> CarState`.
2. **`crates/rl_car_bevy`**: a thin adapter that makes a drivable car in **Bevy 0.19**. It feeds the
   core with Bevy collision (avian3d spatial queries) and runs the 120 Hz loop.
3. **`crates/rl_car_validate`** + **`oracle/`**: a validation harness that runs the same inputs
   through **RocketSim** (the open-source reference simulator) and through the core, then compares
   the two trajectories tick by tick.
4. **`crates/rl_car_ffi`** + **`minecraft/`**: a C ABI over the core (for engines not written in
   Rust), and a **Minecraft (Fabric 26.3)** mod that drives the same core through it, with the
   ball, ball cam, and bumps and demolitions against cars and mobs. See
   [`minecraft/README.md`](minecraft/README.md).
5. **`gta/`**: a **GTA V** (PC, story mode only) Script Hook V plugin that drives the same core
   through the C ABI, with the ball, Rocket League's camera and ball cam, controller support, and
   bumps and demolitions against GTA's cars and pedestrians. See [`gta/README.md`](gta/README.md).
6. **`skyrim/`**: a **Skyrim Special Edition** (AE runtime) SKSE plugin that turns the Dragonborn into
   the car, driving on Skyrim's collision copied out of Havok, with the ball, Rocket League's camera
   and ball cam, and bumps and demolitions of Skyrim's NPCs. See [`skyrim/README.md`](skyrim/README.md).

The core also simulates **the ball** (RocketSim's soccar ball, stepped together with the cars:
`step_scene`), validated against RocketSim the same way as the car.

## Results

Across 59 scripted scenarios (straight boost, throttle cap, braking, turning at five speeds,
powerslides, all 7 hitbox presets, single/short/double jumps, 10 dodges, stall, half-flip, sustained
aerial pitch/yaw/roll, aerial take-off, wall and ceiling driving, ramps, wall hits, landings, the
turtle auto-flip, the side auto-roll), the core tracks RocketSim to:

| | max position error | max velocity error | max orientation error |
|---|---|---|---|
| driving, jumps, dodges, aerials, walls, ramps (53 scenarios) | **0.028 uu** (0.3 mm) | **0.012 uu/s** | **0.002°** |
| car body hitting the world (6 scenarios) | 1.2 uu | 20 uu/s | 0.8° |

No state flag (on ground / jumped / double-jumped / flipped) differs on any tick. The full table is
in [`validation/REPORT.md`](validation/REPORT.md). The larger body-contact errors all come from a car
resting flat on its roof or a side, where which of four equally deep corners gets picked each tick
depends on float noise. The documented tolerances are 0.25 uu / 0.25 uu/s / 0.05° for the first
group and 2.5 uu / 25 uu/s / 2° for body contact. `cargo test` enforces them.

Ball scenarios (drops, rolling, spin, wall and corner bounces, car hits, dodging into the ball, a
pinch against a wall, the ball landing on the roof, wheels touching the ball) are compared against
RocketSim with the ball in the world: 13 scenarios, ball within **0.03 uu** in all but one (wheels
on the ball, 0.4 uu over 120 ticks), the car within 0.03 uu while touching the ball.

The Bevy host was checked separately: driven through avian's spatial queries against the real
arena colliders, the car's trajectory is bit-identical to the analytic `PlaneWorld` for the same
geometry. The car also climbs the procedural quarter-pipe onto the wall
(`crates/rl_car_bevy/src/host_tests.rs`).

## Quick start

```bash
cargo run -p rl_car_bevy --release
```

| Keyboard | Gamepad | Action |
|---|---|---|
| W / S | RT / LT, left stick Y | throttle / reverse; pitch in the air |
| A / D | left stick X | steer; yaw in the air |
| Q / E | LB / RB | air roll left / right |
| Space | A | jump (hold = higher, press again = double jump / dodge in stick direction) |
| Left Shift | B | boost |
| Left Ctrl | X | powerslide; in the air, makes A/D (stick) air roll |
| H | | scripted half-flip (drive backwards first) |
| R / 1–7 | | reset / switch hitbox preset |
| T | | switch team colour (real car models only) |
| I | | infinite boost on / off (on by default, a temporary testing aid) |
| | right stick | swivel the camera around the car |
| middle mouse | R3 (right stick click) | rear camera (look behind) while held |
| C | | cycle Rocket League's camera presets (Default, Balanced, Wide, Custom, Legacy, Modern) |

`cargo run -p rl_car_bevy --release -- --autopilot <dir>` plays a scripted run (boost, front flip,
quarter-pipe, back wall, ceiling), saves four screenshots to `<dir>` and exits.
`-- --showcase <dir>` photographs every car parked, close up, and exits.

### The camera

The camera is Rocket League's own car camera and ball cam, rebuilt from the game's camera script
(`CameraState_Car_TA`, `CameraState_BallCam_TA` and `Camera_TA` in `TAGame.upk`,
`CameraStateBlender_X` in `ProjectX.upk`) and the tuned values of its camera archetypes, in [`crates/rl_car_core/src/camera.rs`](crates/rl_car_core/src/camera.rs). It is
engine-agnostic (Rocket League space, no dependencies); the Bevy demo and the Minecraft mod both
use it.

- **On the ground** it looks along the car's heading on the driving surface, 3° down, and smooths
  fast on the floor and slowly on walls. It stays upright: on walls and the ceiling the car turns
  sideways or upside down on screen, as in the game. Only 10% of the car's sideways lean rolls it.
- **In the air** it ignores the car's rotation. It looks at the car from where it is, like a
  camera on a string, so flips, spins and air rolls leave it pointing forward and it swings
  round to follow the direction of travel.
- **Speed** pulls it back (less with more Stiffness) and widens the FOV by up to 5° (10° when
  supersonic).
- **Swivel**: the right stick orbits it around the car, up to 123° to each side (99° at 2500 uu/s
  and above), 30° up and 49° down. It eases there at the Swivel Speed setting and comes back twice
  as fast. Rear Camera turns it 180°.
- **Ball cam** (hosts with a ball) turns towards the ball from exactly Height above the car, with
  no lag. While the ball is within 22° of level it keeps the Angle setting and only turns; from
  22° to 44° it eases into the ball's pitch and then follows 80% of it, raising its focus a
  little the steeper the ball. Rear Camera in ball cam is the car camera turned around.
  Switching either way eases out the difference over 0.5 s at Transition Speed 1, down to a cut
  at 2.
- **Settings**: Rocket League's FOV (horizontal, at 16:9), Distance, Height, Angle, Stiffness,
  Swivel Speed, Transition Speed and Invert Swivel Pitch, with the game's ranges and presets.

Not included: free look, camera shake, and the bob from the car body's visual suspension. The Bevy
demo has no ball, so it only shows the car camera. Like the game, the camera only keeps 10 uu above the floor and
can see through walls.

### Real Rocket League car models and boost (optional)

If you own Rocket League, `tools/rl_assets/extract.py` pulls the real car models out of your install
into `assets/rl/` (git-ignored), and the demo then uses them instead of the placeholder box car. One
car per hitbox preset: Octane, Dominus, Plank (the Plank-hitbox body `Body_Orion`), Breakout, Hybrid
(Venom), Merc and Psyclops, each in a blue and an orange variant, with the default OEM wheels.

It also extracts the default boost ("Standard"), shown on those cars exactly as the game builds it:
- **Flame cones:** the game's cone meshes, placed per car body by the same data the game uses (the
  `Parent_Boost_Mesh` overrides of each body's `BoostConeMesh` slots; twin cones on the Octane,
  Dominus, Hybrid and Merc). They are drawn with the boost material's own parameters and textures, and
  its shading is a line-by-line port of the material's compiled pixel shader, read from the game's
  shader cache (`crates/rl_car_bevy/src/shaders/boost_flame.wgsl`).
- **Smoke trail:** the `Boost_Painted_PS` particle system while boosting and `Drive_PS` (small exhaust
  puffs) while only throttling, emitted from each body's boost sockets. The particles are simulated
  from the extracted Cascade module data (spawn per distance travelled, lifetimes, sizes, colour and
  alpha curves, acceleration) and drawn with a port of `SmokePuff_Mat`'s compiled shader.
- Not reproduced: the boost's lens flare and the glow it puts on the car body (both are drawn by
  engine code that is not in the packages), and the sound.

**Sounds and effects** (also from your own install): `extract.py --wwiser <wwiser.pyz> --vgmstream <vgmstream-cli.exe>`
extracts the car's Wwise sounds (engine/exhaust, boost loop and tail, jump, double jump, dodge, in-air
whoosh, landings, tyres, body impacts and slide, supersonic enter + loop, empty-boost) and plays them
through a small Wwise-graph player (`src/audio.rs`) driven by the RTPCs the game's native code sets
(`Speed`, `RPM`, `Throttle_Input`, `WheelForwardSpeed`...). The engine RPM model is the one
reconstructed part (the game computes it natively from the engine profile). The FX step extracts the
supersonic speed streaks and wheel trails, jump/dodge smoke and ribbons, impact sparks, camera
shakes and rumble; their materials are ports of the game's compiled shaders (found with
`tools/rl_assets/shader_cache.py`). `--audio-log` logs sound events/levels, `--slowmo 0.1` slows the clock.
The Minecraft mod plays the same sounds and effects (Java ports of both; see
[`minecraft/README.md`](minecraft/README.md#sounds-and-effects)).

The placeholder box car (and a real car when the boost was not extracted) gets a simple flickering
flame cone instead.

```bash
python tools/rl_assets/extract.py --umodel <folder with umodel_64.exe> --upksuite <RL-UPKSuite release folder>
```

It needs Python 3 with `numpy` and `Pillow`, the .NET SDK, [UModel](https://www.gildor.org/en/projects/umodel)
and [RL-UPKSuite](https://github.com/Martinii89/RL-UPKSuite) (its decryptor, wrapped by the tiny CLI in
`tools/rl_assets/rldecrypt`). The game is read from `C:\Program Files\Epic Games\rocketleague`
unless `--game` says otherwise. The work folder (`target/rl_assets_work`) hard-links the game's
`Textures*.tfc` caches, so it must be on the same drive as the game.

What you get is the game's geometry and texture maps. For the Bevy demo the extractor rebuilds each
material for Bevy's PBR: team paint baked from the body's paint masks, clear coat, normal maps,
headlight/tail-light masks as emissive. Expect it to look close to the game, not identical. The
Minecraft mod goes further: it draws the cars with the game's own body, chassis and wheel material
shaders, decompiled from the game's shader cache and translated to GLSL. The extractor writes their
unbaked inputs (`materials.json` per car, `shading/`); see [`minecraft/README.md`](minecraft/README.md#car-materials). The team colours are approximations (the game picks them from
a palette texture that is not extracted).

Read Epic's EULA before doing this: it does not allow extracting the game's assets, and owning the game
does not change that. Keep the extracted files on your machine.

Other commands:

```bash
cargo test --workspace --release                       # core + adapter tests, and core vs committed RocketSim traces
cargo run -p rl_car_validate --release -- report       # rewrite validation/REPORT.md from committed traces
bash oracle/build.sh && cargo run -p rl_car_validate --release   # rebuild the oracle, regenerate traces, compare
```

The oracle build needs `git` and a C++20 `g++` (tested with MSYS2 UCRT64 g++ 15 on Windows).

## Using the core

```rust
use rl_car_core::*;

let world = PlaneWorld::soccar_box();           // or your own CollisionWorld
let mut sim = FixedStepper::new(CarState::new(HitboxPreset::Octane));
// every frame, whatever its length:
sim.advance(frame_seconds, &Controls { throttle: 1.0, boost: true, ..Default::default() }, &world);
let render_pose = (sim.previous, sim.current, sim.alpha()); // interpolate for smooth rendering
```

The host supplies collision by implementing two queries, both in Rocket League units and axes:

```rust
pub trait CollisionWorld {
    /// Closest hit of a segment (one wheel suspension ray per wheel, per tick).
    fn raycast(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<RayHit>;
    /// Car hitbox (oriented box) vs world: the deepest point per touching surface, tagged with a
    /// surface/collider id. The core caches these into persistent 4-point manifolds like the game.
    fn box_contacts(&self, obb: &Obb, margin: f32, out: &mut Vec<Contact>);
}
```

`PlaneWorld` (infinite half-spaces) ships with the core for tests. `rl_car_bevy/src/collision.rs` is
the reference host implementation on top of avian3d (about 100 lines).

## Method

**Clean-room rewrite.** RocketSim's source (MIT) was studied to pull out the model: the order of
operations in a tick, the formulas and the constants. It was then written again from scratch in
Rust. RocketSim relies on a modified Bullet for the rigid body, the suspension raycast vehicle and
the contact solver, so the parts of Bullet's behaviour that shape the car's motion were reproduced
as well:

- semi-implicit integration with exponential-map rotation updates;
- a sequential-impulse contact solver with split-impulse position correction and warm starting;
- persistent contact manifolds that add one point per surface per tick;
- Bullet's box "safe margin" rule, which makes the car's effective hitbox slightly smaller than its
  nominal size.

The last two were found by comparing against the oracle, not guessed. See
[`CONSTANTS.md`](CONSTANTS.md) for every value with its RocketSim file/line or RLBot wiki source.

**Oracle validation.** `oracle/oracle.cpp` links an unmodified RocketSim checkout (pinned commit),
creates a `THE_VOID` arena (no game meshes), adds the scenario's static Bullet planes, sets the car
state, plays a per-tick control script and writes the trajectory to CSV. `rl_car_validate` writes the
same scenarios, runs them through the core in an identical `PlaneWorld`, and compares position,
velocity, angular velocity, orientation, boost and state flags on every tick. The validation used
RocketSim's C++ API rather than its Python bindings because it needs to insert custom static
geometry (planes), which the bindings don't expose. Without that, the only geometry available would
be the game's arena meshes, which can't be redistributed.

**Determinism.** Fixed 120 Hz tick, `f32` everywhere, no clocks, no randomness, no threads, no
hash-ordered iteration. A host's contacts must be returned in a stable order (the Bevy host sorts
them by entity). `FixedStepper` makes the outcome independent of frame rate. Tests check bit-identical
replays and that 60 fps and 144 fps give the same result.

## Units and coordinates

The core works in Rocket League space: **uu** (≈ 1 cm), **Z up**, X forward / Y right / Z up for the
car, a **left-handed** world. The Bevy adapter converts in exactly one place,
[`crates/rl_car_bevy/src/convert.rs`](crates/rl_car_bevy/src/convert.rs):

```
bevy = (rl.x, rl.z, rl.y) / 100        // metres, Y up, right-handed
R_bevy = S * R_rl * S                  // S = the y<->z swap (a reflection: flips handedness)
```

The car model in Bevy uses local +X forward, +Z right, +Y up. A test (`yaw_sign_is_preserved`) pins
the yaw sign: steering right in the core turns the car to its right in Bevy.

## Scope and known limitations

- **Out of scope:** contacts between two simulated cars, boost pads, teams and scoring. Rocket
  League's bump and demolition rule is available (`bump`) for hosts to apply to their own entities.
- The real arena meshes are game assets and are not included. The demo arena is a soccar-sized box
  with procedurally built quarter-pipes and ramps. Bullet's convex-vs-mesh behaviour (one new point
  per *triangle* per tick) is approximated as one point per *collider* by the avian host.
- Only 120 Hz is validated (RocketSim also allows 15–120 Hz).
- When the car touches several surfaces in the same tick, the order in which their manifolds are
  solved can differ from Bullet's broadphase order. The effect is small and only shows when resting
  in corners.
- Bullet's solver reads a 1/60 s default timestep during the very first tick of a freshly created
  world. That one-off quirk is not reproduced: the oracle sets the steady-state value instead.

## Repository layout

```
crates/rl_car_core/       physics core (no dependencies)
  src/sim.rs              the tick: wheels, sticky force, jumps, dodges, air control, boost
  src/solver.rs           car-body contact solver
  src/manifold.rs         persistent contact manifolds
  src/world.rs            CollisionWorld trait, PlaneWorld
  src/stepper.rs          fixed 120 Hz accumulator
  src/maneuvers.rs        dodge / half-flip input scripts
  src/camera.rs           Rocket League's car camera and ball cam (presentation only)
  src/ball.rs             the ball: rules and state
  src/scene.rs            cars + ball in one tick (step_scene), car-ball contacts, extra hit impulse
  src/island.rs           multi-body contact solver with RocketSim's averaged ball contacts
  src/subsimplex.rs       Bullet's ray vs sphere cast (wheels on the ball)
  src/bump.rs             Rocket League's bump / demolition rule
crates/rl_car_bevy/       Bevy 0.19 demo + avian3d collision host
  src/visuals.rs          optional real car models (assets/rl), wheel anchors, mipmaps
  src/boost.rs            boost flame cones and smoke (the game's when extracted), simple flame
  src/shaders/            ports of the boost's flame and smoke material shaders
tools/rl_assets/          extractor for the real car models and boost from your own game install
  boost.py, ue3.py        the boost's meshes, placement, material and particle data
crates/rl_car_validate/   scenarios, comparison, report, regression test
crates/rl_car_ffi/        C ABI over the core + BoxWorld (seamless collision from voxel boxes)
minecraft/                Fabric mod: car entity, renderer, camera, networking, tests
gta/                      GTA V Script Hook V plugin (C++), build script, unit tests
skyrim/                   Skyrim SE/AE SKSE plugin (C++, CommonLibSSE-NG), build script, unit tests
oracle/                   RocketSim oracle (C++), build script
validation/               scenario files, RocketSim traces, REPORT.md
CONSTANTS.md              every constant with its source
THIRD_PARTY_NOTICES.md    RocketSim (MIT) and Bullet (zlib) notices
```

## License

MIT for this repository's code. The physics model follows RocketSim (MIT) and Bullet (zlib); see
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md). RLUtilities (GPL-3.0) was **not** used.

## Getting it working

**Bevy demo (works out of the box)**

1. Install [Rust](https://rustup.rs).
2. Run `cargo run -p rl_car_bevy --release`.
3. The demo uses the placeholder box car unless you add real car models (see below).

**Minecraft mod**

1. Install Java 25 and Rust.
2. Run `cd minecraft && ./gradlew runClient`. Gradle builds the native physics library with cargo.
3. Or build the jar with `./gradlew build` and install it with Fabric Loader 0.19.5+ and Fabric API
   for Minecraft 26.3. The jar only supports the platform it was built on.
4. In game, use the RL Car item or `/rlcar spawn`.
5. Without models it uses the box car.

**GTA V (PC, story mode only)**

1. Install Rust, Visual Studio 2022 with the C++ tools, CMake, and Script Hook V
   (dev-c.com) in your GTA V folder. Set `SHV_SDK` to the Script Hook V SDK folder.
2. Run `gta\build.bat`. It builds everything and stages it in `gta\stage`.
3. Copy the contents of `gta\stage` into your GTA V folder and start story mode.
4. Press F9 to become the car. See [`gta/README.md`](gta/README.md).

**Skyrim Special Edition (AE runtime 1.6/1.7)**

1. Install Rust, Visual Studio 2022 or newer with the C++ tools, and git; in Skyrim, SKSE64 and the
   Address Library for SKSE Plugins.
2. Run `skyrim\build.bat`. It builds everything (CommonLibSSE-NG and its vcpkg dependencies the first
   time) and stages it in `skyrim\stage` with the Mod Organizer layout.
3. Install `skyrim\stage` as a mod and start Skyrim through SKSE.
4. Press F7 to become the car. See [`skyrim/README.md`](skyrim/README.md).

**Optional: real car models**

1. You need your own copy of Rocket League.
2. Install Python 3 with `numpy` and `Pillow`, the .NET SDK, UModel and RL-UPKSuite.
3. Run `python tools/rl_assets/extract.py --umodel <dir> --upksuite <dir>`, which writes `assets/rl/`.
4. For Minecraft, copy `assets/rl` to `<.minecraft>/rlcar-assets`.
5. Read Epic's EULA first: it does not allow extracting the game's assets.

**Developers only: oracle and validation**

`oracle/build.sh` needs `git` and a C++20 `g++`.
