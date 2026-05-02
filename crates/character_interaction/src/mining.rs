use bevy::prelude::*;
use dd40_character_core::components::Character;
use dd40_core::{
    block::{Block, BlockId, events::{AbortMiningRequest, BlockRemoved, MineBlockRequest, StartMiningRequest}},
    chunk::cache::ChunkCache,
    prelude::*,
    tools::{EquippedTool, mining_duration},
};

use crate::targeting::TargetedBlock;

pub use dd40_character_core::mining_state::MiningState;

/// Updates mining state each frame and emits mining request messages.
///
/// Runs for any entity with [`Character`] + [`EquippedTool`] (or bare-hands
/// fallback). The gate for `Controller` vs `FreeCam` mode is the caller's
/// responsibility (typically wired in the `dd40_player` wrapper).
pub(crate) fn update_mining(
    mouse: Res<ButtonInput<MouseButton>>,
    targeted: Res<TargetedBlock>,
    equipped_query: Query<&EquippedTool, With<Character>>,
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
            let Some(block_id) = targeted.block_id else { return };
            let block = Block::new(block_id);

            if registry.is_replaceable(&block) {
                return;
            }
            let Some(block_def) = registry.get(block_id) else { return };
            if !block_def.is_destructible {
                return;
            }
            let Some(duration) = mining_duration(block_def, &tool, &tool_registry) else { return };

            if duration <= 0.0 {
                mine_writer.write(MineBlockRequest { pos, tool });
                return;
            }

            start_writer.write(StartMiningRequest { pos, tool });
            *state = MiningState::Mining { pos, progress: 0.0, required_duration: duration };
        }

        MiningState::Mining { pos: mining_pos, progress, required_duration } => {
            let same_target = targeted.pos == Some(mining_pos);
            if !left_held || !same_target {
                abort_writer.write(AbortMiningRequest { pos: mining_pos });
                *state = MiningState::Idle;
                return;
            }

            let new_progress = (progress + time.delta_secs() / required_duration).clamp(0.0, 1.0);
            if new_progress >= 1.0 {
                mine_writer.write(MineBlockRequest { pos: mining_pos, tool });
                *state = MiningState::Idle;
            } else {
                *state = MiningState::Mining { pos: mining_pos, progress: new_progress, required_duration };
            }
        }
    }
}

/// Applies confirmed block removals to the local [`ChunkCache`].
///
/// Gated only on [`AppState::Playing`] (not [`GameState::Running`]) so that
/// paused clients still receive removals from other players.
pub(crate) fn apply_removed_blocks(
    mut reader: MessageReader<BlockRemoved>,
    mut cache: ResMut<ChunkCache>,
) {
    for removed in reader.read() {
        let chunk_pos = removed.pos.chunk_pos();
        let local = removed.pos.chunk_local();
        if local.y < 0 {
            continue;
        }
        let Some(chunk) = cache.get_mut(&chunk_pos) else { continue };
        chunk.set(local.x as usize, local.y as usize, local.z as usize, Block::new(BlockId::AIR));
        debug!("Applied confirmed removal at {}", removed.pos);
    }
}
