use bevy::prelude::*;
use dd40_core::prelude::BlockPos;

/// The current state of a character's mining action.
///
/// Read this resource to render a progress bar or block-crack animation.
/// This is a pure vocabulary type — the systems that advance it live in
/// `dd40_character_interaction`.
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub enum MiningState {
    /// No mining in progress.
    Idle,
    /// Actively mining a block.
    Mining {
        pos: BlockPos,
        /// Mining progress in `[0.0, 1.0]`.
        progress: f32,
        required_duration: f32,
    },
}

impl Default for MiningState {
    fn default() -> Self {
        Self::Idle
    }
}
