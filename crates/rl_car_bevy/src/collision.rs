//! `CollisionWorld` implemented on top of Bevy/avian spatial queries.
//!
//! The physics core asks in Rocket League space (uu, Z-up); everything is converted through
//! [`crate::convert`] and answered by avian's query pipeline over the arena's static colliders.

use crate::convert::*;
use avian3d::collision::collider::contact_query;
use avian3d::prelude::*;
use bevy::prelude::*;
use rl_car_core::{CollisionWorld, Contact, Obb, RayHit, Vec3 as RVec3};

/// Marker for static colliders the car can touch.
#[derive(Component)]
pub struct ArenaCollider;

pub type ArenaColliderQuery<'w, 's> = Query<'w, 's, (Entity, &'static Collider, &'static Position, &'static Rotation), With<ArenaCollider>>;

pub struct AvianWorld<'a, 'w, 's, 'w2, 's2> {
    pub spatial: &'a SpatialQuery<'w, 's>,
    pub colliders: &'a ArenaColliderQuery<'w2, 's2>,
}

fn surface_id(e: Entity) -> u32 {
    (e.to_bits() & 0xffff_ffff) as u32
}

impl CollisionWorld for AvianWorld<'_, '_, '_, '_, '_> {
    fn raycast(&self, origin: RVec3, dir: RVec3, max_dist: f32) -> Option<RayHit> {
        let o = pos_to_bevy(origin);
        let d = Dir3::new(dir_to_bevy(dir)).ok()?;
        // Non-solid + "front faces only" mirrors Bullet's ray vs triangle/plane behaviour:
        // a ray starting inside geometry does not report the surface it is leaving.
        let hits = self.spatial.ray_hits(o, d, max_dist / UU_PER_M, 8, false, &SpatialQueryFilter::default());
        hits.into_iter()
            .filter(|h| self.colliders.contains(h.entity) && h.normal.dot(*d) < 0.0)
            .min_by(|a, b| a.distance.total_cmp(&b.distance))
            .map(|h| {
                let dist = h.distance * UU_PER_M;
                RayHit { distance: dist, point: origin + dir * dist, normal: dir_to_rl(h.normal.normalize()) }
            })
    }

    fn box_contacts(&self, obb: &Obb, margin: f32, out: &mut Vec<Contact>) {
        // Car-local RL axes (fwd, right, up) are Bevy-local (x, z, y).
        let h = obb.half_extents;
        let size_m = Vec3::new(h.x, h.z, h.y) * 2.0 / UU_PER_M;
        let margin_m = margin / UU_PER_M;
        let center = pos_to_bevy(obb.center);
        let rot = rot_to_bevy(&obb.axes);

        let car = Collider::cuboid(size_m.x, size_m.y, size_m.z);
        let probe = Collider::cuboid(size_m.x + 2.0 * margin_m, size_m.y + 2.0 * margin_m, size_m.z + 2.0 * margin_m);
        let mut candidates = self.spatial.shape_intersections(&probe, center, rot, &SpatialQueryFilter::default());
        // Stable order so the simulation stays deterministic.
        candidates.sort_by_key(|e| e.to_bits());

        for e in candidates {
            let Ok((entity, collider, pos, wrot)) = self.colliders.get(e) else { continue };
            let Ok(Some(c)) = contact_query::contact(&car, center, rot, collider, pos.0, *wrot, margin_m) else { continue };
            // avian reports points/normals rotated into each shape's frame (no translation).
            let world_point = wrot.0 * c.local_point2;
            let mut surface_point = pos_to_rl(world_point);
            let mut local_n = c.local_normal2.normalize();
            // For box colliders, use the exact plane of the world face being touched (Bullet's
            // convex-vs-plane semantics). parry picks the separating axis with the least
            // penetration, which for a slightly tilted car is often the *car's* face normal; that
            // tilt would change which car corner counts as deepest.
            if let Some(cuboid) = collider.shape_scaled().as_cuboid() {
                let half = Vec3::new(cuboid.half_extents.x, cuboid.half_extents.y, cuboid.half_extents.z);
                let p = wrot.0.inverse() * (world_point - pos.0);
                let mut best: Option<(f32, Vec3)> = None;
                for i in 0..3 {
                    let s = if p[i] >= 0.0 { 1.0 } else { -1.0 };
                    let on_face = p[i].abs() >= half[i] - 0.01;
                    let face_n = Vec3::AXES[i] * s;
                    let align = face_n.dot(local_n);
                    if on_face && align > 0.0 && best.is_none_or(|(b, _)| align > b) {
                        best = Some((align, face_n));
                    }
                }
                if let Some((_, face_n)) = best {
                    local_n = face_n;
                    surface_point = pos_to_rl(pos.0 + wrot.0 * (face_n * half));
                }
            }
            let normal = dir_to_rl((wrot.0 * local_n).normalize());
            // parry returns the middle of a face-face contact region; the game (Bullet's
            // convex-vs-plane) uses the box's deepest vertex. Take parry's surface normal and point,
            // and report the car vertex furthest into that surface.
            let vertex = obb.support(-normal);
            let d = normal.dot(vertex - surface_point);
            if d >= margin {
                continue;
            }
            out.push(Contact { point: vertex - normal * d, normal, depth: -d, surface: surface_id(entity) });
        }
    }
}
