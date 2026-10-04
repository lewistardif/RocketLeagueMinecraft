# RL Car for GTA V

> **Unofficial fan project.** Not affiliated with, endorsed by, or connected to Psyonix, Epic Games,
> Rockstar Games or Take-Two. No game files are included: no Rocket League assets, no GTA files, no
> Script Hook V binaries. **Single-player story mode only.** It does not touch GTA Online, FiveM, the
> network or anti-cheat. Use it offline and at your own risk.

A Script Hook V plugin (`RLCar.asi`) that puts the Rocket League car and ball into GTA V. Like the
Minecraft mod, it drives this repository's Rust core (`crates/rl_car_core`) through its C ABI
(`crates/rl_car_ffi`):

- The core simulates the car and the ball at a fixed 120 Hz with Rocket League's physics. GTA's own
  physics is switched off for them, and the GTA vehicle and ball prop are just drawn at the pose the
  core computes.
- GTA's world becomes the core's collision through GTA's shape tests. Each wheel gets one probe.
  The car body and the ball collide with surfaces found by probing around them.
- Invisible quarter-pipes where the ground meets a steep wall let you drive up buildings like the
  arena's curved walls. Turn them off with `WallRamps = 0`.
- The camera is the core's Rocket League car camera with swivel, rear view and ball cam.

## Build and install

You need Rust, Visual Studio 2022 with the C++ tools, CMake, and the
[Script Hook V](http://www.dev-c.com/gtav/scripthookv/) SDK (set `SHV_SDK` to the folder that has
`inc\` and `lib\`).

```bat
gta\build.bat
```

This builds `rl_car_ffi.dll` and `RLCar.asi`, runs the unit tests, and stages everything in
`gta\stage`. Copy the contents of `gta\stage` into the GTA V folder, next to Script Hook V. You get
`RLCar.asi` plus an `RLCar\` folder with the DLL and `RLCar.ini`. The log is `RLCar\RLCar.log`.

The unit tests also run on Linux: `gta/tests/run_tests.sh`.

## Playing

| Action | Keyboard | Gamepad |
|---|---|---|
| Become the car / get back in | F9 (F next to the car) | LS + RS (Y next to the car) |
| Back to your character | F | Back / View |
| Menu (order from the Mechanic, hitbox, team, ball) | F7 | |
| Throttle / reverse, pitch in the air | W / S | RT / LT, left stick |
| Steer, yaw in the air | A / D | left stick |
| Jump, double jump, dodge | Space | A |
| Boost | Left Shift | B |
| Powerslide, free air roll | Left Ctrl | Square / X |
| Air roll left / right | Q / E | LB / RB |
| Rear camera | Middle mouse | RS |
| Spawn the ball | B | D-pad left |
| Ball cam | C | D-pad down |
| Fire / next weapon | Left mouse / X | D-pad up / D-pad right |
| Reset | R | Y |
| Reload `RLCar.ini` | F10 | |

Every binding can be changed in `RLCar.ini`. Use commas for alternatives and `+` for chords.

Hitting a GTA car or pedestrian from any side pushes it away like a Rocket League car of the same
mass would (`VehicleMass`, `PedMass`). A hit with the front bumper adds Rocket League's bump on top,
and at supersonic speed it demolishes: cars explode, pedestrians are knocked out. Turn this off or
scale it in `[Interaction]`.

PlayStation pads (DualSense, DualShock 4) work directly over USB or Bluetooth, without DS4Windows or
Steam Input. Square is powerslide and free air roll.

## Settings

`RLCar.ini` sections:

| Section | What it holds |
|---|---|
| `[General]` | hitbox preset, team, the GTA vehicle drawn for the car, HUD |
| `[Scale]` | `WorldScale`: GTA metres per Rocket League metre. `auto` makes the car as long as the GTA vehicle it replaces. The physics is the same at any scale. |
| `[Car]` | Rocket League's physics values: gravity, boost, jump, friction, top speed, unlimited flips or boost |
| `[Ball]` | ball model, size, mass, drag, bounce, friction, speed and spin caps, hit force |
| `[Camera]` | a Rocket League camera preset, or `Custom` with your own values |
| `[Controls]`, `[Gamepad]` | bindings, deadzones |
| `[Interaction]` | bumps and demolitions of GTA cars and pedestrians |
| `[Effects]` | boost flame, exhaust glow, boost sound |
| `[Models]` | the real car, wheel and ball models: detail level, distances, shading |
| `[Weapons]` | the machine gun and missiles on the car |
| `[World]` | what the car and ball collide with, the wall quarter-pipes |

Every value in `[Car]` and `[Ball]` defaults to Rocket League's own. Changing one makes the physics
differ from the game.

## Car models

If `RLCar\models\` holds converted Rocket League models, the plugin draws the real Octane body,
its four wheels (steering, spinning, on their suspension) and the real ball, and hides the GTA
vehicle and ball prop. The models are drawn as flat-shaded triangles coloured from the game's own
textures, with your team's paint. Without the files the car is a stock GTA vehicle (`FallbackModel`,
default `bifta`).

Making the files from your own Rocket League install:

```
python tools/rl_assets/extract.py --umodel <folder with umodel_64.exe> --upksuite <RL-UPKSuite release folder>
python gta/tools/rl_models.py --assets assets/rl --out "<GTA V folder>/RLCar/models"
```

Extracted or converted models are your own local files: they stay out of git, and Epic's EULA does
not allow extracting the game's assets. A real GTA add-on vehicle with full textures would need
tools like CodeWalker or Sollumz and is not automated.

## Layout

```
src/plugin.cpp       the plugin: car, ball, menu, camera, HUD, input
src/probe_world.*    the core's collision from GTA shape tests, wall quarter-pipes
src/gta_probe.cpp    GTA's synchronous shape test
src/space.h          Rocket League <-> GTA coordinates
src/rlcar_ffi.*      loads rl_car_ffi.dll
src/interact.*       bumps and demolitions of GTA entities
src/model.*          loads and draws the converted Rocket League models
src/hid_pad.*, sony_pad.h   DualSense / DualShock 4 over HID
tools/rl_models.py   converts the extracted glTF models into RLCar\models\*.rlm
src/effects.*        boost flame, glow, sound
src/weapons.*        machine gun, missiles
src/settings.*, ini.*, bindings.*   RLCar.ini
src/natives.h        generated by tools/gen_natives.py from alloc8or/gta5-nativedb-data
tests/tests.cpp      unit tests (no GTA needed)
```
