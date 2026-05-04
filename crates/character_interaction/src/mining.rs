use bevy::prelude::*;
use dd40_character_core::components::Character;
use dd40_character_core::targeted_block::TargetedBlock;
use dd40_core::{
    block::{Block, BlockId, events::{AbortMiningRequest, BlockRemoved, MineBlockRequest, StartMiningRequest}},
    chunk::cache::ChunkCache,
    prelude::*,
    tools::{ToolKindId, ToolTierId, mining_duration},
};
use dd40_item_core::active_item::ActiveItem;
use dd40_item_core::registry::ItemRegistry;

pub use dd40_character_core::mining_state::MiningState;

/// Updates mining state each frame and emits mining request messages.
///
/// Runs for the single [`Character`] entity that owns its [`MiningState`]
/// component.  When no character exists, the system is a no-op.
/// Multi-character clients are out of scope: gating "which character owns
/// the local mouse" belongs to a wrapper plugin (currently `dd40_player`).
///
/// # Tool source
///
/// The tool kind and tier are resolved from the character's [`ActiveItem`]
/// via [`ItemRegistry`].  A character with no [`ActiveItem`], with
/// `ActiveItem(None)`, or whose item has no
/// [`tool`][dd40_item_core::registry::ItemDefinition::tool] field is treated
/// as bare hands ([`ToolKindId::NONE`] / [`ToolTierId::DEFAULT`]).
pub(crate) fn update_mining(
    mouse: Res<ButtonInput<MouseButton>>,
    mut character_query: Query<(&TargetedBlock, &mut MiningState, Option<&ActiveItem>), With<Character>>,
    registry: Res<BlockRegistry>,
    tool_registry: Res<ToolRegistry>,
    items: Res<ItemRegistry>,
    time: Res<Time>,
    mut start_writer: MessageWriter<StartMiningRequest>,
    mut abort_writer: MessageWriter<AbortMiningRequest>,
    mut mine_writer: MessageWriter<MineBlockRequest>,
) {
    let Some((targeted, mut state, active)) = character_query.iter_mut().next() else {
        return;
    };
    let (tool_kind, tool_tier) = active_tool(active, &items);

    let left_held = mouse.pressed(MouseButton::Left);
    let left_just_pressed = mouse.just_pressed(MouseButton::Left);

    match state.clone() {
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
            let Some(duration) = mining_duration(block_def, tool_kind, tool_tier, &tool_registry) else { return };

            if duration <= 0.0 {
                mine_writer.write(MineBlockRequest { pos, tool_kind, tool_tier });
                return;
            }

            start_writer.write(StartMiningRequest { pos, tool_kind, tool_tier });
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
                mine_writer.write(MineBlockRequest { pos: mining_pos, tool_kind, tool_tier });
                *state = MiningState::Idle;
            } else {
                *state = MiningState::Mining { pos: mining_pos, progress: new_progress, required_duration };
            }
        }
    }
}

/// Resolves a character's effective tool kind and tier from its [`ActiveItem`].
///
/// Returns `(ToolKindId::NONE, ToolTierId::DEFAULT)` (bare hands) when the
/// character has no item, no [`ActiveItem`] component, or holds an item that
/// is not a tool.
fn active_tool(active: Option<&ActiveItem>, items: &ItemRegistry) -> (ToolKindId, ToolTierId) {
    let Some(stack) = active.and_then(|a| a.0) else {
        return (ToolKindId::NONE, ToolTierId::DEFAULT);
    };
    match items.get(stack.item).and_then(|def| def.tool) {
        Some(tool) => (tool.kind, tool.tier),
        None => (ToolKindId::NONE, ToolTierId::DEFAULT),
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
