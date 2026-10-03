use crate::consts::UU_TO_BT;
use crate::manifold::Manifolds;
use crate::math::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BallConfig {
    pub radius: f32,
    pub mass: f32,
    pub drag: f32,
    pub world_friction: f32,
    pub world_restitution: f32,
    pub max_speed: f32,
    pub max_ang_speed: f32,
    pub car_friction: f32,
    pub car_restitution: f32,
    pub hit_extra_force_scale: f32,
}

impl Default for BallConfig {
    fn default() -> Self {
        BallConfig {
            radius: BALL_RADIUS,
            mass: BALL_MASS_BT,
            drag: BALL_DRAG,
            world_friction: BALL_WORLD_FRICTION,
            world_restitution: BALL_WORLD_RESTITUTION,
            max_speed: BALL_MAX_SPEED,
            max_ang_speed: BALL_MAX_ANG_SPEED,
            car_friction: CARBALL_COLLISION_FRICTION,
            car_restitution: CARBALL_COLLISION_RESTITUTION,
            hit_extra_force_scale: 1.0,
        }
    }
}

pub const BALL_RADIUS: f32 = 91.25;
pub const BALL_MASS_BT: f32 = crate::consts::CAR_MASS_BT / 6.0;
pub const BALL_DRAG: f32 = 0.03;
pub const BALL_WORLD_FRICTION: f32 = 0.35;
pub const BALL_WORLD_RESTITUTION: f32 = 0.6;
pub const BALL_MAX_SPEED: f32 = 6000.0;
pub const BALL_MAX_ANG_SPEED: f32 = 6.0;
pub const CARBALL_COLLISION_FRICTION: f32 = 2.0;
pub const CARBALL_COLLISION_RESTITUTION: f32 = 0.0;
pub const BALL_REST_Z: f32 = 93.15;

pub mod hit {
    use crate::consts::Curve;
    pub const Z_SCALE: f32 = 0.35;
    pub const FORWARD_SCALE: f32 = 0.65;
    pub const MAX_DELTA_VEL: f32 = 4600.0;
    pub const FACTOR_CURVE: Curve = Curve(&[(0.0, 0.65), (500.0, 0.65), (2300.0, 0.55), (4600.0, 0.30)]);
}

impl BallConfig {
    pub(crate) fn radius_bt(&self) -> f32 {
        self.radius * UU_TO_BT
    }

    pub(crate) fn contact_breaking_threshold_bt(&self) -> f32 {
        (self.radius_bt() + 0.08) * crate::consts::solver::CONTACT_BREAKING_THRESHOLD_BT
    }

    pub(crate) fn inertia_bt(&self) -> f32 {
        let r = self.radius_bt();
        0.4 * self.mass * r * r
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BallState {
    pub position: Vec3,
    pub velocity: Vec3,
    pub angular_velocity: Vec3,
    pub manifolds: Manifolds,
}

impl BallState {
    pub fn new(position: Vec3) -> BallState {
        BallState { position, velocity: Vec3::ZERO, angular_velocity: Vec3::ZERO, manifolds: Manifolds::default() }
    }

    pub fn is_sleeping(&self) -> bool {
        self.velocity.length_squared() == 0.0 && self.angular_velocity.length_squared() == 0.0
    }
}

impl Default for BallState {
    fn default() -> Self {
        BallState::new(Vec3::new(0.0, 0.0, BALL_REST_Z))
    }
}
