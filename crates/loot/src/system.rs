//! The two systems that make up the loot pipeline.

use bevy::math::Vec3;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use dd40_core::block::registry::BlockRegistry;
use dd40_core::block::{BlockId, BlockPos};
use dd40_core::chunk::cache::ChunkCache;
use dd40_core::chunk::events::ChunkChanged;
use dd40_core::chunk::{BlockLocal, CellDataChange, ChunkChange, ChunkPos};
use dd40_inventory_core::block::BlockInventory;
use dd40_inventory_core::drop::DropItems;
use dd40_item_core::active_item::ItemStack;
use dd40_item_core::registry::ItemRegistry;
use dd40_loot_core::table::LootTable;
use dd40_rng::GameRng;

/// Per-frame snapshot of "what was here before the commit" for every
/// cell with a predicted [`ChunkChange::Remove`].
///
/// Filled by [`snapshot_remove_targets`] in
/// [`ChunkAuthoritySet::Validate`][dd40_core::chunk::authority::ChunkAuthoritySet::Validate]
/// and drained by [`emit_loot_drops`] in
/// [`LootSet::EmitDrops`](super::plugin::LootSet::EmitDrops) after
/// `ChunkChanged` has been published.
///
/// The map is keyed by `(ChunkPos, BlockLocal)` because that pair
/// uniquely identifies a cell across the whole world. Stale entries
/// for changes that were rejected are dropped at the end of
/// [`emit_loot_drops`].
#[derive(Resource, Default, Debug)]
pub struct PendingDropSnapshots {
    pub(crate) snapshots: HashMap<(ChunkPos, BlockLocal), RemoveSnapshot>,
}

/// One entry in [`PendingDropSnapshots`].
#[derive(Debug, Clone)]
pub(crate) struct RemoveSnapshot {
    /// Block id that was present before the commit.
    pub prior_block: BlockId,
    /// Snapshot of the cell's [`BlockInventory`] contents if any.
    pub inventory_stacks: Vec<ItemStack>,
    /// Whether the cell had a [`BlockInventory`] at all (used to
    /// decide whether to push a `Clear`).
    pub had_inventory: bool,
}

/// Snapshots prior state for every predicted `Remove` on a dirty
/// chunk. Runs in
/// [`ChunkAuthoritySet::Validate`][dd40_core::chunk::authority::ChunkAuthoritySet::Validate].
///
/// Read-only over [`ChunkCache`]; the resulting [`PendingDropSnapshots`]
/// is consumed later in the frame by [`emit_loot_drops`].
pub fn snapshot_remove_targets(
    cache: Res<ChunkCache>,
    mut snapshots: ResMut<PendingDropSnapshots>,
) {
    snapshots.snapshots.clear();
    for &chunk_pos in cache.dirty_chunks() {
        let Some(chunk) = cache.get(&chunk_pos) else {
            continue;
        };
        for entry in chunk.predicted() {
            let ChunkChange::Remove { local } = entry.change else {
                continue;
            };
            let prior_block = entry.prior.block_id;
            let inv = chunk.cell_data::<BlockInventory>(local);
            let (inventory_stacks, had_inventory) = match inv {
                Some(inv) => (collect_stacks(inv), true),
                None => (Vec::new(), false),
            };
            snapshots.snapshots.insert(
                (chunk_pos, local),
                RemoveSnapshot {
                    prior_block,
                    inventory_stacks,
                    had_inventory,
                },
            );
        }
    }
}

fn collect_stacks(inv: &BlockInventory) -> Vec<ItemStack> {
    inv.inventory()
        .slots()
        .iter()
        .filter_map(|s| s.as_ref().copied())
        .collect()
}

/// Reads accepted `ChunkChanged` deltas and emits one [`DropItems`]
/// per accepted [`ChunkChange::Remove`].
///
/// Drains [`PendingDropSnapshots`] entries that match the accepted
/// changes; any leftover snapshot entry (a snapshot whose predicted
/// `Remove` was rejected) is discarded at the end of the system.
pub fn emit_loot_drops(
    mut reader: MessageReader<ChunkChanged>,
    mut snapshots: ResMut<PendingDropSnapshots>,
    mut cache: ResMut<ChunkCache>,
    block_registry: Res<BlockRegistry>,
    item_registry: Res<ItemRegistry>,
    mut rng: ResMut<GameRng>,
    mut drops: MessageWriter<DropItems>,
) {
    for ev in reader.read() {
        for change in &ev.changes {
            let ChunkChange::Remove { local } = change else {
                continue;
            };
            let Some(snapshot) = snapshots.snapshots.remove(&(ev.pos, *local)) else {
                continue;
            };

            let mut stacks = resolve_loot(
                &cache,
                ev.pos,
                *local,
                snapshot.prior_block,
                &block_registry,
                &item_registry,
                rng.as_mut(),
            );

            stacks.extend(snapshot.inventory_stacks);

            if !stacks.is_empty() {
                let bp = ev.pos.block_pos(*local);
                drops.write(DropItems {
                    origin: block_centre(bp),
                    velocity: Vec3::ZERO,
                    stacks,
                });
            }

            if snapshot.had_inventory {
                cache.push_predicted_cell_data(
                    ev.pos,
                    CellDataChange::new_clear::<BlockInventory>(*local),
                );
            }
        }
    }
    snapshots.snapshots.clear();
}

fn resolve_loot(
    cache: &ChunkCache,
    chunk_pos: ChunkPos,
    local: BlockLocal,
    prior_block: BlockId,
    block_registry: &BlockRegistry,
    item_registry: &ItemRegistry,
    rng: &mut dyn rand::RngCore,
) -> Vec<ItemStack> {
    let block_pos = chunk_pos.block_pos(local);
    if let Some(table) = cache.block_data::<LootTable>(block_pos) {
        return table.roll(rng);
    }
    if let Some(table) = block_registry.block_data::<LootTable>(prior_block) {
        return table.roll(rng);
    }
    fallback_drop(prior_block, item_registry)
}

fn fallback_drop(prior_block: BlockId, item_registry: &ItemRegistry) -> Vec<ItemStack> {
    if prior_block == BlockId::AIR {
        return Vec::new();
    }
    item_registry
        .iter()
        .find(|def| def.placeable == Some(prior_block))
        .map(|def| vec![ItemStack::single(def.id)])
        .unwrap_or_default()
}

fn block_centre(pos: BlockPos) -> Vec3 {
    Vec3::new(pos.x as f32 + 0.5, pos.y as f32 + 0.5, pos.z as f32 + 0.5)
}
