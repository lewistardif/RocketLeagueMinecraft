# RL Car for Skyrim

> **Unofficial fan project.** Not affiliated with, endorsed by, or connected to Psyonix, Epic Games,
> Bethesda or ZeniMax. No game files are included: no Rocket League assets, no Skyrim files, no SKSE
> binaries. Back up your saves.

An SKSE plugin (`RLCar.dll`) that turns the Dragonborn into the Rocket League car. Like the GTA and
Minecraft ports, it drives this repository's Rust core (`crates/rl_car_core`) through its C ABI
(`crates/rl_car_ffi`), in Skyrim's own process:

- The core simulates the car at a fixed 120 Hz with Rocket League's physics. A vanilla Skyrim model
  (a hand cart by default) with its collision turned off is drawn at the pose the core computes.
- Skyrim's world becomes the core's collision: everything solid around the car (terrain, buildings,
  rocks, trees, doors and gates, invisible walls, the ramps over stairs) is copied out of Havok as
  triangles, boxes, capsules and convex hulls, and the car's wheels and body collide with them
  exactly. Loose objects that Skyrim simulates (baskets, bodies) don't stop the car.
- Invisible quarter-pipes where the ground meets a steep wall let you drive up walls and cliffs like
  the arena's curved walls. Turn them off with `WallRamps = 0`.
- The ball is the core's Rocket League ball, stepped together with the car and drawn with a vanilla
  Dwemer gyro scaled to its size. The camera is the core's Rocket League car camera with swivel,
  rear view and ball cam.
- Hitting Skyrim's people and creatures uses Rocket League's bump and demolition rule: they are
  thrown as ragdolls, and a supersonic hit with the front bumper kills them, with the bounty a murder
  carries.
- The Dragonborn rides along hidden under the car, so the world keeps loading around it, and gets
  out where the car is.

## Requirements

| | |
|---|---|
| Skyrim Special Edition, **Anniversary Edition runtime** (1.6.x / 1.7.x) | Tested to build against 1.7.104. Not SE 1.5.97, not VR. |
| [SKSE64](https://skse.silverlock.org/) | For your game version |
| [Address Library for SKSE Plugins](https://www.nexusmods.com/skyrimspecialedition/mods/32444) | The "All in one (Anniversary Edition)" file |

## Build and install

You need Rust, Visual Studio 2022 or newer with the C++ tools (its bundled CMake is used), and git.

```bat
skyrim\build.bat
```

This builds `rl_car_ffi.dll`, CommonLibSSE-NG (a git submodule in `extern/`, fetched if missing)
and `RLCar.dll`, runs the unit tests, and stages everything in `skyrim\stage` with the Mod Organizer
layout:

```
SKSE/Plugins/RLCar.dll
SKSE/Plugins/RLCar/rl_car_ffi.dll
SKSE/Plugins/RLCar/RLCar.ini
```

Don't run it together with SkyCraft (or another plugin that takes over the player's camera and
controls): both hook the same camera and input functions.

The first run clones and bootstraps its own vcpkg in `skyrim\.tools\vcpkg` (git-ignored) and builds
CommonLibSSE-NG's dependencies with it, which takes a few minutes. Install `skyrim\stage` as a mod
(zip it, or copy it into an MO2 mod folder), or copy its `SKSE` folder into Skyrim's `Data`. The log
is `Documents\My Games\Skyrim Special Edition\SKSE\RLCar.log`.

## Playing

| Action | Keyboard | Gamepad |
|---|---|---|
| Become the car / get out | F7 | LS + RS (Back also gets out) |
| Throttle / reverse, pitch in the air | W / S | RT / LT, left stick |
| Steer, yaw in the air | A / D | left stick |
| Jump, double jump, dodge | Space | A |
| Boost | Left Shift | B |
| Powerslide, free air roll | Left Ctrl | Square / X |
| Air roll left / right | Q / E | LB / RB |
| Rear camera | Middle mouse | RS |
| Swivel the camera | | right stick |
| Spawn the ball (in front of you) | B | D-pad left |
| Ball cam | C | D-pad down |
| Reset (upright, where you are) | R | Y |
| Reload `RLCar.ini` | F10 | |

Every binding can be changed in `RLCar.ini`. Use commas for alternatives and `+` for chords. While you
are the car, Skyrim's own controls are off; Esc, the console and the gamepad's Start still open
Skyrim's menus, and the car waits while they are open. After a loading screen (fast travel, a load
door, an interior) or when a script moves you, the car starts again where you arrived. Dying or
loading a save gets you out of the car.

Hitting someone from any side pushes them away like a Rocket League car would push a body of their
mass (`ActorMass`), with a ragdoll (or only a stagger with `Ragdoll = 0`); they then fight you. A hit
with the front bumper adds Rocket League's bump on top, and at supersonic speed it demolishes:
they die, and their crime faction adds Skyrim's murder bounty (assault bounty for a bump). Essential
characters are only knocked down and followers are never demolished. Turn any of it off or scale it
in `[Interaction]`. The ball is removed when it gets too far away (15000 uu) or on a loading screen.

PlayStation pads (DualSense, DualShock 4) work directly over USB or Bluetooth, like in the GTA port.

## Settings

`RLCar.ini` (in `SKSE/Plugins/RLCar/`) sections:

| Section | What it holds |
|---|---|
| `[General]` | hitbox preset, the vanilla model drawn for the car (`CarForm = Plugin|FormID`), its yaw and lift |
| `[Scale]` | `WorldScale`: Skyrim metres per Rocket League metre. `auto` makes the car as long as its model. The physics is the same at any scale. |
| `[Car]` | Rocket League's physics values: gravity, boost, jump, friction, top speed, unlimited flips or boost |
| `[Camera]` | a Rocket League camera preset, or `Custom` with your own values |
| `[Controls]`, `[Gamepad]` | bindings, deadzones |
| `[Ball]` | the ball's model, size, mass, drag, bounce, friction, speed and spin caps, hit force |
| `[Interaction]` | bumps and demolitions of actors: ragdolls, kills, followers, the crime |
| `[World]` | the wall quarter-pipes, how far around the car Skyrim's collision is copied and how often |

Every value in `[Car]` and `[Ball]` defaults to Rocket League's own. Changing one makes the physics differ from the
game. `DebugLog = 1` writes the car's position, speed and collision cache to the log every two seconds.

## Layout

```
src/main.cpp          SKSE entry, log, messages
src/plugin.*          on foot <-> car, the 120 Hz stepping, the car's model and the cache refresh
src/havok_world.*     copies Havok's collision near the car: meshes, boxes, capsules, convex hulls (read lock, fault guards)
src/sky_world.*       the core's three collision callbacks on that copy, wall quarter-pipes
src/puppet.*          hides the Dragonborn, parks their capsule under the car, hands them back
src/camera.*          Skyrim's camera at the Rocket League camera (PlayerCamera::Update hook)
src/input.*           keyboard, XInput and PlayStation pads; Skyrim's controls off while driving
src/visual.*          the vanilla models (car, ball) placed and moved every frame
src/interact.*        bumps and demolitions of actors: ragdoll impulses, kills, bounty
src/geom.h            box overlap (from the GTA port)
src/space.h           Rocket League <-> Skyrim coordinates
src/rlcar_ffi.*, ini.*, bindings.*, settings.*, hid_pad.*, sony_pad.h   from the GTA port
tests/tests.cpp       unit tests (no Skyrim needed): coordinates, contacts, driving on triangles and hulls
extern/CommonLibSSE-NG  git submodule (alandtse/CommonLibVR, branch ng)
```

## Limits

- The car is drawn with a vanilla model, not the Rocket League car.
- The collision is a copy, made again every `CacheRefresh` seconds and whenever the car nears its
  edge: a door that opens in between is seen up to that late. A long frame hitch at supersonic speed
  can carry the car past the copy; it is then made again at once and the log says
  `the car outran its collision copy`.
- Skyrim only has collision where cells are loaded (the uGrids around the player, who rides under the
  car), so very fast long drives can reach ground that isn't there yet.
- Shapes whose layout isn't known are read as their bounding box (logged once per shape type).
- The ball and the car don't collide with actors as solid bodies: actors are pushed by the bump
  rule, and the ball goes through them.
- The bounty is added whether or not anyone saw it.
- No HUD: boost is unlimited by default (`UnlimitedBoost = 1`).
