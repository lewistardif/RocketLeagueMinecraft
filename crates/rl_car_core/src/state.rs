//! Controls and car state.

use crate::config::{CarConfig, HitboxPreset};
use crate::consts::*;
use crate::manifold::Manifolds;
use crate::math::{Mat3, Vec3};

/// One tick of player input.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Controls {
    /// [-1, 1]; negative = brake / reverse.
    pub throttle: f32,
    /// [-1, 1]; positive = right.
    pub steer: f32,
    /// [-1, 1]; positive = nose up.
    pub pitch: f32,
    /// [-1, 1]; positive = right.
    pub yaw: f32,
    /// [-1, 1]; positive = roll right.
    pub roll: f32,
    pub jump: bool,
    pub boost: bool,
    /// Powerslide.
    pub handbrake: bool,
}

impl Controls {
    /// Clamps all analog values to [-1, 1] (NaN becomes 0).
    pub fn clamped(mut self) -> Controls {
        fn c(v: f32) -> f32 {
            if v.is_nan() { 0.0 } else { v.clamp(-1.0, 1.0) }
        }
        self.throttle = c(self.throttle);
        self.steer = c(self.steer);
        self.pitch = c(self.pitch);
        self.yaw = c(self.yaw);
        self.roll = c(self.roll);
        self
    }
}

/// Car orientation as a rotation matrix whose columns are the car's local axes in world space.
/// Rocket League convention: forward = local +X, right = local +Y, up = local +Z.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct RotMat(pub Mat3);

impl RotMat {
    pub const IDENTITY: RotMat = RotMat(Mat3::IDENTITY);

    pub fn forward(&self) -> Vec3 {
        self.0.col(0)
    }
    pub fn right(&self) -> Vec3 {
        self.0.col(1)
    }
    pub fn up(&self) -> Vec3 {
        self.0.col(2)
    }

    /// Builds the orientation from Rocket League Euler angles (radians), with the same
    /// conventions as RocketSim's `Angle`: positive pitch = nose up, positive roll = roll right.
    pub fn from_angles(yaw: f32, pitch: f32, roll: f32) -> RotMat {
        RotMat(Mat3::from_euler_ypr(yaw, -pitch, -roll))
    }

    /// Inverse of [`RotMat::from_angles`] -> (yaw, pitch, roll).
    pub fn to_angles(&self) -> (f32, f32, f32) {
        let (y, p, r) = self.0.to_euler_ypr();
        (y, -p, -r)
    }
}

/// Persistent per-wheel state.
///
/// Rocket League's vehicle computes tire forces at the *start* of a tick using the steer angle,
/// engine/brake force and friction values chosen on the *previous* tick, so these values are part
/// of the state for the simulation to be a pure function of `(state, controls)`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WheelState {
    pub steer_angle: f32,
    /// bt units
    pub engine_force: f32,
    /// bt units
    pub brake: f32,
    pub lat_friction: f32,
    pub long_friction: f32,
    /// Extra suspension impulse when the suspension is bottomed out (bt units).
    pub extra_pushback: f32,
    /// Current suspension length (uu), informational (e.g. for rendering wheels).
    pub suspension_length: f32,
    /// World-space contact point and normal of the last raycast, if any (uu).
    pub contact: Option<(Vec3, Vec3)>,
}

/// Full simulation state of one car. All values are in Rocket League space and units
/// (uu, uu/s, rad/s; Z-up, left-handed world).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CarState {
    pub position: Vec3,
    pub velocity: Vec3,
    pub orientation: RotMat,
    pub angular_velocity: Vec3,

    /// 0..=100
    pub boost_amount: f32,

    /// True if 3 or more wheels have contact.
    pub on_ground: bool,
    /// Whether each wheel has contact (front-right, front-left, back-right, back-left).
    pub wheel_contacts: [bool; 4],

    /// Whether we jumped to get into the air (false while airborne after a flip reset).
    pub has_jumped: bool,
    /// True if we have double jumped and are still in the air (flips do not count).
    pub has_double_jumped: bool,
    /// True if in the air and have flipped (or are flipping).
    pub has_flipped: bool,
    pub is_jumping: bool,
    pub is_flipping: bool,
    /// Time since the jump started (while jumping or until landing), else 0.
    pub jump_timer: f32,
    /// Time since the flip started, else 0.
    pub flip_timer: f32,
    /// Relative flip torque direction (forward flip has positive Y).
    pub flip_rel_torque: Vec3,
    pub air_time: f32,
    pub air_time_since_jump: f32,

    pub is_boosting: bool,
    pub boosting_time: f32,
    pub time_since_boosted: f32,

    pub is_supersonic: bool,
    pub supersonic_time: f32,

    /// Analog powerslide value 0..=1.
    pub handbrake_val: f32,

    pub is_auto_flipping: bool,
    pub auto_flip_timer: f32,
    pub auto_flip_torque_scale: f32,

    /// Normal of a car-body vs world contact from the previous tick, if any.
    pub world_contact_normal: Option<Vec3>,

    pub last_controls: Controls,
    pub wheels: [WheelState; 4],
    /// Cached car-body vs world contact points (see [`crate::manifold`]).
    pub manifolds: Manifolds,
    pub hitbox_preset: HitboxPreset,
}

impl Default for CarState {
    fn default() -> Self {
        CarState::new(HitboxPreset::Octane)
    }
}

impl CarState {
    /// A car at rest at the origin, facing +X, with spawn boost (matches RocketSim's default state).
    pub fn new(preset: HitboxPreset) -> CarState {
        CarState {
            position: Vec3::new(0.0, 0.0, CAR_SPAWN_REST_Z),
            velocity: Vec3::ZERO,
            orientation: RotMat::IDENTITY,
            angular_velocity: Vec3::ZERO,
            boost_amount: BOOST_SPAWN_AMOUNT,
            on_ground: true,
            wheel_contacts: [false; 4],
            has_jumped: false,
            has_double_jumped: false,
            has_flipped: false,
            is_jumping: false,
            is_flipping: false,
            jump_timer: 0.0,
            flip_timer: 0.0,
            flip_rel_torque: Vec3::ZERO,
            air_time: 0.0,
            air_time_since_jump: 0.0,
            is_boosting: false,
            boosting_time: 0.0,
            time_since_boosted: 0.0,
            is_supersonic: false,
            supersonic_time: 0.0,
            handbrake_val: 0.0,
            is_auto_flipping: false,
            auto_flip_timer: 0.0,
            auto_flip_torque_scale: 0.0,
            world_contact_normal: None,
            last_controls: Controls::default(),
            wheels: [WheelState::default(); 4],
            manifolds: Manifolds::default(),
            hitbox_preset: preset,
        }
    }

    pub fn config(&self) -> CarConfig {
        CarConfig::from_preset(self.hitbox_preset)
    }

    pub fn forward(&self) -> Vec3 {
        self.orientation.forward()
    }
    pub fn right(&self) -> Vec3 {
        self.orientation.right()
    }
    pub fn up(&self) -> Vec3 {
        self.orientation.up()
    }

    /// True if the car can currently jump, double-jump or flip.
    pub fn has_flip_or_jump(&self) -> bool {
        self.on_ground || (!self.has_flipped && !self.has_double_jumped && self.air_time_since_jump < DOUBLEJUMP_MAX_DELAY)
    }

    /// True if the car currently holds a flip obtained from a flip reset.
    pub fn has_flip_reset(&self) -> bool {
        !self.on_ground && self.has_flip_or_jump() && !self.has_jumped
    }
}
