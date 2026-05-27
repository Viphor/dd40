//! Foundation crate defining the dd40 input vocabulary.
//!
//! # Overview
//!
//! `dd40_input_core` is the Tier 0 foundation for player input. It owns
//! two things and nothing else:
//!
//! - The set of [`InputAction`](bevy_enhanced_input::prelude::InputAction)
//!   types every other crate refers to when talking about player intent
//!   ([`actions`]).
//! - The [`InputCorePlugin`] which idempotently installs the
//!   [`bevy_enhanced_input`] runtime so other plugins can safely depend on
//!   it being present.
//!
//! This crate contains **no bindings** (keyboard / mouse / gamepad mappings
//! live in the consuming crates) and **no game logic** (translating actions
//! into `CharacterInput` intent lives in `dd40_player_input`).
//!
//! # Usage
//!
//! Add [`InputCorePlugin`] to your [`App`](bevy::prelude::App):
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_input_core::prelude::*;
//!
//! App::new()
//!     .add_plugins(InputCorePlugin)
//!     .run();
//! ```
//!
//! In practice you rarely add it directly — every plugin that uses BEI
//! actions calls `ensure_plugins!(app, InputCorePlugin)` and the macro
//! handles installation exactly once.

pub mod actions;
pub mod contexts;
pub mod plugin;
pub mod prelude;
pub mod prespawn;

pub use plugin::InputCorePlugin;
