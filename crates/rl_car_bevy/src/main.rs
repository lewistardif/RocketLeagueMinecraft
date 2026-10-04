//! Bevy demo: a drivable Rocket League-style car powered by `rl_car_core`.
//!
//! Unofficial fan project — not affiliated with, endorsed by, or connected to Psyonix or Epic Games.
//!
//! Frame flow:
//!   input -> Controls -> FixedStepper (120 Hz ticks, accumulates Bevy frame time)
//!         -> rl_car_core::step(.., AvianWorld) for each tick
//!   render: interpolate previous/current tick state -> car Transform, wheels, camera, HUD.

#![allow(clippy::type_complexity, clippy::too_many_arguments)] // Bevy system parameter lists.

mod arena;
mod audio;
mod boost;
mod collision;
mod convert;
mod fx;
mod input;
mod visuals;
#[cfg(test)]
mod host_tests;

use avian3d::prelude::*;
use boost::BoostData;
use bevy::prelude::*;
use collision::{ArenaColliderQuery, AvianWorld};
use convert::*;
use rl_car_core::maneuvers::{HalfFlip, Maneuver};
use rl_car_core::camera::{CameraSettings, CameraTarget, CarCamera};
use rl_car_core::{CarState, Controls, FixedStepper, HitboxPreset, RotMat, Vec3 as RVec3};
use visuals::{CarVisuals, RealBody, Team};

fn main() {
    let autopilot = Autopilot::from_args();
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window { title: "rl_car_core - Bevy demo (unofficial)".into(), ..default() }),
                    ..default()
                })
                .set(AssetPlugin { file_path: visuals::asset_root().to_string_lossy().into_owned(), ..default() })
                // The wheel mesh's vertex colours (tools/rl_assets/extract.py, for the Minecraft
                // mod's wheel shader); unused here, registered so the loader does not warn.
                // (Named without the underscore: the gltf crate strips it before the lookup.)
                .set(bevy::gltf::GltfPlugin::default().add_custom_vertex_attribute(
                    "RL_VERTEX_COLOR",
                    bevy::mesh::MeshVertexAttribute::new("RlVertexColor", 0x524c_5643, bevy::mesh::VertexFormat::Float32x4),
                )),
        )
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(boost::BoostPlugin)
        .add_plugins(audio::CarAudioPlugin)
        .add_plugins(fx::FxPlugin)
        .insert_resource(ClearColor(Color::srgb(0.55, 0.68, 0.85)))
        .insert_resource(Sim::new(HitboxPreset::Octane))
        .insert_resource(CameraRig::default())
        .insert_resource(CarVisuals::detect())
        .insert_resource(autopilot)
        .insert_resource(Showcase::from_args())
        .add_systems(Startup, (arena::spawn_arena, spawn_car, spawn_camera, spawn_hud, slow_motion))
        .add_systems(Update, (hotkeys, simulate, sync_car, boost::sync_cones, sync_wheels, follow_camera, boost::update, fx::update, fx::draw, audio::drive, audio::update_voices, update_hud, autopilot_shots, showcase, visuals::generate_mipmaps).chain())
        .run();
}

/// `--slowmo <factor>`: run the game clock (physics, effects, sounds' parameters) at `factor` x real
/// time, to look at short-lived effects.
fn slow_motion(mut time: ResMut<Time<Virtual>>) {
    let args: Vec<String> = std::env::args().collect();
    if let Some(f) = args.iter().position(|a| a == "--slowmo").and_then(|i| args.get(i + 1)).and_then(|v| v.parse::<f32>().ok()) {
        time.set_relative_speed(f.clamp(0.01, 10.0));
    }
}

// ------------------------------------------------------------------------------------ autopilot

/// `--autopilot [screenshot_dir]`: drive a fixed script (boost across the pitch and up the far
/// wall), save screenshots along the way and exit. Used to check the real windowed app end to end.
#[derive(Resource, Default)]
struct Autopilot {
    enabled: bool,
    dir: Option<std::path::PathBuf>,
    shots_taken: usize,
}

const AUTOPILOT_SHOTS: [f32; 4] = [1.0, 1.75, 4.9, 5.6];

impl Autopilot {
    fn from_args() -> Autopilot {
        let args: Vec<String> = std::env::args().collect();
        match args.iter().position(|a| a == "--autopilot") {
            Some(i) => Autopilot { enabled: true, dir: args.get(i + 1).map(Into::into), shots_taken: 0 },
            None => Autopilot::default(),
        }
    }

    /// Start in a clear lane (no ramps) heading for the far back wall.
    fn start_state() -> CarState {
        let mut s = spawn_state(HitboxPreset::Octane);
        s.position.x = -1000.0;
        s
    }

    /// Scripted controls by simulated time (seconds): boost, front flip, boost into the back
    /// wall's quarter-pipe and drive up the wall.
    fn controls(t: f32) -> Controls {
        let mut c = Controls { throttle: 1.0, boost: !(1.3..2.4).contains(&t), ..default() };
        if (1.3..1.38).contains(&t) {
            c.jump = true;
        }
        if (1.45..1.55).contains(&t) {
            c.jump = true;
            c.pitch = -1.0; // front flip
        }
        c
    }
}

fn autopilot_shots(mut commands: Commands, mut ap: ResMut<Autopilot>, sim: Res<Sim>, mut exit: MessageWriter<AppExit>) {
    if !ap.enabled {
        return;
    }
    let t = sim.stepper.tick_count as f32 * rl_car_core::TICK_DT;
    if ap.shots_taken < AUTOPILOT_SHOTS.len() && t >= AUTOPILOT_SHOTS[ap.shots_taken] {
        if let Some(dir) = &ap.dir {
            let path = dir.join(format!("autopilot_{}.png", ap.shots_taken));
            commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path));
        }
        let s = &sim.stepper.current;
        info!(
            "autopilot t={t:.2}s pos=({:.0},{:.0},{:.0}) speed={:.0} up=({:.2},{:.2},{:.2}) on_ground={} boost={:.0}",
            s.position.x, s.position.y, s.position.z, s.velocity.length(), s.up().x, s.up().y, s.up().z, s.on_ground, s.boost_amount
        );
        ap.shots_taken += 1;
    }
    if t > AUTOPILOT_SHOTS[AUTOPILOT_SHOTS.len() - 1] + 0.5 {
        exit.write(AppExit::Success);
    }
}

/// `--showcase <dir>`: photograph every car parked, from a close front 3/4 view (blue team, then
/// the orange Octane), and exit. Used to check the extracted Rocket League models.
#[derive(Resource, Default)]
struct Showcase {
    dir: Option<std::path::PathBuf>,
    step: usize,
    wait: f32,
}

const SHOWCASE_STEPS: usize = HitboxPreset::ALL.len() + 1;

impl Showcase {
    fn from_args() -> Showcase {
        let args: Vec<String> = std::env::args().collect();
        let dir = args.iter().position(|a| a == "--showcase").and_then(|i| args.get(i + 1)).map(Into::into);
        Showcase { dir, ..default() }
    }

    fn step_car(step: usize) -> (HitboxPreset, Team) {
        match HitboxPreset::ALL.get(step) {
            Some(&p) => (p, Team::Blue),
            None => (HitboxPreset::Octane, Team::Orange),
        }
    }
}

fn showcase(mut commands: Commands, time: Res<Time>, mut sc: ResMut<Showcase>, mut sim: ResMut<Sim>, mut visuals: ResMut<CarVisuals>, mut exit: MessageWriter<AppExit>) {
    let Some(dir) = sc.dir.clone() else { return };
    if sc.step >= SHOWCASE_STEPS {
        // Give the last screenshot a moment to be written.
        sc.wait += time.delta_secs();
        if sc.wait > 0.5 {
            exit.write(AppExit::Success);
        }
        return;
    }
    let (preset, team) = Showcase::step_car(sc.step);
    if sim.preset != preset || visuals.team != team {
        sim.preset = preset;
        visuals.team = team;
        sim.respawn(spawn_state(preset));
        sc.wait = 0.0;
    }
    sc.wait += time.delta_secs();
    // Let the car settle on its suspension and the textures (and their mips) load.
    if sc.wait > 2.5 {
        let path = dir.join(format!("showcase_{}_{}.png", preset.name(), team.name()));
        commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path));
        sc.step += 1;
        sc.wait = 0.0;
        if let Some((p, t)) = (sc.step < SHOWCASE_STEPS).then(|| Showcase::step_car(sc.step)) {
            sim.preset = p;
            visuals.team = t;
            sim.respawn(spawn_state(p));
        }
    }
}

// ------------------------------------------------------------------------------------ state

#[derive(Resource)]
struct Sim {
    stepper: FixedStepper,
    preset: HitboxPreset,
    maneuver: Option<HalfFlip>,
    last_input: Controls,
    frames: u32,
    ticks_last_frame: u32,
    /// Testing aid: keep the boost tank full (toggle with I).
    infinite_boost: bool,
    /// Bumped on every teleport, so the camera starts over instead of swinging across the map.
    respawns: u32,
    /// The state after each physics tick of the last frame, oldest first (the sounds and effects
    /// react to every tick, not only to the last one).
    ticks: Vec<CarState>,
}

impl Sim {
    fn new(preset: HitboxPreset) -> Sim {
        Sim { stepper: FixedStepper::new(spawn_state(preset)), preset, maneuver: None, last_input: Controls::default(), frames: 0, ticks_last_frame: 0, infinite_boost: true, respawns: 0, ticks: Vec::new() }
    }

    fn respawn(&mut self, state: CarState) {
        self.stepper.reset(state);
        self.respawns += 1;
    }
}

fn spawn_state(preset: HitboxPreset) -> CarState {
    let mut s = CarState::new(preset);
    // Kickoff-like spot, facing +Y (towards the far goal).
    s.position = RVec3::new(0.0, -3000.0, 17.0);
    s.orientation = RotMat::from_angles(std::f32::consts::FRAC_PI_2, 0.0, 0.0);
    s.boost_amount = 100.0;
    s
}

#[derive(Component)]
struct CarRoot;

#[derive(Component)]
struct CarBody;

/// Uniform scale = wheel radius (m); `spin` is the accumulated roll angle (rad).
#[derive(Component)]
struct Wheel {
    index: usize,
    spin: f32,
    spin_rate: f32,
}

/// The simple boost flame (placeholder car, or a real car without the extracted boost): an outer
/// cone and a brighter inner one, flickering in length.
#[derive(Component)]
struct BoostFlame {
    inner: bool,
}

#[derive(Component)]
struct Hud;

// ------------------------------------------------------------------------------------ spawning

fn spawn_car(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, sim: Res<Sim>, visuals: Res<CarVisuals>, assets: Res<AssetServer>) {
    let body_mat = materials.add(StandardMaterial { base_color: Color::srgb(0.95, 0.45, 0.1), perceptual_roughness: 0.4, metallic: 0.2, ..default() });
    let glass_mat = materials.add(StandardMaterial { base_color: Color::srgb(0.1, 0.12, 0.18), perceptual_roughness: 0.1, ..default() });
    let wheel_mat = materials.add(StandardMaterial { base_color: Color::srgb(0.08, 0.08, 0.08), perceptual_roughness: 0.9, ..default() });
    let flame_outer = materials.add(StandardMaterial { base_color: Color::srgba(1.0, 0.35, 0.05, 0.8), unlit: true, alpha_mode: AlphaMode::Add, ..default() });
    let flame_inner = materials.add(StandardMaterial { base_color: Color::srgba(1.0, 0.9, 0.55, 0.9), unlit: true, alpha_mode: AlphaMode::Add, ..default() });

    let root = commands.spawn((CarRoot, Transform::default(), Visibility::default())).id();
    commands.entity(root).with_children(|p| {
        p.spawn((CarBody, Transform::default(), Visibility::default())).with_children(|b| {
            if visuals.real {
                b.spawn(visuals::body_bundle(&assets, sim.preset, visuals.team));
            } else {
                b.spawn((Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))), MeshMaterial3d(body_mat), Transform::default(), Name::new("hull")));
                b.spawn((Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))), MeshMaterial3d(glass_mat), Transform::default(), Name::new("cabin")));
            }
            b.spawn((BoostFlame { inner: false }, Mesh3d(meshes.add(Cone { radius: 0.12, height: 0.6 })), MeshMaterial3d(flame_outer), Transform::default(), Visibility::Hidden));
            b.spawn((BoostFlame { inner: true }, Mesh3d(meshes.add(Cone { radius: 0.06, height: 0.4 })), MeshMaterial3d(flame_inner), Transform::default(), Visibility::Hidden));
        });
        let cylinder = meshes.add(Cylinder::new(1.0, 1.0));
        for i in 0..4 {
            let left = HitboxPreset::Octane.config().wheel(i).0.y < 0.0; // RL +Y is car right
            p.spawn((Wheel { index: i, spin: 0.0, spin_rate: 0.0 }, Transform::default(), Visibility::default())).with_children(|w| {
                if visuals.real {
                    w.spawn(visuals::wheel_bundle(&assets, left));
                } else {
                    // Cylinder axis (Y) -> wheel axle (Z).
                    w.spawn((Mesh3d(cylinder.clone()), MeshMaterial3d(wheel_mat.clone()), Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(1.0, 0.9, 1.0))));
                }
            });
        }
    });
}

/// Lays out hull/cabin for the current preset (hitbox in Bevy car-local space).
fn layout_body(preset: HitboxPreset, children: &Children, q: &mut Query<(&mut Transform, Option<&Name>, Option<&BoostFlame>), Without<CarRoot>>) {
    let cfg = preset.config();
    let half = cfg.effective_half_extents();
    let size = Vec3::new(half.x, half.z, half.y) * 2.0 / UU_PER_M;
    let center = dir_to_bevy(cfg.hitbox_pos_offset) / UU_PER_M;
    for c in children.iter() {
        let Ok((mut t, name, flame)) = q.get_mut(c) else { continue };
        match (name.map(|n| n.as_str()), flame.is_some()) {
            // Lower 62% of the hitbox.
            (Some("hull"), _) => *t = Transform::from_translation(center - Vec3::Y * size.y * 0.19).with_scale(size * Vec3::new(1.0, 0.62, 1.0)),
            (Some("cabin"), _) => {
                *t = Transform::from_translation(center + Vec3::new(-size.x * 0.12, size.y * 0.25, 0.0)).with_scale(Vec3::new(size.x * 0.45, size.y * 0.5, size.z * 0.8))
            }
            // The flame is placed every frame (it flickers), see `sync_car`.
            _ => {}
        }
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((Camera3d::default(), Projection::Perspective(PerspectiveProjection::default()), Transform::from_xyz(0.0, 3.0, -38.0).looking_at(Vec3::new(0.0, 0.5, -30.0), Vec3::Y)));
}

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        Hud,
        Text::new(""),
        TextColor(Color::WHITE),
        Node { position_type: PositionType::Absolute, top: Val::Px(10.0), left: Val::Px(12.0), ..default() },
    ));
}

// ------------------------------------------------------------------------------------ update

fn hotkeys(keys: Res<ButtonInput<KeyCode>>, mut sim: ResMut<Sim>, mut visuals: ResMut<CarVisuals>) {
    let presets = [
        (KeyCode::Digit1, HitboxPreset::Octane),
        (KeyCode::Digit2, HitboxPreset::Dominus),
        (KeyCode::Digit3, HitboxPreset::Plank),
        (KeyCode::Digit4, HitboxPreset::Breakout),
        (KeyCode::Digit5, HitboxPreset::Hybrid),
        (KeyCode::Digit6, HitboxPreset::Merc),
        (KeyCode::Digit7, HitboxPreset::Psyclops),
    ];
    for (key, preset) in presets {
        if keys.just_pressed(key) {
            sim.preset = preset;
            let s = spawn_state(preset);
            sim.respawn(s);
        }
    }
    if keys.just_pressed(KeyCode::KeyR) {
        let s = spawn_state(sim.preset);
        sim.respawn(s);
        sim.maneuver = None;
    }
    if keys.just_pressed(KeyCode::KeyH) {
        sim.maneuver = Some(HalfFlip::new());
    }
    if keys.just_pressed(KeyCode::KeyT) {
        visuals.team = if visuals.team == Team::Blue { Team::Orange } else { Team::Blue };
    }
    if keys.just_pressed(KeyCode::KeyI) {
        sim.infinite_boost = !sim.infinite_boost;
    }
}

fn simulate(time: Res<Time>, keys: Res<ButtonInput<KeyCode>>, gamepads: Query<&Gamepad>, spatial: SpatialQuery, colliders: ArenaColliderQuery, mut sim: ResMut<Sim>, autopilot: Res<Autopilot>) {
    sim.frames += 1;
    // Give avian a couple of frames to build its query pipeline for the static arena.
    if sim.frames < 3 {
        return;
    }
    let world = AvianWorld { spatial: &spatial, colliders: &colliders };
    let input = input::read_controls(&keys, &gamepads);
    let scripted = autopilot.enabled;
    if scripted && sim.stepper.tick_count == 0 {
        sim.respawn(Autopilot::start_state());
    }
    sim.last_input = if scripted { Autopilot::controls(sim.stepper.tick_count as f32 * rl_car_core::TICK_DT) } else { input };

    if sim.infinite_boost {
        sim.stepper.current.boost_amount = 100.0;
    }
    let Sim { stepper, maneuver, ticks: states, .. } = &mut *sim;
    states.clear();
    let mut tick = stepper.tick_count;
    let ticks = stepper.advance_with(time.delta_secs_f64(), &world, |state| {
        states.push(*state);
        tick += 1;
        if scripted {
            return Autopilot::controls(tick as f32 * rl_car_core::TICK_DT);
        }
        if let Some(m) = maneuver.as_mut() {
            match m.tick(state) {
                Some(c) => return c,
                None => *maneuver = None,
            }
        }
        input
    });
    sim.ticks_last_frame = ticks;
    // The closure saw the state before each tick; add the state after the last one.
    let last = sim.stepper.current;
    if ticks > 0 {
        sim.ticks.push(last);
    }

    // Fell out of the world (e.g. tunnelled through geometry at a seam): respawn.
    if sim.stepper.current.position.z < -500.0 {
        let s = spawn_state(sim.preset);
        sim.respawn(s);
    }
}

fn sync_car(
    mut commands: Commands,
    sim: Res<Sim>,
    visuals: Res<CarVisuals>,
    assets: Res<AssetServer>,
    mut root: Query<(&mut Transform, &Children), With<CarRoot>>,
    body: Query<(Entity, &Children), With<CarBody>>,
    real_bodies: Query<(Entity, &RealBody)>,
    mut parts: Query<(&mut Transform, Option<&Name>, Option<&BoostFlame>), Without<CarRoot>>,
    mut flames: Query<&mut Visibility, With<BoostFlame>>,
    mut last_preset: Local<Option<HitboxPreset>>,
    boost: Res<BoostData>,
    time: Res<Time>,
) {
    let Ok((mut t, children)) = root.single_mut() else { return };
    let (a, b) = (&sim.stepper.previous, &sim.stepper.current);
    let alpha = sim.stepper.alpha();
    let pa = pos_to_bevy(a.position);
    let pb = pos_to_bevy(b.position);
    t.translation = pa.lerp(pb, alpha);
    t.rotation = rot_to_bevy(&a.orientation.0).slerp(rot_to_bevy(&b.orientation.0), alpha);

    if *last_preset != Some(sim.preset) {
        for c in children.iter() {
            if let Ok((_, body_children)) = body.get(c) {
                layout_body(sim.preset, body_children, &mut parts);
            }
        }
        *last_preset = Some(sim.preset);
    }
    // Real model: respawn when the preset or team changed.
    for (e, real) in real_bodies.iter() {
        if real.0 != sim.preset || real.1 != visuals.team {
            commands.entity(e).despawn();
            if let Ok((body_entity, _)) = body.single() {
                commands.entity(body_entity).with_child(visuals::body_bundle(&assets, sim.preset, visuals.team));
            }
        }
    }
    // The game's boost replaces the simple flame when it was extracted for this car.
    let simple = !(visuals.real && boost.has(sim.preset));
    for mut v in flames.iter_mut() {
        *v = if b.is_boosting && simple { Visibility::Visible } else { Visibility::Hidden };
    }
    // Base on the back face of the hitbox, pointing backwards (a cone's apex is local +Y), and
    // flickering in length.
    let cfg = sim.preset.config();
    let half = cfg.effective_half_extents();
    let size = Vec3::new(half.x, half.z, half.y) * 2.0 / UU_PER_M;
    let center = dir_to_bevy(cfg.hitbox_pos_offset) / UU_PER_M;
    let now = time.elapsed_secs();
    for (mut tf, _, flame) in parts.iter_mut() {
        let Some(flame) = flame else { continue };
        let height = if flame.inner { 0.4 } else { 0.6 };
        let flicker = 1.0 + 0.12 * (now * 41.0 + flame.inner as u8 as f32).sin() + 0.08 * (now * 67.0).sin();
        let back = center + Vec3::new(-size.x * 0.5 - flicker * height * 0.5, -size.y * 0.1, 0.0);
        *tf = Transform::from_translation(back).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(1.0, flicker, 1.0));
    }
}

fn sync_wheels(time: Res<Time>, sim: Res<Sim>, visuals: Res<CarVisuals>, mut wheels: Query<(&mut Wheel, &mut Transform)>) {
    let s = &sim.stepper.current;
    let cfg = s.config();
    let forward_speed = s.velocity.dot(s.forward()); // uu/s
    for (mut w, mut t) in wheels.iter_mut() {
        let (cp, radius, _, front) = cfg.wheel(w.index);
        let susp = s.wheels[w.index].suspension_length;
        // Wheel centre in RL car-local space: hang down from the attachment by the suspension length.
        let local = RVec3::new(cp.x, cp.y, cp.z - susp);
        t.translation = dir_to_bevy(local) / UU_PER_M;
        // A real model puts its wheels at its own hubs (fore/aft and sideways).
        if let Some(anchor) = visuals.wheel_anchor(sim.preset, front, cp.y < 0.0) {
            t.translation.x = anchor.x;
            t.translation.z = anchor.z;
        }
        // Roll with the ground speed while touching; spin down slowly in the air.
        w.spin_rate = if s.wheel_contacts[w.index] { forward_speed / radius } else { w.spin_rate * (-0.5 * time.delta_secs()).exp() };
        w.spin = (w.spin - w.spin_rate * time.delta_secs()) % std::f32::consts::TAU;
        // Axle = car right (Bevy local Z): steer about car up, then roll about the axle.
        let steer = -s.wheels[w.index].steer_angle;
        t.rotation = Quat::from_rotation_y(steer) * Quat::from_rotation_z(w.spin);
        t.scale = Vec3::splat(radius / UU_PER_M);
    }
}

/// Rocket League's car camera (`rl_car_core::camera`) and the player's camera settings.
#[derive(Resource, Default)]
struct CameraRig {
    camera: CarCamera,
    /// Index into `CameraSettings::PRESETS` (C cycles).
    preset: usize,
    settings: CameraSettings,
    respawns: u32,
}

fn follow_camera(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    gamepads: Query<&Gamepad>,
    car: Query<&Transform, With<CarRoot>>,
    mut cam: Query<(&mut Transform, &mut Projection), (With<Camera3d>, Without<CarRoot>)>,
    mut rig: ResMut<CameraRig>,
    sim: Res<Sim>,
    showcase: Res<Showcase>,
    mut shakes: ResMut<fx::CameraShakes>,
) {
    let (Ok(car), Ok((mut cam, mut projection))) = (car.single(), cam.single_mut()) else { return };
    if showcase.dir.is_some() {
        let (fwd, right) = (car.rotation * Vec3::X, car.rotation * Vec3::Z);
        *cam = Transform::from_translation(car.translation + fwd * 1.8 + right * 1.4 + Vec3::Y * 0.6).looking_at(car.translation + Vec3::Y * 0.15, Vec3::Y);
        return;
    }
    if keys.just_pressed(KeyCode::KeyC) {
        rig.preset = (rig.preset + 1) % CameraSettings::PRESETS.len();
        rig.settings = CameraSettings::PRESETS[rig.preset].1;
    }
    if rig.respawns != sim.respawns {
        rig.respawns = sim.respawns;
        rig.camera.reset();
    }
    let target = CameraTarget::interpolated(&sim.stepper.previous, &sim.stepper.current, sim.stepper.alpha());
    let input = input::read_camera_input(&mouse, &gamepads);
    let settings = rig.settings;
    let view = rig.camera.update(&target, &input, &settings, time.delta_secs());

    let mut location = view.location;
    let o = view.orientation;
    // The game's camera shakes (jump, dodge, landing, impacts), in the camera's frame.
    let (shake_loc, shake_rot) = shakes.advance(time.delta_secs());
    location = location + o.forward() * shake_loc.x + o.right() * shake_loc.y + o.up() * shake_loc.z;
    // Camera_TA.ClipToField: never below 10 uu above the field floor.
    location.z = location.z.max(10.0);
    // Bevy cameras look down local -Z with +Y up and +X right.
    let basis = Mat3::from_cols(dir_to_bevy(o.right()), dir_to_bevy(o.up()), -dir_to_bevy(o.forward()));
    // Unreal pitch up / yaw right / roll right, about the camera's right / up / forward axes.
    let shake = Quat::from_rotation_x(shake_rot.x) * Quat::from_rotation_y(-shake_rot.y) * Quat::from_rotation_z(-shake_rot.z);
    *cam = Transform::from_translation(pos_to_bevy(location)).with_rotation(Quat::from_mat3(&basis).normalize() * shake);
    if let Projection::Perspective(p) = &mut *projection {
        p.fov = view.vertical_fov();
    }
}

fn update_hud(sim: Res<Sim>, rig: Res<CameraRig>, diagnostics: Res<Time>, mut hud: Query<&mut Text, With<Hud>>) {
    let Ok(mut text) = hud.single_mut() else { return };
    let s = &sim.stepper.current;
    let speed = s.velocity.length();
    let c = sim.last_input;
    let fps = 1.0 / diagnostics.delta_secs().max(1e-6);
    text.0 = format!(
        "rl_car_core Bevy demo (unofficial)\n\
         car: {:?}   speed: {:>4.0} uu/s ({:>3.0} km/h){}\n\
         boost: {:>3.0}{}   on ground: {}   wheels: {}\n\
         jumped: {}  double: {}  flipped: {}  {}\n\
         input  thr {:+.1} steer {:+.1} pitch {:+.1} yaw {:+.1} roll {:+.1} {}{}{}\n\
         {:.0} fps, {} physics ticks/frame (120 Hz)\n\
         camera: {} (FOV {:.0}, distance {:.0}, height {:.0}, angle {:.0}, stiffness {:.2}, swivel {:.1})\n\n\
         W/S throttle+pitch  A/D steer+yaw  Q/E air roll  Space jump\n\
         Shift boost  Ctrl powerslide/air roll  H half-flip  R reset  1-7 hitbox\n\
         T team  I infinite boost  C camera preset\n\
         gamepad right stick: swivel camera   R3 / middle mouse: rear view",
        sim.preset,
        speed,
        speed * 0.036,
        if s.is_supersonic { "  SUPERSONIC" } else { "" },
        s.boost_amount,
        if sim.infinite_boost { " (infinite)" } else { "" },
        s.on_ground,
        s.wheel_contacts.iter().map(|&w| if w { 'o' } else { '.' }).collect::<String>(),
        s.has_jumped,
        s.has_double_jumped,
        s.has_flipped,
        if sim.maneuver.is_some() { "[half-flip]" } else { "" },
        c.throttle,
        c.steer,
        c.pitch,
        c.yaw,
        c.roll,
        if c.jump { "J" } else { "" },
        if c.boost { "B" } else { "" },
        if c.handbrake { "H" } else { "" },
        fps,
        sim.ticks_last_frame,
        CameraSettings::PRESETS[rig.preset].0,
        rig.settings.fov,
        rig.settings.distance,
        rig.settings.height,
        rig.settings.angle,
        rig.settings.stiffness,
        rig.settings.swivel_speed,
    );
}
