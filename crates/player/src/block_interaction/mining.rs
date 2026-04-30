//! Client-side mining logic.
//!
//! This module tracks how long the player holds left-click on a targeted block
//! and emits the appropriate mining request messages consumed by the network
//! layer:
//!
//! - [`StartMiningRequest`] — emitted when the player begins holding left-click
//!   on a valid, destructible block.
//! - [`AbortMiningRequest`] — emitted when the player releases left-click or
//!   moves the crosshair to a different block before completing the mine.
//! - [`MineBlockRequest`] — emitted when the local timer expires (client
//!   predicts that mining is done); the server validates this before removing
//!   the block.
//!
//! # Receiving confirmation
//!
//! Block removal only takes effect locally when a [`BlockRemoved`] Bevy message
//! arrives (written by the network layer after the server confirms the removal).
//! [`apply_removed_blocks`] handles this in `PostUpdate` and is gated only on
//! [`AppState::Playing`] — **not** on [`GameState::Running`] — so that paused
//! clients still receive block removals from other players.
//!
//! # Progress feedback
//!
//! The [`MiningState`] resource is publicly readable and updated every frame
//! while mining.  Other crates (HUD, renderer) may read it to display a
//! progress bar or block-crack animation.
//!
//! # Insta-mine
//!
//! If [`mining_duration`] returns `Some(0.0)` (toughness ≤ 0), the system
//! skips [`StartMiningRequest`] and fires [`MineBlockRequest`] immediately on
//! the same frame.
//!
//! [`mining_duration`]: dd40_core::tools::mining_duration

use bevy::prelude::*;
use dd40_core::{
    block::{
        Block, BlockId,
        events::{AbortMiningRequest, BlockRemoved, MineBlockRequest, StartMiningRequest},
    },
    character::Player,
    chunk::cache::ChunkCache,
    prelude::*,
    tools::{EquippedTool, mining_duration},
};

use crate::block_interaction::targeting::TargetedBlock;

// ── Public state resource ─────────────────────────────────────────────────────

/// The current state of the player's mining action.
///
/// Read this resource from any crate to render a progress bar, block-crack
/// animation, or HUD indicator.
///
/// Updated every frame by [`update_mining`].
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub enum MiningState {
    /// The player is not currently mining anything.
    Idle,
    /// The player is actively mining a block.
    Mining {
        /// World-space position of the block being mined.
        pos: BlockPos,
        /// Mining progress in the range `[0.0, 1.0]`.
        ///
        /// `0.0` = just started, `1.0` = complete.
        progress: f32,
        /// Total time in seconds required to mine this block with the current tool.
        required_duration: f32,
    },
}

impl Default for MiningState {
    fn default() -> Self {
        Self::Idle
    }
}

// ── Systems ───────────────────────────────────────────────────────────────────

/// Updates mining state each frame and emits mining request messages.
///
/// # Logic (per frame)
///
/// 1. If left-click is **just pressed** and a destructible, non-replaceable
///    block is targeted:
///    - Compute required duration via [`mining_duration`].
///    - If duration is `0.0`: emit [`MineBlockRequest`] immediately (insta-mine).
///    - Otherwise: transition to [`MiningState::Mining`] and emit [`StartMiningRequest`].
/// 2. If currently mining and left-click is still **held**:
///    - Check that the same block is still targeted; abort if not.
///    - Advance elapsed time and update `progress`.
///    - If `progress >= 1.0`: emit [`MineBlockRequest`], transition to `Idle`.
/// 3. If left-click was **released** while mining: emit [`AbortMiningRequest`],
///    transition to `Idle`.
pub(super) fn update_mining(
    mouse: Res<ButtonInput<MouseButton>>,
    targeted: Res<TargetedBlock>,
    equipped_query: Query<&EquippedTool, With<Player>>,
    registry: Res<BlockRegistry>,
    tool_registry: Res<ToolRegistry>,
    time: Res<Time>,
    mut state: ResMut<MiningState>,
    mut start_writer: MessageWriter<StartMiningRequest>,
    mut abort_writer: MessageWriter<AbortMiningRequest>,
    mut mine_writer: MessageWriter<MineBlockRequest>,
) {
    let bare_hands = EquippedTool::default();
    let tool = equipped_query.iter().next().copied().unwrap_or(bare_hands);

    let left_held = mouse.pressed(MouseButton::Left);
    let left_just_pressed = mouse.just_pressed(MouseButton::Left);

    match state.as_ref().clone() {
        MiningState::Idle => {
            if !left_just_pressed {
                return;
            }

            let Some(pos) = targeted.pos else { return };

            // Resolve the block at the targeted position via the block ID stored
            // in TargetedBlock.
            let Some(block_id) = targeted.block_id else {
                return;
            };
            let block = Block::new(block_id);

            // Don't try to mine replaceable blocks (air) or indestructible ones.
            if registry.is_replaceable(&block) {
                return;
            }

            let Some(block_def) = registry.get(block_id) else {
                return;
            };

            if !block_def.is_destructible {
                return;
            }

            let Some(duration) = mining_duration(block_def, &tool, &tool_registry) else {
                return;
            };

            if duration <= 0.0 {
                // Insta-mine: skip StartMiningRequest, fire immediately.
                mine_writer.write(MineBlockRequest { pos, tool });
                return;
            }

            start_writer.write(StartMiningRequest { pos, tool });
            *state = MiningState::Mining {
                pos,
                progress: 0.0,
                required_duration: duration,
            };
        }

        MiningState::Mining {
            pos: mining_pos,
            progress,
            required_duration,
        } => {
            // Abort if button released or crosshair moved.
            let same_target = targeted.pos == Some(mining_pos);
            if !left_held || !same_target {
                abort_writer.write(AbortMiningRequest { pos: mining_pos });
                *state = MiningState::Idle;
                return;
            }

            let new_progress = (progress + time.delta_secs() / required_duration).clamp(0.0, 1.0);

            if new_progress >= 1.0 {
                mine_writer.write(MineBlockRequest {
                    pos: mining_pos,
                    tool,
                });
                *state = MiningState::Idle;
            } else {
                *state = MiningState::Mining {
                    pos: mining_pos,
                    progress: new_progress,
                    required_duration,
                };
            }
        }
    }
}

/// Applies confirmed block removals to the local [`ChunkCache`].
///
/// Runs in `PostUpdate` and is gated only on [`AppState::Playing`] — not on
/// [`GameState::Running`] — so that paused clients still receive block
/// removals from other players on the server.
pub(super) fn apply_removed_blocks(
    mut reader: MessageReader<BlockRemoved>,
    mut cache: ResMut<ChunkCache>,
) {
    for removed in reader.read() {
        let chunk_pos = removed.pos.chunk_pos();
        let local = removed.pos.chunk_local();

        if local.y < 0 {
            continue;
        }

        let Some(chunk) = cache.get_mut(&chunk_pos) else {
            continue;
        };

        chunk.set(
            local.x as usize,
            local.y as usize,
            local.z as usize,
            Block::new(BlockId::AIR),
        );

        debug!("Applied confirmed removal at {}", removed.pos);
    }
}
