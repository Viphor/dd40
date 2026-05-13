use bevy::prelude::*;
use dd40_core::prelude::BlockPos;

/// The current state of a character's mining action.
///
/// Attach to any [`Character`][crate::components::Character] entity.  The
/// mining system in `dd40_character_interaction` advances this component each
/// frame; HUDs and renderers may read it to draw a progress bar or
/// block-crack overlay.
///
/// Defaults to [`MiningState::Idle`].
#[derive(Component, Debug, Clone, Reflect, Default)]
#[reflect(Component)]
pub enum MiningState {
    /// No mining in progress.
    #[default]
    Idle,
    /// Actively mining a block.
    Mining {
        pos: BlockPos,
        /// Mining progress in `[0.0, 1.0]`.
        progress: f32,
        required_duration: f32,
    },
}
