
use rl_car_core::{CollisionWorld, Contact, Mat3, Obb, RayHit, Vec3};
use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RlRayHit {
    pub distance: f32,
    pub point: [f32; 3],
    pub normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RlContact {
    pub point: [f32; 3],
    pub normal: [f32; 3],
    pub depth: f32,
    pub surface: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RlObb {
    pub center: [f32; 3],
    pub axes: [f32; 9],
    pub half_extents: [f32; 3],
}

pub type RaycastFn = unsafe extern "C" fn(user: *mut c_void, origin: *const f32, dir: *const f32, max_dist: f32, hit: *mut RlRayHit) -> u32;

pub type BoxContactsFn = unsafe extern "C" fn(user: *mut c_void, obb: *const RlObb, margin: f32, out: *mut RlContact, cap: u32) -> u32;

pub type SphereContactsFn =
    unsafe extern "C" fn(user: *mut c_void, center: *const f32, radius: f32, margin: f32, out: *mut RlContact, cap: u32) -> u32;

pub const MAX_CONTACTS: usize = 64;

#[derive(Clone, Copy, Debug)]
pub struct CallbackWorld {
    pub user: *mut c_void,
    pub raycast: Option<RaycastFn>,
    pub box_contacts: Option<BoxContactsFn>,
    pub sphere_contacts: Option<SphereContactsFn>,
}

fn v(a: [f32; 3]) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}

fn push_sorted(raw: &[RlContact], out: &mut Vec<Contact>) {
    let start = out.len();
    out.extend(
        raw.iter()
            .filter(|c| c.depth.is_finite() && c.point.iter().chain(&c.normal).all(|x| x.is_finite()))
            .map(|c| Contact { point: v(c.point), normal: v(c.normal), depth: c.depth, surface: c.surface }),
    );
    out[start..].sort_by_key(|c| c.surface);
}

pub fn obb_to_c(obb: &Obb) -> RlObb {
    let mut axes = [0.0; 9];
    for i in 0..3 {
        axes[i * 3..i * 3 + 3].copy_from_slice(&obb.axes.col(i).to_array());
    }
    RlObb { center: obb.center.to_array(), axes, half_extents: obb.half_extents.to_array() }
}

pub fn obb_from_c(o: &RlObb) -> Obb {
    let a = &o.axes;
    Obb {
        center: v(o.center),
        axes: Mat3::from_cols(Vec3::new(a[0], a[1], a[2]), Vec3::new(a[3], a[4], a[5]), Vec3::new(a[6], a[7], a[8])),
        half_extents: v(o.half_extents),
    }
}

impl CollisionWorld for CallbackWorld {
    fn raycast(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<RayHit> {
        let f = self.raycast?;
        let mut hit = RlRayHit::default();
        let ok = unsafe { f(self.user, origin.to_array().as_ptr(), dir.to_array().as_ptr(), max_dist, &mut hit) };
        if ok == 0 || !(hit.distance >= 0.0 && hit.distance <= max_dist) {
            return None;
        }
        Some(RayHit { distance: hit.distance, point: v(hit.point), normal: v(hit.normal) })
    }

    fn box_contacts(&self, obb: &Obb, margin: f32, out: &mut Vec<Contact>) {
        let Some(f) = self.box_contacts else { return };
        let c = obb_to_c(obb);
        let mut buf = [RlContact::default(); MAX_CONTACTS];
        let n = unsafe { f(self.user, &c, margin, buf.as_mut_ptr(), MAX_CONTACTS as u32) };
        push_sorted(&buf[..(n as usize).min(MAX_CONTACTS)], out);
    }

    fn sphere_contacts(&self, center: Vec3, radius: f32, margin: f32, out: &mut Vec<Contact>) {
        let Some(f) = self.sphere_contacts else { return };
        let mut buf = [RlContact::default(); MAX_CONTACTS];
        let n = unsafe { f(self.user, center.to_array().as_ptr(), radius, margin, buf.as_mut_ptr(), MAX_CONTACTS as u32) };
        push_sorted(&buf[..(n as usize).min(MAX_CONTACTS)], out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rl_car_core::{CarState, Controls, HitboxPreset, PlaneWorld, TICK_DT, step};

    unsafe extern "C" fn ray_cb(user: *mut c_void, o: *const f32, d: *const f32, max: f32, hit: *mut RlRayHit) -> u32 {
        let w = unsafe { &*(user as *const PlaneWorld) };
        let (o, d) = unsafe { (std::slice::from_raw_parts(o, 3), std::slice::from_raw_parts(d, 3)) };
        match w.raycast(Vec3::new(o[0], o[1], o[2]), Vec3::new(d[0], d[1], d[2]), max) {
            Some(h) => {
                unsafe { *hit = RlRayHit { distance: h.distance, point: h.point.to_array(), normal: h.normal.to_array() } };
                1
            }
            None => 0,
        }
    }

    unsafe extern "C" fn box_cb(user: *mut c_void, obb: *const RlObb, margin: f32, out: *mut RlContact, cap: u32) -> u32 {
        let w = unsafe { &*(user as *const PlaneWorld) };
        let mut v = Vec::new();
        w.box_contacts(&obb_from_c(unsafe { &*obb }), margin, &mut v);
        let out = unsafe { std::slice::from_raw_parts_mut(out, cap as usize) };
        let n = v.len().min(cap as usize);
        for (slot, c) in out.iter_mut().zip(v.iter().rev()) {
            *slot = RlContact { point: c.point.to_array(), normal: c.normal.to_array(), depth: c.depth, surface: c.surface };
        }
        n as u32
    }

    fn wrap(w: &PlaneWorld) -> CallbackWorld {
        CallbackWorld { user: w as *const _ as *mut c_void, raycast: Some(ray_cb), box_contacts: Some(box_cb), sphere_contacts: None }
    }

    #[test]
    fn callback_world_is_bit_identical_to_plane_world() {
        let planes = PlaneWorld::soccar_box();
        let cb = wrap(&planes);
        for preset in HitboxPreset::ALL {
            let mut a = CarState::new(preset);
            a.position = Vec3::new(0.0, 4400.0, 60.0);
            let mut b = a;
            for t in 0..1800 {
                let c = Controls {
                    throttle: 1.0,
                    boost: t % 400 < 300,
                    steer: if t % 600 > 500 { 0.6 } else { 0.0 },
                    jump: t % 240 < 10,
                    pitch: if t % 240 < 20 { -1.0 } else { 0.0 },
                    roll: if (900..1100).contains(&t) { 1.0 } else { 0.0 },
                    ..Default::default()
                };
                a = step(&a, &c, &planes, TICK_DT);
                b = step(&b, &c, &cb, TICK_DT);
                assert_eq!(a, b, "{preset:?} diverged at tick {t}");
            }
        }
    }

    #[test]
    fn missing_callbacks_mean_empty_space_and_bad_hits_are_ignored() {
        unsafe extern "C" fn bad_ray(_: *mut c_void, _: *const f32, _: *const f32, max: f32, hit: *mut RlRayHit) -> u32 {
            unsafe { (*hit).distance = max * 2.0 };
            1
        }
        let w = CallbackWorld { user: std::ptr::null_mut(), raycast: Some(bad_ray), box_contacts: None, sphere_contacts: None };
        assert!(w.raycast(Vec3::ZERO, -Vec3::Z, 10.0).is_none());
        let mut out = Vec::new();
        let obb = Obb { center: Vec3::ZERO, axes: Mat3::IDENTITY, half_extents: Vec3::new(1.0, 1.0, 1.0) };
        w.box_contacts(&obb, 1.0, &mut out);
        w.sphere_contacts(Vec3::ZERO, 92.0, 1.0, &mut out);
        assert!(out.is_empty());
    }
}
