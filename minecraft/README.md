# RL Car for Minecraft (Fabric 26.3)

> **Unofficial fan project.** Not affiliated with, endorsed by, or connected to Psyonix, Epic Games
> or Mojang. "Rocket League" is a trademark of Psyonix.

A drivable Rocket League car in Minecraft. The physics is **not** reimplemented in Java: the mod
calls this repository's Rust core (`crates/rl_car_core`) through its C ABI (`crates/rl_car_ffi`)
using Java's built-in FFM API. It is the same validated model as the Bevy demo, with Minecraft
blocks as the collision world.

The cars are drawn with the **real Rocket League models** (the 7 hitbox bodies and the wheel, blue
and orange) extracted from your own game install, the same files the Bevy demo uses. Without them
the mod falls back to a plain box car.

## Build and run

Needs Java 25 and a Rust toolchain. Gradle builds the native library with cargo and bundles it
into the jar.

```bash
cd minecraft
./gradlew runClient
```

`./gradlew build` produces `build/libs/rlcar-<version>.jar`. Install it with Fabric Loader 0.19.5+
and Fabric API for Minecraft 26.3. The jar only contains the native library for the platform it was
built on (`natives/<os>-<arch>/`); see [Other platforms](#other-platforms). Java 25 prints a
one-line warning when a mod loads native code; add `--enable-native-access=ALL-UNNAMED` to the
game's JVM arguments to silence it.

### Car models

The models are not part of this repository or the jar. Extract them from your own install with
`tools/rl_assets/extract.py` (see the root README), which writes `assets/rl/`. `runClient` and
`runClientGameTest` read that folder directly. For a normal Minecraft install, copy `assets/rl`
to `<.minecraft>/rlcar-assets`, or start the game with `-Drlcar.assets=<path to assets/rl>`.
Only the base colour textures are used (no normal maps or emissive lights). They get a full mip
chain so they don't shimmer at a distance.

The same folder holds the default boost (`boost/`): with it, the real cars get the game's flame cones
and smoke trail, as in the Bevy demo (shaders in `src/client/resources/assets/rlcar/shaders/core/`).
Without it, and on the box car, boosting shows a simple flickering flame. The boost material's
parameters are compiled into the shader when the game starts, so a re-extracted boost needs a
restart.

### Playing

In game: take **RL Car** from the *Tools & Utilities* creative tab and use it on a block. Sneak
while placing it to get an orange car. Or run `/rlcar spawn [octane|dominus|plank|breakout|hybrid|merc|psyclops] [blue|orange]`.
Right-click the car to get in. The camera switches to Rocket League's car camera (the same one as
the Bevy demo, see the root README); F5 cycles to a hood cam.

#### Camera

Options > Controls > **RL Car Camera...** has Rocket League's camera settings, with its ranges
and presets: Preset (Default, Balanced, Wide, Legacy, Modern; moving a slider makes it Custom),
Field of View, Distance, Height, Angle, Stiffness, Swivel Speed, Invert Swivel Pitch and Rear
Camera Toggle. They are saved in `config/rlcar-camera.properties`. In the car camera, the Field of View setting replaces
Minecraft's (it is Rocket League's horizontal FOV at 16:9, so 90 looks like Minecraft's 59).
The right stick swivels the camera around the car. Rear Camera works as in Rocket League: it
looks behind while held, or with Rear Camera Toggle on, each press switches between looking behind
and forward. Getting into a car always starts facing forward. Unlike Rocket
League's see-through arena walls, blocks pull the camera in towards the car.

Boost is unlimited for now (Rocket League's "Unlimited" boost mutator): the tank stays full.

#### Controls

Options > Controls > Key Binds has an **RL Car** section with Rocket League's driving bindings, in
Rocket League's order. Every one can be rebound to any key or mouse button (Rocket League's own
defaults, Boost on left click and Jump on right click, work fine).

| Binding | Default key | Default gamepad | Does |
|---|---|---|---|
| Throttle / Reverse | W / S | RT / LT | drive forward / brake and reverse |
| Steer Right / Steer Left | D / A | left stick X | steer on the ground |
| Pitch Up / Pitch Down | S / W | left stick Y | nose up / down in the air |
| Yaw Right / Yaw Left | D / A | left stick X | turn in the air |
| Air Roll Right / Air Roll Left | E / Q | RB / LB | roll in the air |
| Air Roll | Left Ctrl | X | while held, the yaw input rolls instead |
| Jump | Space | A | jump (hold = higher, press again = double jump, or dodge in the stick/key direction) |
| Boost | Left Shift | B | boost |
| Powerslide (Drift) | Left Ctrl | X | powerslide |
| Rear Camera | middle click | R3 | look behind while held (each press switches, with Rear Camera Toggle) |
| (camera swivel) | | right stick | swivel the camera around the car |
| Reset Car | R | Y | put the car back on its wheels |
| Get Out of Car | F | Back | get out |

As in Rocket League, the ground and air controls are separate bindings that share keys by default
(W is Throttle and Pitch Down, Left Ctrl is Powerslide and Air Roll), so either can be moved on
its own, for example pitch to the arrow keys. The Controls screen does not mark those intended
pairs as conflicts, nor a car binding on a key whose vanilla action is off while driving (walking,
Jump, Sneak, Sprint, Drop, Inventory, Swap Hands, Attack, Use, Pick Block). Any other shared key is
still marked. Rocket League's ball cam, scoreboard and chat bindings have nothing to act on here
yet, so they are not listed. Rocket League also swivels the camera with the mouse; that is left
out here, because the mouse already turns the player.

#### Controller bindings

Below the keys, the **RL Car (Controller)** section rebinds the gamepad the same way: click an
action's button, then press any button or trigger on the controller (Escape unbinds it, a click
cancels). Every action in the gamepad column above can be moved, except the sticks: the left one
always steers on the ground and pitches/yaws in the air, the right one swivels the camera. A trigger gives partial values for Throttle,
Reverse and Air Roll Left/Right, and counts as pressed past halfway for Jump, Boost and the other
on/off actions. Button names follow the connected controller (A/B/X/Y or Cross/Circle/Square/Triangle).
As in Rocket League, Powerslide and Air Roll share X by default; any other button used twice is
marked as a conflict. Reset Keys also resets the controller bindings. They are saved in
`config/rlcar-controller.properties`. The first connected gamepad is used.

While driving, the vanilla actions on those keys are off: no walking, dropping items, opening the
inventory, swapping hands, breaking, placing or picking blocks. The player is hidden, because it sits inside the car.

## How it works

```
 driving client                                 server                    other clients
 ─────────────                                  ──────                    ─────────────
 every frame: keys -> rl_car_ffi (120 Hz,       adopts the state,         draw the pose,
   blocks around the car as collision)    ──►   relays the pose     ──►   interpolated
 every tick: full car state ──────────────►     (cars nobody drives:
                                                 simulates them itself,
                                                 sleeps when at rest)
```

* **Who simulates.** The driver's own client runs the car every rendered frame. Input is sampled
  per frame, so jump and dodge timing is as tight as in the Bevy demo, and there is no round trip
  to the server. The client sends the full state (`rlcar_car_save`, lossless) every tick; the
  server keeps it and relays a compact pose to everyone else. When the driver gets out, the server
  continues from that state at 6 core ticks per Minecraft tick, and puts the car to sleep once it
  rests. A sleeping car wakes when the blocks around it change.
* **Collision.** Each tick the mod collects the collision boxes of the blocks within 4 blocks of the
  car (`VoxelShape.toAabbs`, so slabs, stairs, carpets and fences work). `BoxWorld` in
  `rl_car_ffi` turns them into the exposed surface of their union: faces shared by two blocks are
  removed and coplanar faces are merged. A flat block floor is one face, so the car does not catch
  on the seams between blocks. The Rust tests show a car on a block floor rebuilt from a moving
  9×9 window matches an infinite plane **bit for bit** over 600 ticks.
* **Space.** `mc = (rl.x, rl.z, rl.y) / 100` (1 block = 100 uu; the axis swap also converts
  Rocket League's left-handed axes to Minecraft's right-handed ones). See `physics/Space.java`.
  Positions are relative to a per-car whole-block origin, which moves once the car is 512 blocks
  away, so the core's 32-bit floats stay precise anywhere in the world.

## Tests

```bash
cargo test -p rl_car_ffi            # BoxWorld, C ABI, state encoding
cd minecraft
./gradlew runSelftest               # headless dedicated server: /rlcar selftest, then stops
./gradlew runClientGameTest         # real client: drives, jumps, turns, gets out; screenshots
```

`runSelftest` drives a car on a stone track through the real Minecraft path (block snapshot →
Rust → pose). It checks the rest height (0.17 blocks, RL's 17 uu), driving straight across block
seams, stopping at a block wall on throttle, climbing that wall (never entering it) on boost, the
full jump height (2.3 blocks) and the save/load round trip.
`runClientGameTest` opens a game window, checks the RL Car bindings (order, conflict rules), then
drives with simulated key presses (Boost rebound to left click, Air Roll, Rear Camera, Reset Car
included) and checks the client, the server and the camera (Rocket League's position and FOV,
staying level while the car rolls, the rear view, the camera settings screen). It ends with a row of all 7 bodies, and saves screenshots to
`build/run/clientGameTest/screenshots/`.

## Limitations

* **Steps are walls.** A full block (100 uu) is three times the car's height, and even a slab is
  far above its ground clearance, so the car cannot drive up a step; it drives on flat ground,
  carpets and snow layers. Boosting into a wall at speed can tip the car onto it, and then it
  drives up the wall as in Rocket League. Proper ramps and quarter pipes need custom blocks with
  sloped collision, which `BoxWorld` cannot represent yet (it only takes boxes).
* **Car only.** There is no ball yet, and no car-car or car-entity collisions: cars drive through
  each other and through mobs.
* **The client is trusted.** The server adopts whatever state the driver sends. That is fine with
  friends, but the server does not stop a modified client from teleporting its car.
* **Not tested yet:** gamepads (written against SDL3, but no pad was connected during
  development); multiplayer with several real clients (the client↔server packets were exercised
  in singleplayer, which uses the same network code); macOS and Linux builds.
* No sounds, and no effects besides the boost (no supersonic trail, jump or landing effects).
* Each car is re-sent to the GPU every frame (about 28k triangles for a body). That is fine for a
  handful of cars; many more would need cached vertex buffers.

## Other platforms

Build the library for each target and put it in the jar under `natives/<os>-<arch>/`, for example
`natives/linux-x86_64/librl_car_ffi.so` or `natives/macos-aarch64/librl_car_ffi.dylib`. The mod
picks the one matching the running system. During development, `-Drlcar.native=<path>` loads a
library file directly.
