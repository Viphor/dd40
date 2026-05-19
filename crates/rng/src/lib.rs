//! Pluggable random-number generation for dd40 crates.
//!
//! # Overview
//!
//! Provides a single shared [`GameRng`] resource that any system can borrow
//! mutably to roll dice without baking in a specific RNG implementation.
//! Consumer crates only ever see [`rand::RngCore`], so the concrete RNG
//! (deterministic seeded `StdRng`, hardware-entropy `OsRng`, mock RNG in
//! tests, …) can be swapped from the binary without forking any consumer.
//!
//! # Why a plugin and not a thread-local
//!
//! A Bevy [`Resource`] makes the RNG visible to schedule analysis: systems
//! that touch [`GameRng`] cannot run in parallel with one another, which is
//! exactly the behaviour you want for deterministic replay if the RNG ever
//! becomes seeded.
//!
//! # Usage
//!
//! Default (`StdRng` seeded from OS entropy):
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_rng::RngPlugin;
//!
//! App::new().add_plugins(RngPlugin::default()).run();
//! ```
//!
//! Custom seeded RNG (deterministic):
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_rng::RngPlugin;
//! use rand::{SeedableRng, rngs::StdRng};
//!
//! App::new()
//!     .add_plugins(RngPlugin::with_factory(|| Box::new(StdRng::seed_from_u64(42))))
//!     .run();
//! ```
//!
//! Drawing from the RNG in a system:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_rng::GameRng;
//! use rand::Rng;
//!
//! fn roll_dice(mut rng: ResMut<GameRng>) {
//!     let _n: u8 = rng.random_range(1..=6);
//! }
//! ```

pub mod plugin;
pub mod prelude;
pub mod resource;

pub use plugin::RngPlugin;
pub use resource::GameRng;
