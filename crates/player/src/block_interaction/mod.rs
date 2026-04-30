//! Block interaction systems for the player.
//!
//! This module implements the core block-targeting logic used for player
//! interaction with the voxel world.  Every frame it casts a ray from the
//! centre of the player's [`Camera3d`] along the camera's view axis, walks the
//! ray one voxel step at a time (DDA), and finds the first non-air block
//! within a configurable reach distance.  The targeted block is highlighted
//! with a wireframe cuboid drawn via Bevy's [`Gizmos`] API.
//!
//! The [`TargetedBlock`] resource also records which [`BlockFace`] the ray
//! entered from.  This tells placement logic which side of the targeted block
//! to attach the new block to — e.g. looking at the top face of a block means
//! the new block should be placed one voxel above it.
//!
//! # Registration
//!
//! Add [`BlockInteractionPlugin`] to your [`App`]:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_player::block_interaction::BlockInteractionPlugin;
//!
//! App::new()
//!     .add_plugins(BlockInteractionPlugin::default())
//!     .run();
//! ```
//!
//! # Configuration
//!
//! Adjust [`BlockInteractionConfig`] at any time via the Bevy resource system:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_player::block_interaction::{BlockInteractionPlugin, BlockInteractionConfig};
//!
//! App::new()
//!     .add_plugins(BlockInteractionPlugin::default())
//!     .add_systems(Startup, |mut config: ResMut<BlockInteractionConfig>| {
//!         config.max_distance = 10.0;
//!     })
//!     .run();
//! ```

use bevy::prelude::*;
use dd40_core::block::events::{
    AbortMiningRequest, BlockPlaced, BlockRemoved, MineBlockRequest, PlaceBlockRequest,
    StartMiningRequest,
};
use dd40_core::prelude::*;

pub use crate::block_interaction::mining::MiningState;
pub use crate::block_interaction::placement::HeldBlock;
use crate::block_interaction::mining::{apply_removed_blocks, update_mining};
use crate::block_interaction::placement::{apply_placed_blocks, try_place_block};
pub use crate::block_interaction::targeting::{BlockFace, BlockInteractionConfig, TargetedBlock};
use crate::block_interaction::targeting::{
    draw_targeted_block_highlight, spawn_debug_entity, update_debug_info, update_targeted_block,
};
use crate::PlayerMode;

pub mod mining;
pub mod placement;
mod targeting;

// ── Plugin ────────────────────────────────────────────────────────────────────

/// Plugin that adds player block-targeting, highlight rendering, block
/// placement, and block mining.
///
/// Registers the following resources:
/// - [`BlockInteractionConfig`] — raycast reach and highlight colour.
/// - [`TargetedBlock`]          — the block and face the player is looking at.
/// - [`HeldBlock`]              — the block type the player will place on right-click.
/// - [`MiningState`]            — current mining progress (readable by HUD / renderer).
///
/// Registers the following Bevy messages:
/// - [`PlaceBlockRequest`]     — written here, consumed by the network layer.
/// - [`BlockPlaced`]           — written by the network layer; consumed here to
///                               update the local [`ChunkCache`].
/// - [`StartMiningRequest`]    — written here, consumed by the network layer.
/// - [`AbortMiningRequest`]    — written here, consumed by the network layer.
/// - [`MineBlockRequest`]      — written here, consumed by the network layer.
///
/// All gameplay systems run only while the app is in [`AppState::Playing`]
/// **and** [`GameState::Running`], so they are automatically suppressed during
/// loading screens and pause menus.  The exception is [`apply_removed_blocks`],
/// which runs whenever [`AppState::Playing`] — including while paused — so
/// that block removals from other players are applied immediately.
///
/// # Example
///
/// ```no_run
/// use bevy::prelude::*;
/// use dd40_player::block_interaction::BlockInteractionPlugin;
///
/// App::new()
///     .add_plugins(BlockInteractionPlugin::default())
///     .run();
/// ```
///
/// [`ChunkCache`]: dd40_core::chunk::cache::ChunkCache
pub struct BlockInteractionPlugin {
    /// Initial reach distance in world units.  Can be changed later via the
    /// [`BlockInteractionConfig`] resource.
    pub max_distance: f32,
    /// Initial wireframe highlight colour.  Can be changed later via
    /// [`BlockInteractionConfig`].
    pub highlight_color: Color,
}

impl Default for BlockInteractionPlugin {
    fn default() -> Self {
        let defaults = BlockInteractionConfig::default();
        Self {
            max_distance: defaults.max_distance,
            highlight_color: defaults.highlight_color,
        }
    }
}

impl Plugin for BlockInteractionPlugin {
    fn build(&self, app: &mut App) {
        // ── Resources ─────────────────────────────────────────────────────
        app.insert_resource(BlockInteractionConfig {
            max_distance: self.max_distance,
            highlight_color: self.highlight_color,
        })
        .insert_resource(TargetedBlock::default())
        .insert_resource(HeldBlock::default())
        .insert_resource(MiningState::default())
        .register_type::<BlockInteractionConfig>()
        .register_type::<TargetedBlock>()
        .register_type::<HeldBlock>()
        .register_type::<MiningState>();

        // ── Messages ──────────────────────────────────────────────────────
        app.add_message::<PlaceBlockRequest>();
        app.add_message::<BlockPlaced>();
        app.add_message::<BlockRemoved>();
        app.add_message::<StartMiningRequest>();
        app.add_message::<AbortMiningRequest>();
        app.add_message::<MineBlockRequest>();

        // ── Startup ───────────────────────────────────────────────────────
        app.add_systems(Startup, spawn_debug_entity);

        // ── Per-frame gameplay systems ─────────────────────────────────────
        let playing_running_controller = in_state(AppState::Playing)
            .and(in_state(GameState::Running))
            .and(in_state(PlayerMode::Controller));

        app.add_systems(
            Update,
            (
                // 1. Cast ray → write TargetedBlock.
                update_targeted_block,
                // 2. Draw wireframe around the targeted block.
                draw_targeted_block_highlight,
                // 3. Update debug overlay text.
                update_debug_info,
                // 4. On right-click: validate and emit PlaceBlockRequest.
                try_place_block,
                // 5. On left-click: track mining timer, emit mining requests.
                update_mining,
            )
                .chain()
                .run_if(playing_running_controller),
        );

        // Clear state immediately when entering FreeCam so highlights and
        // mining state don't linger until the next raycast update.
        app.add_systems(
            OnEnter(PlayerMode::FreeCam),
            |mut targeted: ResMut<TargetedBlock>, mut mining: ResMut<MiningState>| {
                *targeted = TargetedBlock::default();
                *mining = MiningState::Idle;
            },
        );

        // apply_placed_blocks runs in PostUpdate and is NOT gated on PlayerMode.
        let playing_and_running = in_state(AppState::Playing).and(in_state(GameState::Running));
        app.add_systems(PostUpdate, apply_placed_blocks.run_if(playing_and_running));

        // apply_removed_blocks runs in PostUpdate gated only on AppState::Playing —
        // NOT on GameState::Running — so that block removals from other players
        // are applied even while the local game is paused.
        let playing = in_state(AppState::Playing);
        app.add_systems(PostUpdate, apply_removed_blocks.run_if(playing));
    }
}
