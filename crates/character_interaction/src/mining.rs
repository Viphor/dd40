//! Per-character mining state machine, driven by [`CharacterInput::attack`].
//!
//! The state-machine logic itself is the pure function [`step_mining`], which
//! takes the current state, the character's targeted block, the current
//! `attack` flag, the elapsed delta, and a closure that resolves a block to
//! the mining duration the character would need to break it. The Bevy system
//! [`update_mining`] is a thin wrapper that wires queries and registries to
//! `step_mining` and emits the resulting messages.
//!
//! ## Transition table
//!
//! ```text
//! state          attack target            → action                   new state
//! ─────────────  ─────  ────────────────  ─────────────────────────  ────────────────
//! Idle           false  *                 nothing                    Idle
//! Idle           true   None              nothing                    Idle
//! Idle           true   Some(p) breakable Start                      Mining { p, 0 }
//! Idle           true   Some(p) instant   Mine                       Idle
//! Mining(mp)     false  *                 Abort(mp)                  Idle
//! Mining(mp)     true   Some(p) p == mp   tick progress              Mining(mp, +Δ)
//!                                          → Mine on completion      Idle
//! Mining(mp)     true   _ (different)     Abort(mp)                  Idle
//!                                          (next tick the Idle row picks
//!                                           up the new target automatically)
//! ```
//!
//! "Auto-continue while held" and "auto-restart on target change while held"
//! both fall out of the table without dedicated branches: completing a mine
//! or aborting on target-switch leaves the state in `Idle`, and the next
//! tick's `Idle, true, Some(p)` row picks up where the previous tick stopped.

use bevy::prelude::*;
use dd40_character_core::components::Character;
use dd40_character_core::controller::CharacterInput;
use dd40_character_core::targeted_block::TargetedBlock;
use dd40_core::{
    block::{
        Block, BlockId,
        events::{AbortMiningRequest, BlockRemoved, MineBlockRequest, StartMiningRequest},
    },
    chunk::cache::ChunkCache,
    prelude::*,
    tools::{ToolKindId, ToolTierId, mining_duration},
};
use dd40_item_core::active_item::ActiveItem;
use dd40_item_core::registry::ItemRegistry;

pub use dd40_character_core::mining_state::MiningState;

/// Side effects emitted by one [`step_mining`] tick.
///
/// At most one of [`Self::start`] / [`Self::mine`] / [`Self::abort`] is
/// `Some`. The pure state-machine returns these so the Bevy wrapper can
/// translate them into outgoing messages without `step_mining` itself
/// touching any Bevy types beyond [`BlockPos`].
#[derive(Debug, Default, Clone)]
pub(crate) struct MiningStep {
    /// New state to write back to the character's [`MiningState`] component.
    pub next_state: MiningState,
    /// `Some(pos)` when a [`StartMiningRequest`] should be sent.
    pub start: Option<BlockPos>,
    /// `Some(pos)` when a [`MineBlockRequest`] should be sent (instant-break
    /// from `Idle` or completion from `Mining`).
    pub mine: Option<BlockPos>,
    /// `Some(pos)` when an [`AbortMiningRequest`] should be sent.
    pub abort: Option<BlockPos>,
}

/// Pure mining state-machine step.
///
/// `duration_for` returns:
/// - `None` if the block at that position cannot be mined by this character
///   (replaceable, indestructible, or not in the registry).
/// - `Some(0.0)` for instant-break blocks.
/// - `Some(d > 0.0)` otherwise.
pub(crate) fn step_mining(
    state: MiningState,
    targeted: &TargetedBlock,
    attack: bool,
    delta_secs: f32,
    duration_for: impl Fn(BlockPos, BlockId) -> Option<f32>,
) -> MiningStep {
    match state {
        MiningState::Idle => {
            if !attack {
                return MiningStep::default();
            }
            let (Some(pos), Some(block_id)) = (targeted.pos, targeted.block_id) else {
                return MiningStep::default();
            };
            let Some(duration) = duration_for(pos, block_id) else {
                return MiningStep::default();
            };
            if duration <= 0.0 {
                return MiningStep {
                    next_state: MiningState::Idle,
                    mine: Some(pos),
                    ..default()
                };
            }
            MiningStep {
                next_state: MiningState::Mining {
                    pos,
                    progress: 0.0,
                    required_duration: duration,
                },
                start: Some(pos),
                ..default()
            }
        }

        MiningState::Mining {
            pos: mining_pos,
            progress,
            required_duration,
        } => {
            let same_target = targeted.pos == Some(mining_pos);
            if !attack || !same_target {
                return MiningStep {
                    next_state: MiningState::Idle,
                    abort: Some(mining_pos),
                    ..default()
                };
            }
            let new_progress =
                (progress + delta_secs / required_duration).clamp(0.0, 1.0);
            if new_progress >= 1.0 {
                MiningStep {
                    next_state: MiningState::Idle,
                    mine: Some(mining_pos),
                    ..default()
                }
            } else {
                MiningStep {
                    next_state: MiningState::Mining {
                        pos: mining_pos,
                        progress: new_progress,
                        required_duration,
                    },
                    ..default()
                }
            }
        }
    }
}

/// Updates each character's [`MiningState`] from its [`CharacterInput::attack`]
/// flag and emits mining request messages.
///
/// Multi-character clients are out of scope: the system iterates every
/// character that has the relevant components, but the gating "which
/// character holds the local attack input this frame" is the input layer's
/// responsibility (see `dd40_player_movement`'s mouse-to-input translation).
///
/// # Tool source
///
/// The tool kind and tier are resolved from the character's [`ActiveItem`]
/// via [`ItemRegistry`]. A character with no [`ActiveItem`], with
/// `ActiveItem(None)`, or whose item has no
/// [`tool`][dd40_item_core::registry::ItemDefinition::tool] field is treated
/// as bare hands ([`ToolKindId::NONE`] / [`ToolTierId::DEFAULT`]).
pub(crate) fn update_mining(
    mut character_query: Query<
        (
            &CharacterInput,
            &TargetedBlock,
            &mut MiningState,
            Option<&ActiveItem>,
        ),
        With<Character>,
    >,
    registry: Res<BlockRegistry>,
    tool_registry: Res<ToolRegistry>,
    items: Res<ItemRegistry>,
    time: Res<Time>,
    mut start_writer: MessageWriter<StartMiningRequest>,
    mut abort_writer: MessageWriter<AbortMiningRequest>,
    mut mine_writer: MessageWriter<MineBlockRequest>,
) {
    let dt = time.delta_secs();
    for (input, targeted, mut state, active) in &mut character_query {
        let (tool_kind, tool_tier) = active_tool(active, &items);

        let duration_for = |_pos: BlockPos, block_id: BlockId| -> Option<f32> {
            let block = Block::new(block_id);
            if registry.is_replaceable(&block) {
                return None;
            }
            let block_def = registry.get(block_id)?;
            if !block_def.is_destructible {
                return None;
            }
            mining_duration(block_def, tool_kind, tool_tier, &tool_registry)
        };

        let step = step_mining(state.clone(), targeted, input.attack, dt, duration_for);

        if let Some(pos) = step.start {
            start_writer.write(StartMiningRequest {
                pos,
                tool_kind,
                tool_tier,
            });
        }
        if let Some(pos) = step.mine {
            mine_writer.write(MineBlockRequest {
                pos,
                tool_kind,
                tool_tier,
            });
        }
        if let Some(pos) = step.abort {
            abort_writer.write(AbortMiningRequest { pos });
        }

        *state = step.next_state;
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

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_character_core::targeted_block::BlockFace;

    fn target(pos: Option<BlockPos>) -> TargetedBlock {
        TargetedBlock {
            pos,
            face: pos.map(|_| BlockFace::Top),
            block_id: pos.map(|_| BlockId(1)),
        }
    }

    fn breakable(_pos: BlockPos, _id: BlockId) -> Option<f32> {
        Some(2.0)
    }
    fn instant(_pos: BlockPos, _id: BlockId) -> Option<f32> {
        Some(0.0)
    }
    fn unbreakable(_pos: BlockPos, _id: BlockId) -> Option<f32> {
        None
    }

    #[test]
    fn idle_with_attack_false_stays_idle() {
        let s = step_mining(MiningState::Idle, &target(None), false, 0.016, breakable);
        assert!(matches!(s.next_state, MiningState::Idle));
        assert_eq!(s.start, None);
        assert_eq!(s.mine, None);
        assert_eq!(s.abort, None);
    }

    #[test]
    fn idle_with_attack_true_no_target_stays_idle() {
        let s = step_mining(MiningState::Idle, &target(None), true, 0.016, breakable);
        assert!(matches!(s.next_state, MiningState::Idle));
        assert_eq!(s.start, None);
    }

    #[test]
    fn idle_with_attack_true_breakable_target_starts_mining() {
        let pos = BlockPos::new(1, 2, 3);
        let s = step_mining(MiningState::Idle, &target(Some(pos)), true, 0.016, breakable);
        let MiningState::Mining {
            pos: p,
            progress,
            required_duration,
        } = s.next_state
        else {
            panic!("expected Mining, got {:?}", s.next_state);
        };
        assert_eq!(p, pos);
        assert_eq!(progress, 0.0);
        assert_eq!(required_duration, 2.0);
        assert_eq!(s.start, Some(pos));
        assert_eq!(s.mine, None);
    }

    #[test]
    fn idle_with_instant_break_target_emits_mine_and_stays_idle() {
        let pos = BlockPos::new(0, 0, 0);
        let s = step_mining(MiningState::Idle, &target(Some(pos)), true, 0.016, instant);
        assert!(matches!(s.next_state, MiningState::Idle));
        assert_eq!(s.mine, Some(pos));
        assert_eq!(s.start, None);
    }

    #[test]
    fn idle_with_unbreakable_target_stays_idle() {
        let pos = BlockPos::new(0, 0, 0);
        let s = step_mining(MiningState::Idle, &target(Some(pos)), true, 0.016, unbreakable);
        assert!(matches!(s.next_state, MiningState::Idle));
        assert_eq!(s.start, None);
        assert_eq!(s.mine, None);
    }

    #[test]
    fn mining_with_attack_released_aborts() {
        let pos = BlockPos::new(0, 0, 0);
        let state = MiningState::Mining {
            pos,
            progress: 0.5,
            required_duration: 2.0,
        };
        let s = step_mining(state, &target(Some(pos)), false, 0.016, breakable);
        assert!(matches!(s.next_state, MiningState::Idle));
        assert_eq!(s.abort, Some(pos));
    }

    #[test]
    fn mining_with_held_attack_and_same_target_progresses() {
        let pos = BlockPos::new(0, 0, 0);
        let state = MiningState::Mining {
            pos,
            progress: 0.0,
            required_duration: 2.0,
        };
        let s = step_mining(state, &target(Some(pos)), true, 1.0, breakable);
        let MiningState::Mining { progress, .. } = s.next_state else {
            panic!()
        };
        assert!((progress - 0.5).abs() < 1e-5, "progress = {progress}");
        assert_eq!(s.mine, None);
        assert_eq!(s.abort, None);
    }

    #[test]
    fn mining_completes_when_progress_reaches_one() {
        let pos = BlockPos::new(0, 0, 0);
        let state = MiningState::Mining {
            pos,
            progress: 0.9,
            required_duration: 1.0,
        };
        let s = step_mining(state, &target(Some(pos)), true, 0.5, breakable);
        assert!(matches!(s.next_state, MiningState::Idle));
        assert_eq!(s.mine, Some(pos));
    }

    #[test]
    fn mining_aborts_on_target_change() {
        let mining_pos = BlockPos::new(0, 0, 0);
        let new_target = BlockPos::new(1, 0, 0);
        let state = MiningState::Mining {
            pos: mining_pos,
            progress: 0.5,
            required_duration: 2.0,
        };
        let s = step_mining(state, &target(Some(new_target)), true, 0.016, breakable);
        assert!(matches!(s.next_state, MiningState::Idle));
        assert_eq!(s.abort, Some(mining_pos));
        assert_eq!(s.start, None, "abort and start are separate ticks");
    }

    #[test]
    fn target_switch_then_next_tick_restarts_on_new_target() {
        let mining_pos = BlockPos::new(0, 0, 0);
        let new_target = BlockPos::new(1, 0, 0);
        let state = MiningState::Mining {
            pos: mining_pos,
            progress: 0.5,
            required_duration: 2.0,
        };
        // Tick 1: target changed while attack still held → abort.
        let s1 = step_mining(state, &target(Some(new_target)), true, 0.016, breakable);
        assert_eq!(s1.abort, Some(mining_pos));
        // Tick 2: now Idle + attack still held + new target → start.
        let s2 = step_mining(s1.next_state, &target(Some(new_target)), true, 0.016, breakable);
        assert_eq!(s2.start, Some(new_target));
    }

    #[test]
    fn completion_with_held_attack_then_next_tick_restarts() {
        let pos = BlockPos::new(0, 0, 0);
        let new_target = BlockPos::new(1, 0, 0);
        let state = MiningState::Mining {
            pos,
            progress: 0.99,
            required_duration: 1.0,
        };
        // Tick 1: completes mine → Idle, mine emitted.
        let s1 = step_mining(state, &target(Some(pos)), true, 0.5, breakable);
        assert_eq!(s1.mine, Some(pos));
        assert!(matches!(s1.next_state, MiningState::Idle));
        // Tick 2: still holding attack on a different target → starts fresh.
        let s2 = step_mining(s1.next_state, &target(Some(new_target)), true, 0.016, breakable);
        assert_eq!(s2.start, Some(new_target));
    }

    #[test]
    fn empty_target_to_valid_target_while_held_starts_mining() {
        // Tick 1: looking at nothing while attack held → idle.
        let s1 = step_mining(MiningState::Idle, &target(None), true, 0.016, breakable);
        assert!(matches!(s1.next_state, MiningState::Idle));
        assert_eq!(s1.start, None);
        // Tick 2: now looking at a valid block → start.
        let pos = BlockPos::new(2, 2, 2);
        let s2 = step_mining(s1.next_state, &target(Some(pos)), true, 0.016, breakable);
        assert_eq!(s2.start, Some(pos));
    }
}
