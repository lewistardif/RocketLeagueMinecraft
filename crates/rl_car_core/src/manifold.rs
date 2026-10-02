//! Persistent contact manifolds for car-body vs world contacts.
//!
//! Rocket League's physics (Bullet) does not see a whole resting face at once: each tick the
//! narrow phase reports only the deepest point per touching surface, and points are cached in a
//! per-surface manifold of at most four, refreshed every tick (dropped when they separate or slide
//! away) and warm-started with 85% of last tick's impulse. That is why the car rocks/rotates on
//! the first tick of an impact instead of stopping dead. This module reproduces that behaviour so
//! body collisions match the game.

use crate::math::{Mat3, Vec3};

pub const MAX_MANIFOLDS: usize = 4;
pub const MAX_POINTS: usize = 4;
const WARMSTARTING_FACTOR: f32 = 0.85;

/// A cached contact point (bt units).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ManifoldPoint {
    /// Point on the car, in car-local space.
    pub local_a: Vec3,
    /// Point on the world surface, in world space (world geometry is static).
    pub world_b: Vec3,
    /// Surface normal (world -> car).
    pub normal: Vec3,
    /// Signed separation; negative = penetrating.
    pub distance: f32,
    /// Normal impulse applied last tick (for warm starting).
    pub applied_impulse: f32,
    pub lifetime: u32,
}

/// All cached points against one host surface/collider.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContactManifold {
    pub active: bool,
    pub surface: u32,
    pub count: usize,
    pub points: [ManifoldPoint; MAX_POINTS],
}

impl ContactManifold {
    fn remove(&mut self, index: usize) {
        let last = self.count - 1;
        if index != last {
            self.points[index] = self.points[last];
        }
        self.points[last] = ManifoldPoint::default();
        self.count -= 1;
    }

    /// Bullet `refreshContactPoints` against a static body.
    pub fn refresh(&mut self, pos: Vec3, basis: &Mat3, breaking_threshold: f32) {
        for i in (0..self.count).rev() {
            let p = &mut self.points[i];
            let world_a = *basis * p.local_a + pos;
            p.distance = (world_a - p.world_b).dot(p.normal);
            p.lifetime += 1;
        }
        for i in (0..self.count).rev() {
            let p = self.points[i];
            if p.distance > breaking_threshold {
                self.remove(i);
            } else {
                let world_a = *basis * p.local_a + pos;
                let projected = world_a - p.normal * p.distance;
                let diff = p.world_b - projected;
                if diff.dot(diff) > breaking_threshold * breaking_threshold {
                    self.remove(i);
                }
            }
        }
    }

    /// Bullet `addManifoldPoint` (with RocketSim's "never merge" cache-entry rule).
    pub fn add(&mut self, pt: ManifoldPoint) -> usize {
        let index = if self.count == MAX_POINTS {
            self.sort_cached_points(&pt)
        } else {
            self.count += 1;
            self.count - 1
        };
        self.points[index] = pt;
        index
    }

    /// Choose which cached point a new point replaces: keep the deepest, maximise covered area.
    fn sort_cached_points(&self, pt: &ManifoldPoint) -> usize {
        let mut max_pen_index: i32 = -1;
        let mut max_pen = pt.distance;
        for i in 0..4 {
            if self.points[i].distance < max_pen {
                max_pen_index = i as i32;
                max_pen = self.points[i].distance;
            }
        }
        let p = |i: usize| self.points[i].local_a;
        let a = pt.local_a;
        let mut res = [0.0f32; 4];
        if max_pen_index != 0 {
            res[0] = (a - p(1)).cross(p(3) - p(2)).length_squared();
        }
        if max_pen_index != 1 {
            res[1] = (a - p(0)).cross(p(3) - p(2)).length_squared();
        }
        if max_pen_index != 2 {
            res[2] = (a - p(0)).cross(p(3) - p(1)).length_squared();
        }
        if max_pen_index != 3 {
            res[3] = (a - p(0)).cross(p(2) - p(1)).length_squared();
        }
        let mut best = 0;
        let mut best_val = f32::MIN;
        for (i, v) in res.iter().enumerate() {
            if v.abs() > best_val {
                best = i;
                best_val = v.abs();
            }
        }
        best
    }
}

/// The manifold set carried in [`crate::CarState`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Manifolds {
    pub list: [ContactManifold; MAX_MANIFOLDS],
}

impl Manifolds {
    pub fn refresh_all(&mut self, pos: Vec3, basis: &Mat3, threshold: f32) {
        for m in self.list.iter_mut().filter(|m| m.active && m.count > 0) {
            m.refresh(pos, basis, threshold);
        }
    }

    /// Manifold for `surface`, creating one (or recycling an empty/oldest slot) if needed.
    pub fn get_or_create(&mut self, surface: u32) -> &mut ContactManifold {
        if let Some(i) = self.list.iter().position(|m| m.active && m.surface == surface) {
            return &mut self.list[i];
        }
        let i = self
            .list
            .iter()
            .position(|m| !m.active)
            .or_else(|| self.list.iter().position(|m| m.count == 0))
            .unwrap_or(MAX_MANIFOLDS - 1);
        self.list[i] = ContactManifold { active: true, surface, ..Default::default() };
        &mut self.list[i]
    }

    /// Drop manifolds whose surface was not reported and that hold no points.
    pub fn prune(&mut self) {
        for m in self.list.iter_mut() {
            if m.active && m.count == 0 {
                *m = ContactManifold::default();
            }
        }
    }

    pub fn warmstart_factor() -> f32 {
        WARMSTARTING_FACTOR
    }
}
