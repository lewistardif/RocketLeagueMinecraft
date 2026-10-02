//! Car body / wheel configuration for the standard hitbox presets.
//!
//! Hitbox values follow RocketSim's `CarConfig.cpp`, which were chosen because they reproduce
//! the game's inertia tensor. They are slightly smaller than the values the game reports via
//! `GetLocalCollisionExtent()` (the numbers on the RLBot wiki). See `CONSTANTS.md`.

use crate::math::Vec3;

/// The six standard hitbox presets (plus Psyclops, the only three-wheel-behaviour body).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HitboxPreset {
    #[default]
    Octane,
    Dominus,
    /// a.k.a. Batmobile
    Plank,
    Breakout,
    /// a.k.a. Venom
    Hybrid,
    Merc,
    Psyclops,
}

impl HitboxPreset {
    pub const ALL: [HitboxPreset; 7] = [
        HitboxPreset::Octane,
        HitboxPreset::Dominus,
        HitboxPreset::Plank,
        HitboxPreset::Breakout,
        HitboxPreset::Hybrid,
        HitboxPreset::Merc,
        HitboxPreset::Psyclops,
    ];

    pub fn name(self) -> &'static str {
        match self {
            HitboxPreset::Octane => "octane",
            HitboxPreset::Dominus => "dominus",
            HitboxPreset::Plank => "plank",
            HitboxPreset::Breakout => "breakout",
            HitboxPreset::Hybrid => "hybrid",
            HitboxPreset::Merc => "merc",
            HitboxPreset::Psyclops => "psyclops",
        }
    }

    pub fn from_name(s: &str) -> Option<HitboxPreset> {
        HitboxPreset::ALL.into_iter().find(|p| p.name().eq_ignore_ascii_case(s))
    }

    pub fn config(self) -> CarConfig {
        CarConfig::from_preset(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelPairConfig {
    /// uu
    pub wheel_radius: f32,
    /// How far out the suspension rests (uu), *before* subtracting max suspension travel.
    pub suspension_rest_length: f32,
    /// Suspension attachment point in car-local space (uu). `y` is positive; the left wheel mirrors it.
    pub connection_point_offset: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CarConfig {
    /// Full size (not half-extent) of the hitbox box, uu.
    pub hitbox_size: Vec3,
    /// Offset of the hitbox from the car origin (centre of mass), uu.
    pub hitbox_pos_offset: Vec3,
    pub front_wheels: WheelPairConfig,
    pub back_wheels: WheelPairConfig,
    pub three_wheels: bool,
    /// `|yaw| + |pitch| + |roll|` must reach this for a jump press to become a dodge.
    pub dodge_deadzone: f32,
}

// Index order: Octane, Dominus, Plank, Breakout, Hybrid, Merc, Psyclops.
const HITBOX_SIZES: [[f32; 3]; 7] = [
    [120.507, 86.6994, 38.6591],
    [130.427, 85.7799, 33.8],
    [131.32, 87.1704, 31.8944],
    [133.992, 83.021, 32.8],
    [129.519, 84.6879, 36.6591],
    [123.22, 79.2103, 44.1591],
    [120.507 + 0.134, 86.6994 + 0.134, 38.6591 + 0.134],
];
const HITBOX_OFFSETS: [[f32; 3]; 7] = [
    [13.8757, 0.0, 20.755],
    [9.0, 0.0, 15.75],
    [9.00857, 0.0, 12.0942],
    [12.5, 0.0, 11.75],
    [13.8757, 0.0, 20.755],
    [11.3757, 0.0, 21.505],
    [13.8757, 0.0, 15.0],
];
const FRONT_WHEEL_RADS: [f32; 7] = [12.50, 12.00, 12.50, 13.50, 12.50, 15.00, 12.50];
const BACK_WHEEL_RADS: [f32; 7] = [15.00, 13.50, 17.00, 15.00, 15.00, 15.00, 15.00];
const FRONT_WHEEL_SUS_REST: [f32; 7] = [38.755, 33.95, 31.9242, 29.7, 38.755, 39.505, 33.0];
const BACK_WHEEL_SUS_REST: [f32; 7] = [37.055, 33.85, 27.9242, 29.666, 37.055, 39.105, 31.3];
const FRONT_WHEELS_OFFSET: [[f32; 3]; 7] = [
    [51.25, 25.90, 20.755],
    [50.30, 31.10, 15.75],
    [49.97, 27.80, 12.0942],
    [51.50, 26.67, 11.75],
    [51.25, 25.90, 20.755],
    [51.25, 25.90, 21.505],
    [51.25, 5.000, 15.000],
];
const BACK_WHEELS_OFFSET: [[f32; 3]; 7] = [
    [-33.75, 29.50, 20.755],
    [-34.75, 33.00, 15.75],
    [-35.43, 20.28, 12.0942],
    [-35.75, 35.00, 11.75],
    [-34.00, 29.50, 20.755],
    [-33.75, 29.50, 21.505],
    [-33.75, 29.50, 15.000],
];

impl CarConfig {
    pub fn from_preset(p: HitboxPreset) -> CarConfig {
        let i = p as usize;
        CarConfig {
            hitbox_size: Vec3::from_array(HITBOX_SIZES[i]),
            hitbox_pos_offset: Vec3::from_array(HITBOX_OFFSETS[i]),
            front_wheels: WheelPairConfig {
                wheel_radius: FRONT_WHEEL_RADS[i],
                suspension_rest_length: FRONT_WHEEL_SUS_REST[i],
                connection_point_offset: Vec3::from_array(FRONT_WHEELS_OFFSET[i]),
            },
            back_wheels: WheelPairConfig {
                wheel_radius: BACK_WHEEL_RADS[i],
                suspension_rest_length: BACK_WHEEL_SUS_REST[i],
                connection_point_offset: Vec3::from_array(BACK_WHEELS_OFFSET[i]),
            },
            three_wheels: p == HitboxPreset::Psyclops,
            dodge_deadzone: crate::consts::DODGE_DEADZONE,
        }
    }

    /// Wheel `i` parameters: (connection point in car space (uu), radius (uu), rest length (uu), is_front).
    /// Wheel order: front-right(0), front-left(1), back-right(2), back-left(3)
    /// ("left" wheels have their local y negated, as in RocketSim).
    pub fn wheel(&self, i: usize) -> (Vec3, f32, f32, bool) {
        let front = i < 2;
        let pair = if front { &self.front_wheels } else { &self.back_wheels };
        let mut cp = pair.connection_point_offset;
        if i % 2 == 1 {
            cp.y = -cp.y;
        }
        (cp, pair.wheel_radius, pair.suspension_rest_length, front)
    }

    /// Half extents of the hitbox *as the physics actually uses it* (bt).
    ///
    /// Bullet builds the box as `half - 0.04` (default margin) and then lowers the margin to
    /// 10% of the smallest half extent when that is smaller than 0.04 ("safe margin"), without
    /// re-adding the difference. The effective box is therefore slightly smaller than
    /// `hitbox_size` (0.07 uu per side for the Octane, up to 0.36 uu for the Breakout); this
    /// affects collisions, the inertia tensor and the contact threshold.
    pub fn effective_half_extents_bt(&self) -> Vec3 {
        use crate::consts::solver::BOX_MARGIN_BT;
        let half = self.hitbox_size * crate::consts::UU_TO_BT * 0.5;
        let implicit = half - Vec3::new(BOX_MARGIN_BT, BOX_MARGIN_BT, BOX_MARGIN_BT);
        let min_half = half.x.min(half.y).min(half.z);
        let margin = (0.1 * min_half).min(BOX_MARGIN_BT);
        implicit + Vec3::new(margin, margin, margin)
    }

    /// Effective hitbox half extents in uu (see [`CarConfig::effective_half_extents_bt`]).
    pub fn effective_half_extents(&self) -> Vec3 {
        self.effective_half_extents_bt() * crate::consts::BT_TO_UU
    }

    /// Diagonal of the local inertia tensor, in bt units (Bullet `btBoxShape::calculateLocalInertia`).
    pub fn local_inertia_bt(&self) -> Vec3 {
        let s = self.effective_half_extents_bt() * 2.0;
        let (lx, ly, lz) = (s.x, s.y, s.z);
        let m = crate::consts::CAR_MASS_BT;
        Vec3::new(m / 12.0 * (ly * ly + lz * lz), m / 12.0 * (lx * lx + lz * lz), m / 12.0 * (lx * lx + ly * ly))
    }
}

impl CarConfig {
    /// Bullet's relative contact breaking threshold for the car's collision shape (bt):
    /// 0.02 x (bounding-sphere radius of the hitbox + distance of its centre from the origin).
    pub fn contact_breaking_threshold_bt(&self) -> f32 {
        let half = self.effective_half_extents_bt();
        let off = self.hitbox_pos_offset * crate::consts::UU_TO_BT;
        let (min, max) = (off - half, off + half);
        let radius = (max - min).length() * 0.5;
        let center = (min + max) * 0.5;
        (radius + center.length()) * crate::consts::solver::CONTACT_BREAKING_THRESHOLD_BT
    }
}

impl Default for CarConfig {
    fn default() -> Self {
        CarConfig::from_preset(HitboxPreset::Octane)
    }
}
