use rl_car_core::*;

fn horizontal(v: Vec3) -> f32 {
    (v.x * v.x + v.y * v.y).sqrt()
}

#[test]
fn chained_front_flips_reach_supersonic_without_boost() {
    let world = PlaneWorld::floor();
    let mut car = CarState::new(HitboxPreset::Octane);
    let drive = Controls { throttle: 1.0, ..Default::default() };
    for _ in 0..360 {
        car = step(&car, &drive, &world, TICK_DT);
    }
    let mut supersonic = false;
    let mut top = 0.0f32;
    for _ in 0..6 {
        let script = [(2, Controls { jump: true, ..drive }), (6, drive), (2, Controls { jump: true, pitch: -1.0, ..drive }), (150, drive)];
        for (ticks, c) in script {
            for _ in 0..ticks {
                car = step(&car, &c, &world, TICK_DT);
                supersonic |= car.is_supersonic;
                top = top.max(horizontal(car.velocity));
            }
        }
    }
    assert!(supersonic, "top speed {top}");
    assert!(top >= 2200.0);
    assert!(car.boost_amount == CarState::new(HitboxPreset::Octane).boost_amount);
}

#[test]
fn wavedash_gains_speed() {
    let world = PlaneWorld::floor();
    let drive = Controls { throttle: 1.0, ..Default::default() };
    let mut start = CarState::new(HitboxPreset::Octane);
    start.velocity = Vec3::new(1000.0, 0.0, 0.0);
    for _ in 0..30 {
        start = step(&start, &drive, &world, TICK_DT);
    }
    let base = horizontal(start.velocity);
    let mut best = 0.0f32;
    for dodge_at in 60..130 {
        let mut car = start;
        for t in 0..dodge_at + 120 {
            let c = if t < 2 {
                Controls { jump: true, ..drive }
            } else if t < dodge_at {
                Controls { pitch: 0.3, ..drive }
            } else if t < dodge_at + 2 {
                Controls { jump: true, pitch: -1.0, ..drive }
            } else {
                drive
            };
            car = step(&car, &c, &world, TICK_DT);
            if t == dodge_at + 3 && !car.on_ground {
                break;
            }
        }
        if car.on_ground && car.orientation.up().z > 0.95 {
            best = best.max(horizontal(car.velocity));
        }
    }
    assert!(best > base + 300.0, "base {base}, best wavedash {best}");
}
