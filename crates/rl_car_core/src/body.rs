//! Rigid body in internal (bt) units.

use crate::config::CarConfig;
use crate::consts::*;
use crate::math::{Mat3, Vec3, integrate_transform};

/// The car chassis during one tick. Position/velocity are in bt units (1 bt = 50 uu).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Body {
    pub pos: Vec3,
    pub vel: Vec3,
    pub ang_vel: Vec3,
    pub basis: Mat3,
    pub inv_mass: f32,
    pub inv_inertia_local: Vec3,
    pub inv_inertia_world: Mat3,
    /// Accumulated force for this tick (bt mass units).
    pub total_force: Vec3,
    /// Accumulated angular acceleration for this tick (rad/s^2). Every torque the car model
    /// applies is pre-multiplied by the world inertia tensor, so it is stored as acceleration.
    pub total_ang_accel: Vec3,
}

impl Body {
    pub fn new(pos_uu: Vec3, vel_uu: Vec3, ang_vel: Vec3, basis: Mat3, config: &CarConfig) -> Body {
        let inertia = config.local_inertia_bt();
        let inv = |v: f32| if v != 0.0 { 1.0 / v } else { 0.0 };
        let mut b = Body {
            pos: pos_uu * UU_TO_BT,
            vel: vel_uu * UU_TO_BT,
            ang_vel,
            basis,
            inv_mass: 1.0 / CAR_MASS_BT,
            inv_inertia_local: Vec3::new(inv(inertia.x), inv(inertia.y), inv(inertia.z)),
            inv_inertia_world: Mat3::IDENTITY,
            total_force: Vec3::ZERO,
            total_ang_accel: Vec3::ZERO,
        };
        b.update_inertia_tensor();
        b
    }

    pub fn update_inertia_tensor(&mut self) {
        self.inv_inertia_world = self.basis.scaled(self.inv_inertia_local).mul_mat(&self.basis.transpose());
    }

    pub fn forward(&self) -> Vec3 {
        self.basis.col(0)
    }
    pub fn right(&self) -> Vec3 {
        self.basis.col(1)
    }
    pub fn up(&self) -> Vec3 {
        self.basis.col(2)
    }

    #[inline]
    pub fn velocity_at(&self, rel_pos: Vec3) -> Vec3 {
        self.vel + self.ang_vel.cross(rel_pos)
    }

    #[inline]
    pub fn apply_central_impulse(&mut self, impulse: Vec3) {
        self.vel += impulse * self.inv_mass;
    }

    #[inline]
    pub fn apply_impulse(&mut self, impulse: Vec3, rel_pos: Vec3) {
        self.apply_central_impulse(impulse);
        self.ang_vel += self.inv_inertia_world * rel_pos.cross(impulse);
    }

    #[inline]
    pub fn apply_central_force(&mut self, force: Vec3) {
        self.total_force += force;
    }

    #[inline]
    pub fn apply_ang_accel(&mut self, accel: Vec3) {
        self.total_ang_accel += accel;
    }

    /// Bullet `computeImpulseDenominator` for a point relative to the centre of mass.
    pub fn impulse_denominator(&self, rel_pos: Vec3, normal: Vec3) -> f32 {
        let c0 = rel_pos.cross(normal);
        let vec = (self.inv_inertia_world * c0).cross(rel_pos);
        self.inv_mass + normal.dot(vec)
    }

    /// Effective mass denominator using the local inertia (Bullet `btJacobianEntry` against a
    /// static body), as used by `resolveSingleBilateral`.
    pub fn jacobian_diag(&self, rel_pos: Vec3, axis: Vec3) -> f32 {
        let a_j = self.basis.transpose_mul_vec(rel_pos.cross(axis));
        let minv_jt = self.inv_inertia_local.mul_elem(a_j);
        self.inv_mass + minv_jt.dot(a_j)
    }

    pub fn integrate(&mut self, dt: f32) {
        let (p, b) = integrate_transform(self.pos, &self.basis, self.vel, self.ang_vel, dt);
        self.pos = p;
        self.basis = b;
        self.update_inertia_tensor();
    }
}
