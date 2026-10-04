//! Oracle validation: compare `rl_car_core` trajectories against RocketSim ground truth.

pub mod ball_scenarios;
pub mod scenarios;

use rl_car_core::{CarState, Mat3, Vec3, step};
use scenarios::{Category, Scenario};
use std::path::{Path, PathBuf};

/// One recorded tick of the oracle (or of the core, in the same format).
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub tick: u32,
    pub pos: Vec3,
    pub vel: Vec3,
    pub ang_vel: Vec3,
    pub rot: Mat3,
    pub boost: f32,
    pub on_ground: bool,
    pub has_jumped: bool,
    pub has_double_jumped: bool,
    pub has_flipped: bool,
    pub contacts: [bool; 4],
}

impl Sample {
    pub fn from_state(tick: u32, s: &CarState) -> Sample {
        Sample {
            tick,
            pos: s.position,
            vel: s.velocity,
            ang_vel: s.angular_velocity,
            rot: s.orientation.0,
            boost: s.boost_amount,
            on_ground: s.on_ground,
            has_jumped: s.has_jumped,
            has_double_jumped: s.has_double_jumped,
            has_flipped: s.has_flipped,
            contacts: s.wheel_contacts,
        }
    }
}

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

pub fn validation_dir() -> PathBuf {
    workspace_root().join("validation")
}

pub fn trace_path(name: &str) -> PathBuf {
    validation_dir().join("traces").join(format!("{name}.csv"))
}

pub fn scenario_path(name: &str) -> PathBuf {
    validation_dir().join("scenarios").join(format!("{name}.txt"))
}

pub fn parse_trace(text: &str) -> Result<Vec<Sample>, String> {
    let mut out = Vec::new();
    for (ln, line) in text.lines().enumerate().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let f: Vec<f32> = line.split(',').map(|x| x.trim().parse::<f32>()).collect::<Result<_, _>>().map_err(|e| format!("line {}: {e}", ln + 1))?;
        if f.len() < 28 {
            return Err(format!("line {}: expected 28 columns, got {}", ln + 1, f.len()));
        }
        let v = |i: usize| Vec3::new(f[i], f[i + 1], f[i + 2]);
        out.push(Sample {
            tick: f[0] as u32,
            pos: v(1),
            vel: v(4),
            ang_vel: v(7),
            rot: Mat3::from_cols(v(10), v(13), v(16)),
            boost: f[19],
            on_ground: f[20] != 0.0,
            has_jumped: f[21] != 0.0,
            has_double_jumped: f[22] != 0.0,
            has_flipped: f[23] != 0.0,
            contacts: [f[24] != 0.0, f[25] != 0.0, f[26] != 0.0, f[27] != 0.0],
        });
    }
    Ok(out)
}

/// Runs a scenario through the core. Returns samples for ticks 0..=ticks.
pub fn run_core(sc: &Scenario) -> Vec<Sample> {
    let world = sc.world();
    let mut s = sc.initial_state();
    let mut out = vec![Sample::from_state(0, &s)];
    for t in 0..sc.ticks {
        let c = sc.controls_at(t);
        s = step(&s, &c, &world, rl_car_core::TICK_DT);
        out.push(Sample::from_state(t + 1, &s));
    }
    out
}

/// Angle (degrees) of the rotation taking `a` to `b`, from the Frobenius distance
/// `|A - B|_F = 2 sqrt(2) sin(theta / 2)` (well conditioned for small angles, unlike acos).
pub fn rot_error_deg(a: &Mat3, b: &Mat3) -> f32 {
    let mut sum = 0.0f64;
    for r in 0..3 {
        for c in 0..3 {
            let d = a.m[r][c] as f64 - b.m[r][c] as f64;
            sum += d * d;
        }
    }
    let x = (sum.sqrt() / (2.0 * std::f64::consts::SQRT_2)).min(1.0);
    (2.0 * x.asin()).to_degrees() as f32
}

#[derive(Clone, Debug, Default)]
pub struct Comparison {
    pub ticks: u32,
    pub max_pos: f32,
    pub max_vel: f32,
    pub max_ang_vel: f32,
    pub max_rot_deg: f32,
    pub max_boost: f32,
    pub final_pos: f32,
    pub final_vel: f32,
    /// Ticks where on_ground / jump / flip flags differ.
    pub flag_mismatches: u32,
    /// Ticks where per-wheel contact flags differ.
    pub contact_mismatches: u32,
    /// First tick where position error exceeds 1 uu.
    pub diverge_tick: Option<u32>,
}

pub fn compare(core: &[Sample], oracle: &[Sample], limit: Option<u32>) -> Comparison {
    let mut c = Comparison::default();
    let n = core.len().min(oracle.len());
    let n = limit.map_or(n, |l| n.min(l as usize + 1));
    for i in 0..n {
        let (a, b) = (&core[i], &oracle[i]);
        let pe = (a.pos - b.pos).length();
        let ve = (a.vel - b.vel).length();
        c.max_pos = c.max_pos.max(pe);
        c.max_vel = c.max_vel.max(ve);
        c.max_ang_vel = c.max_ang_vel.max((a.ang_vel - b.ang_vel).length());
        c.max_rot_deg = c.max_rot_deg.max(rot_error_deg(&a.rot, &b.rot));
        c.max_boost = c.max_boost.max((a.boost - b.boost).abs());
        if (a.on_ground, a.has_jumped, a.has_double_jumped, a.has_flipped) != (b.on_ground, b.has_jumped, b.has_double_jumped, b.has_flipped) {
            c.flag_mismatches += 1;
        }
        if a.contacts != b.contacts {
            c.contact_mismatches += 1;
        }
        if pe > 1.0 && c.diverge_tick.is_none() {
            c.diverge_tick = Some(a.tick);
        }
        c.final_pos = pe;
        c.final_vel = ve;
        c.ticks = a.tick;
    }
    c
}

/// Documented acceptance tolerances per category.
#[derive(Clone, Copy, Debug)]
pub struct Tolerance {
    pub pos: f32,
    pub vel: f32,
    pub rot_deg: f32,
    pub ang_vel: f32,
}

/// Wheel-only driving, jumps, dodges and air control are expected to track RocketSim to float
/// precision. Body-contact scenarios end with the car resting flat on a face, where which of four
/// equally deep corners gets re-added each tick is decided by float noise; they get a looser bound.
pub fn tolerance(cat: Category) -> Tolerance {
    match cat {
        Category::Ground | Category::Air | Category::Wall => Tolerance { pos: 0.25, vel: 0.25, rot_deg: 0.05, ang_vel: 0.005 },
        Category::Body => Tolerance { pos: 2.5, vel: 25.0, rot_deg: 2.0, ang_vel: 1.0 },
    }
}

impl Comparison {
    pub fn passes(&self, t: Tolerance) -> bool {
        self.max_pos <= t.pos && self.max_vel <= t.vel && self.max_rot_deg <= t.rot_deg && self.max_ang_vel <= t.ang_vel
    }
}
