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
