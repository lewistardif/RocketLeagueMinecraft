//! # rl_car_ffi
//!
//! A C ABI over [`rl_car_core`], so engines that are not written in Rust (Minecraft/Java via the
//! FFM API, Unity/C#, C++, ...) can drive the car. Everything crosses the boundary as plain
//! numbers, in **Rocket League space and units** (uu, Z-up, left-handed; see `rl_car_core`).
//! Hosts convert once at their side.
//!
//! World geometry comes from a [`BoxWorld`]: the host hands over the axis-aligned collision
//! boxes around the car (for a voxel game, the blocks), and the world turns them into a seamless
//! surface. A world can be shared by any number of cars.
//!
//! All functions accept null handles and return a neutral value for them. Nothing here is
//! thread-safe: a handle must only be used from one thread at a time.

pub mod box_world;
pub mod snapshot;

pub use box_world::{Aabb, BoxWorld, Face};

use rl_car_core::{CarState, Controls, FixedStepper, HitboxPreset, Mat3, Quat, RotMat, TICK_DT, Vec3, step_with};
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Bumped whenever a signature or a buffer layout below changes.
pub const ABI_VERSION: u32 = 2;

/// Floats written by [`rlcar_car_pose`]:
///
/// | index | content |
/// |---|---|
/// | 0..3 | position (uu) |
/// | 3..12 | orientation, columns = forward, right, up (world space) |
/// | 12..15 | velocity (uu/s) |
/// | 15..18 | angular velocity (rad/s) |
/// | 18 | boost amount (0..=100) |
/// | 19..31 | wheel centres in car-local space (uu), FR, FL, BR, BL |
/// | 31..35 | wheel radii (uu) |
/// | 35..39 | wheel steer angles (rad, positive = right) |
/// | 39 | forward speed (uu/s, negative when reversing) |
pub const POSE_FLOATS: usize = 40;

/// Bits of the flags returned by [`rlcar_car_pose`].
pub mod flags {
    pub const ON_GROUND: u32 = 1 << 0;
    pub const BOOSTING: u32 = 1 << 1;
    pub const SUPERSONIC: u32 = 1 << 2;
    pub const HAS_FLIP_OR_JUMP: u32 = 1 << 3;
    pub const FLIPPING: u32 = 1 << 4;
    pub const JUMPING: u32 = 1 << 5;
    /// Bit `WHEEL_CONTACT_SHIFT + i` = wheel `i` touches the ground.
    pub const WHEEL_CONTACT_SHIFT: u32 = 8;
}

/// Bits of the `buttons` argument.
pub mod buttons {
    pub const JUMP: u32 = 1 << 0;
    pub const BOOST: u32 = 1 << 1;
    pub const HANDBRAKE: u32 = 1 << 2;
}

/// A simulated car: the 120 Hz stepper plus its interpolation state.
pub struct Car {
    pub stepper: FixedStepper,
}

fn guard<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

fn controls(throttle: f32, steer: f32, pitch: f32, yaw: f32, roll: f32, b: u32) -> Controls {
    Controls {
        throttle,
        steer,
        pitch,
        yaw,
        roll,
        jump: b & buttons::JUMP != 0,
        boost: b & buttons::BOOST != 0,
        handbrake: b & buttons::HANDBRAKE != 0,
    }
}

unsafe fn read_v(p: *const f32) -> Vec3 {
    let a = unsafe { std::slice::from_raw_parts(p, 3) };
    Vec3::new(a[0], a[1], a[2])
}

#[unsafe(no_mangle)]
pub extern "C" fn rlcar_abi_version() -> u32 {
    ABI_VERSION
}

// ------------------------------------------------------------------------------------- world

#[unsafe(no_mangle)]
pub extern "C" fn rlcar_world_new() -> *mut BoxWorld {
    Box::into_raw(Box::default())
}

/// # Safety
/// `world` is null or a pointer from [`rlcar_world_new`] not freed yet.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_world_free(world: *mut BoxWorld) {
    if !world.is_null() {
        drop(unsafe { Box::from_raw(world) });
    }
}

/// Replaces the world's geometry with `count` boxes: `6 * count` floats, each box as
/// `min.x, min.y, min.z, max.x, max.y, max.z` (uu). Returns the number of surface faces.
///
/// # Safety
/// `world` as above; `boxes` points to `6 * count` readable floats (may be null if `count` is 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_world_set_boxes(world: *mut BoxWorld, boxes: *const f32, count: u32) -> u32 {
    let Some(w) = (unsafe { world.as_mut() }) else { return 0 };
    let raw: &[f32] = if count == 0 || boxes.is_null() { &[] } else { unsafe { std::slice::from_raw_parts(boxes, count as usize * 6) } };
    guard(0, || {
        let list: Vec<Aabb> = raw
            .as_chunks::<6>()
            .0
            .iter()
            .map(|b| Aabb { min: Vec3::new(b[0], b[1], b[2]), max: Vec3::new(b[3], b[4], b[5]) })
            .collect();
        w.set_boxes(&list);
        w.faces().len() as u32
    })
}

// --------------------------------------------------------------------------------------- car

/// A new car resting at the origin, facing +X. `preset` indexes `HitboxPreset::ALL`
/// (0 Octane, 1 Dominus, 2 Plank, 3 Breakout, 4 Hybrid, 5 Merc, 6 Psyclops).
#[unsafe(no_mangle)]
pub extern "C" fn rlcar_car_new(preset: u32) -> *mut Car {
    let p = HitboxPreset::ALL.get(preset as usize).copied().unwrap_or_default();
    Box::into_raw(Box::new(Car { stepper: FixedStepper::new(CarState::new(p)) }))
}

/// # Safety
/// `car` is null or a pointer from [`rlcar_car_new`] not freed yet.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_car_free(car: *mut Car) {
    if !car.is_null() {
        drop(unsafe { Box::from_raw(car) });
    }
}

/// Places the car at `pos` (3 floats, uu) with Rocket League Euler angles (radians), at rest,
/// keeping its preset and boost. Does not interpolate from the old pose.
///
/// # Safety
/// `car` as above; `pos` points to 3 readable floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_car_reset(car: *mut Car, pos: *const f32, yaw: f32, pitch: f32, roll: f32) {
    let Some(c) = (unsafe { car.as_mut() }) else { return };
    if pos.is_null() {
        return;
    }
    let pos = unsafe { read_v(pos) };
    let old = c.stepper.current;
    let mut s = CarState::new(old.hitbox_preset);
    s.boost_amount = old.boost_amount;
    s.position = pos;
    s.orientation = RotMat::from_angles(yaw, pitch, roll);
    s.on_ground = false;
    c.stepper.reset(s);
}

/// Shifts the car (and its cached contacts) by `delta` uu, for hosts that move their local
/// origin to keep coordinates small. Interpolation is preserved.
///
/// # Safety
/// `car` as above; `delta` points to 3 readable floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_car_translate(car: *mut Car, delta: *const f32) {
    let Some(c) = (unsafe { car.as_mut() }) else { return };
    if delta.is_null() {
        return;
    }
    let d = unsafe { read_v(delta) };
    for s in [&mut c.stepper.previous, &mut c.stepper.current] {
        translate_state(s, d);
    }
}

pub fn translate_state(s: &mut CarState, d: Vec3) {
    s.position += d;
    for w in s.wheels.iter_mut() {
        if let Some((p, _)) = w.contact.as_mut() {
            *p += d;
        }
    }
    // Manifold world points are stored in Bullet units.
    let d_bt = d * rl_car_core::consts::UU_TO_BT;
    for m in s.manifolds.list.iter_mut() {
        for p in m.points.iter_mut().take(m.count) {
            p.world_b += d_bt;
        }
    }
}

/// Runs exactly `ticks` 1/120 s ticks with the same controls (for hosts with their own fixed
/// tick, e.g. 6 per Minecraft server tick). Afterwards `previous` is the state before the last
/// tick, so [`rlcar_car_pose`] with `alpha` in 0..1 interpolates within that tick.
///
/// # Safety
/// `car` and `world` are valid handles or null (a null world means empty space).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_car_step(
    car: *mut Car,
    world: *const BoxWorld,
    ticks: u32,
    throttle: f32,
    steer: f32,
    pitch: f32,
    yaw: f32,
    roll: f32,
    buttons: u32,
) {
    let Some(c) = (unsafe { car.as_mut() }) else { return };
    let empty = BoxWorld::default();
    let w = unsafe { world.as_ref() }.unwrap_or(&empty);
    let ctl = controls(throttle, steer, pitch, yaw, roll, buttons);
    guard((), || {
        let st = &mut c.stepper;
        for _ in 0..ticks {
            st.previous = st.current;
            st.current = step_with(&st.current, &ctl, w, &st.config, TICK_DT);
            st.tick_count += 1;
        }
    });
}

/// Adds `frame_dt` seconds of real time and runs as many 1/120 s ticks as fit (for hosts that
/// simulate per rendered frame). Returns the number of ticks run; use [`rlcar_car_alpha`] for
/// the interpolation factor.
///
/// # Safety
/// As [`rlcar_car_step`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_car_advance(
    car: *mut Car,
    world: *const BoxWorld,
    frame_dt: f64,
    throttle: f32,
    steer: f32,
    pitch: f32,
    yaw: f32,
    roll: f32,
    buttons: u32,
) -> u32 {
    let Some(c) = (unsafe { car.as_mut() }) else { return 0 };
    let empty = BoxWorld::default();
    let w = unsafe { world.as_ref() }.unwrap_or(&empty);
    let ctl = controls(throttle, steer, pitch, yaw, roll, buttons);
    guard(0, || c.stepper.advance(frame_dt, &ctl, w))
}

/// Interpolation factor (0..1) between the previous and current tick after [`rlcar_car_advance`].
///
/// # Safety
/// `car` as above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_car_alpha(car: *const Car) -> f32 {
    unsafe { car.as_ref() }.map_or(1.0, |c| c.stepper.alpha())
}

/// Writes [`POSE_FLOATS`] floats describing the car interpolated `alpha` (0..1) of the way from
/// the previous to the current tick, and returns the [`flags`] of the current tick.
///
/// # Safety
/// `car` as above; `out` points to [`POSE_FLOATS`] writable floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_car_pose(car: *const Car, alpha: f32, out: *mut f32) -> u32 {
    let Some(c) = (unsafe { car.as_ref() }) else { return 0 };
    if out.is_null() {
        return 0;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, POSE_FLOATS) };
    guard(0, || write_pose(&c.stepper.previous, &c.stepper.current, alpha, out))
}

pub fn write_pose(a: &CarState, b: &CarState, alpha: f32, out: &mut [f32]) -> u32 {
    let t = if alpha.is_finite() { alpha.clamp(0.0, 1.0) } else { 1.0 };
    let lerp = |x: f32, y: f32| x + (y - x) * t;
    let lerp_v = |x: Vec3, y: Vec3| x + (y - x) * t;
    let mut put = |i: usize, v: Vec3| out[i..i + 3].copy_from_slice(&v.to_array());

    put(0, lerp_v(a.position, b.position));
    let rot = Mat3::from_quat(nlerp(a.orientation.0.to_quat(), b.orientation.0.to_quat(), t));
    put(3, rot.col(0));
    put(6, rot.col(1));
    put(9, rot.col(2));
    put(12, lerp_v(a.velocity, b.velocity));
    put(15, lerp_v(a.angular_velocity, b.angular_velocity));
    out[18] = lerp(a.boost_amount, b.boost_amount);
    let cfg = b.config();
    for i in 0..4 {
        let (cp, radius, _, _) = cfg.wheel(i);
        let susp = lerp(a.wheels[i].suspension_length, b.wheels[i].suspension_length);
        let c = Vec3::new(cp.x, cp.y, cp.z - susp);
        out[19 + i * 3..22 + i * 3].copy_from_slice(&c.to_array());
        out[31 + i] = radius;
        out[35 + i] = lerp(a.wheels[i].steer_angle, b.wheels[i].steer_angle);
    }
    out[39] = b.velocity.dot(b.forward());

    let mut f = 0;
    for (bit, on) in [
        (flags::ON_GROUND, b.on_ground),
        (flags::BOOSTING, b.is_boosting),
        (flags::SUPERSONIC, b.is_supersonic),
        (flags::HAS_FLIP_OR_JUMP, b.has_flip_or_jump()),
        (flags::FLIPPING, b.is_flipping),
        (flags::JUMPING, b.is_jumping),
    ] {
        if on {
            f |= bit;
        }
    }
    for (i, &w) in b.wheel_contacts.iter().enumerate() {
        if w {
            f |= 1 << (flags::WHEEL_CONTACT_SHIFT + i as u32);
        }
    }
    f
}

/// Normalised lerp along the shorter arc (ticks are 1/120 s apart, so this is as good as slerp).
fn nlerp(a: Quat, b: Quat, t: f32) -> Quat {
    let dot = a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w;
    let s = if dot < 0.0 { -1.0 } else { 1.0 };
    Quat {
        x: a.x + (b.x * s - a.x) * t,
        y: a.y + (b.y * s - a.y) * t,
        z: a.z + (b.z * s - a.z) * t,
        w: a.w + (b.w * s - a.w) * t,
    }
    .safe_normalized()
}

/// Encodes the current state into `out` (capacity `cap` bytes). Returns the encoded size, or 0
/// if `cap` is too small (call with `cap = 0` to query the size).
///
/// # Safety
/// `car` as above; `out` points to `cap` writable bytes (may be null if `cap` is 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_car_save(car: *const Car, out: *mut u8, cap: u32) -> u32 {
    let Some(c) = (unsafe { car.as_ref() }) else { return 0 };
    let bytes = snapshot::encode(&c.stepper.current);
    if out.is_null() || (cap as usize) < bytes.len() {
        return if cap == 0 { bytes.len() as u32 } else { 0 };
    }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len()) };
    bytes.len() as u32
}

/// Replaces the car's state with one encoded by [`rlcar_car_save`] (no interpolation from the
/// old one). Returns 1 on success, 0 if the data is invalid (the car is left unchanged).
///
/// # Safety
/// `car` as above; `data` points to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_car_load(car: *mut Car, data: *const u8, len: u32) -> u32 {
    let Some(c) = (unsafe { car.as_mut() }) else { return 0 };
    if data.is_null() {
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, len as usize) };
    match guard(None, || snapshot::decode(bytes)) {
        Some(s) => {
            c.stepper.reset(s);
            1
        }
        None => 0,
    }
}

/// Turns Rocket League's "Unlimited" boost mutator on (`on != 0`) or off. While on, the tank is
/// kept full; turning it on also fills it now.
///
/// # Safety
/// `car` as above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_car_set_unlimited_boost(car: *mut Car, on: u32) {
    let Some(c) = (unsafe { car.as_mut() }) else { return };
    let on = on != 0;
    c.stepper.config.unlimited_boost = on;
    if on {
        for s in [&mut c.stepper.previous, &mut c.stepper.current] {
            s.boost_amount = rl_car_core::consts::BOOST_MAX;
        }
    }
}

/// Index of the car's hitbox preset in `HitboxPreset::ALL`.
///
/// # Safety
/// `car` as above.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_car_preset(car: *const Car) -> u32 {
    unsafe { car.as_ref() }.map_or(0, |c| c.stepper.current.hitbox_preset as u32)
}

/// Hitbox of a preset, in car-local uu: writes 6 floats, the full size (length, width, height)
/// then the box centre's offset from the car origin (forward, right, up).
///
/// # Safety
/// `out` points to 6 writable floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_preset_hitbox(preset: u32, out: *mut f32) {
    let p = HitboxPreset::ALL.get(preset as usize).copied().unwrap_or_default();
    let cfg = p.config();
    if !out.is_null() {
        let out = unsafe { std::slice::from_raw_parts_mut(out, 6) };
        out[..3].copy_from_slice(&cfg.hitbox_size.to_array());
        out[3..].copy_from_slice(&cfg.hitbox_pos_offset.to_array());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_floor_boxes(n: i32) -> Vec<f32> {
        let mut v = Vec::new();
        for x in -n..n {
            for y in -n..n {
                v.extend_from_slice(&[x as f32 * 100.0, y as f32 * 100.0, -100.0, x as f32 * 100.0 + 100.0, y as f32 * 100.0 + 100.0, 0.0]);
            }
        }
        v
    }

    #[test]
    fn c_api_drives_a_car_on_blocks() {
        unsafe {
            assert_eq!(rlcar_abi_version(), ABI_VERSION);
            let w = rlcar_world_new();
            let boxes = block_floor_boxes(20);
            assert_eq!(rlcar_world_set_boxes(w, boxes.as_ptr(), (boxes.len() / 6) as u32), 6);
            let car = rlcar_car_new(0);
            rlcar_car_reset(car, [0.0f32, 0.0, 60.0].as_ptr(), 0.0, 0.0, 0.0);
            rlcar_car_step(car, w, 120, 0.0, 0.0, 0.0, 0.0, 0.0, 0);
            let mut pose = [0.0f32; POSE_FLOATS];
            let f = rlcar_car_pose(car, 1.0, pose.as_mut_ptr());
            assert!(f & flags::ON_GROUND != 0, "flags {f:b}");
            assert_eq!(f >> flags::WHEEL_CONTACT_SHIFT & 0xf, 0xf);
            assert!((pose[2] - 17.0).abs() < 0.1);

            // A frame-rate driven second of boost: 60 frames of 1/60 s = 120 ticks.
            let mut ticks = 0;
            for _ in 0..60 {
                ticks += rlcar_car_advance(car, w, 1.0 / 60.0, 1.0, 0.0, 0.0, 0.0, 0.0, buttons::BOOST);
            }
            assert!((119..=121).contains(&ticks));
            rlcar_car_pose(car, rlcar_car_alpha(car), pose.as_mut_ptr());
            assert!(pose[39] > 1000.0, "forward speed {}", pose[39]);
            assert!(pose[3] > 0.999 && pose[4] == 0.0, "still heading +X: {:?}", &pose[3..6]);

            // Save / load into another car.
            let size = rlcar_car_save(car, std::ptr::null_mut(), 0);
            let mut buf = vec![0u8; size as usize];
            assert_eq!(rlcar_car_save(car, buf.as_mut_ptr(), size), size);
            let other = rlcar_car_new(3);
            assert_eq!(rlcar_car_load(other, buf.as_ptr(), size), 1);
            assert_eq!((*other).stepper.current, (*car).stepper.current);
            assert_eq!(rlcar_car_preset(other), 0);
            assert_eq!(rlcar_car_load(other, buf.as_ptr(), size - 1), 0);

            // Translating both world and car by whole blocks changes nothing physically.
            rlcar_car_translate(car, [300.0f32, -200.0, 0.0].as_ptr());
            let shifted: Vec<f32> = boxes.chunks(6).flat_map(|b| [b[0] + 300.0, b[1] - 200.0, b[2], b[3] + 300.0, b[4] - 200.0, b[5]]).collect();
            let w2 = rlcar_world_new();
            rlcar_world_set_boxes(w2, shifted.as_ptr(), (shifted.len() / 6) as u32);
            rlcar_car_step(car, w2, 60, 1.0, 0.5, 0.0, 0.0, 0.0, 0);
            rlcar_car_step(other, w, 60, 1.0, 0.5, 0.0, 0.0, 0.0, 0);
            let (a, b) = ((*car).stepper.current.position, (*other).stepper.current.position);
            assert!((a - b - Vec3::new(300.0, -200.0, 0.0)).length() < 0.05, "{a:?} vs {b:?}");

            // Unlimited boost: the tank stays full through two seconds of boosting.
            let fast = rlcar_car_new(0);
            rlcar_car_set_unlimited_boost(fast, 1);
            rlcar_car_step(fast, w, 240, 1.0, 0.0, 0.0, 0.0, 0.0, buttons::BOOST);
            rlcar_car_pose(fast, 1.0, pose.as_mut_ptr());
            assert_eq!(pose[18], 100.0);
            assert!(rlcar_car_pose(fast, 1.0, pose.as_mut_ptr()) & flags::BOOSTING != 0);
            assert!(pose[39] > 2200.0, "boosted to supersonic: {}", pose[39]);
            rlcar_car_set_unlimited_boost(fast, 0);
            rlcar_car_step(fast, w, 240, 1.0, 0.0, 0.0, 0.0, 0.0, buttons::BOOST);
            rlcar_car_pose(fast, 1.0, pose.as_mut_ptr());
            assert!(pose[18] < 100.0);
            rlcar_car_free(fast);

            for p in [car, other] {
                rlcar_car_free(p);
            }
            rlcar_world_free(w);
            rlcar_world_free(w2);
            // Null handles are harmless.
            rlcar_car_step(std::ptr::null_mut(), std::ptr::null(), 1, 0.0, 0.0, 0.0, 0.0, 0.0, 0);
            assert_eq!(rlcar_car_pose(std::ptr::null(), 0.0, pose.as_mut_ptr()), 0);
        }
    }
}
