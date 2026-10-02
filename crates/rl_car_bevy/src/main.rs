//! Bevy demo: a drivable Rocket League-style car powered by `rl_car_core`.
//!
//! Unofficial fan project — not affiliated with, endorsed by, or connected to Psyonix or Epic Games.
//!
//! Frame flow:
//!   input -> Controls -> FixedStepper (120 Hz ticks, accumulates Bevy frame time)
//!         -> rl_car_core::step(.., AvianWorld) for each tick
//!   render: interpolate previous/current tick state -> car Transform, wheels, camera, HUD.

#![allow(clippy::type_complexity)] // Bevy system parameter types.

mod arena;
mod collision;
mod convert;
mod input;
#[cfg(test)]
mod host_tests;

use avian3d::prelude::*;
use bevy::prelude::*;
use collision::{ArenaColliderQuery, AvianWorld};
use convert::*;
use rl_car_core::maneuvers::{HalfFlip, Maneuver};
use rl_car_core::{CarState, Controls, FixedStepper, HitboxPreset, RotMat, Vec3 as RVec3};

fn main() {
    let autopilot = Autopilot::from_args();
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "rl_car_core - Bevy demo (unofficial)".into(), ..default() }),
            ..default()
        }))
        .add_plugins(PhysicsPlugins::default())
        .insert_resource(ClearColor(Color::srgb(0.55, 0.68, 0.85)))
        .insert_resource(Sim::new(HitboxPreset::Octane))
        .insert_resource(CameraRig::default())
        .insert_resource(autopilot)
        .add_systems(Startup, (arena::spawn_arena, spawn_car, spawn_camera, spawn_hud))
        .add_systems(Update, (hotkeys, simulate, sync_car, sync_wheels, follow_camera, update_hud, autopilot_shots).chain())
        .run();
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

// ------------------------------------------------------------------------------------ state

#[derive(Resource)]
struct Sim {
    stepper: FixedStepper,
    preset: HitboxPreset,
    maneuver: Option<HalfFlip>,
    last_input: Controls,
    frames: u32,
    ticks_last_frame: u32,
}

impl Sim {
    fn new(preset: HitboxPreset) -> Sim {
        Sim { stepper: FixedStepper::new(spawn_state(preset)), preset, maneuver: None, last_input: Controls::default(), frames: 0, ticks_last_frame: 0 }
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

#[derive(Component)]
struct Wheel(usize);

#[derive(Component)]
struct BoostFlame;

#[derive(Component)]
struct Hud;

// ------------------------------------------------------------------------------------ spawning

fn spawn_car(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, sim: Res<Sim>) {
    let body_mat = materials.add(StandardMaterial { base_color: Color::srgb(0.95, 0.45, 0.1), perceptual_roughness: 0.4, metallic: 0.2, ..default() });
    let glass_mat = materials.add(StandardMaterial { base_color: Color::srgb(0.1, 0.12, 0.18), perceptual_roughness: 0.1, ..default() });
    let wheel_mat = materials.add(StandardMaterial { base_color: Color::srgb(0.08, 0.08, 0.08), perceptual_roughness: 0.9, ..default() });
    let flame_mat = materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.6, 0.1), emissive: LinearRgba::rgb(8.0, 3.0, 0.5), unlit: true, ..default() });

    let root = commands.spawn((CarRoot, Transform::default(), Visibility::default())).id();
    commands.entity(root).with_children(|p| {
        p.spawn((CarBody, Transform::default(), Visibility::default())).with_children(|b| {
            b.spawn((Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))), MeshMaterial3d(body_mat), Transform::default(), Name::new("hull")));
            b.spawn((Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))), MeshMaterial3d(glass_mat), Transform::default(), Name::new("cabin")));
            b.spawn((BoostFlame, Mesh3d(meshes.add(Cone { radius: 0.12, height: 0.6 })), MeshMaterial3d(flame_mat), Transform::default(), Visibility::Hidden));
        });
        for i in 0..4 {
            p.spawn((Wheel(i), Mesh3d(meshes.add(Cylinder::new(1.0, 1.0))), MeshMaterial3d(wheel_mat.clone()), Transform::default()));
        }
    });
    let _ = sim;
}

/// Lays out hull/cabin/flame for the current preset (hitbox in Bevy car-local space).
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
            (_, true) => {
                *t = Transform::from_translation(center + Vec3::new(-size.x * 0.5 - 0.25, -size.y * 0.1, 0.0)).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
            }
            _ => {}
        }
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((Camera3d::default(), Projection::Perspective(PerspectiveProjection { fov: 100f32.to_radians() * 0.75, ..default() }), Transform::from_xyz(0.0, 3.0, -38.0).looking_at(Vec3::new(0.0, 0.5, -30.0), Vec3::Y)));
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

fn hotkeys(keys: Res<ButtonInput<KeyCode>>, mut sim: ResMut<Sim>) {
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
            sim.stepper.reset(s);
        }
    }
    if keys.just_pressed(KeyCode::KeyR) {
        let s = spawn_state(sim.preset);
        sim.stepper.reset(s);
        sim.maneuver = None;
    }
    if keys.just_pressed(KeyCode::KeyH) {
        sim.maneuver = Some(HalfFlip::new());
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
        sim.stepper.reset(Autopilot::start_state());
    }
    sim.last_input = if scripted { Autopilot::controls(sim.stepper.tick_count as f32 * rl_car_core::TICK_DT) } else { input };

    let Sim { stepper, maneuver, .. } = &mut *sim;
    let mut tick = stepper.tick_count;
    let ticks = stepper.advance_with(time.delta_secs_f64(), &world, |state| {
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

    // Fell out of the world (e.g. tunnelled through geometry at a seam): respawn.
    if sim.stepper.current.position.z < -500.0 {
        let s = spawn_state(sim.preset);
        sim.stepper.reset(s);
    }
}

fn sync_car(sim: Res<Sim>, mut root: Query<(&mut Transform, &Children), With<CarRoot>>, body: Query<&Children, With<CarBody>>, mut parts: Query<(&mut Transform, Option<&Name>, Option<&BoostFlame>), Without<CarRoot>>, mut flames: Query<&mut Visibility, With<BoostFlame>>, mut last_preset: Local<Option<HitboxPreset>>) {
    let Ok((mut t, children)) = root.single_mut() else { return };
    let (a, b) = (&sim.stepper.previous, &sim.stepper.current);
    let alpha = sim.stepper.alpha();
    let pa = pos_to_bevy(a.position);
    let pb = pos_to_bevy(b.position);
    t.translation = pa.lerp(pb, alpha);
    t.rotation = rot_to_bevy(&a.orientation.0).slerp(rot_to_bevy(&b.orientation.0), alpha);

    if *last_preset != Some(sim.preset) {
        for c in children.iter() {
            if let Ok(body_children) = body.get(c) {
                layout_body(sim.preset, body_children, &mut parts);
            }
        }
        *last_preset = Some(sim.preset);
    }
    for mut v in flames.iter_mut() {
        *v = if b.is_boosting { Visibility::Visible } else { Visibility::Hidden };
    }
}

fn sync_wheels(sim: Res<Sim>, mut wheels: Query<(&Wheel, &mut Transform)>) {
    let s = &sim.stepper.current;
    let cfg = s.config();
    for (w, mut t) in wheels.iter_mut() {
        let (cp, radius, _, _) = cfg.wheel(w.0);
        let susp = s.wheels[w.0].suspension_length;
        // Wheel centre in RL car-local space: hang down from the attachment by the suspension length.
        let local = RVec3::new(cp.x, cp.y, cp.z - susp);
        let r = radius / UU_PER_M;
        t.translation = dir_to_bevy(local) / UU_PER_M;
        // Cylinder axis (Y) -> car right (Bevy local Z), then steer about car up.
        let steer = -s.wheels[w.0].steer_angle;
        t.rotation = Quat::from_rotation_y(steer) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        t.scale = Vec3::new(r, 0.12, r);
    }
}

#[derive(Resource)]
struct CameraRig {
    pos: Vec3,
    look: Vec3,
    up: Vec3,
}

impl Default for CameraRig {
    fn default() -> Self {
        CameraRig { pos: Vec3::new(0.0, 3.0, -38.0), look: Vec3::new(0.0, 0.5, -30.0), up: Vec3::Y }
    }
}

fn follow_camera(time: Res<Time>, car: Query<&Transform, With<CarRoot>>, mut cam: Query<&mut Transform, (With<Camera3d>, Without<CarRoot>)>, mut rig: ResMut<CameraRig>, sim: Res<Sim>) {
    let (Ok(car), Ok(mut cam)) = (car.single(), cam.single_mut()) else { return };
    let s = &sim.stepper.current;
    let fwd = car.rotation * Vec3::X;
    let car_up = car.rotation * Vec3::Y;
    // On a surface (floor, wall, ceiling): chase in the car's own frame, so wall driving reads
    // like floor driving. In the air: world-up chase behind the direction of travel.
    let (heading, up) = if s.on_ground {
        (fwd, car_up)
    } else {
        let vel = dir_to_bevy(s.velocity) / UU_PER_M;
        let flat = if vel.length() > 5.0 { Vec3::new(vel.x, 0.0, vel.z) } else { Vec3::new(fwd.x, 0.0, fwd.z) };
        (flat.try_normalize().unwrap_or(Vec3::Z), Vec3::Y)
    };
    let target_pos = car.translation - heading * 2.8 + up * 1.1;
    let target_look = car.translation + heading * 1.5 + up * 0.35;
    let k = 1.0 - (-12.0 * time.delta_secs()).exp();
    rig.pos = rig.pos.lerp(target_pos, k);
    rig.look = rig.look.lerp(target_look, k);
    rig.up = rig.up.lerp(up, 1.0 - (-6.0 * time.delta_secs()).exp()).normalize();
    *cam = Transform::from_translation(rig.pos).looking_at(rig.look, rig.up);
}

fn update_hud(sim: Res<Sim>, diagnostics: Res<Time>, mut hud: Query<&mut Text, With<Hud>>) {
    let Ok(mut text) = hud.single_mut() else { return };
    let s = &sim.stepper.current;
    let speed = s.velocity.length();
    let c = sim.last_input;
    let fps = 1.0 / diagnostics.delta_secs().max(1e-6);
    text.0 = format!(
        "rl_car_core Bevy demo (unofficial)\n\
         car: {:?}   speed: {:>4.0} uu/s ({:>3.0} km/h){}\n\
         boost: {:>3.0}   on ground: {}   wheels: {}\n\
         jumped: {}  double: {}  flipped: {}  {}\n\
         input  thr {:+.1} steer {:+.1} pitch {:+.1} yaw {:+.1} roll {:+.1} {}{}{}\n\
         {:.0} fps, {} physics ticks/frame (120 Hz)\n\n\
         W/S throttle+pitch  A/D steer+yaw  Q/E air roll  Space jump\n\
         Shift boost  Ctrl powerslide/air roll  H half-flip  R reset  1-7 hitbox",
        sim.preset,
        speed,
        speed * 0.036,
        if s.is_supersonic { "  SUPERSONIC" } else { "" },
        s.boost_amount,
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
    );
}
