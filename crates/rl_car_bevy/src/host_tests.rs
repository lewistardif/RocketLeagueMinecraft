//! Headless tests of the avian collision host: the car is simulated through `AvianWorld` against
//! the real arena colliders (no window, no rendering).

#![allow(clippy::field_reassign_with_default)]

use crate::arena;
use crate::collision::{ArenaColliderQuery, AvianWorld};
use avian3d::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use rl_car_core::*;
use rl_car_core::Vec3;

fn headless_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), bevy::mesh::MeshPlugin, PhysicsPlugins::default()));
    app.add_systems(Startup, |mut commands: Commands| arena::spawn_colliders(&mut commands));
    // Let avian create Position/Rotation and build its spatial query pipeline.
    for _ in 0..5 {
        app.update();
    }
    app
}

/// Runs `ticks` ticks through the avian host, with per-tick controls.
fn run_host(app: &mut App, start: CarState, ticks: u32, controls: impl Fn(u32, &CarState) -> Controls + Send + Sync + 'static) -> Vec<CarState> {
    app.world_mut()
        .run_system_once(move |spatial: SpatialQuery, colliders: ArenaColliderQuery| {
            let world = AvianWorld { spatial: &spatial, colliders: &colliders };
            let mut s = start;
            let mut out = vec![s];
            for t in 0..ticks {
                s = step(&s, &controls(t, &s), &world, TICK_DT);
                out.push(s);
            }
            out
        })
        .unwrap()
}

#[test]
fn rests_on_floor_like_plane_world() {
    let mut app = headless_app();
    let start = CarState::default();
    let host = run_host(&mut app, start, 240, |_, _| Controls::default());
    let last = host.last().unwrap();
    assert!(last.on_ground && last.wheel_contacts.iter().all(|&c| c), "{:?}", last.wheel_contacts);
    assert!((last.position.z - 17.0).abs() < 0.1, "rest z {}", last.position.z);

    // Same geometry analytically: the avian host should agree with PlaneWorld closely.
    let floor = PlaneWorld::floor();
    let mut s = start;
    for _ in 0..240 {
        s = step(&s, &Controls::default(), &floor, TICK_DT);
    }
    assert!((s.position - last.position).length() < 1e-3, "plane {:?} vs avian {:?}", s.position, last.position);
}

#[test]
fn drives_through_quarter_pipe_onto_wall() {
    let mut app = headless_app();
    let mut start = CarState::default();
    start.position = Vec3::new(2800.0, 0.0, 17.0);
    start.velocity = Vec3::new(1400.0, 0.0, 0.0);
    start.boost_amount = 100.0;
    let host = run_host(&mut app, start, 150, |_, _| Controls { throttle: 1.0, boost: true, ..Default::default() });
    let max_z = host.iter().map(|s| s.position.z).fold(0.0f32, f32::max);
    let on_wall = host.iter().any(|s| s.on_ground && s.up().x < -0.9);
    assert!(on_wall, "never reached the wall");
    assert!(max_z > 800.0, "did not climb the wall: max z {max_z}");
    assert!(host.iter().all(|s| s.position.x < 4096.0), "went through the wall");
}

#[test]
fn lands_on_roof_without_tunnelling() {
    let mut app = headless_app();
    let mut start = CarState::default();
    start.position = Vec3::new(0.0, 0.0, 300.0);
    start.orientation = RotMat::from_angles(0.0, 0.0, std::f32::consts::PI);
    let host = run_host(&mut app, start, 240, |_, _| Controls::default());
    let last = host.last().unwrap();
    assert!(last.up().z < -0.95, "should be lying on its roof");
    assert!((35.0..45.0).contains(&last.position.z), "roof rest height {}", last.position.z);
    assert!(last.velocity.length() < 5.0);
}

/// Box-vs-floor contacts through avian must reproduce the analytic plane (the geometry the oracle
/// validated): same deepest-vertex contact points, same trajectory.
#[test]
fn roof_drop_matches_plane_world() {
    let mut app = headless_app();
    let mut start = CarState::default();
    start.position = Vec3::new(0.0, 0.0, 300.0);
    start.orientation = RotMat::from_angles(0.3, 0.0, std::f32::consts::PI);
    let host = run_host(&mut app, start, 180, |_, _| Controls::default());
    let floor = PlaneWorld::floor();
    let mut s = start;
    let mut max_err = 0.0f32;
    for h in host.iter().skip(1) {
        s = step(&s, &Controls::default(), &floor, TICK_DT);
        max_err = max_err.max((s.position - h.position).length());
    }
    assert!(max_err < 1e-3, "avian host diverged from PlaneWorld by {max_err} uu");
}

