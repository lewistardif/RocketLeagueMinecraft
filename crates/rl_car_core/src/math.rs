//! Minimal f32 linear algebra.
//!
//! Everything here is plain scalar code with a fixed operation order so results are
//! bit-for-bit reproducible across runs on the same platform (no SIMD, no FMA, no
//! platform intrinsics beyond `sqrt`/`sin`/`cos`/`atan2`/`asin`).

use core::ops::{Add, AddAssign, Div, Index, IndexMut, Mul, MulAssign, Neg, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3::new(0.0, 0.0, 0.0);
    pub const X: Vec3 = Vec3::new(1.0, 0.0, 0.0);
    pub const Y: Vec3 = Vec3::new(0.0, 1.0, 0.0);
    pub const Z: Vec3 = Vec3::new(0.0, 0.0, 1.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Vec3 { x, y, z }
    }

    #[inline]
    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    #[inline]
    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    #[inline]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    #[inline]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// Normalizes; the caller guarantees a non-zero length.
    #[inline]
    pub fn normalized(self) -> Vec3 {
        self / self.length()
    }

    /// Bullet's `safeNormalize`: returns (1,0,0) for (near-)zero vectors.
    #[inline]
    pub fn safe_normalized(self) -> Vec3 {
        let l2 = self.length_squared();
        if l2 >= f32::EPSILON * f32::EPSILON {
            self / l2.sqrt()
        } else {
            Vec3::X
        }
    }

    #[inline]
    pub fn is_zero(self) -> bool {
        self.x == 0.0 && self.y == 0.0 && self.z == 0.0
    }

    /// Bullet's `fuzzyZero`.
    #[inline]
    pub fn fuzzy_zero(self) -> bool {
        self.length_squared() < f32::EPSILON * f32::EPSILON
    }

    #[inline]
    pub fn mul_elem(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x * o.x, self.y * o.y, self.z * o.z)
    }

    pub fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    pub fn from_array(a: [f32; 3]) -> Vec3 {
        Vec3::new(a[0], a[1], a[2])
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    #[inline]
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for Vec3 {
    type Output = Vec3;
    #[inline]
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Neg for Vec3 {
    type Output = Vec3;
    #[inline]
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}
impl Mul<f32> for Vec3 {
    type Output = Vec3;
    #[inline]
    fn mul(self, s: f32) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}
impl Mul<Vec3> for f32 {
    type Output = Vec3;
    #[inline]
    fn mul(self, v: Vec3) -> Vec3 {
        v * self
    }
}
impl Div<f32> for Vec3 {
    type Output = Vec3;
    #[inline]
    fn div(self, s: f32) -> Vec3 {
        Vec3::new(self.x / s, self.y / s, self.z / s)
    }
}
impl AddAssign for Vec3 {
    #[inline]
    fn add_assign(&mut self, o: Vec3) {
        *self = *self + o;
    }
}
impl SubAssign for Vec3 {
    #[inline]
    fn sub_assign(&mut self, o: Vec3) {
        *self = *self - o;
    }
}
impl MulAssign<f32> for Vec3 {
    #[inline]
    fn mul_assign(&mut self, s: f32) {
        *self = *self * s;
    }
}
impl Index<usize> for Vec3 {
    type Output = f32;
    fn index(&self, i: usize) -> &f32 {
        match i {
            0 => &self.x,
            1 => &self.y,
            2 => &self.z,
            _ => panic!("Vec3 index out of range"),
        }
    }
}
impl IndexMut<usize> for Vec3 {
    fn index_mut(&mut self, i: usize) -> &mut f32 {
        match i {
            0 => &mut self.x,
            1 => &mut self.y,
            2 => &mut self.z,
            _ => panic!("Vec3 index out of range"),
        }
    }
}

/// Row-major 3x3 matrix (`m[row][col]`), same layout as Bullet's `btMatrix3x3`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat3 {
    pub m: [[f32; 3]; 3],
}

impl Default for Mat3 {
    fn default() -> Self {
        Mat3::IDENTITY
    }
}

impl Mat3 {
    pub const IDENTITY: Mat3 = Mat3 { m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] };

    pub fn from_cols(c0: Vec3, c1: Vec3, c2: Vec3) -> Mat3 {
        Mat3 { m: [[c0.x, c1.x, c2.x], [c0.y, c1.y, c2.y], [c0.z, c1.z, c2.z]] }
    }

    pub fn diag(d: Vec3) -> Mat3 {
        Mat3 { m: [[d.x, 0.0, 0.0], [0.0, d.y, 0.0], [0.0, 0.0, d.z]] }
    }

    #[inline]
    pub fn col(&self, c: usize) -> Vec3 {
        Vec3::new(self.m[0][c], self.m[1][c], self.m[2][c])
    }

    #[inline]
    pub fn row(&self, r: usize) -> Vec3 {
        Vec3::new(self.m[r][0], self.m[r][1], self.m[r][2])
    }

    pub fn transpose(&self) -> Mat3 {
        Mat3::from_cols(self.row(0), self.row(1), self.row(2))
    }

    /// `M * diag(s)` (Bullet `scaled`).
    pub fn scaled(&self, s: Vec3) -> Mat3 {
        let m = &self.m;
        Mat3 {
            m: [
                [m[0][0] * s.x, m[0][1] * s.y, m[0][2] * s.z],
                [m[1][0] * s.x, m[1][1] * s.y, m[1][2] * s.z],
                [m[2][0] * s.x, m[2][1] * s.y, m[2][2] * s.z],
            ],
        }
    }

    pub fn mul_mat(&self, o: &Mat3) -> Mat3 {
        let mut r = [[0.0f32; 3]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = self.m[i][0] * o.m[0][j] + self.m[i][1] * o.m[1][j] + self.m[i][2] * o.m[2][j];
            }
        }
        Mat3 { m: r }
    }

    #[inline]
    pub fn mul_vec(&self, v: Vec3) -> Vec3 {
        Vec3::new(self.row(0).dot(v), self.row(1).dot(v), self.row(2).dot(v))
    }

    /// `M^T * v`
    #[inline]
    pub fn transpose_mul_vec(&self, v: Vec3) -> Vec3 {
        Vec3::new(self.col(0).dot(v), self.col(1).dot(v), self.col(2).dot(v))
    }

    /// Rotation matrix -> quaternion (Bullet `getRotation`, scalar path).
    pub fn to_quat(&self) -> Quat {
        let m = &self.m;
        let trace = m[0][0] + m[1][1] + m[2][2];
        let mut t = [0.0f32; 4];
        if trace > 0.0 {
            let mut s = (trace + 1.0).sqrt();
            t[3] = s * 0.5;
            s = 0.5 / s;
            t[0] = (m[2][1] - m[1][2]) * s;
            t[1] = (m[0][2] - m[2][0]) * s;
            t[2] = (m[1][0] - m[0][1]) * s;
        } else {
            let i = if m[0][0] < m[1][1] {
                if m[1][1] < m[2][2] { 2 } else { 1 }
            } else if m[0][0] < m[2][2] {
                2
            } else {
                0
            };
            let j = (i + 1) % 3;
            let k = (i + 2) % 3;
            let mut s = (m[i][i] - m[j][j] - m[k][k] + 1.0).sqrt();
            t[i] = s * 0.5;
            s = 0.5 / s;
            t[3] = (m[k][j] - m[j][k]) * s;
            t[j] = (m[j][i] + m[i][j]) * s;
            t[k] = (m[k][i] + m[i][k]) * s;
        }
        Quat { x: t[0], y: t[1], z: t[2], w: t[3] }
    }

    /// Quaternion -> rotation matrix (Bullet `setRotation`, scalar path).
    pub fn from_quat(q: Quat) -> Mat3 {
        let d = q.length_squared();
        let s = 2.0 / d;
        let (xs, ys, zs) = (q.x * s, q.y * s, q.z * s);
        let (wx, wy, wz) = (q.w * xs, q.w * ys, q.w * zs);
        let (xx, xy, xz) = (q.x * xs, q.x * ys, q.x * zs);
        let (yy, yz, zz) = (q.y * ys, q.y * zs, q.z * zs);
        Mat3 {
            m: [
                [1.0 - (yy + zz), xy - wz, xz + wy],
                [xy + wz, 1.0 - (xx + zz), yz - wx],
                [xz - wy, yz + wx, 1.0 - (xx + yy)],
            ],
        }
    }

    /// Bullet `setEulerYPR(yaw, pitch, roll)` (== `setEulerZYX(roll, pitch, yaw)`).
    pub fn from_euler_ypr(yaw: f32, pitch: f32, roll: f32) -> Mat3 {
        let (ci, cj, ch) = (roll.cos(), pitch.cos(), yaw.cos());
        let (si, sj, sh) = (roll.sin(), pitch.sin(), yaw.sin());
        let (cc, cs, sc, ss) = (ci * ch, ci * sh, si * ch, si * sh);
        Mat3 {
            m: [
                [cj * ch, sj * sc - cs, sj * cc + ss],
                [cj * sh, sj * ss + cc, sj * cs - sc],
                [-sj, cj * si, cj * ci],
            ],
        }
    }

    /// Bullet `getEulerYPR` -> (yaw, pitch, roll).
    pub fn to_euler_ypr(&self) -> (f32, f32, f32) {
        let m = &self.m;
        let mut yaw = m[1][0].atan2(m[0][0]);
        let pitch = (-m[2][0]).clamp(-1.0, 1.0).asin();
        let mut roll = m[2][1].atan2(m[2][2]);
        if pitch.abs() == core::f32::consts::FRAC_PI_2 {
            yaw += if yaw > 0.0 { -core::f32::consts::PI } else { core::f32::consts::PI };
            roll += if roll > 0.0 { -core::f32::consts::PI } else { core::f32::consts::PI };
        }
        (yaw, pitch, roll)
    }
}

impl Mul<Vec3> for Mat3 {
    type Output = Vec3;
    #[inline]
    fn mul(self, v: Vec3) -> Vec3 {
        self.mul_vec(v)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quat {
    pub const IDENTITY: Quat = Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };

    #[inline]
    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w
    }

    /// Hamilton product `self * o` (Bullet `btQuaternion::operator*`).
    pub fn mul(self, o: Quat) -> Quat {
        Quat {
            x: self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            y: self.w * o.y + self.y * o.w + self.z * o.x - self.x * o.z,
            z: self.w * o.z + self.z * o.w + self.x * o.y - self.y * o.x,
            w: self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
        }
    }

    /// Bullet `safeNormalize`.
    pub fn safe_normalized(self) -> Quat {
        let l2 = self.length_squared();
        if l2 > f32::EPSILON {
            let l = l2.sqrt();
            Quat { x: self.x / l, y: self.y / l, z: self.z / l, w: self.w / l }
        } else {
            self
        }
    }

    /// Rotation of `angle` radians about unit `axis` (Bullet `btQuaternion(axis, angle)`).
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Quat {
        let d = axis.length();
        let s = (angle * 0.5).sin() / d;
        Quat { x: axis.x * s, y: axis.y * s, z: axis.z * s, w: (angle * 0.5).cos() }
    }
}

/// Exponential-map transform integration (Bullet `btTransformUtil::integrateTransform`).
/// Returns the new position and basis.
pub fn integrate_transform(pos: Vec3, basis: &Mat3, lin_vel: Vec3, ang_vel: Vec3, dt: f32) -> (Vec3, Mat3) {
    const ANGULAR_MOTION_THRESHOLD: f32 = 0.5 * core::f32::consts::FRAC_PI_2;
    let new_pos = pos + lin_vel * dt;

    let angle2 = ang_vel.length_squared();
    let mut angle = 0.0;
    if angle2 > f32::EPSILON {
        angle = angle2.sqrt();
    }
    if angle * dt > ANGULAR_MOTION_THRESHOLD {
        angle = ANGULAR_MOTION_THRESHOLD / dt;
    }
    let axis = if angle < 0.001 {
        ang_vel * (0.5 * dt - (dt * dt * dt) * 0.020833333333 * angle * angle)
    } else {
        ang_vel * ((0.5 * angle * dt).sin() / angle)
    };
    let dorn = Quat { x: axis.x, y: axis.y, z: axis.z, w: (angle * dt * 0.5).cos() };
    let orn0 = basis.to_quat();
    let predicted = dorn.mul(orn0).safe_normalized();
    (new_pos, Mat3::from_quat(predicted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quat_matrix_round_trip() {
        let m = Mat3::from_euler_ypr(0.7, -0.3, 2.9);
        let back = Mat3::from_quat(m.to_quat());
        for r in 0..3 {
            for c in 0..3 {
                assert!((m.m[r][c] - back.m[r][c]).abs() < 1e-5);
            }
        }
        let (y, p, r) = m.to_euler_ypr();
        assert!((y - 0.7).abs() < 1e-5 && (p + 0.3).abs() < 1e-5 && (r - 2.9).abs() < 1e-5);
    }

    #[test]
    fn integrate_rotation_about_z() {
        let (_, b) = integrate_transform(Vec3::ZERO, &Mat3::IDENTITY, Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0), 0.5);
        // Positive rotation about +Z moves +X towards +Y.
        assert!((b.col(0).y - 0.5f32.sin()).abs() < 1e-6);
    }
}
