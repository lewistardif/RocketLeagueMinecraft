//! The one and only conversion layer between Rocket League space and Bevy space.
//!
//! | | Rocket League (core) | Bevy |
//! |---|---|---|
//! | unit | uu (~1 cm) | metre |
//! | up | +Z | +Y |
//! | handedness | left-handed | right-handed |
//! | car forward / right / up | +X / +Y / +Z (local) | +X / +Z / +Y (local) |
//!
//! Mapping: `bevy = (rl.x, rl.z, rl.y) / 100`. Swapping two axes is a reflection (det = -1),
//! which is exactly what converts a left-handed basis into a right-handed one, so "turn right"
//! in Rocket League is still "turn right" in Bevy (see the `yaw_sign_is_preserved` test).
//! Rotations are converted by conjugation with the same swap: `R_bevy = S * R_rl * S`.

use bevy::math::{Mat3 as BMat3, Quat as BQuat, Vec3 as BVec3};
use rl_car_core::{Mat3 as RMat3, Vec3 as RVec3};

/// uu per metre.
pub const UU_PER_M: f32 = 100.0;

#[inline]
pub fn pos_to_bevy(v: RVec3) -> BVec3 {
    BVec3::new(v.x, v.z, v.y) / UU_PER_M
}

#[inline]
pub fn pos_to_rl(v: BVec3) -> RVec3 {
    RVec3::new(v.x * UU_PER_M, v.z * UU_PER_M, v.y * UU_PER_M)
}

/// Directions/normals (no scaling).
#[inline]
pub fn dir_to_bevy(v: RVec3) -> BVec3 {
    BVec3::new(v.x, v.z, v.y)
}

#[inline]
pub fn dir_to_rl(v: BVec3) -> RVec3 {
    RVec3::new(v.x, v.z, v.y)
}

/// Car orientation (columns = forward, right, up in RL world space) -> Bevy rotation.
/// The Bevy car model must use local +X forward, +Z right, +Y up.
pub fn rot_to_bevy(m: &RMat3) -> BQuat {
    // S * M * S: swap rows 1<->2 and columns 1<->2.
    let c = |col: usize| {
        let v = m.col(col);
        BVec3::new(v.x, v.z, v.y)
    };
    let b = BMat3::from_cols(c(0), c(2), c(1));
    BQuat::from_mat3(&b).normalize()
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn rot_to_rl(q: BQuat) -> RMat3 {
    let b = BMat3::from_quat(q);
    let col = |v: BVec3| RVec3::new(v.x, v.z, v.y);
    RMat3::from_cols(col(b.x_axis), col(b.z_axis), col(b.y_axis))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rl_car_core::*;

    #[test]
    fn positions_round_trip() {
        let p = RVec3::new(123.0, -456.0, 789.0);
        let b = pos_to_bevy(p);
        assert_eq!(b, BVec3::new(1.23, 7.89, -4.56));
        let back = pos_to_rl(b);
        assert!((back - p).length() < 1e-3);
    }

    #[test]
    fn rotation_is_proper_and_round_trips() {
        let r = RotMat::from_angles(0.8, -0.4, 2.1);
        let q = rot_to_bevy(&r.0);
        assert!((BMat3::from_quat(q).determinant() - 1.0).abs() < 1e-5);
        // Car local axes map onto the converted world axes.
        assert!((q * BVec3::X - dir_to_bevy(r.forward())).length() < 1e-5);
        assert!((q * BVec3::Z - dir_to_bevy(r.right())).length() < 1e-5);
        assert!((q * BVec3::Y - dir_to_bevy(r.up())).length() < 1e-5);
        let back = rot_to_rl(q);
        for i in 0..3 {
            assert!((back.col(i) - r.0.col(i)).length() < 1e-5);
        }
    }

    /// Pins the yaw sign: steering right in the core turns the car to *its right* in Bevy,
    /// where a Y-up right-handed "right" is `forward x up`.
    #[test]
    fn yaw_sign_is_preserved() {
        let world = PlaneWorld::floor();
        let mut s = CarState::default();
        s.velocity.x = 1000.0;
        let start = rot_to_bevy(&s.orientation.0);
        let fwd0 = start * BVec3::X;
        let right0 = fwd0.cross(BVec3::Y);
        for _ in 0..60 {
            s = step(&s, &Controls { throttle: 1.0, steer: 1.0, ..Default::default() }, &world, TICK_DT);
        }
        let fwd1 = rot_to_bevy(&s.orientation.0) * BVec3::X;
        assert!(fwd1.dot(right0) > 0.3, "car turned the wrong way: {fwd1:?}");
        // And it moved to the right of its starting heading.
        assert!(pos_to_bevy(s.position).dot(right0) > 0.5);
    }
}
