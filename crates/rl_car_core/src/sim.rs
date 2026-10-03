//! The car tick: `step(state, controls, world, dt) -> state`.
//!
//! Order of operations per tick (mirrors the game's/RocketSim's):
//! 1. Wheel transforms + suspension raycasts; tire friction impulses are *computed* from the
//!    previous tick's steer/engine/brake/friction values.
//! 2. Car logic: throttle/brake/steer/friction for next tick, sticky force, air control, jump,
//!    auto-flip, double-jump/dodge, auto-roll.
//! 3. Suspension and tire friction impulses are applied, boost force is added.
//! 4. Physics step: gravity, body-vs-world contacts, integration.
//! 5. Post: supersonic bookkeeping, speed / angular speed clamps.

// Wheel loops index several parallel per-wheel arrays; a range loop reads clearest.
#![allow(clippy::needless_range_loop)]

use crate::body::Body;
use crate::config::CarConfig;
use crate::consts::suspension::*;
use crate::consts::*;
use crate::math::{Mat3, Quat, Vec3};
use crate::manifold::{ManifoldPoint, Manifolds};
use crate::solver::{self, SolverContact};
use crate::state::{CarState, Controls, RotMat};
use crate::world::{CollisionWorld, Contact, Obb};

/// Tunable game rules ("mutators"). `Default` is standard Rocket League.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SimConfig {
    /// uu/s^2
    pub gravity: Vec3,
    pub boost_accel_ground: f32,
    pub boost_accel_air: f32,
    pub boost_used_per_second: f32,
    pub jump_accel: f32,
    pub jump_immediate_force: f32,
    pub car_world_friction: f32,
    pub car_world_restitution: f32,
    pub unlimited_flips: bool,
    pub unlimited_double_jumps: bool,
    /// Rocket League's "Unlimited" boost mutator: the tank stays full.
    pub unlimited_boost: bool,
    pub recharge_boost_enabled: bool,
    pub recharge_boost_per_second: f32,
    pub recharge_boost_delay: f32,
    pub car_max_speed: f32,
}

impl Default for SimConfig {
    fn default() -> Self {
        SimConfig {
            gravity: Vec3::new(0.0, 0.0, GRAVITY_Z),
            boost_accel_ground: BOOST_ACCEL_GROUND,
            boost_accel_air: BOOST_ACCEL_AIR,
            boost_used_per_second: BOOST_USED_PER_SECOND,
            jump_accel: JUMP_ACCEL,
            jump_immediate_force: JUMP_IMMEDIATE_FORCE,
            car_world_friction: CARWORLD_COLLISION_FRICTION,
            car_world_restitution: CARWORLD_COLLISION_RESTITUTION,
            unlimited_flips: false,
            unlimited_double_jumps: false,
            unlimited_boost: false,
            recharge_boost_enabled: false,
            recharge_boost_per_second: RECHARGE_BOOST_PER_SECOND,
            recharge_boost_delay: RECHARGE_BOOST_DELAY,
            car_max_speed: CAR_MAX_SPEED,
        }
    }
}

/// Advance the car by one tick with standard rules. `dt` should be [`TICK_DT`] (1/120 s);
/// the model is validated only at that rate (RocketSim supports 15..=120 Hz).
pub fn step(state: &CarState, controls: &Controls, world: &dyn CollisionWorld, dt: f32) -> CarState {
    step_with(state, controls, world, &SimConfig::default(), dt)
}

/// Per-tick scratch data for one wheel (bt units).
#[derive(Clone, Copy, Default)]
struct WheelTick {
    hard_point: Vec3,
    lat_dir: Vec3,
    in_contact: bool,
    contact_point: Vec3,
    contact_normal: Vec3,
    suspension_length: f32,
    suspension_relative_velocity: f32,
    clipped_inv_contact_dot_suspension: f32,
    impulse: Vec3,
    rest_length: f32,
    radius: f32,
    force_scale: f32,
    on_dynamic: bool,
}

#[inline]
fn sgn(v: f32) -> i32 {
    (v > 0.0) as i32 - (v < 0.0) as i32
}

/// Advance the car by one tick with custom rules.
pub fn step_with(state: &CarState, controls: &Controls, world: &dyn CollisionWorld, cfg: &SimConfig, dt: f32) -> CarState {
    let mut t = car_begin(state, controls, world, cfg, dt, None);
    car_collide_world(&mut t, world);
    let contacts = car_world_contacts(&t);
    let applied = solver::solve_and_integrate(&mut t.body, &contacts, cfg.car_world_friction, cfg.car_world_restitution, dt);
    car_store_world_impulses(&mut t, &applied);
    car_end(t, cfg, dt)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DynamicGround {
    pub center: Vec3,
    pub radius: f32,
    pub vel: Vec3,
    pub ang_vel: Vec3,
    pub inv_mass: f32,
    pub inv_inertia: f32,
}

impl DynamicGround {
    fn velocity_at(&self, rel: Vec3) -> Vec3 {
        self.vel + self.ang_vel.cross(rel)
    }

    fn raycast(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<(f32, Vec3, Vec3)> {
        let to = origin + dir * max_dist;
        let (fraction, normal) = crate::subsimplex::ray_sphere(origin, to, self.center, self.radius)?;
        Some((fraction * max_dist, origin * (1.0 - fraction) + to * fraction, normal))
    }
}

pub(crate) struct CarTick {
    pub s: CarState,
    pub controls: Controls,
    pub car: CarConfig,
    pub body: Body,
    pub breaking_threshold: f32,
}

pub(crate) fn car_begin(
    state: &CarState,
    controls: &Controls,
    world: &dyn CollisionWorld,
    cfg: &SimConfig,
    dt: f32,
    ground: Option<&DynamicGround>,
) -> CarTick {
    let mut s = *state;
    let controls = controls.clamped();
    let car = s.config();
    if cfg.unlimited_boost {
        s.boost_amount = BOOST_MAX;
    }
    let mut body = Body::new(s.position, s.velocity, s.angular_velocity, s.orientation.0, &car);

    // ---------------------------------------------------------------- 1. vehicle (first half)
    let mut w = [WheelTick::default(); 4];
    let suspension_travel = (MAX_TRAVEL * UU_TO_BT) * 100.0 / 100.0;
    for (i, wt) in w.iter_mut().enumerate() {
        let (cp_uu, radius_uu, rest_uu, front) = car.wheel(i);
        let ws = &mut s.wheels[i];
        wt.radius = radius_uu * UU_TO_BT;
        wt.rest_length = (rest_uu - MAX_TRAVEL) * UU_TO_BT;
        wt.force_scale = if front { FORCE_SCALE_FRONT } else { FORCE_SCALE_BACK };

        let up = body.up();
        wt.hard_point = body.basis * (cp_uu * UU_TO_BT) + body.pos;
        let wheel_dir = body.basis * Vec3::new(0.0, 0.0, -1.0);
        let axle = body.basis * Vec3::new(0.0, -1.0, 0.0);

        // Wheel basis: steering rotation (previous tick's angle) applied to the car's right axis.
        let wheel_up = -wheel_dir;
        let steering = Mat3::from_quat(Quat::from_axis_angle(wheel_up, ws.steer_angle));
        wt.lat_dir = steering * (-axle);

        // Suspension raycast.
        let ray_len = wt.rest_length + suspension_travel + wt.radius - SUBTRACTION_BT;
        let mut hit = world.raycast(wt.hard_point * BT_TO_UU, wheel_dir, ray_len * BT_TO_UU);
        wt.on_dynamic = false;
        if let Some((t, point, normal)) = ground.and_then(|g| g.raycast(wt.hard_point, wheel_dir, ray_len))
            && hit.is_none_or(|h| t * BT_TO_UU < h.distance)
        {
            hit = Some(crate::world::RayHit { distance: t * BT_TO_UU, point: point * BT_TO_UU, normal });
            wt.on_dynamic = true;
        }
        if let Some(hit) = hit {
            let fraction = if ray_len > 0.0 { (hit.distance * UU_TO_BT / ray_len).clamp(0.0, 1.0) } else { 0.0 };
            let target = wt.hard_point + wheel_dir * ray_len;
            wt.contact_point = wt.hard_point + (target - wt.hard_point) * fraction;
            wt.contact_normal = hit.normal;
            wt.in_contact = true;

            let trace_len = (wt.hard_point - wt.contact_point).dot(up);
            wt.suspension_length =
                (trace_len - wt.radius).clamp(wt.rest_length - suspension_travel, wt.rest_length + suspension_travel);

            let denominator = wt.contact_normal.dot(up);
            let vel_at_contact = body.velocity_at(wt.contact_point - body.pos);
            let proj_vel = wt.contact_normal.dot(vel_at_contact);
            if denominator > 0.1 {
                let inv = 1.0 / denominator;
                wt.suspension_relative_velocity = proj_vel * inv;
                wt.clipped_inv_contact_dot_suspension = inv;
            } else {
                wt.suspension_relative_velocity = 0.0;
                wt.clipped_inv_contact_dot_suspension = 10.0;
            }

            // Bottomed-out suspension: extra push-back (Bullet `resolveSingleCollision`).
            // NOTE: like the game, the previous value is kept when not bottomed out.
            let pushback_thresh = (wt.rest_length + wt.radius) - SUBTRACTION_BT;
            if !wt.on_dynamic && trace_len < pushback_thresh {
                let distance = trace_len - pushback_thresh;
                let rel = wt.contact_point - body.pos;
                let rel_vel = wt.contact_normal.dot(body.velocity_at(rel));
                let positional_error = crate::consts::solver::ERP * -distance / dt;
                let velocity_error = -(1.0 + 0.0) * rel_vel;
                let jac_inv = 1.0 / body.impulse_denominator(rel, wt.contact_normal);
                let impulse = positional_error * jac_inv + velocity_error * jac_inv;
                ws.extra_pushback = impulse.max(0.0) / 4.0;
            }
        } else {
            wt.in_contact = false;
            wt.contact_point = wt.hard_point + wheel_dir * ray_len;
            wt.suspension_length = wt.rest_length + suspension_travel;
            wt.suspension_relative_velocity = 0.0;
            wt.contact_normal = -wheel_dir;
            wt.clipped_inv_contact_dot_suspension = 1.0;
            ws.extra_pushback = 0.0;
        }
    }

    // Tire friction impulses (from last tick's engine/brake/friction values).
    let friction_scale = CAR_MASS_BT / 3.0;
    for (i, wt) in w.iter_mut().enumerate() {
        let ws = &s.wheels[i];
        if !wt.in_contact {
            wt.impulse = Vec3::ZERO;
            continue;
        }
        let n = wt.contact_normal;
        let mut axle_dir = wt.lat_dir;
        let proj = axle_dir.dot(n);
        axle_dir -= n * proj;
        axle_dir = axle_dir.safe_normalized();
        let forward_dir = n.cross(axle_dir).safe_normalized();

        // Side impulse (Bullet `resolveSingleBilateral` against static ground).
        let rel = wt.contact_point - body.pos;
        let dyn_ground = if wt.on_dynamic { ground } else { None };
        let side_impulse = match dyn_ground {
            None => {
                let rel_vel = axle_dir.dot(body.velocity_at(rel));
                let jac_inv = 1.0 / body.jacobian_diag(rel, axle_dir);
                -SIDE_FRICTION_DAMPING * rel_vel * jac_inv
            }
            Some(g) => {
                let rel2 = wt.contact_point - g.center;
                let rel_vel = axle_dir.dot(body.velocity_at(rel) - g.velocity_at(rel2));
                let b_j = rel2.cross(-axle_dir);
                let jac = body.jacobian_diag(rel, axle_dir) + g.inv_mass + (b_j * g.inv_inertia).dot(b_j);
                -SIDE_FRICTION_DAMPING * rel_vel * (1.0 / jac)
            }
        };

        let rolling_friction = if ws.engine_force == 0.0 {
            if ws.brake != 0.0 {
                let mut rel_vel = match dyn_ground {
                    None => body.velocity_at(rel).dot(forward_dir),
                    Some(g) => (body.velocity_at(rel) - g.velocity_at(rel)).dot(forward_dir),
                };
                if dt > 1.0 / 80.0 {
                    let threshold = -(1.0 / (dt * 150.0)) + 0.8;
                    if rel_vel.abs() < threshold {
                        rel_vel = 0.0;
                    }
                }
                (-rel_vel * ROLLING_FRICTION_SCALE_MAGIC).clamp(-ws.brake, ws.brake)
            } else {
                0.0
            }
        } else {
            -ws.engine_force / friction_scale
        };

        let total = forward_dir * rolling_friction * ws.long_friction + axle_dir * side_impulse * ws.lat_friction;
        wt.impulse = total * friction_scale;
    }

    // ---------------------------------------------------------------- 2. car logic
    let jump_pressed = controls.jump && !s.last_controls.jump;
    let mut num_contacts = 0;
    for i in 0..4 {
        s.wheel_contacts[i] = w[i].in_contact;
        num_contacts += w[i].in_contact as i32;
    }
    s.on_ground = num_contacts >= 3;
    let forward_speed = body.vel.dot(body.forward()) * BT_TO_UU;

    let upwards_from_wheels = |w: &[WheelTick; 4], body: &Body| {
        let mut sum = Vec3::ZERO;
        for wt in w.iter().filter(|wt| wt.in_contact) {
            sum += wt.contact_normal;
        }
        if sum.is_zero() { body.up() } else { sum.safe_normalized() }
    };

    update_wheels(&mut s, &controls, &car, &mut body, &w, num_contacts, forward_speed, dt, &upwards_from_wheels);

    if num_contacts < 3 {
        update_air_torque(&mut s, &controls, &mut body, num_contacts == 0);
    } else {
        s.is_flipping = false;
    }

    update_jump(&mut s, &controls, cfg, &mut body, jump_pressed, dt);
    update_auto_flip(&mut s, &mut body, jump_pressed, dt);
    update_double_jump_or_flip(&mut s, &controls, &car, cfg, &mut body, jump_pressed, forward_speed, dt);

    if controls.throttle != 0.0 && ((num_contacts > 0 && num_contacts < 4) || s.world_contact_normal.is_some()) {
        let ground_up = if num_contacts > 0 {
            upwards_from_wheels(&w, &body)
        } else {
            s.world_contact_normal.unwrap_or(Vec3::Z)
        };
        update_auto_roll(&mut body, ground_up);
    }
    s.world_contact_normal = None;

    // ---------------------------------------------------------------- 3. vehicle (second half)
    let mut susp_force = [0.0f32; 4];
    for (i, wt) in w.iter().enumerate() {
        if wt.in_contact {
            let force = (wt.rest_length - wt.suspension_length) * STIFFNESS * wt.clipped_inv_contact_dot_suspension;
            let damping = if wt.suspension_relative_velocity < 0.0 { DAMPING_COMPRESSION } else { DAMPING_RELAXATION };
            let mut f = force - (damping * wt.suspension_relative_velocity);
            f *= wt.force_scale;
            susp_force[i] = f.max(0.0);
        }
    }
    for (i, wt) in w.iter().enumerate() {
        if susp_force[i] != 0.0 {
            let offset = wt.contact_point - body.pos;
            let scale = (susp_force[i] * dt) + s.wheels[i].extra_pushback;
            body.apply_impulse(wt.contact_normal * scale, offset);
        }
    }
    let up = body.up();
    for wt in w.iter() {
        if !wt.impulse.is_zero() {
            let offset = wt.contact_point - body.pos;
            let rel = offset - up * up.dot(offset);
            body.apply_impulse(wt.impulse * dt, rel);
        }
    }

    update_boost(&mut s, &controls, cfg, &mut body, dt);

    // Informational wheel data for hosts (renderers).
    for i in 0..4 {
        s.wheels[i].suspension_length = w[i].suspension_length * BT_TO_UU;
        s.wheels[i].contact = if w[i].in_contact {
            Some((w[i].contact_point * BT_TO_UU, w[i].contact_normal))
        } else {
            None
        };
    }

    // ---------------------------------------------------------------- 4. physics step
    body.apply_central_force(cfg.gravity * UU_TO_BT * (1.0 / body.inv_mass));
    let breaking_threshold = car.contact_breaking_threshold_bt();
    CarTick { s, controls, car, body, breaking_threshold }
}

pub(crate) fn car_collide_world(t: &mut CarTick, world: &dyn CollisionWorld) {
    let CarTick { s, car, body, breaking_threshold, .. } = t;
    let (car, body, breaking_threshold) = (&*car, &*body, *breaking_threshold);
    // Body vs world: refresh cached contact points, add this tick's new points, solve.
    s.manifolds.refresh_all(body.pos, &body.basis, breaking_threshold);
    let obb = Obb {
        center: (body.pos + body.basis * (car.hitbox_pos_offset * UU_TO_BT)) * BT_TO_UU,
        axes: body.basis,
        half_extents: car.effective_half_extents(),
    };
    let mut new_contacts: Vec<Contact> = Vec::new();
    world.box_contacts(&obb, breaking_threshold * BT_TO_UU, &mut new_contacts);
    for c in &new_contacts {
        let distance = -c.depth * UU_TO_BT;
        if distance >= breaking_threshold {
            continue;
        }
        let on_b = c.point * UU_TO_BT;
        let on_a = on_b + c.normal * distance;
        let local_a = body.basis.transpose_mul_vec(on_a - body.pos);
        s.manifolds.get_or_create(c.surface).add(ManifoldPoint {
            local_a,
            world_b: on_b,
            normal: c.normal,
            distance,
            applied_impulse: 0.0,
            lifetime: 0,
        });
        s.world_contact_normal = Some(c.normal);
    }
    s.manifolds.refresh_all(body.pos, &body.basis, breaking_threshold);
}

pub(crate) fn car_world_contacts(t: &CarTick) -> Vec<SolverContact> {
    let (s, body) = (&t.s, &t.body);
    let mut solver_contacts = Vec::new();
    for m in s.manifolds.list.iter().filter(|m| m.active) {
        for p in &m.points[..m.count] {
            solver_contacts.push(SolverContact {
                point_on_car: body.basis * p.local_a + body.pos,
                normal: p.normal,
                distance: p.distance,
                warm_impulse: p.applied_impulse * Manifolds::warmstart_factor(),
            });
        }
    }
    solver_contacts
}

pub(crate) fn car_store_world_impulses(t: &mut CarTick, applied: &[f32]) {
    let mut k = 0;
    for m in t.s.manifolds.list.iter_mut().filter(|m| m.active) {
        for p in m.points[..m.count].iter_mut() {
            p.applied_impulse = applied[k];
            k += 1;
        }
    }
}

pub(crate) fn car_end(t: CarTick, cfg: &SimConfig, dt: f32) -> CarState {
    let CarTick { mut s, controls, mut body, .. } = t;
    // ---------------------------------------------------------------- 5. post tick
    s.orientation = RotMat(body.basis);
    {
        let speed_sq = (body.vel * BT_TO_UU).length_squared();
        if s.is_supersonic && s.supersonic_time < SUPERSONIC_MAINTAIN_MAX_TIME {
            s.is_supersonic = speed_sq >= SUPERSONIC_MAINTAIN_MIN_SPEED * SUPERSONIC_MAINTAIN_MIN_SPEED;
        } else {
            s.is_supersonic = speed_sq >= SUPERSONIC_START_SPEED * SUPERSONIC_START_SPEED;
        }
        if s.is_supersonic {
            s.supersonic_time += dt;
        } else {
            s.supersonic_time = 0.0;
        }
    }
    s.last_controls = controls;

    let max_speed_bt = cfg.car_max_speed * UU_TO_BT;
    if body.vel.length_squared() > max_speed_bt * max_speed_bt {
        body.vel = body.vel.normalized() * max_speed_bt;
    }
    if body.ang_vel.length_squared() > CAR_MAX_ANG_SPEED * CAR_MAX_ANG_SPEED {
        body.ang_vel = body.ang_vel.normalized() * CAR_MAX_ANG_SPEED;
    }

    s.position = body.pos * BT_TO_UU;
    s.velocity = body.vel * BT_TO_UU;
    s.angular_velocity = body.ang_vel;
    s
}

#[allow(clippy::too_many_arguments)]
fn update_wheels(
    s: &mut CarState,
    c: &Controls,
    car: &CarConfig,
    body: &mut Body,
    w: &[WheelTick; 4],
    num_contacts: i32,
    forward_speed: f32,
    dt: f32,
    upwards_from_wheels: &dyn Fn(&[WheelTick; 4], &Body) -> Vec3,
) {
    let abs_speed = forward_speed.abs();
    let wheels_have_world_contact = w.iter().any(|wt| wt.in_contact && !wt.on_dynamic);

    if c.handbrake {
        s.handbrake_val += POWERSLIDE_RISE_RATE * dt;
    } else {
        s.handbrake_val -= POWERSLIDE_FALL_RATE * dt;
    }
    s.handbrake_val = s.handbrake_val.clamp(0.0, 1.0);

    let mut real_throttle = c.throttle;
    let mut real_brake = 0.0;
    if c.boost && s.boost_amount > 0.0 {
        real_throttle = 1.0;
    }

    // Throttle / brake.
    {
        let mut drive_speed_scale = DRIVE_SPEED_TORQUE_FACTOR.eval(abs_speed);
        let mut engine_throttle = real_throttle;
        if !c.handbrake {
            if real_throttle.abs() >= THROTTLE_DEADZONE {
                if abs_speed > STOPPING_FORWARD_VEL && sgn(real_throttle) != sgn(forward_speed) {
                    // Trying to drive the opposite way: full brake, no engine.
                    real_brake = 1.0;
                    if abs_speed > BRAKING_NO_THROTTLE_SPEED_THRESH {
                        engine_throttle = 0.0;
                    }
                }
            } else {
                // Coasting.
                engine_throttle = 0.0;
                real_brake = if abs_speed < STOPPING_FORWARD_VEL { 1.0 } else { COASTING_BRAKE_FACTOR };
            }
        }
        if num_contacts < 3 {
            drive_speed_scale /= 4.0;
        }
        let engine_force = engine_throttle * (THROTTLE_TORQUE_AMOUNT * UU_TO_BT) * drive_speed_scale;
        let brake_force = real_brake * (BRAKE_TORQUE_AMOUNT * UU_TO_BT);
        for ws in s.wheels.iter_mut() {
            ws.engine_force = engine_force;
            ws.brake = brake_force;
        }
    }

    // Steering.
    {
        let curve = if car.three_wheels { STEER_ANGLE_FROM_SPEED_THREEWHEEL } else { STEER_ANGLE_FROM_SPEED };
        let mut steer_angle = curve.eval(abs_speed);
        if s.handbrake_val != 0.0 {
            steer_angle += (POWERSLIDE_STEER_ANGLE_FROM_SPEED.eval(abs_speed) - steer_angle) * s.handbrake_val;
        }
        steer_angle *= c.steer;
        s.wheels[0].steer_angle = steer_angle;
        s.wheels[1].steer_angle = steer_angle;
    }

    // Tire friction for next tick.
    for i in 0..4 {
        let wt = &w[i];
        if !wt.in_contact {
            continue;
        }
        let lat_dir = wt.lat_dir;
        let long_dir = lat_dir.cross(wt.contact_normal);
        let wheel_delta = wt.hard_point - body.pos;
        let cross_vec = (body.ang_vel.cross(wheel_delta) + body.vel) * BT_TO_UU;
        let base_friction = cross_vec.dot(lat_dir).abs();
        let mut curve_input = 0.0;
        if base_friction > 5.0 {
            curve_input = base_friction / (cross_vec.dot(long_dir).abs() + base_friction);
        }
        let mut lat = if car.three_wheels { LAT_FRICTION_THREEWHEEL } else { LAT_FRICTION }.eval(curve_input);
        let mut long = LONG_FRICTION.eval(curve_input);
        if s.handbrake_val != 0.0 {
            let hb = s.handbrake_val;
            lat *= (HANDBRAKE_LAT_FRICTION_FACTOR.eval(curve_input) - 1.0) * hb + 1.0;
            long *= (HANDBRAKE_LONG_FRICTION_FACTOR.eval(curve_input) - 1.0) * hb + 1.0;
        } else {
            long = 1.0;
        }
        if real_throttle == 0.0 {
            let non_sticky = NON_STICKY_FRICTION_FACTOR.eval(wt.contact_normal.z);
            lat *= non_sticky;
            long *= non_sticky;
        }
        s.wheels[i].lat_friction = lat;
        s.wheels[i].long_friction = long;
    }

    // Sticky force: pulls the car into the surface its wheels touch (walls, ceiling).
    if wheels_have_world_contact {
        let upwards = upwards_from_wheels(w, body);
        let full_stick = real_throttle != 0.0 || abs_speed > STOPPING_FORWARD_VEL;
        let mut scale = if car.three_wheels { 0.0 } else { 0.5 };
        if full_stick {
            scale += 1.0 - upwards.z.abs();
        }
        body.apply_central_force(upwards * scale * (GRAVITY_Z * UU_TO_BT) * CAR_MASS_BT);
    }
}

fn update_air_torque(s: &mut CarState, c: &Controls, body: &mut Body, update_air_control: bool) {
    let dir_pitch = -body.right();
    let dir_yaw = body.up();
    let dir_roll = -body.forward();

    let mut do_air_control = false;
    if s.is_flipping {
        s.is_flipping = s.has_flipped && s.flip_timer < FLIP_TORQUE_TIME;
    }
    if s.is_flipping {
        let mut rel = s.flip_rel_torque;
        if !rel.is_zero() {
            // Flip cancel: holding pitch in the flip's direction scales the flip pitch torque down.
            let mut pitch_scale = 1.0;
            if rel.y != 0.0 && c.pitch != 0.0 && sgn(rel.y) == sgn(c.pitch) {
                pitch_scale = 1.0 - c.pitch.abs().min(1.0);
                do_air_control = true;
            }
            rel.y *= pitch_scale;
            let dodge_torque = rel.mul_elem(Vec3::new(FLIP_TORQUE_X, FLIP_TORQUE_Y, 0.0));
            body.apply_ang_accel(body.basis * dodge_torque);
        } else {
            // Stall: a "flip" with no direction keeps full air control.
            do_air_control = true;
        }
    } else {
        do_air_control = true;
    }

    do_air_control &= !s.is_auto_flipping;
    do_air_control &= update_air_control;
    if do_air_control {
        let mut pitch_torque_scale = 1.0;
        let torque = if c.pitch != 0.0 || c.yaw != 0.0 || c.roll != 0.0 {
            if s.is_flipping || (s.has_flipped && s.flip_timer < FLIP_TORQUE_TIME + FLIP_PITCHLOCK_EXTRA_TIME) {
                pitch_torque_scale = 0.0;
            }
            (c.pitch * dir_pitch * pitch_torque_scale * CAR_AIR_CONTROL_TORQUE[0])
                + (c.yaw * dir_yaw * CAR_AIR_CONTROL_TORQUE[1])
                + (c.roll * dir_roll * CAR_AIR_CONTROL_TORQUE[2])
        } else {
            Vec3::ZERO
        };

        let w = body.ang_vel;
        let damp_pitch = dir_pitch.dot(w) * CAR_AIR_CONTROL_DAMPING[0] * (1.0 - (c.pitch * pitch_torque_scale).abs());
        let damp_yaw = dir_yaw.dot(w) * CAR_AIR_CONTROL_DAMPING[1] * (1.0 - c.yaw.abs());
        let damp_roll = dir_roll.dot(w) * CAR_AIR_CONTROL_DAMPING[2];
        let damping = (dir_yaw * damp_yaw) + (dir_pitch * damp_pitch) + (dir_roll * damp_roll);
        body.apply_ang_accel((torque - damping) * CAR_TORQUE_SCALE);
    }

    if c.throttle != 0.0 {
        body.apply_central_force(body.forward() * c.throttle * THROTTLE_AIR_ACCEL * UU_TO_BT * CAR_MASS_BT);
    }
}

fn update_jump(s: &mut CarState, c: &Controls, cfg: &SimConfig, body: &mut Body, jump_pressed: bool, dt: f32) {
    if s.on_ground && !s.is_jumping {
        if s.has_jumped && s.jump_timer < JUMP_MIN_TIME + JUMP_RESET_TIME_PAD {
            // Still leaving the ground after a minimum-length jump; don't reset yet.
        } else {
            s.has_jumped = false;
            s.jump_timer = 0.0;
        }
    }

    if s.is_jumping {
        s.is_jumping = s.jump_timer < JUMP_MIN_TIME || (c.jump && s.jump_timer < JUMP_MAX_TIME);
    } else if s.on_ground && jump_pressed {
        s.is_jumping = true;
        s.jump_timer = 0.0;
        body.apply_central_impulse(body.up() * cfg.jump_immediate_force * UU_TO_BT * CAR_MASS_BT);
    }

    if s.is_jumping {
        s.has_jumped = true;
        let mut f = body.up() * cfg.jump_accel;
        if s.jump_timer < JUMP_MIN_TIME {
            f *= JUMP_PRE_MIN_ACCEL_SCALE;
        }
        body.apply_central_force(f * UU_TO_BT * CAR_MASS_BT);
    }

    if s.is_jumping || s.has_jumped {
        s.jump_timer += dt;
    }
}

fn update_auto_flip(s: &mut CarState, body: &mut Body, jump_pressed: bool, dt: f32) {
    if jump_pressed
        && let Some(n) = s.world_contact_normal
        && n.z > CAR_AUTOFLIP_NORMZ_THRESH
    {
        let (_, _, roll) = s.orientation.to_angles();
        let abs_roll = roll.abs();
        if abs_roll > CAR_AUTOFLIP_ROLL_THRESH {
            s.auto_flip_timer = CAR_AUTOFLIP_TIME * (abs_roll / core::f32::consts::PI);
            s.auto_flip_torque_scale = if roll > 0.0 { 1.0 } else { -1.0 };
            s.is_auto_flipping = true;
            body.apply_central_impulse(-body.up() * CAR_AUTOFLIP_IMPULSE * UU_TO_BT * CAR_MASS_BT);
        }
    }

    if s.is_auto_flipping {
        if s.auto_flip_timer <= 0.0 {
            s.is_auto_flipping = false;
            s.auto_flip_timer = 0.0;
        } else {
            body.ang_vel += body.forward() * CAR_AUTOFLIP_TORQUE * s.auto_flip_torque_scale * dt;
            s.auto_flip_timer -= dt;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_double_jump_or_flip(
    s: &mut CarState,
    c: &Controls,
    car: &CarConfig,
    cfg: &SimConfig,
    body: &mut Body,
    jump_pressed: bool,
    forward_speed: f32,
    dt: f32,
) {
    let tick_time_scale = dt / (1.0 / 120.0);

    if s.on_ground {
        s.has_double_jumped = false;
        s.has_flipped = false;
        s.air_time = 0.0;
        s.air_time_since_jump = 0.0;
        s.flip_timer = 0.0;
    } else {
        s.air_time += dt;
        if s.has_jumped && !s.is_jumping {
            s.air_time_since_jump += dt;
        } else {
            s.air_time_since_jump = 0.0;
        }

        if jump_pressed && s.air_time_since_jump < DOUBLEJUMP_MAX_DELAY {
            let input_magnitude = c.yaw.abs() + c.pitch.abs() + c.roll.abs();
            let is_flip_input = input_magnitude >= car.dodge_deadzone;
            let fresh = !s.has_double_jumped && !s.has_flipped;
            let mut can_use = if is_flip_input { fresh || cfg.unlimited_flips } else { fresh || cfg.unlimited_double_jumps };
            if s.is_auto_flipping {
                can_use = false;
            }

            if can_use {
                if is_flip_input {
                    s.flip_timer = 0.0;
                    s.has_flipped = true;
                    s.is_flipping = true;

                    let forward_speed_ratio = forward_speed.abs() / CAR_MAX_SPEED;
                    let mut dodge_dir = Vec3::new(-c.pitch, c.yaw + c.roll, 0.0);
                    if (c.yaw + c.roll).abs() < 0.1 && c.pitch.abs() < 0.1 {
                        dodge_dir = Vec3::ZERO;
                    } else {
                        dodge_dir = dodge_dir.safe_normalized();
                    }

                    s.flip_rel_torque = Vec3::new(-dodge_dir.y / tick_time_scale, dodge_dir.x / tick_time_scale, 0.0);

                    if dodge_dir.x.abs() < 0.1 {
                        dodge_dir.x = 0.0;
                    }
                    if dodge_dir.y.abs() < 0.1 {
                        dodge_dir.y = 0.0;
                    }

                    if !dodge_dir.fuzzy_zero() {
                        let should_dodge_backwards = if forward_speed.abs() < 100.0 {
                            dodge_dir.x < 0.0
                        } else {
                            (dodge_dir.x >= 0.0) != (forward_speed >= 0.0)
                        };

                        let mut initial = dodge_dir * FLIP_INITIAL_VEL_SCALE;
                        let max_speed_scale_x = if should_dodge_backwards {
                            FLIP_BACKWARD_IMPULSE_MAX_SPEED_SCALE
                        } else {
                            FLIP_FORWARD_IMPULSE_MAX_SPEED_SCALE
                        };
                        initial.x *= ((max_speed_scale_x - 1.0) * forward_speed_ratio) + 1.0;
                        initial.y *= ((FLIP_SIDE_IMPULSE_MAX_SPEED_SCALE - 1.0) * forward_speed_ratio) + 1.0;
                        if should_dodge_backwards {
                            initial.x *= FLIP_BACKWARD_IMPULSE_SCALE_X;
                        }

                        let f = body.forward();
                        let flat = Vec3::new(f.x, f.y, 0.0);
                        let flat_len = flat.length();
                        let forward_2d = if flat_len > f32::EPSILON * f32::EPSILON { flat / flat_len } else { Vec3::ZERO };
                        let right_2d = Vec3::new(-forward_2d.y, forward_2d.x, 0.0);
                        let final_delta_vel = initial.x * forward_2d + initial.y * right_2d;
                        body.apply_central_impulse(final_delta_vel * UU_TO_BT * CAR_MASS_BT);
                    }
                } else {
                    // Double jump.
                    body.apply_central_impulse(body.up() * JUMP_IMMEDIATE_FORCE * UU_TO_BT * CAR_MASS_BT);
                    s.has_double_jumped = true;
                }
            }
        }
    }

    if s.is_flipping {
        s.flip_timer += dt;
        if s.flip_timer <= FLIP_TORQUE_TIME
            && s.flip_timer >= FLIP_Z_DAMP_START
            && (body.vel.z < 0.0 || s.flip_timer < FLIP_Z_DAMP_END)
        {
            body.vel.z *= (1.0 - FLIP_Z_DAMP_120).powf(tick_time_scale);
        }
    } else if s.has_flipped {
        s.flip_timer += dt;
    }
}

fn update_auto_roll(body: &mut Body, ground_up: Vec3) {
    let ground_down = -ground_up;
    let (forward, right) = (body.forward(), body.right());

    let cross_right = ground_up.cross(forward);
    let cross_forward = ground_down.cross(cross_right);

    let right_torque_factor = 1.0 - right.dot(cross_right).clamp(0.0, 1.0);
    let forward_torque_factor = 1.0 - forward.dot(cross_forward).clamp(0.0, 1.0);

    let torque_dir_right = forward * if right.dot(ground_up) >= 0.0 { -1.0 } else { 1.0 };
    let torque_dir_forward = right * if forward.dot(ground_up) >= 0.0 { 1.0 } else { -1.0 };

    let torque_right = torque_dir_right * right_torque_factor;
    let torque_forward = torque_dir_forward * forward_torque_factor;

    body.apply_central_force(ground_down * CAR_AUTOROLL_FORCE * UU_TO_BT * CAR_MASS_BT);
    body.apply_ang_accel((torque_forward + torque_right) * CAR_AUTOROLL_TORQUE);
}

fn update_boost(s: &mut CarState, c: &Controls, cfg: &SimConfig, body: &mut Body, dt: f32) {
    if s.boost_amount > 0.0 {
        if s.is_boosting {
            s.is_boosting = c.boost || s.boosting_time < BOOST_MIN_TIME;
        } else if c.boost {
            s.is_boosting = true;
        }
    } else {
        s.is_boosting = false;
    }

    if s.is_boosting {
        s.boosting_time += dt;
    } else {
        s.boosting_time = 0.0;
    }

    if s.is_boosting {
        if !cfg.unlimited_boost {
            s.boost_amount = (s.boost_amount - cfg.boost_used_per_second * dt).max(0.0);
        }
        let accel = if s.on_ground { cfg.boost_accel_ground } else { cfg.boost_accel_air };
        body.apply_central_force(accel * UU_TO_BT * body.forward() * CAR_MASS_BT);
        s.time_since_boosted = 0.0;
    } else {
        s.time_since_boosted += dt;
        if cfg.recharge_boost_enabled && s.time_since_boosted >= cfg.recharge_boost_delay {
            s.boost_amount += cfg.recharge_boost_per_second * dt;
        }
    }
    s.boost_amount = s.boost_amount.min(BOOST_MAX);
}
