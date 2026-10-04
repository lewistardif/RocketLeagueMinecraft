#![allow(clippy::missing_safety_doc)]

use crate::{Car, read_v};
use rl_car_core::{Bump, BumpVictim, bump};

#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_car_bump(
    car: *const Car,
    victim_pos: *const f32,
    victim_vel: *const f32,
    victim_on_ground: u32,
    victim_up: *const f32,
    contact_local_x: f32,
    force_scale: f32,
    allow_demolish: u32,
    out_vel: *mut f32,
) -> u32 {
    let Some(c) = (unsafe { car.as_ref() }) else { return 0 };
    if victim_pos.is_null() || victim_vel.is_null() || victim_up.is_null() {
        return 0;
    }
    let victim = unsafe {
        BumpVictim { position: read_v(victim_pos), velocity: read_v(victim_vel), on_ground: victim_on_ground != 0, up: read_v(victim_up) }
    };
    match bump(&c.stepper.current, &victim, contact_local_x, force_scale, allow_demolish != 0) {
        None => 0,
        Some(Bump::Demolish) => 2,
        Some(Bump::Push(v)) => {
            if !out_vel.is_null() {
                unsafe { std::slice::from_raw_parts_mut(out_vel, 3) }.copy_from_slice(&v.to_array());
            }
            1
        }
    }
}

/// Adds `delta` (3 floats, uu/s) to the car's velocity, as Rocket League does to a car that gets
/// bumped. Interpolation is preserved.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_car_add_velocity(car: *mut Car, delta: *const f32) {
    let Some(c) = (unsafe { car.as_mut() }) else { return };
    if delta.is_null() {
        return;
    }
    let d = unsafe { read_v(delta) };
    if d.x.is_finite() && d.y.is_finite() && d.z.is_finite() {
        c.stepper.current.velocity += d;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    #[test]
    fn bump_then_add_velocity_through_the_c_api() {
        unsafe {
            let attacker = rlcar_car_new(0);
            let victim = rlcar_car_new(0);
            let world = rlcar_world_new();
            let floor = [-5000.0f32, -5000.0, -100.0, 5000.0, 5000.0, 0.0];
            rlcar_world_set_boxes(world, floor.as_ptr(), 1);
            rlcar_car_step(attacker, world, 120, 1.0, 0.0, 0.0, 0.0, 0.0, buttons::BOOST);
            let mut pose = [0.0f32; POSE_FLOATS];
            rlcar_car_pose(attacker, 1.0, pose.as_mut_ptr());
            let ahead = [pose[0] + 200.0, pose[1], pose[2]];
            let (still, up) = ([0.0f32; 3], [0.0f32, 0.0, 1.0]);
            let mut dv = [0.0f32; 3];
            let r = rlcar_car_bump(attacker, ahead.as_ptr(), still.as_ptr(), 1, up.as_ptr(), 80.0, 1.0, 1, dv.as_mut_ptr());
            assert_eq!(r, 1, "expected a push");
            assert!(dv[0] > 500.0 && dv[2] > 0.0, "{dv:?}");

            rlcar_car_add_velocity(victim, dv.as_ptr());
            rlcar_car_pose(victim, 1.0, pose.as_mut_ptr());
            assert_eq!(&pose[12..15], &dv);
            let nan = [f32::NAN, 0.0, 0.0];
            rlcar_car_add_velocity(victim, nan.as_ptr());
            rlcar_car_pose(victim, 1.0, pose.as_mut_ptr());
            assert_eq!(&pose[12..15], &dv);
            rlcar_world_free(world);
            rlcar_car_free(victim);
            rlcar_car_free(attacker);
        }
    }
}
