#![allow(clippy::missing_safety_doc)]

use crate::callback_world::CallbackWorld;
use crate::{BoxWorld, CAMERA_SETTINGS_FLOATS, CAMERA_VIEW_FLOATS, Car, camera_flags, controls, guard, read_v, settings_from};
use rl_car_core::camera::{CameraInput, CameraTarget, CarCamera};
use rl_car_core::{BallConfig, BallState, CollisionWorld, Controls, EmptyWorld, Scene, TICK_DT, Vec3, step_scene};

pub const BALL_CONFIG_FLOATS: usize = 10;
pub const BALL_POSE_FLOATS: usize = 9;

pub struct Ball {
    pub previous: BallState,
    pub current: BallState,
    pub config: BallConfig,
    pub tick: u64,
}

fn config_to(c: &BallConfig, out: &mut [f32]) {
    out.copy_from_slice(&[
        c.radius,
        c.mass,
        c.drag,
        c.world_friction,
        c.world_restitution,
        c.max_speed,
        c.max_ang_speed,
        c.car_friction,
        c.car_restitution,
        c.hit_extra_force_scale,
    ]);
}

fn config_from(f: &[f32]) -> BallConfig {
    let d = BallConfig::default();
    let pick = |v: f32, def: f32, min: f32| if v.is_finite() && v >= min { v } else { def };
    BallConfig {
        radius: pick(f[0], d.radius, 1.0),
        mass: pick(f[1], d.mass, 0.01),
        drag: pick(f[2], d.drag, 0.0).min(1.0),
        world_friction: pick(f[3], d.world_friction, 0.0),
        world_restitution: pick(f[4], d.world_restitution, 0.0),
        max_speed: pick(f[5], d.max_speed, 0.0),
        max_ang_speed: pick(f[6], d.max_ang_speed, 0.0),
        car_friction: pick(f[7], d.car_friction, 0.0),
        car_restitution: pick(f[8], d.car_restitution, 0.0),
        hit_extra_force_scale: pick(f[9], d.hit_extra_force_scale, 0.0),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rlcar_ball_new() -> *mut Ball {
    let b = BallState::default();
    Box::into_raw(Box::new(Ball { previous: b, current: b, config: BallConfig::default(), tick: 0 }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_ball_free(ball: *mut Ball) {
    if !ball.is_null() {
        drop(unsafe { Box::from_raw(ball) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_ball_reset(ball: *mut Ball, pos: *const f32, vel: *const f32, ang_vel: *const f32) {
    let Some(b) = (unsafe { ball.as_mut() }) else { return };
    if pos.is_null() {
        return;
    }
    let mut s = BallState::new(unsafe { read_v(pos) });
    if !vel.is_null() {
        s.velocity = unsafe { read_v(vel) };
    }
    if !ang_vel.is_null() {
        s.angular_velocity = unsafe { read_v(ang_vel) };
    }
    b.previous = s;
    b.current = s;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_ball_translate(ball: *mut Ball, delta: *const f32) {
    let Some(b) = (unsafe { ball.as_mut() }) else { return };
    if delta.is_null() {
        return;
    }
    let d = unsafe { read_v(delta) };
    for s in [&mut b.previous, &mut b.current] {
        s.position += d;
        for m in s.manifolds.list.iter_mut() {
            for p in m.points.iter_mut() {
                p.world_b += d * rl_car_core::consts::UU_TO_BT;
            }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_default_ball_config(out: *mut f32) {
    if !out.is_null() {
        config_to(&BallConfig::default(), unsafe { std::slice::from_raw_parts_mut(out, BALL_CONFIG_FLOATS) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_ball_config(ball: *const Ball, out: *mut f32) {
    if let (Some(b), false) = (unsafe { ball.as_ref() }, out.is_null()) {
        config_to(&b.config, unsafe { std::slice::from_raw_parts_mut(out, BALL_CONFIG_FLOATS) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_ball_set_config(ball: *mut Ball, config: *const f32) {
    if let (Some(b), false) = (unsafe { ball.as_mut() }, config.is_null()) {
        b.config = config_from(unsafe { std::slice::from_raw_parts(config, BALL_CONFIG_FLOATS) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_ball_pose(ball: *const Ball, alpha: f32, out: *mut f32) -> u32 {
    let Some(b) = (unsafe { ball.as_ref() }) else { return 0 };
    if out.is_null() {
        return 0;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(out, BALL_POSE_FLOATS) };
    let t = if alpha.is_finite() { alpha.clamp(0.0, 1.0) } else { 1.0 };
    let lerp = |x: Vec3, y: Vec3| x + (y - x) * t;
    let (a, c) = (&b.previous, &b.current);
    out[0..3].copy_from_slice(&lerp(a.position, c.position).to_array());
    out[3..6].copy_from_slice(&lerp(a.velocity, c.velocity).to_array());
    out[6..9].copy_from_slice(&lerp(a.angular_velocity, c.angular_velocity).to_array());
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_car_ball_touch(car: *mut Car, out: *mut f32) -> u32 {
    let Some(c) = (unsafe { car.as_mut() }) else { return 0 };
    let touched = c.ball_touched;
    if !out.is_null() {
        unsafe { std::slice::from_raw_parts_mut(out, 3) }.copy_from_slice(&c.ball_hit.to_array());
    }
    c.ball_touched = false;
    c.ball_hit = Vec3::ZERO;
    touched as u32
}

fn step_scene_ticks(c: &mut Car, b: &mut Ball, w: &dyn CollisionWorld, ticks: u32, ctl: Controls) {
    guard((), || {
        for _ in 0..ticks {
            c.stepper.previous = c.stepper.current;
            b.previous = b.current;
            let mut link = c.link;
            link.state = c.stepper.current;
            let mut scene = Scene { cars: vec![link], ball: b.current, tick: b.tick };
            step_scene(&mut scene, &[ctl], w, &c.stepper.config, &b.config, TICK_DT);
            let link = scene.cars[0];
            c.stepper.current = link.state;
            c.link = link;
            if link.touched_ball {
                c.ball_touched = true;
            }
            if link.ball_hit_extra_velocity != Vec3::ZERO {
                c.ball_hit = link.ball_hit_extra_velocity;
            }
            b.current = scene.ball;
            b.tick = scene.tick;
            c.stepper.tick_count += 1;
        }
    });
}

#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_scene_step_cb(
    car: *mut Car,
    ball: *mut Ball,
    world: *const CallbackWorld,
    ticks: u32,
    throttle: f32,
    steer: f32,
    pitch: f32,
    yaw: f32,
    roll: f32,
    buttons: u32,
) {
    let (Some(c), Some(b)) = (unsafe { car.as_mut() }, unsafe { ball.as_mut() }) else { return };
    let ctl = controls(throttle, steer, pitch, yaw, roll, buttons);
    match unsafe { world.as_ref() } {
        Some(w) => step_scene_ticks(c, b, w, ticks, ctl),
        None => step_scene_ticks(c, b, &EmptyWorld, ticks, ctl),
    }
}

#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_scene_advance_cb(
    car: *mut Car,
    ball: *mut Ball,
    world: *const CallbackWorld,
    frame_dt: f64,
    throttle: f32,
    steer: f32,
    pitch: f32,
    yaw: f32,
    roll: f32,
    buttons: u32,
) -> u32 {
    let (Some(c), Some(b)) = (unsafe { car.as_mut() }, unsafe { ball.as_mut() }) else { return 0 };
    let ctl = controls(throttle, steer, pitch, yaw, roll, buttons);
    let n = c.stepper.consume(frame_dt);
    match unsafe { world.as_ref() } {
        Some(w) => step_scene_ticks(c, b, w, n, ctl),
        None => step_scene_ticks(c, b, &EmptyWorld, n, ctl),
    }
    n
}

#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_scene_step(
    car: *mut Car,
    ball: *mut Ball,
    world: *const BoxWorld,
    ticks: u32,
    throttle: f32,
    steer: f32,
    pitch: f32,
    yaw: f32,
    roll: f32,
    buttons: u32,
) {
    let (Some(c), Some(b)) = (unsafe { car.as_mut() }, unsafe { ball.as_mut() }) else { return };
    let ctl = controls(throttle, steer, pitch, yaw, roll, buttons);
    let empty = BoxWorld::default();
    let w = unsafe { world.as_ref() }.unwrap_or(&empty);
    step_scene_ticks(c, b, w, ticks, ctl);
}

#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rlcar_camera_update_ball(
    camera: *mut CarCamera,
    car: *const Car,
    ball: *const Ball,
    alpha: f32,
    dt: f32,
    settings: *const f32,
    look_right: f32,
    look_up: f32,
    flags: u32,
    out: *mut f32,
) -> u32 {
    let (Some(cam), Some(car)) = (unsafe { camera.as_mut() }, unsafe { car.as_ref() }) else { return 0 };
    if settings.is_null() || out.is_null() {
        return 0;
    }
    let settings = settings_from(unsafe { std::slice::from_raw_parts(settings, CAMERA_SETTINGS_FLOATS) });
    let out = unsafe { std::slice::from_raw_parts_mut(out, CAMERA_VIEW_FLOATS) };
    let ball_pos = unsafe { ball.as_ref() }.filter(|_| flags & camera_flags::BALL_CAM != 0).map(|b| {
        let t = if alpha.is_finite() { alpha.clamp(0.0, 1.0) } else { 1.0 };
        b.previous.position + (b.current.position - b.previous.position) * t
    });
    guard(0, || {
        let target = CameraTarget::interpolated(&car.stepper.previous, &car.stepper.current, alpha);
        let input = CameraInput { look_right, look_up, rear_view: flags & camera_flags::REAR_VIEW != 0 };
        let v = cam.update_with_ball(&target, ball_pos, &input, &settings, dt);
        out[0..3].copy_from_slice(&v.location.to_array());
        for i in 0..3 {
            out[3 + i * 3..6 + i * 3].copy_from_slice(&v.orientation.0.col(i).to_array());
        }
        out[12] = v.fov;
        out[13] = v.vertical_fov().to_degrees();
        out[14..17].copy_from_slice(&v.focus.to_array());
        1
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    #[test]
    fn scene_hits_the_ball_through_the_c_api() {
        unsafe {
            let car = rlcar_car_new(0);
            let ball = rlcar_ball_new();
            let world = rlcar_world_new();
            let floor = [-5000.0f32, -5000.0, -100.0, 5000.0, 5000.0, 0.0];
            rlcar_world_set_boxes(world, floor.as_ptr(), 1);
            let pos = [800.0f32, 0.0, 93.15];
            rlcar_ball_reset(ball, pos.as_ptr(), std::ptr::null(), std::ptr::null());
            let mut touched = 0;
            for _ in 0..240 {
                rlcar_scene_step(car, ball, world, 1, 1.0, 0.0, 0.0, 0.0, 0.0, buttons::BOOST);
                touched |= rlcar_car_ball_touch(car, std::ptr::null_mut());
            }
            let mut pose = [0.0f32; BALL_POSE_FLOATS];
            assert_eq!(rlcar_ball_pose(ball, 1.0, pose.as_mut_ptr()), 1);
            assert_eq!(touched, 1);
            assert!(pose[3] > 1000.0, "ball velocity {:?}", &pose[3..6]);
            rlcar_world_free(world);
            rlcar_ball_free(ball);
            rlcar_car_free(car);
        }
    }

    #[test]
    fn ball_config_round_trips_and_rejects_garbage() {
        unsafe {
            let ball = rlcar_ball_new();
            let mut cfg = [0.0f32; BALL_CONFIG_FLOATS];
            rlcar_default_ball_config(cfg.as_mut_ptr());
            assert_eq!(cfg[0], 91.25);
            cfg[0] = 150.0;
            cfg[1] = f32::NAN;
            rlcar_ball_set_config(ball, cfg.as_ptr());
            let mut back = [0.0f32; BALL_CONFIG_FLOATS];
            rlcar_ball_config(ball, back.as_mut_ptr());
            assert_eq!(back[0], 150.0);
            assert_eq!(back[1], BallConfig::default().mass);
            rlcar_ball_free(ball);
        }
    }
}
