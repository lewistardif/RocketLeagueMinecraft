//! # rl_car_core
//!
//! A clean-room, engine-agnostic reimplementation of Rocket League's car physics.
//! *Unofficial — not affiliated with, endorsed by, or connected to Psyonix or Epic Games.*
//!
//! The crate has **no dependencies** and no knowledge of any game engine or renderer.
//! The whole model is one pure function:
//!
//! ```
//! use rl_car_core::*;
//! let world = PlaneWorld::floor();          // host-provided collision (any CollisionWorld)
//! let mut car = CarState::new(HitboxPreset::Octane);
//! let input = Controls { throttle: 1.0, boost: true, ..Default::default() };
//! for _ in 0..120 {
//!     car = step(&car, &input, &world, TICK_DT);   // one 1/120 s tick
//! }
//! assert!(car.velocity.x > 1000.0);
//! ```
//!
//! * Space and units are Rocket League's: Unreal units (uu, ~1 cm), Z up, X forward, Y right
//!   (left-handed world). Hosts convert once at their boundary.
//! * The model runs at a fixed 120 Hz. Use [`FixedStepper`] to drive it from a variable frame rate.
//! * World geometry is supplied by the host through [`CollisionWorld`] (wheel raycasts + box contacts).
//! * Deterministic: no clocks, no randomness, no threads, fixed float operation order.

pub mod body;
pub mod config;
pub mod consts;
pub mod maneuvers;
pub mod manifold;
pub mod math;
pub mod sim;
pub mod solver;
pub mod state;
pub mod stepper;
pub mod world;

pub use config::{CarConfig, HitboxPreset, WheelPairConfig};
pub use consts::{TICK_DT, TICK_RATE};
pub use math::{Mat3, Quat, Vec3};
pub use sim::{SimConfig, step, step_with};
pub use state::{CarState, Controls, RotMat, WheelState};
pub use stepper::FixedStepper;
pub use world::{CollisionWorld, Contact, EmptyWorld, Obb, Plane, PlaneWorld, RayHit};
