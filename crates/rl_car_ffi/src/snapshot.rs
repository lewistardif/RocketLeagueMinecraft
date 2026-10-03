//! Lossless binary encoding of [`CarState`], for hosts that hand a car between machines (client
//! to server) or store it (save files). Little-endian, versioned; `decode(encode(s)) == s`.

use rl_car_core::manifold::{ContactManifold, MAX_MANIFOLDS, MAX_POINTS, ManifoldPoint};
use rl_car_core::{CarState, Controls, HitboxPreset, Mat3, RotMat, Vec3, WheelState};

const MAGIC: [u8; 4] = *b"RLC1";

#[derive(Default)]
struct W(Vec<u8>);

impl W {
    fn f(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn b(&mut self, v: bool) {
        self.0.push(v as u8);
    }
    fn v(&mut self, v: Vec3) {
        self.f(v.x);
        self.f(v.y);
        self.f(v.z);
    }
    fn opt_v(&mut self, v: Option<Vec3>) {
        self.b(v.is_some());
        self.v(v.unwrap_or(Vec3::ZERO));
    }
}

struct R<'a>(&'a [u8]);

impl R<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, rest) = self.0.split_first_chunk::<N>()?;
        self.0 = rest;
        Some(*head)
    }
    fn f(&mut self) -> Option<f32> {
        self.take::<4>().map(f32::from_le_bytes)
    }
    fn u(&mut self) -> Option<u32> {
        self.take::<4>().map(u32::from_le_bytes)
    }
    fn b(&mut self) -> Option<bool> {
        match self.take::<1>()?[0] {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }
    fn v(&mut self) -> Option<Vec3> {
        Some(Vec3::new(self.f()?, self.f()?, self.f()?))
    }
    fn opt_v(&mut self) -> Option<Option<Vec3>> {
        let some = self.b()?;
        let v = self.v()?;
        Some(some.then_some(v))
    }
}

fn write_controls(w: &mut W, c: &Controls) {
    for x in [c.throttle, c.steer, c.pitch, c.yaw, c.roll] {
        w.f(x);
    }
    w.b(c.jump);
    w.b(c.boost);
    w.b(c.handbrake);
}

fn read_controls(r: &mut R) -> Option<Controls> {
    Some(Controls {
        throttle: r.f()?,
        steer: r.f()?,
        pitch: r.f()?,
        yaw: r.f()?,
        roll: r.f()?,
        jump: r.b()?,
        boost: r.b()?,
        handbrake: r.b()?,
    })
}

pub fn encode(s: &CarState) -> Vec<u8> {
    let mut w = W::default();
    w.0.extend_from_slice(&MAGIC);
    w.u(s.hitbox_preset as u32);
    w.v(s.position);
    w.v(s.velocity);
    for c in 0..3 {
        w.v(s.orientation.0.col(c));
    }
    w.v(s.angular_velocity);
    w.f(s.boost_amount);
    w.b(s.on_ground);
    for c in s.wheel_contacts {
        w.b(c);
    }
    for b in [s.has_jumped, s.has_double_jumped, s.has_flipped, s.is_jumping, s.is_flipping] {
        w.b(b);
    }
    w.f(s.jump_timer);
    w.f(s.flip_timer);
    w.v(s.flip_rel_torque);
    w.f(s.air_time);
    w.f(s.air_time_since_jump);
    w.b(s.is_boosting);
    w.f(s.boosting_time);
    w.f(s.time_since_boosted);
    w.b(s.is_supersonic);
    w.f(s.supersonic_time);
    w.f(s.handbrake_val);
    w.b(s.is_auto_flipping);
    w.f(s.auto_flip_timer);
    w.f(s.auto_flip_torque_scale);
    w.opt_v(s.world_contact_normal);
    write_controls(&mut w, &s.last_controls);
    for wh in &s.wheels {
        for x in [wh.steer_angle, wh.engine_force, wh.brake, wh.lat_friction, wh.long_friction, wh.extra_pushback, wh.suspension_length] {
            w.f(x);
        }
        w.b(wh.contact.is_some());
        let (p, n) = wh.contact.unwrap_or((Vec3::ZERO, Vec3::ZERO));
        w.v(p);
        w.v(n);
    }
    for m in &s.manifolds.list {
        w.b(m.active);
        w.u(m.surface);
        w.u(m.count as u32);
        for p in &m.points {
            w.v(p.local_a);
            w.v(p.world_b);
            w.v(p.normal);
            w.f(p.distance);
            w.f(p.applied_impulse);
            w.u(p.lifetime);
        }
    }
    w.0
}

pub fn decode(bytes: &[u8]) -> Option<CarState> {
    let mut r = R(bytes);
    if r.take::<4>()? != MAGIC {
        return None;
    }
    let preset = *HitboxPreset::ALL.get(r.u()? as usize)?;
    let mut s = CarState::new(preset);
    s.position = r.v()?;
    s.velocity = r.v()?;
    let (c0, c1, c2) = (r.v()?, r.v()?, r.v()?);
    s.orientation = RotMat(Mat3::from_cols(c0, c1, c2));
    s.angular_velocity = r.v()?;
    s.boost_amount = r.f()?;
    s.on_ground = r.b()?;
    for c in s.wheel_contacts.iter_mut() {
        *c = r.b()?;
    }
    s.has_jumped = r.b()?;
    s.has_double_jumped = r.b()?;
    s.has_flipped = r.b()?;
    s.is_jumping = r.b()?;
    s.is_flipping = r.b()?;
    s.jump_timer = r.f()?;
    s.flip_timer = r.f()?;
    s.flip_rel_torque = r.v()?;
    s.air_time = r.f()?;
    s.air_time_since_jump = r.f()?;
    s.is_boosting = r.b()?;
    s.boosting_time = r.f()?;
    s.time_since_boosted = r.f()?;
    s.is_supersonic = r.b()?;
    s.supersonic_time = r.f()?;
    s.handbrake_val = r.f()?;
    s.is_auto_flipping = r.b()?;
    s.auto_flip_timer = r.f()?;
    s.auto_flip_torque_scale = r.f()?;
    s.world_contact_normal = r.opt_v()?;
    s.last_controls = read_controls(&mut r)?;
    for wh in s.wheels.iter_mut() {
        *wh = WheelState {
            steer_angle: r.f()?,
            engine_force: r.f()?,
            brake: r.f()?,
            lat_friction: r.f()?,
            long_friction: r.f()?,
            extra_pushback: r.f()?,
            suspension_length: r.f()?,
            contact: None,
        };
        let some = r.b()?;
        let (p, n) = (r.v()?, r.v()?);
        wh.contact = some.then_some((p, n));
    }
    for m in s.manifolds.list.iter_mut() {
        let active = r.b()?;
        let surface = r.u()?;
        let count = r.u()? as usize;
        if count > MAX_POINTS {
            return None;
        }
        let mut points = [ManifoldPoint::default(); MAX_POINTS];
        for p in points.iter_mut() {
            *p = ManifoldPoint {
                local_a: r.v()?,
                world_b: r.v()?,
                normal: r.v()?,
                distance: r.f()?,
                applied_impulse: r.f()?,
                lifetime: r.u()?,
            };
        }
        *m = ContactManifold { active, surface, count, points };
    }
    debug_assert_eq!(s.manifolds.list.len(), MAX_MANIFOLDS);
    r.0.is_empty().then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rl_car_core::*;

    #[test]
    fn round_trips_a_state_with_contacts_and_flips() {
        let world = PlaneWorld::soccar_box();
        let mut s = CarState::new(HitboxPreset::Dominus);
        s.position.x = 4000.0;
        s.velocity.x = 1500.0;
        // Drive into the wall (body contacts, manifolds), then jump and dodge.
        for t in 0..200 {
            let c = Controls { throttle: 1.0, jump: t > 150, pitch: if t > 170 { -1.0 } else { 0.0 }, ..Default::default() };
            s = step(&s, &c, &world, TICK_DT);
        }
        assert!(s.manifolds.list.iter().any(|m| m.active) || s.has_flipped);
        let bytes = encode(&s);
        assert_eq!(decode(&bytes), Some(s));
        assert_eq!(decode(&bytes[..bytes.len() - 1]), None);
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(decode(&longer), None);
    }
}
