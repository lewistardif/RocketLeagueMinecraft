//! Rocket League's player camera (car cam and ball cam), engine-agnostic.
//!
//! Presentation only: nothing here feeds back into [`crate::step`]. Hosts call
//! [`CarCamera::update`] (or [`CarCamera::update_with_ball`]) once per rendered frame with the
//! (interpolated) car and get back where to put the camera, in Rocket League space and units like
//! the rest of the crate.
//!
//! This follows the game's own camera script (`CameraState_Car_TA`, `CameraState_BallCam_TA`,
//! `Camera_TA` in `TAGame.upk` and `CameraStateBlender_X`, `CameraUtils_X` in `ProjectX.upk` of the
//! installed game) and the tuned values of its `Archetypes.Camera` objects.
//!
//! Car cam (`CameraState_Car_TA`):
//!
//! * **Focus**: the car position plus `Height` uu of offset (along the car's up while fully on the
//!   ground, world up otherwise), smoothed but never more than 100 uu behind. Only the part of the
//!   smoothing that is across the view lags behind; along the view it is exact. `Stiffness` blends
//!   the lagging focus back to the true one.
//! * **Ground**: look along the car's forward projected onto the driving surface, `Angle` degrees
//!   down. The camera stays upright: on walls and the ceiling it does not roll with the car (only
//!   10 % of the car's sideways lean on the ground). Rotation is smoothed fast on the floor and
//!   slowly on walls.
//! * **Air**: the car's rotation is ignored. The camera turns to look at the car from where it is,
//!   like a camera on a string, so flips, spins and air rolls leave it pointing forward.
//! * **Distance**: `Distance` uu, pulled out while moving away from the camera (less with more
//!   `Stiffness`). **FOV**: `FOV` degrees, up to +5 with speed and +10 when supersonic.
//!
//! Ball cam (`CameraState_BallCam_TA`, a car cam with these overrides):
//!
//! * **Focus**: exactly `Height` uu above the car along world up, with no lag at all, raised a
//!   little more the steeper the ball is above or below (0.005 uu per rotation unit of pitch: about
//!   +80 uu with the ball straight up).
//! * **Rotation**: towards the ball from the focus, smoothed at a fixed rate whatever the car does
//!   on the ground or in the air. Pitch is the game's trick for keeping the car in view: while the
//!   ball is within 22° of level the camera keeps the `Angle` setting and only yaws; from 22° to 44°
//!   it eases in, and beyond that it follows 80 % of the ball's pitch. No roll.
//! * **Rear view** while in ball cam is plain car cam (turned 180°); toggling it snaps.
//! * **Switching** (both ways): the camera freezes the view it had, starts the new mode fresh and
//!   eases out the difference (focus, rotation, distance, FOV) with a smoothstep over 0.5 s at
//!   `Transition Speed` 1, down to an instant cut at 2.
//!
//! Both: **Swivel** (right stick) orbits the camera around the focus by up to ±123° yaw (±99° at
//! 2500 uu/s and above), 30° up and 49° down, eased at `Swivel Speed` and returning twice as fast.
//! **Rear view** turns it 180°.
//!
//! Left out: the free-look camera mode (the "Modern" preset's unconstrained rotation), camera
//! shake, the bob from the car body's visual suspension, and picking a ball cam target among
//! several (the host passes the one ball). Clipping the camera against the world (Rocket League
//! only keeps it 10 uu above the field floor) is up to the host.

use crate::consts::CAR_MAX_SPEED;
use crate::math::Vec3;
use crate::state::{CarState, RotMat};

/// Unreal rotation units (65536 per turn) to radians.
const UU_TO_RAD: f32 = std::f32::consts::TAU / 65536.0;

/// The player's camera settings (Rocket League: Settings > Camera, plus Controls > Invert Swivel
/// Pitch). Use [`CameraSettings::clamped`] before trusting user input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraSettings {
    /// Horizontal field of view in degrees at 16:9 (60..=110).
    pub fov: f32,
    /// Height of the camera's focus above the car (uu, 40..=200).
    pub height: f32,
    /// Camera pitch in degrees, negative = looking down (-15..=0).
    pub angle: f32,
    /// Distance from the focus (uu, 100..=400).
    pub distance: f32,
    /// How rigidly the camera sticks to the car (0..=1).
    pub stiffness: f32,
    /// How fast the right stick swivels the camera (1..=10).
    pub swivel_speed: f32,
    /// How fast the camera blends to and from ball cam (1..=2).
    pub transition_speed: f32,
    /// Pushing the stick up looks down.
    pub invert_swivel_pitch: bool,
}

impl Default for CameraSettings {
    fn default() -> Self {
        CameraSettings::DEFAULT
    }
}

/// Slider range of one setting: (min, max, step).
pub type SettingRange = (f32, f32, f32);

impl CameraSettings {
    /// The game's "Default" preset.
    pub const DEFAULT: CameraSettings = CameraSettings::preset(90.0, 100.0, 270.0, 0.5, 2.5, 1.0);

    /// The game's presets, in its own order (`ECameraSettingsPreset`). "Custom" is the default
    /// values; "Modern" also unlocks free look in the game, which this camera does not do.
    pub const PRESETS: [(&'static str, CameraSettings); 6] = [
        ("Default", CameraSettings::DEFAULT),
        ("Balanced", CameraSettings::preset(100.0, 100.0, 270.0, 0.5, 2.5, 1.2)),
        ("Wide", CameraSettings::preset(110.0, 110.0, 280.0, 0.5, 5.0, 1.5)),
        ("Custom", CameraSettings::DEFAULT),
        ("Legacy", CameraSettings::preset(90.0, 100.0, 260.0, 0.3, 2.5, 1.0)),
        ("Modern", CameraSettings::preset(90.0, 100.0, 290.0, 0.5, 2.5, 1.0)),
    ];

    pub const FOV_RANGE: SettingRange = (60.0, 110.0, 1.0);
    pub const HEIGHT_RANGE: SettingRange = (40.0, 200.0, 10.0);
    pub const ANGLE_RANGE: SettingRange = (-15.0, 0.0, 1.0);
    pub const DISTANCE_RANGE: SettingRange = (100.0, 400.0, 10.0);
    pub const STIFFNESS_RANGE: SettingRange = (0.0, 1.0, 0.05);
    pub const SWIVEL_SPEED_RANGE: SettingRange = (1.0, 10.0, 0.1);
    pub const TRANSITION_SPEED_RANGE: SettingRange = (1.0, 2.0, 0.1);

    const fn preset(fov: f32, height: f32, distance: f32, stiffness: f32, swivel_speed: f32, transition_speed: f32) -> CameraSettings {
        CameraSettings { fov, height, angle: -3.0, distance, stiffness, swivel_speed, transition_speed, invert_swivel_pitch: false }
    }

    /// Every value clamped to the game's slider range (non-finite values become the default).
    pub fn clamped(self) -> CameraSettings {
        let c = |v: f32, d: f32, (lo, hi, _): SettingRange| if v.is_finite() { v.clamp(lo, hi) } else { d };
        let d = CameraSettings::DEFAULT;
        CameraSettings {
            fov: c(self.fov, d.fov, Self::FOV_RANGE),
            height: c(self.height, d.height, Self::HEIGHT_RANGE),
            angle: c(self.angle, d.angle, Self::ANGLE_RANGE),
            distance: c(self.distance, d.distance, Self::DISTANCE_RANGE),
            stiffness: c(self.stiffness, d.stiffness, Self::STIFFNESS_RANGE),
            swivel_speed: c(self.swivel_speed, d.swivel_speed, Self::SWIVEL_SPEED_RANGE),
            transition_speed: c(self.transition_speed, d.transition_speed, Self::TRANSITION_SPEED_RANGE),
            invert_swivel_pitch: self.invert_swivel_pitch,
        }
    }

    /// `CameraState_Car_TA.StaticOverrideBlendParams`: seconds to blend between car cam and ball
    /// cam (0.5 at Transition Speed 1, an instant cut at 2).
    pub fn transition_time(&self) -> f32 {
        let alpha = (self.clamped().transition_speed - 1.0).clamp(0.0, 1.0);
        lerp(BLEND_TIME, 0.0, alpha)
    }
}

/// The game's tuning (`Archetypes.Camera.CameraState_Car`, `Archetypes.Camera.CameraState_Ballcam`,
/// `Archetypes.Camera.Camera_Default`).
mod tuning {
    pub const INTERP_TO_GROUND_RATE: f32 = 2.0;
    pub const INTERP_TO_AIR_RATE: f32 = 4.0;
    pub const FOCUS_RATE: f32 = 6.32;
    /// `FocusInterp.MaxDistance`: the smoothed focus never lags further than this.
    pub const FOCUS_MAX_DISTANCE: f32 = 100.0;
    pub const FOCUS_OFFSET_RATE: f32 = 2.03;
    pub const DISTANCE_RATE: f32 = 4.14;
    pub const GROUND_ROTATION_RATE: f32 = 13.39;
    pub const GROUND_ROTATION_RATE_WALL: f32 = 2.03;
    /// Ground rotation rates are scaled by `lerp(1, this, stiffness)`.
    pub const STIFFNESS_ROTATION_SCALE: f32 = 3.427;
    pub const GROUND_NORMAL_RATE: f32 = 10.46;
    /// Air rotation rate at rest / at max speed (per 1/60 s).
    pub const AIR_VELOCITY_INFLUENCE: f32 = 35.0;
    pub const AIR_VELOCITY_INFLUENCE_MAX_SPEED: f32 = 10.0;
    pub const DISTANCE_SPEED_SCALE: f32 = 0.05;
    pub const DISTANCE_OFFSET_MIN: f32 = -50.0;
    pub const MAX_SPEED_FOV: f32 = 5.0;
    pub const FOV_INTERP_SPEED: f32 = 25.0;
    pub const SUPERSONIC_FOV: f32 = 10.0;
    pub const SUPERSONIC_FOV_INTERP_SPEED: f32 = 40.0;
    pub const ROLL_SCALE: f32 = 0.1;
    /// Car pitch (rotation units) at which the ground view would fully turn to the surface normal.
    pub const GROUND_PITCH_BLEND: f32 = 48000.0;
    /// First-frame air view: yaw blends from the car's heading to its velocity up to this speed.
    pub const AIR_START_VELOCITY: f32 = 500.0;
    // Swivel (rotation units; speeds uu/s).
    pub const SWIVEL_YAW_MAX_SLOW: f32 = 22500.0;
    pub const SWIVEL_YAW_MAX_FAST: f32 = 18000.0;
    pub const SWIVEL_PITCH_MAX: f32 = 5500.0;
    pub const SWIVEL_PITCH_MIN: f32 = -8900.0;
    pub const SWIVEL_FAST_SPEED: f32 = 2500.0;
    pub const SWIVEL_DIE_RATE: f32 = 2.0;
    /// `DefaultBlendParams.BlendTime` of both states, before Transition Speed.
    pub const BLEND_TIME: f32 = 0.5;

    // Ball cam (`CameraState_BallCam_TA`).
    /// `FocusInterp.MaxDistance` while in ball cam: the focus does not lag at all.
    pub const BALL_FOCUS_MAX_DISTANCE: f32 = 0.001;
    pub const BALL_ROTATION_RATE: f32 = 8.59;
    /// Focus raise (uu) per rotation unit of pitch towards the ball.
    pub const BALL_PITCH_FOCUS_Z_FACTOR: f32 = 0.005;
    /// Pitch towards the ball (rotation units) where the camera starts / ends easing into it.
    pub const BALL_PITCH_EXTENT_MIN: f32 = 4000.0;
    pub const BALL_PITCH_EXTENT_MAX: f32 = 8000.0;
    /// Share of the ball's pitch the camera follows past `BALL_PITCH_EXTENT_MAX`.
    pub const BALL_PITCH_SCALE: f32 = 0.8;
}
use tuning::*;

/// What the camera needs to know about the car this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraTarget {
    pub position: Vec3,
    pub velocity: Vec3,
    pub orientation: RotMat,
    /// On the ground as the camera sees it: three or more wheels down and not in a jump.
    pub on_ground: bool,
    /// Normal of the surface under the wheels (the car's up when no wheel touches).
    pub ground_normal: Vec3,
    pub supersonic: bool,
}

impl CameraTarget {
    /// Takes everything from one simulation state.
    pub fn from_state(s: &CarState) -> CameraTarget {
        CameraTarget::new(s, s.position, s.velocity, s.orientation)
    }

    /// `a` moved `alpha` (0..1) of the way to `b` (position, velocity and orientation; the rest is
    /// from `b`), for rendering between two ticks.
    pub fn interpolated(a: &CarState, b: &CarState, alpha: f32) -> CameraTarget {
        let t = if alpha.is_finite() { alpha.clamp(0.0, 1.0) } else { 1.0 };
        let qa = a.orientation.0.to_quat();
        let mut qb = b.orientation.0.to_quat();
        if qa.x * qb.x + qa.y * qb.y + qa.z * qb.z + qa.w * qb.w < 0.0 {
            qb = crate::math::Quat { x: -qb.x, y: -qb.y, z: -qb.z, w: -qb.w };
        }
        let q = crate::math::Quat {
            x: qa.x + (qb.x - qa.x) * t,
            y: qa.y + (qb.y - qa.y) * t,
            z: qa.z + (qb.z - qa.z) * t,
            w: qa.w + (qb.w - qa.w) * t,
        }
        .safe_normalized();
        let lerp = |x: Vec3, y: Vec3| x + (y - x) * t;
        CameraTarget::new(b, lerp(a.position, b.position), lerp(a.velocity, b.velocity), RotMat(crate::math::Mat3::from_quat(q)))
    }

    fn new(s: &CarState, position: Vec3, velocity: Vec3, orientation: RotMat) -> CameraTarget {
        let mut normal = Vec3::ZERO;
        for w in &s.wheels {
            if let Some((_, n)) = w.contact {
                normal += n;
            }
        }
        let ground_normal = if normal.length_squared() > 1e-6 { normal.normalized() } else { orientation.up() };
        CameraTarget { position, velocity, orientation, on_ground: s.on_ground && !s.is_jumping, ground_normal, supersonic: s.is_supersonic }
    }
}

/// Camera controls for one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CameraInput {
    /// Swivel right (positive) / left, -1..=1 (gamepad right stick X, after its deadzone).
    pub look_right: f32,
    /// Swivel up (positive) / down, -1..=1 (right stick Y, up positive).
    pub look_up: f32,
    /// Rocket League's "Rear Camera": look behind the car while held.
    pub rear_view: bool,
}

/// Where the camera is this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraView {
    /// Camera position (uu).
    pub location: Vec3,
    /// Camera axes in world space: columns = forward (view direction), right, up.
    pub orientation: RotMat,
    /// Horizontal field of view in degrees at 16:9.
    pub fov: f32,
    /// The point the camera orbits and looks over (uu).
    pub focus: Vec3,
}

impl CameraView {
    /// Vertical field of view (radians). Rocket League's FOV is horizontal at 16:9; wider screens
    /// see more at the sides with the same vertical FOV.
    pub fn vertical_fov(&self) -> f32 {
        2.0 * ((self.fov.to_radians() * 0.5).tan() * 9.0 / 16.0).atan()
    }
}

/// An Unreal rotator in radians (pitch up, yaw right, roll right; applied yaw, pitch, roll).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Rot {
    pitch: f32,
    yaw: f32,
    roll: f32,
}

fn wrap(a: f32) -> f32 {
    let t = std::f32::consts::TAU;
    let mut a = a % t;
    if a > std::f32::consts::PI {
        a -= t;
    } else if a < -std::f32::consts::PI {
        a += t;
    }
    a
}

impl Rot {
    fn of_dir(v: Vec3) -> Rot {
        if v.x == 0.0 && v.y == 0.0 && v.z == 0.0 {
            return Rot::default();
        }
        Rot { pitch: v.z.atan2((v.x * v.x + v.y * v.y).sqrt()), yaw: v.y.atan2(v.x), roll: 0.0 }
    }

    fn dir(self) -> Vec3 {
        let (sp, cp) = self.pitch.sin_cos();
        let (sy, cy) = self.yaw.sin_cos();
        Vec3::new(cp * cy, cp * sy, sp)
    }

    /// Unreal's `FRotationMatrix` axes: forward, right, up.
    fn axes(self) -> RotMat {
        let (sp, cp) = self.pitch.sin_cos();
        let (sy, cy) = self.yaw.sin_cos();
        let (sr, cr) = self.roll.sin_cos();
        RotMat(crate::math::Mat3::from_cols(
            Vec3::new(cp * cy, cp * sy, sp),
            Vec3::new(sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, -sr * cp),
            Vec3::new(-(cr * sp * cy + sr * sy), cy * sr - cr * sp * sy, cr * cp),
        ))
    }

    fn add(self, o: Rot) -> Rot {
        Rot { pitch: self.pitch + o.pitch, yaw: self.yaw + o.yaw, roll: self.roll + o.roll }
    }

    fn sub(self, o: Rot) -> Rot {
        Rot { pitch: self.pitch - o.pitch, yaw: self.yaw - o.yaw, roll: self.roll - o.roll }
    }

    fn scale(self, k: f32) -> Rot {
        Rot { pitch: self.pitch * k, yaw: self.yaw * k, roll: self.roll * k }
    }

    fn normalized(self) -> Rot {
        Rot { pitch: wrap(self.pitch), yaw: wrap(self.yaw), roll: wrap(self.roll) }
    }

    /// `RLerp(self, to, alpha, bShortestPath = true)`.
    fn lerp(self, to: Rot, alpha: f32) -> Rot {
        Rot {
            pitch: self.pitch + wrap(to.pitch - self.pitch) * alpha,
            yaw: self.yaw + wrap(to.yaw - self.yaw) * alpha,
            roll: self.roll + wrap(to.roll - self.roll) * alpha,
        }
    }

    /// `RSmoothInterpTo`: frame-rate independent exponential approach.
    fn smooth_to(self, to: Rot, rate: f32, dt: f32) -> Rot {
        self.lerp(to, smooth_alpha(rate, dt))
    }

    /// `AddCameraPitchOffset`: tilt by `offset` without passing straight up.
    fn add_pitch_offset(mut self, offset: f32) -> Rot {
        self.pitch += offset.min(std::f32::consts::FRAC_PI_2 - self.pitch);
        self
    }
}

fn smooth_alpha(rate: f32, dt: f32) -> f32 {
    1.0 - (-rate * dt.max(0.0)).exp()
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn vlerp(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a + (b - a) * t
}

/// `InterpVector` with the game's frame-rate independent smoothing (`UpdateInterpVector`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Smoothed {
    value: Option<Vec3>,
}

impl Smoothed {
    /// Moves towards `target` at `rate`, but never stays more than `max_distance` uu away from it
    /// (0 = no limit).
    fn update(&mut self, target: Vec3, rate: f32, max_distance: f32, dt: f32) -> Vec3 {
        let v = match self.value {
            Some(v) => {
                let v = v + (target - v) * smooth_alpha(rate, dt);
                let lag = target - v;
                if max_distance > 0.0 && lag.length_squared() > max_distance * max_distance { target - lag.normalized() * max_distance } else { v }
            }
            None => target,
        };
        self.value = Some(v);
        v
    }
}

/// The camera's own view, before blending, swivel and rear view (`CameraOrientation`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Pov {
    focus: Vec3,
    rotation: Rot,
    distance: f32,
    fov: f32,
    location: Vec3,
}

impl Pov {
    /// `FinalizeOrientation`.
    fn finalize(&mut self) {
        self.rotation = self.rotation.normalized();
        self.location = self.focus - self.rotation.dir() * self.distance;
    }
}

/// What a camera-state transition adds to the new state's view (`TransitionDelta`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct PovDelta {
    focus: Vec3,
    rotation: Rot,
    distance: f32,
    fov: f32,
}

impl PovDelta {
    fn scale(self, k: f32) -> PovDelta {
        PovDelta { focus: self.focus * k, rotation: self.rotation.scale(k), distance: self.distance * k, fov: self.fov * k }
    }
}

/// A blend from the previous camera state (`CameraTransition`): `offset` (previous view minus
/// the new one, when the switch happened) eased out over `time` seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Transition {
    remaining: f32,
    time: f32,
    offset: PovDelta,
}

impl Transition {
    /// `CameraUtils_X.GetBlendPercent` with the default `VTBlend_Cubic`: how much of `offset` is
    /// still applied (1 at the switch, 0 at the end).
    fn weight(&self) -> f32 {
        let t = (self.remaining / self.time).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
}

/// Which camera state is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Car,
    Ball,
}

/// Interpolation memory of the active camera state, reset whenever a state begins
/// (`BeginCameraState` / `ResetInterpState`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct StateMemory {
    /// `bFirstExecution`: snap instead of smoothing this frame.
    first: bool,
    on_ground: bool,
    /// 1 = ground camera, 0 = air camera.
    air_ground_blend: f32,
    ground_normal: Vec3,
    focus: Smoothed,
    focus_offset: Smoothed,
    distance: Smoothed,
}

impl StateMemory {
    fn begin(car: &CameraTarget) -> StateMemory {
        StateMemory {
            first: true,
            on_ground: car.on_ground,
            air_ground_blend: if car.on_ground { 1.0 } else { 0.0 },
            ground_normal: car.ground_normal,
            focus: Smoothed::default(),
            focus_offset: Smoothed::default(),
            distance: Smoothed::default(),
        }
    }

    /// `CameraState_Car_TA.UpdateValidPOV`, the whole car cam.
    fn car_cam(&mut self, pov: &mut Pov, car: &CameraTarget, settings: &CameraSettings, focus_max_distance: f32, dt: f32) {
        let pitch_offset = settings.angle.to_radians();
        self.update_air_ground_blend(car, dt);

        // UpdateFocusWorldOffset: above the car along its up only while fully on the ground.
        let height = settings.height;
        let offset = if self.air_ground_blend >= 1.0 { car.orientation.0.mul_vec(Vec3::new(0.0, 0.0, height)) } else { Vec3::new(0.0, 0.0, height) };
        let offset = self.focus_offset.update(offset, FOCUS_OFFSET_RATE, 0.0, dt);
        self.update_focus(pov, car, offset, focus_max_distance, settings, dt);

        // UpdateAirAndGroundCamera.
        let blend = self.air_ground_blend;
        pov.rotation = if blend >= 1.0 {
            self.ground_rotation(pov, car, settings, pitch_offset, dt)
        } else if blend <= 0.0 {
            self.air_rotation(pov, car, pitch_offset)
        } else {
            let air = self.air_rotation(pov, car, pitch_offset);
            let ground = self.ground_rotation(pov, car, settings, pitch_offset, dt);
            air.lerp(ground, blend)
        };

        self.update_distance(pov, car, settings, dt);
        self.update_fov(pov, car, settings, dt);

        // UpdateRotationModifiers: a little of the car's sideways lean while on the ground.
        if blend > 0.0 {
            let right = car.orientation.right();
            let lean = right.z.atan2((right.x * right.x + right.y * right.y).sqrt());
            pov.rotation.roll = lean * -ROLL_SCALE * blend;
        }

        self.first = false;
        pov.finalize();
    }

    /// `CameraState_BallCam_TA.UpdateValidPOV` with the rear camera off: the car cam with its own
    /// focus offset, focus and rotation, and no rotation modifiers.
    fn ball_cam(&mut self, pov: &mut Pov, car: &CameraTarget, ball: Vec3, settings: &CameraSettings, dt: f32) {
        self.update_air_ground_blend(car, dt);

        // UpdateFocusWorldOffset: straight up, not smoothed.
        let offset = Vec3::new(0.0, 0.0, settings.height);
        self.focus_offset.value = Some(offset);
        self.update_focus(pov, car, offset, BALL_FOCUS_MAX_DISTANCE, settings, dt);

        // UpdateAirAndGroundCamera.
        let to_ball = Rot::of_dir(ball - pov.focus);
        pov.focus.z += to_ball.pitch.abs() / UU_TO_RAD * BALL_PITCH_FOCUS_Z_FACTOR;
        let mut target = Rot::of_dir(ball - pov.focus);
        let strength = ((target.pitch.abs() / UU_TO_RAD - BALL_PITCH_EXTENT_MIN) / (BALL_PITCH_EXTENT_MAX - BALL_PITCH_EXTENT_MIN)).clamp(0.0, 1.0);
        target.pitch = lerp(settings.angle.to_radians(), target.pitch, BALL_PITCH_SCALE * strength);
        pov.rotation = if self.first { target } else { pov.rotation.smooth_to(target, BALL_ROTATION_RATE, dt) };

        self.update_distance(pov, car, settings, dt);
        self.update_fov(pov, car, settings, dt);

        self.first = false;
        pov.finalize();
    }

    /// `UpdateAirGroundBlend`.
    fn update_air_ground_blend(&mut self, car: &CameraTarget, dt: f32) {
        if self.on_ground != car.on_ground {
            self.on_ground = car.on_ground;
            if self.on_ground {
                self.ground_normal = car.ground_normal;
            }
        }
        self.air_ground_blend += if self.on_ground { INTERP_TO_GROUND_RATE * dt } else { -INTERP_TO_AIR_RATE * dt };
        self.air_ground_blend = self.air_ground_blend.clamp(0.0, 1.0);
    }

    /// `UpdateFocus`: the smoothed focus lags only across the view; Stiffness pulls it back.
    fn update_focus(&mut self, pov: &mut Pov, car: &CameraTarget, offset: Vec3, max_distance: f32, settings: &CameraSettings, dt: f32) {
        let focus = car.position + offset;
        let lagging = self.focus.update(focus, FOCUS_RATE, max_distance, dt);
        let forward = pov.rotation.dir();
        let mut across = lagging - pov.location;
        across -= forward * forward.dot(across);
        across += forward * forward.dot(focus - pov.location);
        pov.focus = vlerp(pov.location + across, focus, settings.stiffness);
    }

    /// `UpdateDistance`: pulled out while moving away from the camera.
    fn update_distance(&mut self, pov: &mut Pov, car: &CameraTarget, settings: &CameraSettings, dt: f32) {
        let away = car.velocity.dot(pov.rotation.dir());
        let target = settings.distance + (away * DISTANCE_SPEED_SCALE * (1.0 - settings.stiffness)).max(DISTANCE_OFFSET_MIN);
        pov.distance = self.distance.update(Vec3::new(target, 0.0, 0.0), DISTANCE_RATE, 0.0, dt).x;
    }

    /// `UpdateFOV`.
    fn update_fov(&self, pov: &mut Pov, car: &CameraTarget, settings: &CameraSettings, dt: f32) {
        let (fov, fov_speed) = if car.supersonic {
            (settings.fov + SUPERSONIC_FOV, SUPERSONIC_FOV_INTERP_SPEED)
        } else {
            (lerp(settings.fov, settings.fov + MAX_SPEED_FOV, car.velocity.length() / CAR_MAX_SPEED), FOV_INTERP_SPEED)
        };
        pov.fov = if self.first { fov } else { pov.fov + (fov - pov.fov).clamp(-fov_speed * dt, fov_speed * dt) };
    }

    /// `UpdateGroundPOV`.
    fn ground_rotation(&mut self, pov: &Pov, car: &CameraTarget, settings: &CameraSettings, pitch_offset: f32, dt: f32) -> Rot {
        self.ground_normal = (self.ground_normal + (car.ground_normal - self.ground_normal) * smooth_alpha(GROUND_NORMAL_RATE, dt)).safe_normalized();
        let up = self.ground_normal;
        let right = up.cross(car.orientation.forward());
        let forward = right.cross(up);
        let car_pitch = Rot::of_dir(forward).pitch;
        let pitch_blend = (car_pitch.abs() / (GROUND_PITCH_BLEND * UU_TO_RAD)).min(1.0);
        let dir = vlerp(forward, if car_pitch > 0.0 { -up } else { up }, pitch_blend);
        let target = Rot::of_dir(dir).add_pitch_offset(pitch_offset);
        if self.first {
            return target;
        }
        let scale = lerp(1.0, STIFFNESS_ROTATION_SCALE, settings.stiffness);
        let current = pov.rotation;
        let floor = if settings.stiffness < 1.0 { current.smooth_to(target, GROUND_ROTATION_RATE * scale, dt) } else { target };
        let wall = current.smooth_to(target, GROUND_ROTATION_RATE_WALL * scale, dt);
        floor.lerp(wall, 1.0 - up.z.abs())
    }

    /// `UpdateAirPOV`.
    fn air_rotation(&self, pov: &Pov, car: &CameraTarget, pitch_offset: f32) -> Rot {
        let v = car.velocity;
        let speed_2d = (v.x * v.x + v.y * v.y).sqrt();
        if self.first {
            let heading = Rot::of_dir(car.orientation.forward());
            let yaw = heading.lerp(Rot::of_dir(v), (speed_2d / AIR_START_VELOCITY).min(1.0)).yaw;
            return Rot { pitch: 0.0, yaw, roll: 0.0 }.add_pitch_offset(pitch_offset);
        }
        let rate = lerp(AIR_VELOCITY_INFLUENCE, AIR_VELOCITY_INFLUENCE_MAX_SPEED, (v.length() / CAR_MAX_SPEED).min(1.0));
        // Look at the car from where the camera is. The game applies this per frame, not per second.
        let toward = Rot::of_dir(pov.focus - pov.location);
        let mut rot = pov.rotation.lerp(toward, (rate / 60.0).min(1.0));
        // ScalePitch: flatten while the car comes towards the camera.
        if v.dot(rot.dir()) < 0.0 {
            rot.pitch *= 1.0 - speed_2d / CAR_MAX_SPEED;
        }
        rot
    }
}

/// Rocket League's player camera: car cam, and ball cam when the host passes a ball. Keep one per
/// viewed car and call [`CarCamera::update`] or [`CarCamera::update_with_ball`] every rendered
/// frame; call [`CarCamera::reset`] when the car teleports.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CarCamera {
    /// None until the first update.
    mode: Option<Mode>,
    state: StateMemory,
    /// The active state's view. It carries over when the state changes, like the game's.
    pov: Pov,
    /// Current swivel (pitch, yaw), radians.
    swivel: (f32, f32),
    transition: Option<Transition>,
    /// What the transition added last frame (`TransitionDelta`).
    transition_delta: PovDelta,
    /// Where ball cam last saw the ball (`OldBallLocation`).
    last_ball: Option<Vec3>,
    /// Ball cam was showing the rear view last frame (`bWasReverseCam`).
    was_rear: bool,
}

impl Default for CarCamera {
    fn default() -> Self {
        CarCamera::new()
    }
}

impl CarCamera {
    pub fn new() -> CarCamera {
        CarCamera {
            mode: None,
            state: StateMemory {
                first: true,
                on_ground: true,
                air_ground_blend: 1.0,
                ground_normal: Vec3::Z,
                focus: Smoothed::default(),
                focus_offset: Smoothed::default(),
                distance: Smoothed::default(),
            },
            pov: Pov::default(),
            swivel: (0.0, 0.0),
            transition: None,
            transition_delta: PovDelta::default(),
            last_ball: None,
            was_rear: false,
        }
    }

    /// Starts over from the car's current pose on the next update (no smoothing from the old one).
    pub fn reset(&mut self) {
        *self = CarCamera::new();
    }

    /// Moves everything the camera remembers by `delta` uu (when the host shifts its origin).
    pub fn translate(&mut self, delta: Vec3) {
        // The focus offset is relative to the car and transitions are relative to the view; the
        // rest are world positions.
        if let Some(v) = self.state.focus.value.as_mut() {
            *v += delta;
        }
        self.pov.focus += delta;
        self.pov.location += delta;
        if let Some(b) = self.last_ball.as_mut() {
            *b += delta;
        }
    }

    /// How much of the view is ball cam: 1 in ball cam, 0 in car cam, in between while blending.
    pub fn ball_cam_blend(&self) -> f32 {
        let ball = if self.mode == Some(Mode::Ball) { 1.0 } else { 0.0 };
        match self.transition {
            Some(t) => lerp(ball, 1.0 - ball, t.weight()),
            None => ball,
        }
    }

    /// Whether the camera is (blending towards) its ground behaviour: 1 = ground, 0 = air.
    pub fn air_ground_blend(&self) -> f32 {
        self.state.air_ground_blend
    }

    /// Advances the car cam by `dt` seconds of real time and returns this frame's view.
    pub fn update(&mut self, car: &CameraTarget, input: &CameraInput, settings: &CameraSettings, dt: f32) -> CameraView {
        self.update_with_ball(car, None, input, settings, dt)
    }

    /// Like [`CarCamera::update`], in ball cam while `ball` (its position) is given and car cam
    /// otherwise, blending between the two when it changes. Pass `None` while ball cam is off or
    /// there is no ball to look at.
    pub fn update_with_ball(&mut self, car: &CameraTarget, ball: Option<Vec3>, input: &CameraInput, settings: &CameraSettings, dt: f32) -> CameraView {
        let settings = settings.clamped();
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let ball = ball.filter(|b| b.x.is_finite() && b.y.is_finite() && b.z.is_finite());
        if ball.is_some() {
            self.last_ball = ball;
        }
        let want = if ball.is_some() { Mode::Ball } else { Mode::Car };

        // CameraStateBlender_X.Tick.
        if let Some(t) = self.transition.as_mut() {
            t.remaining -= dt;
            if t.remaining <= 0.0 {
                self.transition = None;
                self.transition_delta = PovDelta::default();
            }
        }

        match self.mode {
            None => {
                self.mode = Some(want);
                self.state = StateMemory::begin(car);
                self.run_state(want, car, input, &settings, dt);
            }
            Some(mode) if mode != want => {
                // TransitionToState / BlendCameraState: one last frame of the old state (keeping
                // its rotation from the frame before), then the new state from scratch, and the
                // difference eased out from there.
                let time = settings.transition_time();
                let previous = if time > 0.0 {
                    let mut snapshot = self.pov;
                    let mut old = *self;
                    old.run_state(mode, car, input, &settings, dt);
                    snapshot.focus = old.pov.focus;
                    snapshot.distance = old.pov.distance;
                    snapshot.fov = old.pov.fov;
                    Some(snapshot)
                } else {
                    None
                };
                self.mode = Some(want);
                self.state = StateMemory::begin(car);
                self.run_state(want, car, input, &settings, dt);
                self.transition = previous.map(|p| {
                    let d = self.transition_delta;
                    Transition {
                        remaining: time,
                        time,
                        offset: PovDelta {
                            focus: p.focus - self.pov.focus + d.focus,
                            rotation: p.rotation.sub(self.pov.rotation).normalized().add(d.rotation).normalized(),
                            distance: p.distance - self.pov.distance + d.distance,
                            fov: p.fov - self.pov.fov + d.fov,
                        },
                    }
                });
            }
            Some(mode) => self.run_state(mode, car, input, &settings, dt),
        }

        // CameraStateBlender_X.PostProcessPOV.
        self.transition_delta = self.transition.map_or(PovDelta::default(), |t| t.offset.scale(t.weight()));
        let d = self.transition_delta;
        let mut post = self.pov;
        post.focus += d.focus;
        post.rotation = post.rotation.add(d.rotation);
        post.distance += d.distance;
        post.fov += d.fov;

        // Camera_TA.PostProcessCameraState: swivel, then rear view, around the focus.
        self.update_swivel(car, input, &settings, dt);
        post.rotation = post.rotation.add(Rot { pitch: self.swivel.0, yaw: self.swivel.1, roll: 0.0 });
        if input.rear_view {
            post.rotation.yaw += std::f32::consts::PI;
        }
        post.finalize();
        CameraView { location: post.location, orientation: post.rotation.axes(), fov: post.fov, focus: post.focus }
    }

    /// One frame of a camera state's `UpdatePOV`.
    fn run_state(&mut self, mode: Mode, car: &CameraTarget, input: &CameraInput, settings: &CameraSettings, dt: f32) {
        match mode {
            Mode::Car => self.state.car_cam(&mut self.pov, car, settings, FOCUS_MAX_DISTANCE, dt),
            Mode::Ball => {
                // CameraState_BallCam_TA.UpdateValidPOV: the rear view is the car cam, and
                // switching between them snaps.
                if input.rear_view {
                    self.state.first = !self.was_rear;
                    self.was_rear = true;
                } else {
                    self.state.first |= self.was_rear;
                    self.was_rear = false;
                }
                match (input.rear_view, self.last_ball) {
                    (false, Some(ball)) => self.state.ball_cam(&mut self.pov, car, ball, settings, dt),
                    _ => self.state.car_cam(&mut self.pov, car, settings, FOCUS_MAX_DISTANCE, dt),
                }
            }
        }
    }

    /// `Camera_TA.UpdateSwivel` / `GetDesiredSwivel`.
    fn update_swivel(&mut self, car: &CameraTarget, input: &CameraInput, settings: &CameraSettings, dt: f32) {
        // PlayerInput_TA scales look input by FOV / 90; the camera receives it as a -1..1 byte.
        let fov_scale = self.pov.fov / 90.0;
        let clean = |v: f32| if v.is_finite() { (v * fov_scale).clamp(-1.0, 1.0) } else { 0.0 };
        let look_right = clean(input.look_right);
        let look_up = clean(input.look_up) * if settings.invert_swivel_pitch { -1.0 } else { 1.0 };

        let alpha = (car.velocity.length() / SWIVEL_FAST_SPEED).clamp(0.0, 1.0);
        let yaw_max = lerp(SWIVEL_YAW_MAX_SLOW, SWIVEL_YAW_MAX_FAST, alpha) * UU_TO_RAD;
        let desired_yaw = look_right * yaw_max;
        let desired_pitch = if look_up > 0.0 { look_up * SWIVEL_PITCH_MAX } else { look_up.abs() * SWIVEL_PITCH_MIN } * UU_TO_RAD;

        let (pitch, yaw) = self.swivel;
        let speed = |desired: f32, current: f32| settings.swivel_speed * if desired.abs() < current.abs() { SWIVEL_DIE_RATE } else { 1.0 };
        self.swivel = (
            lerp(pitch, desired_pitch, (speed(desired_pitch, pitch) * dt).min(1.0)),
            lerp(yaw, desired_yaw, (speed(desired_yaw, yaw) * dt).min(1.0)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    const DT: f32 = 1.0 / 60.0;

    fn driving(speed: f32) -> CarState {
        let mut s = CarState::new(HitboxPreset::Octane);
        s.position = Vec3::new(0.0, 0.0, 17.0);
        s.velocity = Vec3::new(speed, 0.0, 0.0);
        s.on_ground = true;
        s
    }

    fn settle(cam: &mut CarCamera, s: &CarState, input: &CameraInput, frames: usize) -> CameraView {
        let mut v = cam.update(&CameraTarget::from_state(s), input, &CameraSettings::DEFAULT, DT);
        for _ in 1..frames {
            v = cam.update(&CameraTarget::from_state(s), input, &CameraSettings::DEFAULT, DT);
        }
        v
    }

    fn deg(a: f32) -> f32 {
        a.to_degrees()
    }

    #[test]
    fn rotator_axes_match_unreal() {
        let r = Rot { pitch: 0.3, yaw: -1.2, roll: 0.7 }.axes();
        assert!((r.forward() - Rot { pitch: 0.3, yaw: -1.2, roll: 0.0 }.dir()).length() < 1e-6);
        // Same handedness as the car's own basis.
        assert!((r.forward().cross(r.right()) - r.up()).length() < 1e-5);
        // Positive roll tips the right side down, like the car's roll.
        let car = RotMat::from_angles(-1.2, 0.3, 0.7);
        for (a, b) in [(r.forward(), car.forward()), (r.right(), car.right()), (r.up(), car.up())] {
            assert!((a - b).length() < 1e-5, "{a:?} vs {b:?}");
        }
    }

    /// Parked on the floor: Distance behind and Height above, Angle degrees down, at the FOV.
    #[test]
    fn rests_behind_the_car() {
        let s = driving(0.0);
        let v = settle(&mut CarCamera::new(), &s, &CameraInput::default(), 300);
        let focus = s.position + Vec3::new(0.0, 0.0, 100.0);
        assert!((v.focus - focus).length() < 0.01, "{:?}", v.focus);
        let expected = focus - Rot { pitch: (-3f32).to_radians(), yaw: 0.0, roll: 0.0 }.dir() * 270.0;
        assert!((v.location - expected).length() < 0.05, "{:?} vs {expected:?}", v.location);
        assert!((v.fov - 90.0).abs() < 1e-4);
        assert!(v.orientation.up().z > 0.99);
    }

    /// Driving away from the camera pulls it back (half as much at the default Stiffness).
    #[test]
    fn speed_pulls_the_camera_out_and_widens_the_fov() {
        let s = driving(2000.0);
        let v = settle(&mut CarCamera::new(), &s, &CameraInput::default(), 600);
        let d = (v.focus - v.location).length();
        let expected = 270.0 + 2000.0 * (-3f32).to_radians().cos() * 0.05 * 0.5;
        assert!((d - expected).abs() < 0.5, "{d} vs {expected}");
        assert!((v.fov - (90.0 + 5.0 * 2000.0 / 2300.0)).abs() < 0.01, "{}", v.fov);
    }

    /// Driving up a wall: the camera stays upright and does not roll onto the wall.
    #[test]
    fn stays_upright_on_walls() {
        let mut s = driving(1000.0);
        // Nose up a wall at +X: car up = -X, forward = +Z.
        s.orientation = RotMat(Mat3::from_cols(Vec3::Z, Vec3::Y, -Vec3::X));
        s.velocity = Vec3::new(0.0, 0.0, 1000.0);
        for w in s.wheels.iter_mut() {
            w.contact = Some((Vec3::ZERO, -Vec3::X));
        }
        let v = settle(&mut CarCamera::new(), &s, &CameraInput::default(), 600);
        // Looking up the wall, tilted towards it (the camera sits out from the wall), no roll.
        assert!(v.orientation.forward().z > 0.8, "{:?}", v.orientation.forward());
        assert!(v.orientation.forward().x > 0.1, "{:?}", v.orientation.forward());
        assert!(v.orientation.right().z.abs() < 1e-4, "rolled: {:?}", v.orientation.right());
        assert!(v.location.x < s.position.x - 50.0);
    }

    /// The point of the air camera: spinning the car does not spin the camera.
    #[test]
    fn ignores_car_rotation_in_the_air() {
        let mut s = driving(1200.0);
        let mut cam = CarCamera::new();
        settle(&mut cam, &s, &CameraInput::default(), 120);
        s.on_ground = false;
        s.position.z = 500.0;
        let mut views = Vec::new();
        for i in 0..90 {
            // Flip and spin wildly while flying straight.
            s.orientation = RotMat::from_angles(i as f32 * 0.4, i as f32 * 0.7, i as f32 * 0.9);
            s.position += s.velocity * DT;
            views.push(cam.update(&CameraTarget::from_state(&s), &CameraInput::default(), &CameraSettings::DEFAULT, DT));
        }
        for v in &views[30..] {
            let f = v.orientation.forward();
            assert!(f.x > 0.95, "camera turned with the car: {f:?}");
            assert!(v.orientation.up().z > 0.95);
        }
    }

    /// In the air the camera is dragged along the path: flying sideways turns it to follow.
    #[test]
    fn follows_the_direction_of_travel_in_the_air() {
        let mut s = driving(0.0);
        let mut cam = CarCamera::new();
        settle(&mut cam, &s, &CameraInput::default(), 60);
        s.on_ground = false;
        s.position.z = 500.0;
        s.velocity = Vec3::new(0.0, 1500.0, 0.0);
        let mut v = None;
        for _ in 0..240 {
            s.position += s.velocity * DT;
            v = Some(cam.update(&CameraTarget::from_state(&s), &CameraInput::default(), &CameraSettings::DEFAULT, DT));
        }
        let f = v.unwrap().orientation.forward();
        assert!(f.y > 0.9, "{f:?}");
    }

    #[test]
    fn swivel_orbits_and_returns() {
        let s = driving(0.0);
        let mut cam = CarCamera::new();
        let rest = settle(&mut cam, &s, &CameraInput::default(), 60);
        let right = CameraInput { look_right: 1.0, ..Default::default() };
        let v = settle(&mut cam, &s, &right, 600);
        // Full right stick at rest: 22500 units (123.6 degrees) to the right, around the focus.
        let yaw = deg(v.orientation.forward().y.atan2(v.orientation.forward().x));
        assert!((yaw - 22500.0 * 360.0 / 65536.0).abs() < 0.1, "{yaw}");
        assert!(((v.location - v.focus).length() - (rest.location - rest.focus).length()).abs() < 0.01);
        // Half way there after ln(2) / 2.5 s at Swivel Speed 2.5 (per-frame lerp, so roughly).
        let mut cam2 = cam;
        cam2.reset();
        settle(&mut cam2, &s, &CameraInput::default(), 60);
        let v = settle(&mut cam2, &s, &right, (0.277 / DT) as usize);
        let yaw = deg(v.orientation.forward().y.atan2(v.orientation.forward().x));
        assert!((yaw / 123.6 - 0.5).abs() < 0.05, "{yaw}");
        // Released: back twice as fast.
        let v = settle(&mut cam, &s, &CameraInput::default(), 120);
        assert!(v.orientation.forward().x > 0.99);

        let up = CameraInput { look_up: 1.0, ..Default::default() };
        let v = settle(&mut CarCamera::new(), &s, &up, 600);
        let pitch = deg(v.orientation.forward().z.asin());
        assert!((pitch - (-3.0 + 5500.0 * 360.0 / 65536.0)).abs() < 0.1, "{pitch}");
        let down = CameraInput { look_up: -1.0, ..Default::default() };
        let v = settle(&mut CarCamera::new(), &s, &down, 600);
        let pitch = deg(v.orientation.forward().z.asin());
        assert!((pitch - (-3.0 - 8900.0 * 360.0 / 65536.0)).abs() < 0.1, "{pitch}");
    }

    #[test]
    fn rear_view_looks_back_from_in_front() {
        let s = driving(0.0);
        let v = settle(&mut CarCamera::new(), &s, &CameraInput { rear_view: true, ..Default::default() }, 120);
        assert!(v.orientation.forward().x < -0.99);
        assert!(v.location.x > s.position.x + 200.0);
    }

    #[test]
    fn ball_cam_looks_past_the_car_at_the_ball() {
        let s = driving(0.0);
        let target = CameraTarget::from_state(&s);
        let ball = s.position + Vec3::new(0.0, 2000.0, 0.0);
        let mut cam = CarCamera::new();
        let mut v = cam.update(&target, &CameraInput::default(), &CameraSettings::DEFAULT, 1.0 / 60.0);
        for _ in 0..240 {
            v = cam.update_with_ball(&target, Some(ball), &CameraInput::default(), &CameraSettings::DEFAULT, 1.0 / 60.0);
        }
        assert_eq!(cam.ball_cam_blend(), 1.0);
        assert!(v.orientation.forward().y > 0.95, "{:?}", v.orientation.forward());
        assert!(v.location.y < s.position.y - 150.0);
        for _ in 0..240 {
            v = cam.update(&target, &CameraInput::default(), &CameraSettings::DEFAULT, 1.0 / 60.0);
        }
        assert_eq!(cam.ball_cam_blend(), 0.0);
        assert!(v.orientation.forward().x > 0.95);
    }

    fn ball_cam(cam: &mut CarCamera, s: &CarState, ball: Vec3, input: &CameraInput, settings: &CameraSettings, frames: usize) -> CameraView {
        let mut v = None;
        for _ in 0..frames {
            v = Some(cam.update_with_ball(&CameraTarget::from_state(s), Some(ball), input, settings, DT));
        }
        v.unwrap()
    }

    fn pitch_deg(v: &CameraView) -> f32 {
        deg(v.orientation.forward().z.asin())
    }

    /// The game's ball cam pitch: while the ball is within 4000 rotation units (22°) of level the
    /// camera keeps the Angle setting and only turns.
    #[test]
    fn ball_cam_keeps_the_angle_while_the_ball_is_level() {
        let s = driving(0.0);
        let ball = Vec3::new(0.0, 2000.0, 93.0);
        let v = ball_cam(&mut CarCamera::new(), &s, ball, &CameraInput::default(), &CameraSettings::DEFAULT, 300);
        let f = v.orientation.forward();
        assert!((deg(f.y.atan2(f.x)) - 90.0).abs() < 0.01, "{f:?}");
        assert!((pitch_deg(&v) + 3.0).abs() < 0.01, "{}", pitch_deg(&v));
        assert!(v.orientation.right().z.abs() < 1e-5, "rolled");
        // Behind the car, opposite the ball.
        assert!(v.location.y < -200.0 && v.location.x.abs() < 1.0, "{:?}", v.location);
    }

    /// A high ball: the focus rises by 0.005 uu per rotation unit of pitch and the camera follows
    /// 80 % of the ball's pitch once it is 8000 units (44°) up.
    #[test]
    fn ball_cam_eases_into_a_high_ball() {
        let s = driving(0.0);
        let unit = 65536.0 / 360.0;
        for (ball, eased) in [(Vec3::new(1200.0, 0.0, 1000.0), 0.0), (Vec3::new(400.0, 0.0, 1500.0), 1.0)] {
            let v = ball_cam(&mut CarCamera::new(), &s, ball, &CameraInput::default(), &CameraSettings::DEFAULT, 300);
            let base = s.position + Vec3::new(0.0, 0.0, 100.0);
            let p1 = deg((ball - base).z.atan2(ball.x - base.x));
            let focus = base + Vec3::new(0.0, 0.0, p1.abs() * unit * 0.005);
            assert!((v.focus - focus).length() < 0.05, "{:?} vs {focus:?}", v.focus);
            let p2 = deg((ball - focus).z.atan2(ball.x - focus.x));
            let strength = ((p2.abs() * unit - 4000.0) / 4000.0).clamp(0.0, 1.0);
            if eased == 1.0 {
                assert_eq!(strength, 1.0);
            } else {
                assert!(strength > 0.0 && strength < 1.0, "{strength}");
            }
            let expected = -3.0 + (p2 + 3.0) * 0.8 * strength;
            assert!((pitch_deg(&v) - expected).abs() < 0.05, "{} vs {expected}", pitch_deg(&v));
        }
    }

    /// Car cam's focus trails a fast car (at most 100 uu); ball cam's does not trail at all.
    #[test]
    fn ball_cam_focus_does_not_lag() {
        let mut s = driving(0.0);
        s.velocity = Vec3::new(0.0, 2000.0, 0.0);
        let ball = Vec3::new(3000.0, 0.0, 93.0);
        let mut car = CarCamera::new();
        let mut ball_cam = CarCamera::new();
        let (mut cv, mut bv) = (None, None);
        for _ in 0..120 {
            s.position += s.velocity * DT;
            let t = CameraTarget::from_state(&s);
            cv = Some(car.update(&t, &CameraInput::default(), &CameraSettings::DEFAULT, DT));
            bv = Some(ball_cam.update_with_ball(&t, Some(ball), &CameraInput::default(), &CameraSettings::DEFAULT, DT));
        }
        let focus = s.position + Vec3::new(0.0, 0.0, 100.0);
        let lag = (cv.unwrap().focus - focus).length();
        assert!(lag > 5.0 && lag <= 100.0, "{lag}");
        let b = bv.unwrap().focus;
        assert!((b.x - focus.x).abs() < 0.01 && (b.y - focus.y).abs() < 0.01, "{b:?} vs {focus:?}");
    }

    #[test]
    fn focus_lag_is_capped() {
        let mut f = Smoothed::default();
        f.update(Vec3::ZERO, FOCUS_RATE, 100.0, DT);
        let v = f.update(Vec3::new(5000.0, 0.0, 0.0), FOCUS_RATE, 100.0, DT);
        assert!((v.x - 4900.0).abs() < 1e-3, "{v:?}");
    }

    /// Switching to ball cam starts from the car cam view and eases into ball cam over 0.5 s at
    /// Transition Speed 1 (smoothstep), and cuts at Transition Speed 2.
    #[test]
    fn ball_cam_transition_blends_over_the_transition_time() {
        let s = driving(0.0);
        let t = CameraTarget::from_state(&s);
        let ball = Vec3::new(0.0, 2000.0, 93.0);
        let input = CameraInput::default();
        let settings = CameraSettings::DEFAULT;

        let mut cam = CarCamera::new();
        let before = settle(&mut cam, &s, &input, 120);
        let first = cam.update_with_ball(&t, Some(ball), &input, &settings, DT);
        assert!((first.location - before.location).length() < 0.5, "{:?} vs {:?}", first.location, before.location);
        assert!((first.orientation.forward() - before.orientation.forward()).length() < 1e-3);
        assert!(cam.ball_cam_blend() < 0.01);
        // Half way in time is half way in the blend (smoothstep(0.5) = 0.5).
        let mut v = first;
        for _ in 0..15 {
            v = cam.update_with_ball(&t, Some(ball), &input, &settings, DT);
        }
        assert!((cam.ball_cam_blend() - 0.5).abs() < 0.01, "{}", cam.ball_cam_blend());
        let yaw = deg(v.orientation.forward().y.atan2(v.orientation.forward().x));
        assert!((yaw - 45.0).abs() < 1.0, "{yaw}");
        for _ in 0..15 {
            v = cam.update_with_ball(&t, Some(ball), &input, &settings, DT);
        }
        assert_eq!(cam.ball_cam_blend(), 1.0);
        let pure = ball_cam(&mut CarCamera::new(), &s, ball, &input, &settings, 1);
        assert!((v.location - pure.location).length() < 0.01, "{:?} vs {:?}", v.location, pure.location);

        // And back to car cam.
        let back = cam.update(&t, &input, &settings, DT);
        assert!((back.location - v.location).length() < 0.5);
        let back = settle(&mut cam, &s, &input, 30);
        assert_eq!(cam.ball_cam_blend(), 0.0);
        assert!((back.location - before.location).length() < 0.01);

        // Transition Speed 2: straight cut.
        let fast = CameraSettings { transition_speed: 2.0, ..settings };
        assert_eq!(fast.transition_time(), 0.0);
        let mut cam = CarCamera::new();
        settle(&mut cam, &s, &input, 60);
        let v = cam.update_with_ball(&t, Some(ball), &input, &fast, DT);
        assert_eq!(cam.ball_cam_blend(), 1.0);
        assert!((v.location - pure.location).length() < 0.01);
    }

    /// Rear view in ball cam is the car cam turned around, not ball cam turned around.
    #[test]
    fn rear_view_in_ball_cam_is_the_car_cam() {
        let s = driving(0.0);
        let ball = Vec3::new(0.0, 2000.0, 93.0);
        let rear = CameraInput { rear_view: true, ..Default::default() };
        let mut cam = CarCamera::new();
        let v = ball_cam(&mut cam, &s, ball, &rear, &CameraSettings::DEFAULT, 120);
        let car = settle(&mut CarCamera::new(), &s, &rear, 120);
        assert!((v.location - car.location).length() < 0.01, "{:?} vs {:?}", v.location, car.location);
        // Released: straight back to looking at the ball (it snaps).
        let v = ball_cam(&mut cam, &s, ball, &CameraInput::default(), &CameraSettings::DEFAULT, 1);
        assert!(v.orientation.forward().y > 0.99, "{:?}", v.orientation.forward());
    }

    #[test]
    fn settings_are_clamped() {
        let s = CameraSettings { fov: 200.0, distance: f32::NAN, stiffness: -1.0, ..CameraSettings::DEFAULT }.clamped();
        assert_eq!((s.fov, s.distance, s.stiffness), (110.0, 270.0, 0.0));
        assert!((CameraView { location: Vec3::ZERO, orientation: RotMat::IDENTITY, fov: 90.0, focus: Vec3::ZERO }.vertical_fov().to_degrees() - 58.72).abs() < 0.01);
    }
}
