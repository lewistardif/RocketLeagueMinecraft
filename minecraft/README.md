# RL Car for Minecraft (Fabric 26.3)

> **Unofficial fan project.** Not affiliated with, endorsed by, or connected to Psyonix, Epic Games
> or Mojang. "Rocket League" is a trademark of Psyonix.

A drivable Rocket League car, and Rocket League's ball, in Minecraft. The physics is **not**
reimplemented in Java: the mod calls this repository's Rust core (`crates/rl_car_core`) through its
C ABI (`crates/rl_car_ffi`) using Java's built-in FFM API. It is the same validated model as the
Bevy demo and the GTA V port, with Minecraft blocks as the collision world.

The cars and the ball are drawn with the **real Rocket League models** (the 7 hitbox bodies and the
wheel, blue and orange, and the ball) extracted from your own game install, the same files the Bevy
demo uses. Without them the mod falls back to a plain box car and a plain panelled ball.

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
The ball is read from the same folder (`ball/`). Textures get a full mip chain so they don't
shimmer at a distance.

#### Car materials

The cars and the wheels are drawn with Rocket League's own material shaders, decompiled from the
game's shader cache (`RefShaderCache-PC-D3D-SM5.upk`, the base pass pixel shaders of
`Body_Paintable_Mat`, the chassis materials and `Wheel_Master_Mat`) and translated instruction by
instruction to GLSL (`src/client/resources/assets/rlcar/shaders/core/rl_body.fsh`,
`rl_chassis.fsh`, `rl_wheel.fsh`):

- **Paint**: team, accent and paint colours mixed by the body's own skin and curvature masks, lit
  through the game's lighting ramp atlas (`LightFalloffArray`, one row per paint finish). That ramp
  gives Rocket League's soft wrap-around shading and tight clear-coat highlights. Reflections come
  from the environment and the game's `ENVPack` texture, and chrome parts reflect fully.
- **Windows and trims**: the curvature pack's green channel marks them. They are drawn dark in the
  trim colour with the game's Fresnel rim, so the glass brightens at grazing angles. The shader's
  tertiary (carbon) layer is masked by the body mask's alpha; none of the seven default bodies has
  one, so it stays off, as in the game.
- **Chassis**: three base materials, each ported from its own shader: `MasterChassis_MAT`
  (Octane, Hybrid, Merc, Plank), `MAT_Chassis_Paintable` (Breakout) and the Dominus'
  `MAT_BANDAID_Chassis_Paintable` (Fresnel trims). Normal map plus a tiled brushed-metal detail,
  a swirl or cube reflection, a sharp highlight; tail lights, headlights and the boost glow come
  from the masks' red, green and blue where its alpha is set. The boost glow's intensity is raised
  by native game code while boosting; here it keeps the material's value (off). The Psyclops'
  `GoodChassis_Painted_Mat` cannot be read from the shader cache and is drawn with the
  `MasterChassis_MAT` port.
- **Wheels**: brushed-metal rims with a swirling reflection and a sharp highlight, and rubber tyres.

The game's lights are replaced by Minecraft's: the sun (or the moon at night) is the key light,
dimmed by rain and by the sky light at the car (no sun in a cave). The lightmap gives the ambient
light, so torches and night darken and tint the car. The sky and fog colours, with the sun in
them, are what the paint reflects. The result is tone mapped like the game's HDR output.

`tools/rl_assets/extract.py` writes these shaders' inputs next to the models: `materials.json` per
car (the material instances' textures and parameter values, resolved through their parents) and
the shared textures in `shading/`. The parameters are compiled into the shaders when the game
starts, so a re-extracted car needs a restart. An older extraction without `materials.json` still
works and shows the plain textured models.

The same folder holds the default boost (`boost/`): with it, the real cars get the game's flame cones
and smoke trail, as in the Bevy demo (shaders in `src/client/resources/assets/rlcar/shaders/core/`).
Without it, and on the box car, boosting shows a simple flickering flame. The boost material's
parameters are compiled into the shader when the game starts, so a re-extracted boost needs a
restart.

#### Sounds and effects

With the sounds and effects extracted (`audio/`, `fx/`; see the root README, the sounds need
`--wwiser` and `--vgmstream`), every car sounds and looks like the Bevy demo's, from the same data
and ports of the same code (`RlAudio`, `RlFx`):

- **Sounds**: engine and exhaust (pitched by a reconstructed RPM), tyres, jump, double jump, dodge,
  the in-air whoosh (your own car only, as in the game), wheel landings, body impacts and slides,
  supersonic, the boost loop and tail, the empty-tank dry fire. They are played by a small Wwise
  graph player straight through OpenAL on Minecraft's context, so the engine's pitch range and
  frame-rate updates survive. Your car is heard as in the game; other cars are positioned and fade
  out by 64 blocks. The volume follows the *Players* slider. `-Drlcar.audioLog` logs the driven
  car's parameters and voices.
- **Effects**: jump smoke, double jump and dodge smoke and ribbons, supersonic speed streaks (your
  own car, as in the game) and wheel trails, impact sparks, simulated from the extracted particle
  systems and drawn with ports of their shaders (`rl_fx.fsh`).
- **Camera shakes**: jump, double jump, dodge, landing, impacts and boost shake the car camera.
  Gamepad rumble is not played (GLFW has no rumble).

They need the extended car state of `rl_car_ffi`'s `rlcar_car_contacts` (velocity, wheel and body
contacts, jump/flip flags), sent with every car pose so other players' cars play them too.

#### The ball's material and markers

The ball is drawn with a port of its own material, `MAT_Ball_V3` (`rl_ball.fsh`, same lighting as
the cars). It uses the normal map, the tiled `Detail_Matte` normal and the mask, all extracted unbaked
into `ball/` with a `materials.json`. The light strips pulse once a second, in the colour of the
field half the ball is in (the game's `TeamColor_WorldSpace`): warm white within about 10 blocks of
the kickoff spot (where the ball was put down), blue past that towards -Z (Rocket League's -Y),
orange towards +Z. The game also brightens the ball near a goal; there are no goals, so that stays
off, as do the arena-box reflections (the sky dome is reflected instead).

Around it are the markers the game's ball FX actor (`FXActors.Ball.Ball_FXActor`) attaches, drawn
with ports of their shaders (`rl_marker.fsh`):

- **Ground reticle** (`Ball_GroundReticle_DMat`): a ring the size of the ball, projected straight
  down onto the blocks under it (up to 31 blocks below), with an inner ring that closes in as the
  ball climbs (fully closed 10 blocks up), cut by the reticle texture's cross. This is the landing
  marker.
- **Location line** (`Ball_LocationBeam01_PS`, after the ball's first second): a faint dashed line
  25 blocks down from the ball; a ring around the ball drawn through everything once the ball is 20
  to 41 blocks away (its outline); and a dark halo behind it at that distance, which keeps the ball
  readable against the sky.

They hide with the HUD (F1), as the game hides them with its world UI. Without `ball/materials.json`
the ball keeps its baked texture, and the reticle has no cross cut.

### Playing

In game: take **RL Car** from the *Tools & Utilities* creative tab and use it on a block. Sneak
while placing it to get an orange car. Or run `/rlcar spawn [octane|dominus|plank|breakout|hybrid|merc|psyclops] [blue|orange]`.
Right-click the car to get in. The camera switches to Rocket League's car camera (the same one as
the Bevy demo, see the root README); F5 cycles to a hood cam.

#### The ball

Take **RL Ball** from the same creative tab and use it on a block, or run `/rlcar ball`. Drive into
it: car hits, dribbles and pinches against blocks use the core's ball physics (RocketSim's ball,
stepped in one solve with the car, validated against RocketSim). On foot, hitting the ball kicks it
where you look; sneak and hit it to pick it up. A ball nobody plays settles and sleeps, and wakes
when the blocks under it change. The ball is 1.8 blocks across, as in Rocket League, so it does not
fit through a one-block gap.

#### Bumps and demolitions

Rocket League's bump rule applies to whatever a car hits: other cars and mobs (players on foot too).
Hit something with the front bumper while driving at it and it gets Rocket League's bump velocity;
do it supersonic and it is demolished. A demolished mob or player takes explosion damage (no blocks
are broken); a demolished car throws its driver out, disappears and comes back at rest where it
was destroyed three seconds later. Slower or glancing hits just shove mobs and cars aside.

#### Camera

Options > Controls > **RL Car Camera...** has Rocket League's camera settings, with its ranges
and presets: Preset (Default, Balanced, Wide, Legacy, Modern; moving a slider makes it Custom),
Field of View, Distance, Height, Angle, Stiffness, Swivel Speed, Transition Speed, Invert Swivel
Pitch and Rear Camera Toggle. They are saved in `config/rlcar-camera.properties`. In the car camera, the Field of View setting replaces
Minecraft's (it is Rocket League's horizontal FOV at 16:9, so 90 looks like Minecraft's 59).
The right stick swivels the camera around the car. Rear Camera works as in Rocket League: it
looks behind while held, or with Rear Camera Toggle on, each press switches between looking behind
and forward. Getting into a car always starts facing forward. **Ball Cam** (V, or Y on a
controller) switches Rocket League's ball cam: the camera turns to keep the nearest ball in view,
blending at the Transition Speed setting, until it is pressed again. Unlike Rocket
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
| Ball Cam | V | Y | switch ball cam on and off |
| Rear Camera | middle click | R3 | look behind while held (each press switches, with Rear Camera Toggle) |
| (camera swivel) | | right stick | swivel the camera around the car |
| Reset Car | R | D-pad up | put the car back on its wheels |
| Get Out of Car | F | Back | get out |

As in Rocket League, the ground and air controls are separate bindings that share keys by default
(W is Throttle and Pitch Down, Left Ctrl is Powerslide and Air Roll), so either can be moved on
its own, for example pitch to the arrow keys. The Controls screen does not mark those intended
pairs as conflicts, nor a car binding on a key whose vanilla action is off while driving (walking,
Jump, Sneak, Sprint, Drop, Inventory, Swap Hands, Attack, Use, Pick Block). Any other shared key is
still marked. Rocket League's scoreboard and chat bindings have nothing to act on here, so they
are not listed. Rocket League's own keyboard default for Ball Cam, Space, is Jump here, so Ball Cam
is on V, which vanilla leaves free. Rocket League also swivels the camera with the mouse; that is left
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
* **The ball.** The server simulates a ball nobody plays. When a driver's car comes within 8
  blocks, the server lends the ball to that driver's client, which steps it in one solve with its
  car every frame (so hits feel as immediate as the driving) and streams its state back; the server
  relays it to everyone else. It takes the ball back when the car is 12 blocks away, the driver gets
  out, or the client stops sending, and continues from the last state.
* **Bumps.** The server has every car's state (from the drivers, or its own simulation), so it
  checks each car's hitbox against the mobs and cars around it every tick and applies the core's
  bump rule (`rlcar_car_bump`). A bumped car with a driver gets the velocity through its driver's
  client (`rlcar_car_add_velocity`).
* **Collision.** Each tick the mod collects the collision boxes of the blocks within 4 blocks of the
  car (or of the ball, or of both while they are close) (`VoxelShape.toAabbs`, so slabs, stairs, carpets and fences work). `BoxWorld` in
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
full jump height (2.3 blocks) and the save/load round trip. With the ball, it checks that it bounces
and rests on the block floor at its radius, sleeps and falls into a hole opened under it, and flies
down the track when a boosting car hits it.
`runClientGameTest` opens a game window, checks the RL Car bindings (order, conflict rules), then
drives with simulated key presses (Boost rebound to left click, Air Roll, Rear Camera, Reset Car
included) and checks the client, the server and the camera (Rocket League's position and FOV,
staying level while the car rolls, the rear view, the camera settings screen). It plays a ball
(the server lends it to the driver, ball cam turns to it and back, a hit sends it down the road on
the client and the server, getting out hands it back), bumps a pig and demolishes a parked car that
then respawns. It ends with a row of all 7 bodies, and saves screenshots to
`build/run/clientGameTest/screenshots/`.

## Limitations

* **Steps are walls.** A full block (100 uu) is three times the car's height, and even a slab is
  far above its ground clearance, so the car cannot drive up a step; it drives on flat ground,
  carpets and snow layers. Boosting into a wall at speed can tip the car onto it, and then it
  drives up the wall as in Rocket League. Proper ramps and quarter pipes need custom blocks with
  sloped collision, which `BoxWorld` cannot represent yet (it only takes boxes).
* **One car per ball solve.** The core steps a car and a ball together, so only the driver the ball
  is lent to plays it; other drivers' cars pass through it until it is lent to them. Cars nobody
  drives (rolling after their driver got out) pass through the ball too.
* **No car-car contact.** Bumps and demolitions follow Rocket League's rule, but the core has no
  contact solver between two cars, so their bodies pass through each other; the bump velocity is
  what pushes the victim away.
* **The client is trusted.** The server adopts whatever state the driver sends. That is fine with
  friends, but the server does not stop a modified client from teleporting its car.
* **Not tested yet:** gamepads (written against SDL3, but no pad was connected during
  development); multiplayer with several real clients (ball lending between two drivers in
  particular) (the client↔server packets were exercised
  in singleplayer, which uses the same network code); macOS and Linux builds.
* No Wwise filters or Rocket League's own 3D attenuation curves for the sounds, no jump distortion
  sphere, no boost glow on the chassis, no gamepad rumble (see *Sounds and effects*).
* Each car is re-sent to the GPU every frame (about 28k triangles for a body). That is fine for a
  handful of cars; many more would need cached vertex buffers.

## Other platforms

Build the library for each target and put it in the jar under `natives/<os>-<arch>/`, for example
`natives/linux-x86_64/librl_car_ffi.so` or `natives/macos-aarch64/librl_car_ffi.dylib`. The mod
picks the one matching the running system. During development, `-Drlcar.native=<path>` loads a
library file directly.
