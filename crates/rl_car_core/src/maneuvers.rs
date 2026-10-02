//! Derived maneuvers built purely from [`Controls`] sequences (no extra physics).
//!
//! These are the standard input recipes players/bots use; the physics that makes them work
//! (jump timing, dodge impulse, flip cancel, air roll) lives in [`crate::sim`].

use crate::state::{CarState, Controls};

/// A stateful controller that produces controls tick by tick until it finishes.
pub trait Maneuver {
    /// Controls for this tick, or `None` once the maneuver is complete.
    fn tick(&mut self, state: &CarState) -> Option<Controls>;
}

/// Jump, wait, then dodge in a direction given as (pitch, yaw/roll) stick input.
/// `dir_forward = 1` is a front flip, `-1` a backflip; `dir_right = 1` flips to the right.
#[derive(Clone, Debug)]
pub struct Dodge {
    pub dir_forward: f32,
    pub dir_right: f32,
    /// Ticks to hold the first jump.
    pub jump_ticks: u32,
    /// Ticks with jump released before the dodge press.
    pub wait_ticks: u32,
    /// Ticks to keep the stick held after the dodge press.
    pub hold_ticks: u32,
    pub throttle: f32,
    t: u32,
}

impl Dodge {
    pub fn new(dir_forward: f32, dir_right: f32) -> Dodge {
        Dodge { dir_forward, dir_right, jump_ticks: 6, wait_ticks: 4, hold_ticks: 60, throttle: 1.0, t: 0 }
    }
}

impl Maneuver for Dodge {
    fn tick(&mut self, _state: &CarState) -> Option<Controls> {
        let t = self.t;
        self.t += 1;
        let press = self.jump_ticks + self.wait_ticks;
        let mut c = Controls { throttle: self.throttle, ..Default::default() };
        if t < self.jump_ticks {
            c.jump = true;
        } else if t < press {
            // released
        } else if t < press + self.hold_ticks {
            c.jump = t == press;
            c.pitch = -self.dir_forward;
            c.yaw = self.dir_right;
        } else {
            return None;
        }
        Some(c)
    }
}

/// Half-flip: while driving backwards, jump, backflip, cancel the flip by pushing the stick
/// forward, then air-roll upright. The car ends up facing the way it was travelling.
#[derive(Clone, Debug)]
pub struct HalfFlip {
    t: u32,
    /// Tick at which the backflip is pressed.
    pub flip_tick: u32,
    /// Ticks after the flip press when the cancel (stick forward) starts.
    pub cancel_delay: u32,
    /// Hard time limit in ticks.
    pub timeout: u32,
    landed_ticks: u32,
}

impl Default for HalfFlip {
    fn default() -> Self {
        HalfFlip { t: 0, flip_tick: 8, cancel_delay: 10, timeout: 240, landed_ticks: 0 }
    }
}

impl HalfFlip {
    pub fn new() -> HalfFlip {
        HalfFlip::default()
    }
}

impl Maneuver for HalfFlip {
    fn tick(&mut self, s: &CarState) -> Option<Controls> {
        let t = self.t;
        self.t += 1;
        if t >= self.timeout {
            return None;
        }
        let mut c = Controls::default();
        if t < 5 {
            c.jump = true;
            c.throttle = -1.0;
        } else if t < self.flip_tick {
            c.throttle = -1.0;
        } else if t == self.flip_tick {
            c.jump = true;
            c.pitch = 1.0; // backflip
        } else if t < self.flip_tick + self.cancel_delay {
            c.pitch = 1.0;
        } else {
            // Cancel the flip and roll upright.
            c.pitch = -1.0;
            c.throttle = 1.0;
            if s.up().z < 0.9 {
                c.roll = if s.right().z >= 0.0 { 1.0 } else { -1.0 };
            }
            if s.on_ground {
                self.landed_ticks += 1;
                if self.landed_ticks > 3 {
                    return None;
                }
            }
        }
        Some(c)
    }
}
