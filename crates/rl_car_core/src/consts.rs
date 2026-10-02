//! Physical constants of the Rocket League car model.
//!
//! Every value is a *fact about the game* as measured/documented by the community.
//! Sources are listed per value in `CONSTANTS.md` at the repository root
//! (primarily RocketSim `src/RLConst.h`, cross-checked against the RLBot wiki).
//!
//! Units: "uu" = Unreal units (~1 cm). "bt" = the internal Bullet scale used by the
//! game's physics, where 1 bt = 50 uu. Values whose name ends in `_BT` are in bt.

/// Physics tick rate of Rocket League / RocketSim.
pub const TICK_RATE: f32 = 120.0;
/// Fixed timestep (seconds).
pub const TICK_DT: f32 = 1.0 / TICK_RATE;

/// uu per bt.
pub const BT_TO_UU: f32 = 50.0;
/// bt per uu.
pub const UU_TO_BT: f32 = 1.0 / 50.0;

pub const GRAVITY_Z: f32 = -650.0;

pub const CAR_MASS_BT: f32 = 180.0;
pub const CAR_MAX_SPEED: f32 = 2300.0;
pub const CAR_MAX_ANG_SPEED: f32 = 5.5;

pub const CARWORLD_COLLISION_FRICTION: f32 = 0.3;
pub const CARWORLD_COLLISION_RESTITUTION: f32 = 0.3;

pub const BOOST_MAX: f32 = 100.0;
pub const BOOST_USED_PER_SECOND: f32 = BOOST_MAX / 3.0;
pub const BOOST_MIN_TIME: f32 = 0.1;
pub const BOOST_ACCEL_GROUND: f32 = 2975.0 / 3.0;
pub const BOOST_ACCEL_AIR: f32 = 3175.0 / 3.0;
pub const BOOST_SPAWN_AMOUNT: f32 = BOOST_MAX / 3.0;
pub const RECHARGE_BOOST_PER_SECOND: f32 = 10.0;
pub const RECHARGE_BOOST_DELAY: f32 = 0.25;

pub const SUPERSONIC_START_SPEED: f32 = 2200.0;
pub const SUPERSONIC_MAINTAIN_MIN_SPEED: f32 = SUPERSONIC_START_SPEED - 100.0;
pub const SUPERSONIC_MAINTAIN_MAX_TIME: f32 = 1.0;

pub const POWERSLIDE_RISE_RATE: f32 = 5.0;
pub const POWERSLIDE_FALL_RATE: f32 = 2.0;

pub const THROTTLE_TORQUE_AMOUNT: f32 = CAR_MASS_BT * 400.0;
pub const BRAKE_TORQUE_AMOUNT: f32 = CAR_MASS_BT * (14.25 + (1.0 / 3.0));
pub const STOPPING_FORWARD_VEL: f32 = 25.0;
pub const COASTING_BRAKE_FACTOR: f32 = 0.15;
pub const BRAKING_NO_THROTTLE_SPEED_THRESH: f32 = 0.01;
pub const THROTTLE_DEADZONE: f32 = 0.001;
pub const THROTTLE_AIR_ACCEL: f32 = 200.0 / 3.0;

pub const JUMP_ACCEL: f32 = 4375.0 / 3.0;
pub const JUMP_IMMEDIATE_FORCE: f32 = 875.0 / 3.0;
pub const JUMP_MIN_TIME: f32 = 0.025;
pub const JUMP_RESET_TIME_PAD: f32 = 1.0 / 40.0;
pub const JUMP_MAX_TIME: f32 = 0.2;
/// Jump force scale while `jump_time < JUMP_MIN_TIME`.
pub const JUMP_PRE_MIN_ACCEL_SCALE: f32 = 0.62;
pub const DOUBLEJUMP_MAX_DELAY: f32 = 1.25;

pub const FLIP_Z_DAMP_120: f32 = 0.35;
pub const FLIP_Z_DAMP_START: f32 = 0.15;
pub const FLIP_Z_DAMP_END: f32 = 0.21;
pub const FLIP_TORQUE_TIME: f32 = 0.65;
pub const FLIP_TORQUE_MIN_TIME: f32 = 0.41;
pub const FLIP_PITCHLOCK_TIME: f32 = 1.0;
pub const FLIP_PITCHLOCK_EXTRA_TIME: f32 = 0.3;
pub const FLIP_INITIAL_VEL_SCALE: f32 = 500.0;
pub const FLIP_TORQUE_X: f32 = 260.0;
pub const FLIP_TORQUE_Y: f32 = 224.0;
pub const FLIP_FORWARD_IMPULSE_MAX_SPEED_SCALE: f32 = 1.0;
pub const FLIP_SIDE_IMPULSE_MAX_SPEED_SCALE: f32 = 1.9;
pub const FLIP_BACKWARD_IMPULSE_MAX_SPEED_SCALE: f32 = 2.5;
pub const FLIP_BACKWARD_IMPULSE_SCALE_X: f32 = 16.0 / 15.0;
/// Default dodge deadzone: `|pitch| + |yaw| + |roll| >= 0.5` makes a jump press a dodge.
pub const DODGE_DEADZONE: f32 = 0.5;

/// Converts the game's air-control torque units (UE rotator units) to rad/s^2.
pub const CAR_TORQUE_SCALE: f32 = (2.0 * core::f64::consts::PI / 65536.0 * 1000.0) as f32;
/// Air control torque, in (pitch, yaw, roll) order.
pub const CAR_AIR_CONTROL_TORQUE: [f32; 3] = [130.0, 95.0, 400.0];
/// Air control damping, in (pitch, yaw, roll) order.
pub const CAR_AIR_CONTROL_DAMPING: [f32; 3] = [30.0, 20.0, 50.0];

pub const CAR_AUTOFLIP_IMPULSE: f32 = 200.0;
pub const CAR_AUTOFLIP_TORQUE: f32 = 50.0;
pub const CAR_AUTOFLIP_TIME: f32 = 0.4;
pub const CAR_AUTOFLIP_NORMZ_THRESH: f32 = core::f32::consts::FRAC_1_SQRT_2;
pub const CAR_AUTOFLIP_ROLL_THRESH: f32 = 2.8;

pub const CAR_AUTOROLL_FORCE: f32 = 100.0;
pub const CAR_AUTOROLL_TORQUE: f32 = 80.0;

pub const CAR_SPAWN_REST_Z: f32 = 17.0;

/// Suspension / raycast-vehicle parameters (bt units where applicable).
pub mod suspension {
    pub const FORCE_SCALE_FRONT: f32 = 36.0 - (1.0 / 4.0);
    pub const FORCE_SCALE_BACK: f32 = 54.0 + (1.0 / 4.0) + (1.5 / 100.0);
    pub const STIFFNESS: f32 = 500.0;
    pub const DAMPING_COMPRESSION: f32 = 25.0;
    pub const DAMPING_RELAXATION: f32 = 40.0;
    /// uu
    pub const MAX_TRAVEL: f32 = 12.0;
    /// bt (!) — subtracted from the raycast length.
    pub const SUBTRACTION_BT: f32 = 0.05;
    /// Bullet `resolveSingleBilateral` contact damping used for tire side friction.
    pub const SIDE_FRICTION_DAMPING: f32 = 0.2;
    /// Rolling-friction gain when braking with no throttle.
    pub const ROLLING_FRICTION_SCALE_MAGIC: f32 = 113.73963;
}

/// Contact solver settings mirroring Bullet's `btContactSolverInfo` as configured by RocketSim.
pub mod solver {
    pub const NUM_ITERATIONS: usize = 10;
    /// Baumgarte factor of the wheel "extra pushback" (`m_erp`).
    pub const ERP: f32 = 0.2;
    /// Split-impulse position recovery factor (`m_erp2`, set by RocketSim).
    pub const ERP2: f32 = 0.8;
    pub const SPLIT_IMPULSE_TURN_ERP: f32 = 0.1;
    /// bt/s
    pub const RESTITUTION_VELOCITY_THRESHOLD_BT: f32 = 0.2;
    /// Bullet contact breaking threshold (bt) — contacts closer than this are kept.
    pub const CONTACT_BREAKING_THRESHOLD_BT: f32 = 0.02;
    /// Bullet's default collision margin for boxes (bt).
    pub const BOX_MARGIN_BT: f32 = 0.04;
}

/// Piecewise-linear curve: points must be sorted by input. Values outside the range clamp
/// to the end points. An empty curve returns `default`.
#[derive(Clone, Copy, Debug)]
pub struct Curve(pub &'static [(f32, f32)]);

impl Curve {
    pub fn eval_or(&self, input: f32, default: f32) -> f32 {
        let pts = self.0;
        if pts.is_empty() {
            return default;
        }
        if input <= pts[0].0 {
            return pts[0].1;
        }
        for i in 1..pts.len() {
            if pts[i].0 > input {
                let (x0, y0) = pts[i - 1];
                let (x1, y1) = pts[i];
                let t = (input - x0) / (x1 - x0);
                return y0 + (y1 - y0) * t;
            }
        }
        pts[pts.len() - 1].1
    }

    pub fn eval(&self, input: f32) -> f32 {
        self.eval_or(input, 1.0)
    }
}

/// Forward speed (uu/s) -> max steer angle (rad).
pub const STEER_ANGLE_FROM_SPEED: Curve = Curve(&[
    (0.0, 0.53356),
    (500.0, 0.31930),
    (1000.0, 0.18203),
    (1500.0, 0.10570),
    (1750.0, 0.08507),
    (3000.0, 0.03454),
]);
pub const STEER_ANGLE_FROM_SPEED_THREEWHEEL: Curve = Curve(&[(0.0, 0.342473), (2300.0, 0.034837)]);
/// Forward speed (uu/s) -> steer angle while fully powersliding (rad).
pub const POWERSLIDE_STEER_ANGLE_FROM_SPEED: Curve = Curve(&[(0.0, 0.39235), (2500.0, 0.12610)]);
/// Forward speed (uu/s) -> engine torque factor. Reaches 0 at 1410 uu/s (max throttle speed).
pub const DRIVE_SPEED_TORQUE_FACTOR: Curve = Curve(&[(0.0, 1.0), (1400.0, 0.1), (1410.0, 0.0)]);
/// Contact normal Z -> friction scale when no throttle is held.
pub const NON_STICKY_FRICTION_FACTOR: Curve = Curve(&[(0.0, 0.1), (0.7075, 0.5), (1.0, 1.0)]);
/// Slip ratio -> lateral friction.
pub const LAT_FRICTION: Curve = Curve(&[(0.0, 1.0), (1.0, 0.2)]);
pub const LAT_FRICTION_THREEWHEEL: Curve = Curve(&[(0.0, 0.30), (1.0, 0.25)]);
/// Slip ratio -> longitudinal friction (empty: always the default of 1).
pub const LONG_FRICTION: Curve = Curve(&[]);
pub const HANDBRAKE_LAT_FRICTION_FACTOR: Curve = Curve(&[(0.0, 0.1)]);
pub const HANDBRAKE_LONG_FRICTION_FACTOR: Curve = Curve(&[(0.0, 0.5), (1.0, 0.9)]);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves() {
        assert_eq!(DRIVE_SPEED_TORQUE_FACTOR.eval(0.0), 1.0);
        assert!((DRIVE_SPEED_TORQUE_FACTOR.eval(1405.0) - 0.05).abs() < 1e-6);
        assert_eq!(DRIVE_SPEED_TORQUE_FACTOR.eval(5000.0), 0.0);
        assert_eq!(LONG_FRICTION.eval(0.3), 1.0);
        assert_eq!(HANDBRAKE_LAT_FRICTION_FACTOR.eval(0.7), 0.1);
    }
}
