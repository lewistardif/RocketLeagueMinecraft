//! Behavioural tests of the core on its own (no oracle needed).

use rl_car_core::maneuvers::{HalfFlip, Maneuver};
use rl_car_core::*;

fn run(state: CarState, c: Controls, world: &dyn CollisionWorld, ticks: u32) -> CarState {
    let mut s = state;
    for _ in 0..ticks {
        s = step(&s, &c, world, TICK_DT);
    }
    s
}

#[test]
fn deterministic_bit_for_bit() {
    let world = PlaneWorld::soccar_box();
    let script = |t: u32| Controls {
        throttle: 1.0,
        steer: if (t / 50).is_multiple_of(2) { 0.7 } else { -0.4 },
        jump: (100..110).contains(&t) || t == 118,
        pitch: if t > 112 { -1.0 } else { 0.0 },
        boost: !t.is_multiple_of(3),
        ..Default::default()
    };
    let go = || {
        let mut s = CarState::new(HitboxPreset::Dominus);
        for t in 0..600 {
            s = step(&s, &script(t), &world, TICK_DT);
        }
        s
    };
    let (a, b) = (go(), go());
    assert_eq!(a, b);
    assert_eq!(a.position.x.to_bits(), b.position.x.to_bits());
}

#[test]
fn throttle_top_speed_is_about_1410() {
    let s = run(CarState::default(), Controls { throttle: 1.0, ..Default::default() }, &PlaneWorld::floor(), 1200);
    let v = s.velocity.length();
    assert!((1400.0..=1411.0).contains(&v), "speed {v}");
}

#[test]
fn boost_reaches_2300_and_supersonic() {
    let full = CarState { boost_amount: 100.0, ..CarState::default() };
    let s = run(full, Controls { throttle: 1.0, boost: true, ..Default::default() }, &PlaneWorld::floor(), 300);
    assert!((s.velocity.length() - 2300.0).abs() < 0.5, "{}", s.velocity.length());
    assert!(s.is_supersonic);
    // Spawn boost (33.3) at 33.3 boost/s runs empty after 1 s.
    let s = run(CarState::default(), Controls { throttle: 1.0, boost: true, ..Default::default() }, &PlaneWorld::floor(), 121);
    assert_eq!(s.boost_amount, 0.0);
}

#[test]
fn free_fall_matches_gravity() {
    let mut s = CarState::default();
    s.position.z = 5000.0;
    let s = run(s, Controls::default(), &EmptyWorld, 120);
    assert!((s.velocity.z - -650.0).abs() < 0.01, "{}", s.velocity.z);
}

#[test]
fn full_jump_height() {
    let world = PlaneWorld::floor();
    let mut s = CarState::default();
    let mut max_z: f32 = 0.0;
    for t in 0..150 {
        s = step(&s, &Controls { jump: t < 30, ..Default::default() }, &world, TICK_DT);
        max_z = max_z.max(s.position.z);
    }
    // RocketSim's full single jump peaks at z = 231.9 (~215 uu above rest height).
    assert!((max_z - 231.9).abs() < 0.5, "jump apex {max_z}");
}

#[test]
fn fixed_stepper_is_frame_rate_independent() {
    let world = PlaneWorld::floor();
    let c = Controls { throttle: 1.0, steer: 0.5, boost: true, ..Default::default() };
    let mut a = FixedStepper::new(CarState::default());
    let mut b = FixedStepper::new(CarState::default());
    for _ in 0..60 {
        a.advance(1.0 / 60.0, &c, &world);
    }
    for _ in 0..144 {
        b.advance(1.0 / 144.0, &c, &world);
    }
    assert_eq!(a.tick_count, b.tick_count);
    assert_eq!(a.current, b.current);
}

#[test]
fn half_flip_turns_the_car_around() {
    let world = PlaneWorld::floor();
    let mut s = CarState::default();
    s.velocity.x = -800.0; // driving backwards along +X facing
    let mut m = HalfFlip::new();
    let mut ticks = 0;
    while let Some(c) = m.tick(&s) {
        s = step(&s, &c, &world, TICK_DT);
        ticks += 1;
    }
    assert!(ticks < 240, "half flip did not finish");
    assert!(s.on_ground);
    assert!(s.up().z > 0.9, "not upright: {:?}", s.up());
    assert!(s.forward().x < -0.8, "not facing back: {:?}", s.forward());
    assert!(s.velocity.x < -500.0);
}

#[test]
fn rest_height_matches_spawn() {
    let s = run(CarState::default(), Controls::default(), &PlaneWorld::floor(), 240);
    assert!((s.position.z - 17.0).abs() < 0.1, "rest z {}", s.position.z);
    assert!(s.on_ground && s.wheel_contacts.iter().all(|&c| c));
}
