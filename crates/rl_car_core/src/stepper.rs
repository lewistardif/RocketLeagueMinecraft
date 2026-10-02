//! Fixed-timestep driver: runs the 120 Hz simulation against an arbitrary host frame rate.

use crate::consts::TICK_DT;
use crate::sim::{SimConfig, step_with};
use crate::state::{CarState, Controls};
use crate::world::CollisionWorld;

/// Accumulates host frame time and advances the car in whole 1/120 s ticks.
///
/// The physics never sees the host's frame time, so results are identical at 30, 60 or 144 FPS.
/// `previous` and `current` are kept so the host can render an interpolated pose
/// (see [`FixedStepper::alpha`]).
#[derive(Clone, Debug)]
pub struct FixedStepper {
    pub previous: CarState,
    pub current: CarState,
    pub config: SimConfig,
    accumulator: f64,
    /// Total number of ticks simulated.
    pub tick_count: u64,
    /// Upper bound on ticks per `advance` call, to avoid a spiral of death after a hitch.
    pub max_ticks_per_frame: u32,
}

impl FixedStepper {
    pub fn new(state: CarState) -> FixedStepper {
        FixedStepper {
            previous: state,
            current: state,
            config: SimConfig::default(),
            accumulator: 0.0,
            tick_count: 0,
            max_ticks_per_frame: 24,
        }
    }

    /// Replace the state (teleport/reset) without interpolating from the old one.
    pub fn reset(&mut self, state: CarState) {
        self.previous = state;
        self.current = state;
        self.accumulator = 0.0;
    }

    /// Add `frame_dt` seconds and run as many ticks as fit. `controls` is sampled once per
    /// frame and held for every tick in it. Returns the number of ticks run.
    pub fn advance(&mut self, frame_dt: f64, controls: &Controls, world: &dyn CollisionWorld) -> u32 {
        self.advance_with(frame_dt, world, |_| *controls)
    }

    /// Like [`FixedStepper::advance`], but asks `controls_for` for the controls of every tick
    /// (given the state about to be stepped) — for scripted maneuvers or bots.
    pub fn advance_with(&mut self, frame_dt: f64, world: &dyn CollisionWorld, mut controls_for: impl FnMut(&CarState) -> Controls) -> u32 {
        self.accumulator += frame_dt.max(0.0);
        let tick = TICK_DT as f64;
        let mut n = 0;
        while self.accumulator >= tick {
            if n >= self.max_ticks_per_frame {
                // Drop the backlog rather than trying to catch up forever.
                self.accumulator = 0.0;
                break;
            }
            self.previous = self.current;
            let controls = controls_for(&self.current);
            self.current = step_with(&self.current, &controls, world, &self.config, TICK_DT);
            self.accumulator -= tick;
            self.tick_count += 1;
            n += 1;
        }
        n
    }

    /// Fraction (0..1) of a tick elapsed since `current`; render `lerp(previous, current, alpha)`.
    pub fn alpha(&self) -> f32 {
        (self.accumulator / TICK_DT as f64).clamp(0.0, 1.0) as f32
    }
}
