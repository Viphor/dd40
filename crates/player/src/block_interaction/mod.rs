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
use dd40_core::block::events::{BlockPlaced, PlaceBlockRequest};
use dd40_core::prelude::*;

pub use crate::block_interaction::placement::HeldBlock;
use crate::block_interaction::placement::{apply_placed_blocks, try_place_block};
pub use crate::block_interaction::targeting::{BlockFace, BlockInteractionConfig, TargetedBlock};
use crate::block_interaction::targeting::{
    draw_targeted_block_highlight, spawn_debug_entity, update_debug_info, update_targeted_block,
};

pub mod placement;
mod targeting;

// ── Plugin ────────────────────────────────────────────────────────────────────

/// Plugin that adds player block-targeting, highlight rendering, and block
/// placement.
///
/// Registers the following resources:
/// - [`BlockInteractionConfig`] — raycast reach and highlight colour.
/// - [`TargetedBlock`]          — the block and face the player is looking at.
/// - [`HeldBlock`]              — the block type the player will place on
///                                right-click.
///
/// Registers the following Bevy messages:
/// - [`PlaceBlockRequest`] — written by [`try_place_block`] and consumed by
///   the network layer to forward the request to the server.
/// - [`BlockPlaced`]       — written by the network layer when a placement is
///   confirmed; consumed by [`apply_placed_blocks`] to update the local
///   [`ChunkCache`].
///
/// All gameplay systems run only while the app is in [`AppState::Playing`]
/// **and** [`GameState::Running`], so they are automatically suppressed during
/// loading screens and pause menus.
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
        .register_type::<BlockInteractionConfig>()
        .register_type::<TargetedBlock>()
        .register_type::<HeldBlock>();

        // ── Messages ──────────────────────────────────────────────────────
        // PlaceBlockRequest: written here, consumed by the network layer.
        app.add_message::<PlaceBlockRequest>();
        // BlockPlaced: written by the network layer, consumed here.
        app.add_message::<BlockPlaced>();

        // ── Startup ───────────────────────────────────────────────────────
        app.add_systems(Startup, spawn_debug_entity);

        // ── Per-frame gameplay systems ─────────────────────────────────────
        let playing_and_running = in_state(AppState::Playing).and(in_state(GameState::Running));

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
            )
                .chain()
                .run_if(playing_and_running.clone()),
        );

        // apply_placed_blocks runs in PostUpdate so it always sees messages
        // written by the network layer during Update (receive_placed_blocks).
        app.add_systems(PostUpdate, apply_placed_blocks.run_if(playing_and_running));
    }
}
