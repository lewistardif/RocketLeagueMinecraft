//! Rocket League's car camera ("car cam", ball cam off), engine-agnostic.
//!
//! Presentation only: nothing here feeds back into [`crate::step`]. Hosts call
//! [`CarCamera::update`] once per rendered frame with the (interpolated) car and get back where
//! to put the camera, in Rocket League space and units like the rest of the crate.
//!
//! This follows the game's own camera script (`CameraState_Car_TA`, `Camera_TA`; read from
//! `TAGame.upk` of the installed game) and the tuned values of its `Archetypes.Camera` objects:
//!
//! * **Focus**: the car position plus `Height` uu of offset (along the car's up while fully on the
//!   ground, world up otherwise), smoothed. Only the part of the smoothing that is across the view
//!   lags behind; along the view it is exact. `Stiffness` blends the lagging focus back to the true
//!   one.
//! * **Ground**: look along the car's forward projected onto the driving surface, `Angle` degrees
//!   down. The camera stays upright: on walls and the ceiling it does not roll with the car (only
//!   10 % of the car's sideways lean on the ground). Rotation is smoothed fast on the floor and
//!   slowly on walls.
//! * **Air**: the car's rotation is ignored. The camera turns to look at the car from where it is,
//!   like a camera on a string, so flips, spins and air rolls leave it pointing forward.
//! * **Distance**: `Distance` uu, pulled out while moving away from the camera (less with more
//!   `Stiffness`). **FOV**: `FOV` degrees, up to +5 with speed and +10 when supersonic.
//! * **Swivel** (right stick): orbits the camera around the focus by up to ±123° yaw (±99° at
//!   2500 uu/s and above), 30° up and 49° down, eased at `Swivel Speed` and returning twice as fast.
//!   **Rear view** turns it 180°.
//!
//! Left out: ball cam and the free-look camera mode (nothing to target; the "Modern" preset's
//! unconstrained rotation), camera shake, and the bob from the car body's visual suspension.
//! Clipping the camera against the world (Rocket League only keeps it 10 uu above the field floor)
//! is up to the host.

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
    /// How fast the camera blends to and from ball cam (1..=2). Kept for completeness: there is no
    /// ball cam here.
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
}

/// The game's tuning (`Archetypes.Camera.CameraState_Car`, `Archetypes.Camera.Camera_Default`).
mod tuning {
    pub const INTERP_TO_GROUND_RATE: f32 = 2.0;
    pub const INTERP_TO_AIR_RATE: f32 = 4.0;
    pub const FOCUS_RATE: f32 = 6.32;
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

/// `InterpVector` with the game's frame-rate independent smoothing (`VSmoothInterpTo`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Smoothed {
    value: Option<Vec3>,
}

impl Smoothed {
    fn update(&mut self, target: Vec3, rate: f32, dt: f32) -> Vec3 {
        let v = match self.value {
            Some(v) => v + (target - v) * smooth_alpha(rate, dt),
            None => target,
        };
        self.value = Some(v);
        v
    }
}

/// The camera's own view, before swivel and rear view (`PreProcessPOV`).
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

/// Rocket League's car camera. Keep one per viewed car and call [`CarCamera::update`] every
/// rendered frame; call [`CarCamera::reset`] when the car teleports.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CarCamera {
    first: bool,
    on_ground: bool,
    /// 1 = ground camera, 0 = air camera.
    air_ground_blend: f32,
    ground_normal: Vec3,
    focus: Smoothed,
    focus_offset: Smoothed,
    distance: Smoothed,
    pov: Pov,
    /// Current swivel (pitch, yaw), radians.
    swivel: (f32, f32),
}

impl Default for CarCamera {
    fn default() -> Self {
        CarCamera::new()
    }
}

impl CarCamera {
    pub fn new() -> CarCamera {
        CarCamera {
            first: true,
            on_ground: true,
            air_ground_blend: 1.0,
            ground_normal: Vec3::Z,
            focus: Smoothed::default(),
            focus_offset: Smoothed::default(),
            distance: Smoothed::default(),
            pov: Pov::default(),
            swivel: (0.0, 0.0),
        }
    }

    /// Starts over from the car's current pose on the next update (no smoothing from the old one).
    pub fn reset(&mut self) {
        *self = CarCamera::new();
    }

    /// Moves everything the camera remembers by `delta` uu (when the host shifts its origin).
    pub fn translate(&mut self, delta: Vec3) {
        // The focus offset is relative to the car; the rest are world positions.
        if let Some(v) = self.focus.value.as_mut() {
            *v += delta;
        }
        self.pov.focus += delta;
        self.pov.location += delta;
    }

    /// Whether the camera is (blending towards) its ground behaviour: 1 = ground, 0 = air.
    pub fn air_ground_blend(&self) -> f32 {
        self.air_ground_blend
    }

    /// Advances the camera by `dt` seconds of real time and returns this frame's view.
    pub fn update(&mut self, car: &CameraTarget, input: &CameraInput, settings: &CameraSettings, dt: f32) -> CameraView {
        let settings = settings.clamped();
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let first = self.first;
        let height = settings.height;
        let pitch_offset = settings.angle.to_radians();

        // UpdateAirGroundBlend (BeginCameraState on the first frame).
        if first {
            self.on_ground = car.on_ground;
            self.air_ground_blend = if car.on_ground { 1.0 } else { 0.0 };
            self.ground_normal = car.ground_normal;
        } else {
            if self.on_ground != car.on_ground {
                self.on_ground = car.on_ground;
                if self.on_ground {
                    self.ground_normal = car.ground_normal;
                }
            }
            self.air_ground_blend += if self.on_ground { INTERP_TO_GROUND_RATE * dt } else { -INTERP_TO_AIR_RATE * dt };
            self.air_ground_blend = self.air_ground_blend.clamp(0.0, 1.0);
        }

        // UpdateFocusWorldOffset: above the car along its up only while fully on the ground.
        let offset = if self.air_ground_blend >= 1.0 { car.orientation.0.mul_vec(Vec3::new(0.0, 0.0, height)) } else { Vec3::new(0.0, 0.0, height) };
        let offset = self.focus_offset.update(offset, FOCUS_OFFSET_RATE, dt);

        // UpdateFocus: the smoothed focus lags only across the view; Stiffness pulls it back.
        let focus = car.position + offset;
        let lagging = self.focus.update(focus, FOCUS_RATE, dt);
        let forward = self.pov.rotation.dir();
        let mut across = lagging - self.pov.location;
        across -= forward * forward.dot(across);
        across += forward * forward.dot(focus - self.pov.location);
        self.pov.focus = vlerp(self.pov.location + across, focus, settings.stiffness);

        // UpdateAirAndGroundCamera.
        let blend = self.air_ground_blend;
        self.pov.rotation = if blend >= 1.0 {
            self.ground_rotation(car, &settings, pitch_offset, dt)
        } else if blend <= 0.0 {
            self.air_rotation(car, pitch_offset)
        } else {
            let air = self.air_rotation(car, pitch_offset);
            let ground = self.ground_rotation(car, &settings, pitch_offset, dt);
            air.lerp(ground, blend)
        };

        // UpdateDistance: pulled out while moving away from the camera.
        let away = car.velocity.dot(self.pov.rotation.dir());
        let target = settings.distance + (away * DISTANCE_SPEED_SCALE * (1.0 - settings.stiffness)).max(DISTANCE_OFFSET_MIN);
        self.pov.distance = self.distance.update(Vec3::new(target, 0.0, 0.0), DISTANCE_RATE, dt).x;

        // UpdateFOV.
        let (fov, fov_speed) = if car.supersonic {
            (settings.fov + SUPERSONIC_FOV, SUPERSONIC_FOV_INTERP_SPEED)
        } else {
            (lerp(settings.fov, settings.fov + MAX_SPEED_FOV, car.velocity.length() / CAR_MAX_SPEED), FOV_INTERP_SPEED)
        };
        self.pov.fov = if first { fov } else { self.pov.fov + (fov - self.pov.fov).clamp(-fov_speed * dt, fov_speed * dt) };

        // UpdateRotationModifiers: a little of the car's sideways lean while on the ground.
        if blend > 0.0 {
            let right = car.orientation.right();
            let lean = right.z.atan2((right.x * right.x + right.y * right.y).sqrt());
            self.pov.rotation.roll = lean * -ROLL_SCALE * blend;
        }

        self.first = false;
        self.pov.finalize();

        // Camera_TA.PostProcessCameraState: swivel, then rear view, around the focus.
        self.update_swivel(car, input, &settings, dt);
        let mut post = self.pov;
        post.rotation = post.rotation.add(Rot { pitch: self.swivel.0, yaw: self.swivel.1, roll: 0.0 });
        if input.rear_view {
            post.rotation.yaw += std::f32::consts::PI;
        }
        post.finalize();
        CameraView { location: post.location, orientation: post.rotation.axes(), fov: post.fov, focus: post.focus }
    }

    /// `UpdateGroundPOV`.
    fn ground_rotation(&mut self, car: &CameraTarget, settings: &CameraSettings, pitch_offset: f32, dt: f32) -> Rot {
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
        let current = self.pov.rotation;
        let floor = if settings.stiffness < 1.0 { current.smooth_to(target, GROUND_ROTATION_RATE * scale, dt) } else { target };
        let wall = current.smooth_to(target, GROUND_ROTATION_RATE_WALL * scale, dt);
        floor.lerp(wall, 1.0 - up.z.abs())
    }

    /// `UpdateAirPOV`.
    fn air_rotation(&self, car: &CameraTarget, pitch_offset: f32) -> Rot {
        let v = car.velocity;
        let speed_2d = (v.x * v.x + v.y * v.y).sqrt();
        if self.first {
            let heading = Rot::of_dir(car.orientation.forward());
            let yaw = heading.lerp(Rot::of_dir(v), (speed_2d / AIR_START_VELOCITY).min(1.0)).yaw;
            return Rot { pitch: 0.0, yaw, roll: 0.0 }.add_pitch_offset(pitch_offset);
        }
        let rate = lerp(AIR_VELOCITY_INFLUENCE, AIR_VELOCITY_INFLUENCE_MAX_SPEED, (v.length() / CAR_MAX_SPEED).min(1.0));
        // Look at the car from where the camera is. The game applies this per frame, not per second.
        let toward = Rot::of_dir(self.pov.focus - self.pov.location);
        let mut rot = self.pov.rotation.lerp(toward, (rate / 60.0).min(1.0));
        // ScalePitch: flatten while the car comes towards the camera.
        if v.dot(rot.dir()) < 0.0 {
            rot.pitch *= 1.0 - speed_2d / CAR_MAX_SPEED;
        }
        rot
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
    fn settings_are_clamped() {
        let s = CameraSettings { fov: 200.0, distance: f32::NAN, stiffness: -1.0, ..CameraSettings::DEFAULT }.clamped();
        assert_eq!((s.fov, s.distance, s.stiffness), (110.0, 270.0, 0.0));
        assert!((CameraView { location: Vec3::ZERO, orientation: RotMat::IDENTITY, fov: 90.0, focus: Vec3::ZERO }.vertical_fov().to_degrees() - 58.72).abs() < 0.01);
    }
}
