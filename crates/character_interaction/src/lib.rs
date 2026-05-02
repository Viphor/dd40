//! Block targeting, mining, and placement for any [`Character`] entity.
//!
//! This crate is the generalised equivalent of `dd40_player`'s old
//! `block_interaction` module, lifted out of the player-specific crate so that
//! AI-controlled characters and multiplayer clients can share the same systems.
//!
//! Add [`CharacterInteractionPlugin`] to your app. Systems run for every entity
//! that carries a [`Character`] component; the `PlayerMode` gate is the
//! caller's responsibility (typically set in `dd40_player`).
//!
//! [`Character`]: dd40_character_core::components::Character

pub mod mining;
pub mod placement;
pub mod plugin;
pub mod targeting;

pub use dd40_character_core::mining_state::MiningState;
pub use placement::HeldBlock;
pub use plugin::CharacterInteractionPlugin;
pub use targeting::{BlockFace, BlockInteractionConfig, TargetedBlock};
