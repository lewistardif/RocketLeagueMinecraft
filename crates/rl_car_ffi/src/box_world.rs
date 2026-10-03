//! [`CollisionWorld`] over a soup of axis-aligned boxes (voxel worlds: Minecraft blocks, slabs,
//! stairs, ...), in Rocket League space and units.
//!
//! A voxel floor is hundreds of separate boxes. Colliding against each box on its own produces
//! "ghost" contacts: the car catches on the side faces *between* coplanar blocks, which do not
//! exist in a real continuous floor. So the boxes are first turned into the **exposed surface**
//! of their union:
//!
//! 1. Every box contributes its 6 faces.
//! 2. On each plane, a face is cut away where an opposite-facing face touches it (two boxes
//!    sharing a side: neither side is reachable).
//! 3. What is left is merged greedily into as few rectangles as possible, so a flat floor of
//!    blocks becomes one face.
//!
//! Queries then work like [`rl_car_core::PlaneWorld`] restricted to those rectangles. As long as
//! the car's deepest corner is over a face, the result is bit-identical to an infinite plane
//! (see the tests). Surface ids are derived from the plane, so they stay stable when the
//! host rebuilds the world every tick around a moving car.

use rl_car_core::{CollisionWorld, Contact, Obb, RayHit, Vec3};

/// An axis-aligned box (uu, Rocket League axes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

/// One exposed, merged rectangle of the world surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Face {
    /// Axis of the normal (0 = X, 1 = Y, 2 = Z).
    pub axis: usize,
    /// +1 or -1: the normal is `sign * axis`, pointing into free space.
    pub sign: f32,
    /// Position of the plane along `axis`.
    pub coord: f32,
    /// Extent along the two other axes, `u = (axis + 1) % 3`, `v = (axis + 2) % 3`.
    pub u: [f32; 2],
    pub v: [f32; 2],
    pub surface: u32,
}

impl Face {
    pub fn normal(&self) -> Vec3 {
        let mut n = Vec3::ZERO;
        n[self.axis] = self.sign;
        n
    }

    #[inline]
    fn uv_axes(&self) -> (usize, usize) {
        ((self.axis + 1) % 3, (self.axis + 2) % 3)
    }

    #[inline]
    fn contains_uv(&self, p: Vec3, eps: f32) -> bool {
        let (ua, va) = self.uv_axes();
        p[ua] >= self.u[0] - eps && p[ua] <= self.u[1] + eps && p[va] >= self.v[0] - eps && p[va] <= self.v[1] + eps
    }
}

/// One box face before merging: (normal axis, plane position, normal sign, u extent, v extent).
type RawFace = (usize, f32, f32, [f32; 2], [f32; 2]);

/// Tolerance for "is this point on the face" checks (uu).
const EDGE_EPS: f32 = 1e-3;

/// Contacts deeper than this (uu) are rejected. A car moves at most ~19 uu per tick and contacts
/// are reported speculatively before it touches, so real penetrations stay far below this. Deeper
/// ones come from faces the car is *beside* or *behind* (the side of a step it is driving over,
/// the far side of a thin slab), which a mesh-based collider would never report.
pub const DEFAULT_MAX_PENETRATION: f32 = 30.0;

#[derive(Clone, Debug)]
pub struct BoxWorld {
    faces: Vec<Face>,
    pub max_penetration: f32,
}

impl Default for BoxWorld {
    fn default() -> Self {
        BoxWorld { faces: Vec::new(), max_penetration: DEFAULT_MAX_PENETRATION }
    }
}

impl BoxWorld {
    pub fn new(boxes: &[Aabb]) -> BoxWorld {
        let mut w = BoxWorld::default();
        w.set_boxes(boxes);
        w
    }

    pub fn faces(&self) -> &[Face] {
        &self.faces
    }

    /// Rebuilds the surface from `boxes`. The result does not depend on the order of `boxes`.
    pub fn set_boxes(&mut self, boxes: &[Aabb]) {
        let mut raw: Vec<RawFace> = Vec::with_capacity(boxes.len() * 6);
        for b in boxes {
            let size = b.max - b.min;
            if !(size.x > EDGE_EPS && size.y > EDGE_EPS && size.z > EDGE_EPS) {
                continue; // degenerate or NaN
            }
            for axis in 0..3 {
                let (ua, va) = ((axis + 1) % 3, (axis + 2) % 3);
                let u = [b.min[ua], b.max[ua]];
                let v = [b.min[va], b.max[va]];
                raw.push((axis, b.min[axis], -1.0, u, v));
                raw.push((axis, b.max[axis], 1.0, u, v));
            }
        }
        raw.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));

        self.faces.clear();
        let mut start = 0;
        while start < raw.len() {
            let (axis, coord) = (raw[start].0, raw[start].1);
            let mut end = start;
            while end < raw.len() && raw[end].0 == axis && raw[end].1.to_bits() == coord.to_bits() {
                end += 1;
            }
            merge_plane(axis, coord, &raw[start..end], &mut self.faces);
            start = end;
        }
    }
}

/// Exposed surface of one plane: union of faces of each sign minus the opposite sign, merged.
fn merge_plane(axis: usize, coord: f32, rects: &[RawFace], out: &mut Vec<Face>) {
    let mut us: Vec<f32> = rects.iter().flat_map(|r| r.3).collect();
    let mut vs: Vec<f32> = rects.iter().flat_map(|r| r.4).collect();
    for c in [&mut us, &mut vs] {
        c.sort_by(f32::total_cmp);
        c.dedup_by(|a, b| a.to_bits() == b.to_bits());
    }
    let (nu, nv) = (us.len() - 1, vs.len() - 1);
    let idx = |c: &[f32], x: f32| c.binary_search_by(|p| p.total_cmp(&x)).unwrap();

    // Coverage grid per sign: bit 0 = some +face covers the cell, bit 1 = some -face does.
    let mut cover = vec![0u8; nu * nv];
    for r in rects {
        let bit = if r.2 > 0.0 { 1 } else { 2 };
        let (u0, u1) = (idx(&us, r.3[0]), idx(&us, r.3[1]));
        let (v0, v1) = (idx(&vs, r.4[0]), idx(&vs, r.4[1]));
        for j in v0..v1 {
            for i in u0..u1 {
                cover[j * nu + i] |= bit;
            }
        }
    }

    for (sign, bit) in [(-1.0f32, 2u8), (1.0f32, 1u8)] {
        let surface = surface_id(axis, sign, coord);
        let exposed = |c: u8| c == bit; // covered by this sign only
        let mut used = vec![false; nu * nv];
        for j in 0..nv {
            for i in 0..nu {
                if used[j * nu + i] || !exposed(cover[j * nu + i]) {
                    continue;
                }
                // Greedy: widen along u, then grow along v while the whole row span is free.
                let mut i1 = i + 1;
                while i1 < nu && !used[j * nu + i1] && exposed(cover[j * nu + i1]) {
                    i1 += 1;
                }
                let mut j1 = j + 1;
                while j1 < nv && (i..i1).all(|k| !used[j1 * nu + k] && exposed(cover[j1 * nu + k])) {
                    j1 += 1;
                }
                for jj in j..j1 {
                    for k in i..i1 {
                        used[jj * nu + k] = true;
                    }
                }
                out.push(Face { axis, sign, coord, u: [us[i], us[i1]], v: [vs[j], vs[j1]], surface });
            }
        }
    }
}

/// Stable id of a plane (FNV-1a over axis, sign and position).
fn surface_id(axis: usize, sign: f32, coord: f32) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for byte in [axis as u8, (sign > 0.0) as u8].into_iter().chain(coord.to_bits().to_le_bytes()) {
        h ^= byte as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

impl CollisionWorld for BoxWorld {
    fn raycast(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<RayHit> {
        let mut best: Option<RayHit> = None;
        for f in &self.faces {
            let n = f.normal();
            // Same arithmetic as PlaneWorld so a block floor matches an infinite plane exactly.
            let mut on_plane = Vec3::ZERO;
            on_plane[f.axis] = f.coord;
            let d0 = n.dot(origin - on_plane);
            let denom = n.dot(dir);
            if d0 < 0.0 || denom >= 0.0 {
                continue;
            }
            let t = -d0 / denom;
            if t > max_dist || best.is_some_and(|b| t >= b.distance) {
                continue;
            }
            let point = origin + dir * t;
            if f.contains_uv(point, EDGE_EPS) {
                best = Some(RayHit { distance: t, point, normal: n });
            }
        }
        best
    }

    fn box_contacts(&self, obb: &Obb, margin: f32, out: &mut Vec<Contact>) {
        for f in &self.faces {
            let n = f.normal();
            let mut on_plane = Vec3::ZERO;
            on_plane[f.axis] = f.coord;
            // Fast reject with the box's deepest vertex against the infinite plane.
            let vertex = obb.support(-n);
            let d = n.dot(vertex - on_plane);
            if d >= margin || -d > self.max_penetration + obb_extent_along(obb, n) * 2.0 {
                continue;
            }
            let deepest = if f.contains_uv(vertex, 0.0) { Some(vertex) } else { deepest_in_prism(obb, f) };
            let Some(p) = deepest else { continue };
            let d = n.dot(p - on_plane);
            if d >= margin || -d > self.max_penetration {
                continue;
            }
            out.push(Contact { point: p - n * d, normal: n, depth: -d, surface: f.surface });
        }
    }
}

fn obb_extent_along(obb: &Obb, n: Vec3) -> f32 {
    (0..3).map(|a| obb.axes.col(a).dot(n).abs() * obb.half_extents[a]).sum()
}

/// Deepest point (minimum along the face normal) of the box clipped to the infinite prism over
/// the face rectangle, or `None` if the box does not overlap the prism.
///
/// The minimum of a linear function over the convex intersection lies at one of its vertices,
/// which are: box corners inside the prism, box edges crossing the prism's side planes, and the
/// prism's four edge lines entering/leaving the box.
fn deepest_in_prism(obb: &Obb, f: &Face) -> Option<Vec3> {
    let (ua, va) = f.uv_axes();
    let axis = f.axis;
    let mut best: Option<(f32, Vec3)> = None;
    let mut consider = |p: Vec3| {
        if f.contains_uv(p, EDGE_EPS) {
            let key = f.sign * p[axis];
            if best.is_none_or(|(k, _)| key < k) {
                best = Some((key, p));
            }
        }
    };

    let corners = obb.corners();
    for &c in &corners {
        consider(c);
    }
    for i in 0..8usize {
        for bit in [1usize, 2, 4] {
            if i & bit != 0 {
                continue;
            }
            let (a, b) = (corners[i], corners[i | bit]);
            for (ax, bounds) in [(ua, f.u), (va, f.v)] {
                let len = b[ax] - a[ax];
                if len == 0.0 {
                    continue;
                }
                for c in bounds {
                    let t = (c - a[ax]) / len;
                    if (0.0..=1.0).contains(&t) {
                        let mut p = a + (b - a) * t;
                        p[ax] = c;
                        consider(p);
                    }
                }
            }
        }
    }
    // Prism edges: lines {u = uc, v = vc} along the normal axis, clipped by the box slabs.
    for uc in f.u {
        for vc in f.v {
            let mut origin = Vec3::ZERO;
            origin[ua] = uc;
            origin[va] = vc;
            let mut dir = Vec3::ZERO;
            dir[axis] = 1.0;
            let (mut s0, mut s1) = (f32::NEG_INFINITY, f32::INFINITY);
            let rel = origin - obb.center;
            let mut hit = true;
            for k in 0..3 {
                let e = obb.axes.col(k);
                let (o, d, h) = (e.dot(rel), e[axis], obb.half_extents[k]);
                if d.abs() < 1e-9 {
                    if o.abs() > h {
                        hit = false;
                        break;
                    }
                } else {
                    let (mut ta, mut tb) = ((-h - o) / d, (h - o) / d);
                    if ta > tb {
                        std::mem::swap(&mut ta, &mut tb);
                    }
                    s0 = s0.max(ta);
                    s1 = s1.min(tb);
                }
            }
            if hit && s0 <= s1 {
                let s = if f.sign > 0.0 { s0 } else { s1 };
                let mut p = origin;
                p[axis] = s;
                consider(p);
            }
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rl_car_core::*;

    fn block(x: i32, y: i32, z: i32) -> Aabb {
        let min = Vec3::new(x as f32, y as f32, z as f32) * 100.0;
        Aabb { min, max: min + Vec3::new(100.0, 100.0, 100.0) }
    }

    /// Blocks whose tops form the floor z = 0, for x, y in [lo, hi).
    fn floor(lo: i32, hi: i32) -> Vec<Aabb> {
        let mut v = Vec::new();
        for x in lo..hi {
            for y in lo..hi {
                v.push(block(x, y, -1));
            }
        }
        v
    }

    #[test]
    fn block_floor_merges_into_one_top_face() {
        let w = BoxWorld::new(&floor(-5, 5));
        let tops: Vec<_> = w.faces().iter().filter(|f| f.axis == 2 && f.sign > 0.0).collect();
        assert_eq!(tops.len(), 1);
        assert_eq!((tops[0].u, tops[0].v, tops[0].coord), ([-500.0, 500.0], [-500.0, 500.0], 0.0));
        // 6 faces total: no internal faces between neighbouring blocks survive.
        assert_eq!(w.faces().len(), 6);
    }

    #[test]
    fn stacked_blocks_hide_touching_faces() {
        let w = BoxWorld::new(&[block(0, 0, 0), block(0, 0, 1)]);
        assert!(!w.faces().iter().any(|f| f.axis == 2 && f.coord == 100.0));
        assert_eq!(w.faces().iter().filter(|f| f.axis == 2).count(), 2);
    }

    #[test]
    fn order_independent() {
        let mut boxes = floor(-3, 3);
        boxes.push(block(1, 1, 0));
        boxes.push(Aabb { min: Vec3::new(-300.0, 0.0, 0.0), max: Vec3::new(-200.0, 100.0, 50.0) }); // slab
        let a = BoxWorld::new(&boxes);
        boxes.reverse();
        let b = BoxWorld::new(&boxes);
        assert_eq!(a.faces(), b.faces());
    }

    fn run(world: &dyn CollisionWorld, start: CarState, ticks: u32, c: Controls) -> Vec<CarState> {
        let mut s = start;
        let mut out = vec![s];
        for _ in 0..ticks {
            s = step(&s, &c, world, TICK_DT);
            out.push(s);
        }
        out
    }

    /// The host rebuilds the world every few ticks from the blocks around the car, like the
    /// Minecraft mod does; the car must not notice the seams or the window edges.
    #[test]
    fn windowed_block_floor_matches_infinite_plane_exactly() {
        let plane = PlaneWorld::floor();
        let controls = Controls { throttle: 1.0, steer: 0.3, boost: true, ..Default::default() };
        let start = CarState::default();
        let reference = run(&plane, start, 600, controls);

        let mut s = start;
        let mut world = BoxWorld::default();
        for t in 0..600 {
            if t % 6 == 0 {
                let (cx, cy) = ((s.position.x / 100.0).floor() as i32, (s.position.y / 100.0).floor() as i32);
                let mut boxes = Vec::new();
                for x in cx - 4..=cx + 4 {
                    for y in cy - 4..=cy + 4 {
                        boxes.push(block(x, y, -1));
                    }
                }
                world.set_boxes(&boxes);
            }
            s = step(&s, &controls, &world, TICK_DT);
            assert_eq!(s.position, reference[t + 1].position, "tick {t}");
            assert_eq!(s.orientation, reference[t + 1].orientation, "tick {t}");
        }
        assert!(s.velocity.length() > 1000.0, "{:?}", s.velocity);
    }

    #[test]
    fn rests_on_a_single_block() {
        let w = BoxWorld::new(&[block(-1, -1, -1), block(0, -1, -1), block(-1, 0, -1), block(0, 0, -1)]);
        let states = run(&w, CarState::default(), 240, Controls::default());
        let last = states.last().unwrap();
        assert!(last.on_ground && (last.position.z - 17.0).abs() < 0.1, "{:?}", last.position);
    }

    #[test]
    fn drives_off_a_ledge_without_ghost_impulses() {
        // Floor ends at x = 300; a lower floor one block down continues.
        let mut boxes = Vec::new();
        for x in -10..3 {
            for y in -3..3 {
                boxes.push(block(x, y, -1));
            }
        }
        for x in 3..30 {
            for y in -3..3 {
                boxes.push(block(x, y, -2));
            }
        }
        let w = BoxWorld::new(&boxes);
        let mut s = CarState::default();
        s.position.x = -500.0;
        s.velocity.x = 1200.0;
        let states = run(&w, s, 240, Controls { throttle: 1.0, ..Default::default() });
        for (t, st) in states.iter().enumerate() {
            assert!(st.velocity.y.abs() < 1.0, "tick {t}: sideways {:?}", st.velocity);
            assert!(st.velocity.x > 1000.0, "tick {t}: slowed by an edge {:?}", st.velocity);
        }
        let last = states.last().unwrap();
        assert!(last.on_ground && (last.position.z - (17.0 - 100.0)).abs() < 0.5, "{:?}", last.position);
    }

    #[test]
    fn block_wall_stops_the_car() {
        let mut boxes = floor(-10, 10);
        for y in -10..10 {
            for z in 0..3 {
                boxes.push(block(5, y, z));
            }
        }
        let w = BoxWorld::new(&boxes);
        let states = run(&w, CarState::default(), 360, Controls { throttle: 1.0, boost: true, ..Default::default() });
        let max_x = states.iter().map(|s| s.position.x).fold(f32::MIN, f32::max);
        let half_len = CarState::default().config().hitbox_size.x * 0.5 + 14.0;
        // A fast impact sinks a few uu into the wall before the contact pushes back (as in RL).
        assert!(max_x < 500.0 - half_len + 10.0, "went into the wall: {max_x}");
        assert!(states.last().unwrap().position.x < 500.0 - half_len, "{:?}", states.last().unwrap().position);
    }

    #[test]
    fn carpet_strip_is_driven_over_but_a_full_block_is_not() {
        let carpet = |x: i32, y: i32| Aabb {
            min: Vec3::new(x as f32 * 100.0, y as f32 * 100.0, 0.0),
            max: Vec3::new(x as f32 * 100.0 + 100.0, y as f32 * 100.0 + 100.0, 6.25),
        };
        let mut boxes = floor(-10, 30);
        for y in -10..30 {
            boxes.push(carpet(4, y));
        }
        let w = BoxWorld::new(&boxes);
        let mut s = CarState::default();
        s.velocity.x = 800.0;
        let states = run(&w, s, 240, Controls { throttle: 1.0, ..Default::default() });
        assert!(states.last().unwrap().position.x > 900.0, "stuck at the carpet");

        let mut boxes = floor(-10, 30);
        for y in -10..30 {
            boxes.push(block(4, y, 0));
        }
        let w = BoxWorld::new(&boxes);
        let states = run(&w, s, 240, Controls { throttle: 1.0, ..Default::default() });
        assert!(states.iter().all(|s| s.position.x < 400.0), "drove through a full block step");
    }

    #[test]
    fn ray_from_inside_geometry_ignores_the_face_it_leaves() {
        let w = BoxWorld::new(&[block(0, 0, 0)]);
        assert!(w.raycast(Vec3::new(50.0, 50.0, 50.0), Vec3::new(0.0, 0.0, -1.0), 500.0).is_none());
        let hit = w.raycast(Vec3::new(50.0, 50.0, 150.0), Vec3::new(0.0, 0.0, -1.0), 500.0).unwrap();
        assert_eq!((hit.distance, hit.normal), (50.0, Vec3::Z));
    }
}
