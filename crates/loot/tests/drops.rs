//! Integration tests for [`LootPlugin`].

use bevy::MinimalPlugins;
use bevy::prelude::*;

use dd40_core::block::registry::BlockRegistry;
use dd40_core::block::{Block, BlockDefinition, BlockId};
use dd40_core::chunk::authority::ChunkAuthorityPlugin;
use dd40_core::chunk::cache::ChunkCache;
use dd40_core::chunk::{BlockLocal, Chunk, ChunkChange, ChunkPos};
use dd40_inventory_core::block::BlockInventory;
use dd40_inventory_core::drop::DropItems;
use dd40_inventory_core::inventory::Inventory;
use dd40_item_core::active_item::ItemStack;
use dd40_item_core::registry::{ItemDefinition, ItemId, ItemRegistry};
use dd40_loot::LootPlugin;
use dd40_loot_core::table::{LootEntry, LootTable};

const STONE: BlockId = BlockId(1);
const CHEST: BlockId = BlockId(2);
const COBBLESTONE_ITEM: ItemId = ItemId(1);
const CHEST_ITEM: ItemId = ItemId(2);
const APPLE_ITEM: ItemId = ItemId(3);

fn lp(x: u8, y: u16, z: u8) -> BlockLocal {
    BlockLocal::new(x, y, z)
}

fn build_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(LootPlugin);
    app.add_plugins(ChunkAuthorityPlugin);

    // Run Startup once so ItemCorePlugin etc. settle.
    app.update();

    {
        let world = app.world_mut();
        let mut block_reg = world.resource_mut::<BlockRegistry>();
        block_reg.register_without_event(BlockDefinition::new(STONE, "stone").with_data(
            LootTable::with_entries(vec![LootEntry::Fixed {
                item: COBBLESTONE_ITEM,
                count: 1,
            }]),
        ));
        block_reg.register_without_event(BlockDefinition::new(CHEST, "chest"));
    }
    {
        let world = app.world_mut();
        let mut item_reg = world.resource_mut::<ItemRegistry>();
        item_reg.register(ItemDefinition::new(COBBLESTONE_ITEM, "cobblestone"));
        item_reg.register(ItemDefinition::new(CHEST_ITEM, "chest").with_placeable(CHEST));
        item_reg.register(ItemDefinition::new(APPLE_ITEM, "apple"));
    }
    app
}

fn collect_drops(app: &App) -> Vec<DropItems> {
    app.world()
        .resource::<Messages<DropItems>>()
        .iter_current_update_messages()
        .cloned()
        .collect()
}

#[test]
fn breaks_block_with_definition_loot_table_drops_rolled_stacks() {
    let mut app = build_app();
    let pos = ChunkPos::new(0, 0, 0);
    {
        let mut cache = app.world_mut().resource_mut::<ChunkCache>();
        let mut chunk = Chunk::new(pos);
        chunk.set_local(lp(0, 0, 0), Block::new(STONE));
        cache.insert(chunk);
        cache.push_predicted(pos, ChunkChange::new_remove(lp(0, 0, 0)));
    }

    app.update();

    let drops = collect_drops(&app);
    assert_eq!(drops.len(), 1, "expected one DropItems");
    assert_eq!(drops[0].stacks, vec![ItemStack::single(COBBLESTONE_ITEM)]);
    assert_eq!(drops[0].velocity, Vec3::ZERO);
}

#[test]
fn breaks_block_without_loot_table_drops_placeable_item() {
    let mut app = build_app();
    let pos = ChunkPos::new(0, 0, 0);
    {
        let mut cache = app.world_mut().resource_mut::<ChunkCache>();
        let mut chunk = Chunk::new(pos);
        chunk.set_local(lp(1, 2, 3), Block::new(CHEST));
        cache.insert(chunk);
        cache.push_predicted(pos, ChunkChange::new_remove(lp(1, 2, 3)));
    }

    app.update();

    let drops = collect_drops(&app);
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].stacks, vec![ItemStack::single(CHEST_ITEM)]);
}

#[test]
fn breaks_block_with_block_inventory_appends_contents_and_clears_cell_data() {
    let mut app = build_app();
    let pos = ChunkPos::new(0, 0, 0);
    {
        let mut cache = app.world_mut().resource_mut::<ChunkCache>();
        let mut chunk = Chunk::new(pos);
        chunk.set_local(lp(0, 0, 0), Block::new(CHEST));
        let mut inv = Inventory::with_capacity(9);
        inv.set_slot(0, Some(ItemStack::single(APPLE_ITEM)));
        chunk.set_cell_data(lp(0, 0, 0), BlockInventory::from_inventory(inv));
        cache.insert(chunk);
        cache.push_predicted(pos, ChunkChange::new_remove(lp(0, 0, 0)));
    }

    app.update();

    let drops = collect_drops(&app);
    assert_eq!(drops.len(), 1);
    // Fallback (chest item) plus inventory contents (apple).
    assert!(drops[0].stacks.contains(&ItemStack::single(CHEST_ITEM)));
    assert!(drops[0].stacks.contains(&ItemStack::single(APPLE_ITEM)));

    // The cell-data clear is pushed as a predicted change; commit on the next
    // frame should evict the BlockInventory.
    app.update();
    let cache = app.world().resource::<ChunkCache>();
    let chunk = cache.get(&pos).expect("chunk present");
    assert!(chunk.cell_data::<BlockInventory>(lp(0, 0, 0)).is_none());
}

#[test]
fn rejected_remove_does_not_drop() {
    let mut app = build_app();
    let pos = ChunkPos::new(0, 0, 0);
    {
        let mut cache = app.world_mut().resource_mut::<ChunkCache>();
        cache.insert(Chunk::new(pos));
        // Cell is air (default).  Remove of air → default validator rejects
        // (air is not destructible).
        cache.push_predicted(pos, ChunkChange::new_remove(lp(0, 0, 0)));
    }

    app.update();

    let drops = collect_drops(&app);
    assert!(drops.is_empty(), "rejected remove must not drop: {drops:?}");
}
