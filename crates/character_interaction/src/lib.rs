//! Block targeting, mining, and placement for any [`Character`] entity.
//!
//! This crate hosts the systems that used to live in the now-deleted
//! `dd40_player` wrapper, generalised so that AI-controlled characters
//! and multiplayer clients share the same code path.
//!
//! Add [`CharacterInteractionPlugin`] to your app. Systems run for every entity
//! that carries a [`Character`] component; the `PlayerMode` gate is the
//! caller's responsibility (typically set in `dd40_player_input`).
//!
//! [`Character`]: dd40_character_core::components::Character

pub mod face_drive;
pub mod interact;
pub mod mining;
pub mod placement;
pub mod plugin;
pub mod targeting;
pub mod validators;

pub use dd40_character_core::mining_state::MiningState;
pub use plugin::CharacterInteractionPlugin;
pub use targeting::{BlockFace, BlockInteractionConfig, TargetedBlock};
