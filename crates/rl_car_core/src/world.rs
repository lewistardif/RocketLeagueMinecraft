//! The host-provided collision interface.
//!
//! The core never owns world geometry. Every tick it asks the host two kinds of question,
//! always in Rocket League space and units (uu, Z-up):
//!
//! 1. **Wheel raycasts** — one short ray per wheel along the suspension direction.
//! 2. **Body contacts** — the car hitbox (an oriented box) against static world geometry.
//!
//! Hosts implement [`CollisionWorld`] on top of whatever collision system they have
//! (Bevy/avian spatial queries, a voxel grid, a navmesh, analytic planes, ...).
//! Implementations must be deterministic and side-effect free for the core to be deterministic.

use crate::math::{Mat3, Vec3};

/// Result of a raycast.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    /// Distance along the ray (uu), in `[0, max_dist]`.
    pub distance: f32,
    /// Hit point in world space (uu).
    pub point: Vec3,
    /// Unit surface normal at the hit point, facing the ray origin side.
    pub normal: Vec3,
}

/// An oriented box (the car hitbox).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obb {
    /// Box centre in world space (uu).
    pub center: Vec3,
    /// Columns are the box's local X/Y/Z axes in world space (unit, orthonormal).
    pub axes: Mat3,
    /// Half size along each local axis (uu).
    pub half_extents: Vec3,
}

impl Obb {
    /// The 8 corners in world space.
    pub fn corners(&self) -> [Vec3; 8] {
        let mut out = [Vec3::ZERO; 8];
        for (i, c) in out.iter_mut().enumerate() {
            let sx = if i & 1 == 0 { -1.0 } else { 1.0 };
            let sy = if i & 2 == 0 { -1.0 } else { 1.0 };
            let sz = if i & 4 == 0 { -1.0 } else { 1.0 };
            *c = self.center
                + self.axes.col(0) * (sx * self.half_extents.x)
                + self.axes.col(1) * (sy * self.half_extents.y)
                + self.axes.col(2) * (sz * self.half_extents.z);
        }
        out
    }

    /// Support point in direction `dir`.
    pub fn support(&self, dir: Vec3) -> Vec3 {
        let mut p = self.center;
        for a in 0..3 {
            let axis = self.axes.col(a);
            let s = if axis.dot(dir) >= 0.0 { 1.0 } else { -1.0 };
            p += axis * (s * self.half_extents[a]);
        }
        p
    }
}

/// One contact between the car body and the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    /// Contact point on the **world** surface (uu).
    pub point: Vec3,
    /// Unit normal pointing from the world towards the car.
    pub normal: Vec3,
    /// Penetration depth along `normal` (uu). Positive = overlapping. Slightly negative values
    /// (down to `-margin` passed to [`CollisionWorld::box_contacts`]) are allowed and act as
    /// speculative "do not approach" contacts, like Bullet's contact breaking threshold.
    pub depth: f32,
    /// Host-defined id of the collider/surface this contact belongs to (e.g. plane index or
    /// entity id). The core caches up to 4 points per surface across ticks, like the game does.
    pub surface: u32,
}

/// Static world collision provided by the host. All quantities in uu, Rocket League axes.
pub trait CollisionWorld {
    /// Closest hit of the segment `origin + dir * t`, `t in [0, max_dist]`. `dir` is unit length.
    fn raycast(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<RayHit>;

    /// Append contacts between `obb` and the world to `out`, for every surface the box
    /// penetrates or is within `margin` of.
    ///
    /// Report **the single deepest point per touching face/triangle** (for a plane: the box
    /// vertex [`Obb::support`]`(-normal)`), tagged with the id of the collider it belongs to.
    /// The core accumulates these into persistent 4-point manifolds over successive ticks, which
    /// is how Rocket League (Bullet) resolves body contacts; reporting all corners at once makes
    /// impacts noticeably stiffer than in the game.
    fn box_contacts(&self, obb: &Obb, margin: f32, out: &mut Vec<Contact>);
}

/// A world with no geometry at all (free fall forever).
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyWorld;

impl CollisionWorld for EmptyWorld {
    fn raycast(&self, _: Vec3, _: Vec3, _: f32) -> Option<RayHit> {
        None
    }
    fn box_contacts(&self, _: &Obb, _: f32, _: &mut Vec<Contact>) {}
}

/// An infinite static plane: points `p` with `normal · (p - point) >= 0` are free space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    pub point: Vec3,
    /// Unit normal pointing into free space.
    pub normal: Vec3,
}

/// A world made of infinite half-space planes. Exact, cheap, and the geometry used by the
/// validation harness (it is reproduced identically on the RocketSim side).
#[derive(Clone, Debug, Default)]
pub struct PlaneWorld {
    pub planes: Vec<Plane>,
}

impl PlaneWorld {
    pub fn new(planes: Vec<Plane>) -> PlaneWorld {
        PlaneWorld { planes }
    }

    /// Just a floor at z = 0.
    pub fn floor() -> PlaneWorld {
        PlaneWorld::new(vec![Plane { point: Vec3::ZERO, normal: Vec3::Z }])
    }

    /// The flat parts of a standard soccar arena (floor, ceiling, four walls), without the
    /// curved corners/goals (those come from game meshes that are not redistributable).
    pub fn soccar_box() -> PlaneWorld {
        PlaneWorld::new(vec![
            Plane { point: Vec3::ZERO, normal: Vec3::Z },
            Plane { point: Vec3::new(0.0, 0.0, 2048.0), normal: -Vec3::Z },
            Plane { point: Vec3::new(-4096.0, 0.0, 1024.0), normal: Vec3::X },
            Plane { point: Vec3::new(4096.0, 0.0, 1024.0), normal: -Vec3::X },
            Plane { point: Vec3::new(0.0, -5120.0, 1024.0), normal: Vec3::Y },
            Plane { point: Vec3::new(0.0, 5120.0, 1024.0), normal: -Vec3::Y },
        ])
    }
}

impl CollisionWorld for PlaneWorld {
    fn raycast(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<RayHit> {
        let mut best: Option<RayHit> = None;
        for pl in &self.planes {
            let d0 = pl.normal.dot(origin - pl.point);
            let denom = pl.normal.dot(dir);
            // Only rays travelling into the plane from the free side hit it.
            if d0 < 0.0 || denom >= 0.0 {
                continue;
            }
            let t = -d0 / denom;
            if t <= max_dist && best.is_none_or(|b| t < b.distance) {
                best = Some(RayHit { distance: t, point: origin + dir * t, normal: pl.normal });
            }
        }
        best
    }

    fn box_contacts(&self, obb: &Obb, margin: f32, out: &mut Vec<Contact>) {
        for (i, pl) in self.planes.iter().enumerate() {
            let deepest = obb.support(-pl.normal);
            let d = pl.normal.dot(deepest - pl.point);
            if d < margin {
                out.push(Contact { point: deepest - pl.normal * d, normal: pl.normal, depth: -d, surface: i as u32 });
            }
        }
    }
}
