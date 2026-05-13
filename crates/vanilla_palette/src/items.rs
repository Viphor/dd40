//! Vanilla item definitions.
//!
//! Provides [`VanillaItemsPlugin`] (registers in [`ItemRegistrySet`]) and the
//! [`VanillaItems`] constant struct for access to vanilla [`ItemId`] values
//! elsewhere.
//!
//! # ID layout
//!
//! | Range       | Contents                               |
//! |-------------|----------------------------------------|
//! | `1..=99`    | Hand-placeable block items             |
//! | `100..=124` | Tools (5 kinds × 5 tiers, fixed order) |
//!
//! IDs are assigned deterministically so save files and network protocols
//! remain stable across builds.  The tool matrix is laid out
//! tier-major (`PICKAXE` of all tiers, then `AXE` of all tiers, ...) so a
//! later tier addition slots in cleanly without disturbing existing IDs.
//!
//! [`ItemRegistrySet`]: dd40_item_core::registry::ItemRegistrySet

use std::num::NonZero;

use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_item_core::plugin::ItemCorePlugin;
use dd40_item_core::registry::{ItemDefinition, ItemId, ItemRegistry, ItemRegistrySet};

use crate::blocks::VanillaBlocks;
use crate::tools::{VanillaToolKinds, VanillaToolTiers};

// ── Constants ─────────────────────────────────────────────────────────────────

/// [`ItemId`] constants for the vanilla items.
///
/// IDs are stable; new items must be appended (or inserted into a documented
/// gap) to avoid breaking existing save files.
pub struct VanillaItems;

impl VanillaItems {
    // Placeable block items (1..=6).
    /// Stone block item — places [`VanillaBlocks::STONE`].
    pub const STONE: ItemId = ItemId(1);
    /// Dirt block item — places [`VanillaBlocks::DIRT`].
    pub const DIRT: ItemId = ItemId(2);
    /// Grass block item — places [`VanillaBlocks::GRASS`].
    pub const GRASS: ItemId = ItemId(3);
    /// Sand block item — places [`VanillaBlocks::SAND`].
    pub const SAND: ItemId = ItemId(4);
    /// Wood (log) block item — places [`VanillaBlocks::WOOD`].
    pub const WOOD: ItemId = ItemId(5);
    /// Leaves block item — places [`VanillaBlocks::LEAVES`].
    pub const LEAVES: ItemId = ItemId(6);

    // Tool items (100..=124). Layout is kind-major: 5 tiers per kind,
    // in the order WOOD, STONE, IRON, DIAMOND, GOLD.

    // Pickaxes (100..=104).
    /// Wooden pickaxe.
    pub const WOODEN_PICKAXE: ItemId = ItemId(100);
    /// Stone pickaxe.
    pub const STONE_PICKAXE: ItemId = ItemId(101);
    /// Iron pickaxe.
    pub const IRON_PICKAXE: ItemId = ItemId(102);
    /// Diamond pickaxe.
    pub const DIAMOND_PICKAXE: ItemId = ItemId(103);
    /// Golden pickaxe.
    pub const GOLDEN_PICKAXE: ItemId = ItemId(104);

    // Axes (105..=109).
    /// Wooden axe.
    pub const WOODEN_AXE: ItemId = ItemId(105);
    /// Stone axe.
    pub const STONE_AXE: ItemId = ItemId(106);
    /// Iron axe.
    pub const IRON_AXE: ItemId = ItemId(107);
    /// Diamond axe.
    pub const DIAMOND_AXE: ItemId = ItemId(108);
    /// Golden axe.
    pub const GOLDEN_AXE: ItemId = ItemId(109);

    // Shovels (110..=114).
    /// Wooden shovel.
    pub const WOODEN_SHOVEL: ItemId = ItemId(110);
    /// Stone shovel.
    pub const STONE_SHOVEL: ItemId = ItemId(111);
    /// Iron shovel.
    pub const IRON_SHOVEL: ItemId = ItemId(112);
    /// Diamond shovel.
    pub const DIAMOND_SHOVEL: ItemId = ItemId(113);
    /// Golden shovel.
    pub const GOLDEN_SHOVEL: ItemId = ItemId(114);

    // Hoes (115..=119).
    /// Wooden hoe.
    pub const WOODEN_HOE: ItemId = ItemId(115);
    /// Stone hoe.
    pub const STONE_HOE: ItemId = ItemId(116);
    /// Iron hoe.
    pub const IRON_HOE: ItemId = ItemId(117);
    /// Diamond hoe.
    pub const DIAMOND_HOE: ItemId = ItemId(118);
    /// Golden hoe.
    pub const GOLDEN_HOE: ItemId = ItemId(119);

    // Shears (120..=124). All five tiers are registered for symmetry; the
    // inventory crate decides which tiers are actually craftable.
    /// Wooden shears.
    pub const WOODEN_SHEARS: ItemId = ItemId(120);
    /// Stone shears.
    pub const STONE_SHEARS: ItemId = ItemId(121);
    /// Iron shears.
    pub const IRON_SHEARS: ItemId = ItemId(122);
    /// Diamond shears.
    pub const DIAMOND_SHEARS: ItemId = ItemId(123);
    /// Golden shears.
    pub const GOLDEN_SHEARS: ItemId = ItemId(124);
}

// ── Registration system ───────────────────────────────────────────────────────

/// Tier × kind matrix used to drive bulk registration.  The order here defines
/// the [`VanillaItems`] tool ID layout and must not change without bumping the
/// disk format.
const TOOL_TIERS: [(&str, dd40_core::tools::ToolTierId); 5] = [
    ("wooden", VanillaToolTiers::WOOD),
    ("stone", VanillaToolTiers::STONE),
    ("iron", VanillaToolTiers::IRON),
    ("diamond", VanillaToolTiers::DIAMOND),
    ("golden", VanillaToolTiers::GOLD),
];

const TOOL_KINDS: [(&str, dd40_core::tools::ToolKindId, ItemId); 5] = [
    (
        "pickaxe",
        VanillaToolKinds::PICKAXE,
        VanillaItems::WOODEN_PICKAXE,
    ),
    ("axe", VanillaToolKinds::AXE, VanillaItems::WOODEN_AXE),
    (
        "shovel",
        VanillaToolKinds::SHOVEL,
        VanillaItems::WOODEN_SHOVEL,
    ),
    ("hoe", VanillaToolKinds::HOE, VanillaItems::WOODEN_HOE),
    (
        "shears",
        VanillaToolKinds::SHEARS,
        VanillaItems::WOODEN_SHEARS,
    ),
];

const PLACEABLE_ITEMS: [(ItemId, &str, dd40_core::block::BlockId); 6] = [
    (VanillaItems::STONE, "stone", VanillaBlocks::STONE),
    (VanillaItems::DIRT, "dirt", VanillaBlocks::DIRT),
    (VanillaItems::GRASS, "grass", VanillaBlocks::GRASS),
    (VanillaItems::SAND, "sand", VanillaBlocks::SAND),
    (VanillaItems::WOOD, "wood", VanillaBlocks::WOOD),
    (VanillaItems::LEAVES, "leaves", VanillaBlocks::LEAVES),
];

fn register_vanilla_items(mut registry: ResMut<ItemRegistry>) {
    for (item_id, name, block_id) in PLACEABLE_ITEMS {
        registry.register(ItemDefinition::new(item_id, name).with_placeable(block_id));
    }

    for (kind_name, kind_id, base_item) in TOOL_KINDS {
        for (tier_index, (tier_name, tier_id)) in TOOL_TIERS.iter().enumerate() {
            let id = ItemId(base_item.0 + tier_index as u16);
            let name = format!("{tier_name}_{kind_name}");
            registry.register(
                ItemDefinition::new(id, name)
                    .with_max_stack(NonZero::<u16>::MIN)
                    .with_tool(kind_id, *tier_id),
            );
        }
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────────

/// Plugin that registers all vanilla items.
///
/// Auto-adds [`ItemCorePlugin`] via `ensure_plugins!`, so consumers only need
/// to add this plugin (and [`CorePlugin`][dd40_core::plugin::CorePlugin]
/// upstream) to get the full vanilla item palette.
#[derive(Default)]
pub struct VanillaItemsPlugin;

impl Plugin for VanillaItemsPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, ItemCorePlugin);
        app.add_systems(Startup, register_vanilla_items.in_set(ItemRegistrySet));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::plugin::CorePlugin;

    fn build_app() -> App {
        let mut app = App::new();
        app.add_plugins((CorePlugin, ItemCorePlugin, VanillaItemsPlugin));
        app.update();
        app
    }

    #[test]
    fn placeable_items_register_at_expected_ids() {
        let app = build_app();
        let registry = app.world().resource::<ItemRegistry>();
        for (id, name, block) in PLACEABLE_ITEMS {
            let def = registry.get(id).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(def.name, name);
            assert_eq!(def.placeable, Some(block));
            assert!(def.tool.is_none());
            assert_eq!(def.max_stack.get(), 64);
        }
    }

    #[test]
    fn tool_items_register_at_expected_ids() {
        let app = build_app();
        let registry = app.world().resource::<ItemRegistry>();
        for (kind_name, kind_id, base) in TOOL_KINDS {
            for (tier_index, (tier_name, tier_id)) in TOOL_TIERS.iter().enumerate() {
                let id = ItemId(base.0 + tier_index as u16);
                let def = registry
                    .get(id)
                    .unwrap_or_else(|| panic!("{tier_name}_{kind_name} missing"));
                assert_eq!(def.name, format!("{tier_name}_{kind_name}"));
                assert_eq!(def.max_stack.get(), 1);
                let tool = def.tool.expect("tool item has tool behaviour");
                assert_eq!(tool.kind, kind_id);
                assert_eq!(tool.tier, *tier_id);
                assert!(def.placeable.is_none());
            }
        }
    }

    #[test]
    fn registers_exactly_31_vanilla_items() {
        let app = build_app();
        let registry = app.world().resource::<ItemRegistry>();
        // 6 placeables + 25 tools = 31 entries up to ID 124, with the gap
        // (0, 7..=99) filled by `unknown_*` placeholders.
        let count = registry
            .iter()
            .filter(|d| !d.name.starts_with("unknown_"))
            .count();
        assert_eq!(
            count,
            PLACEABLE_ITEMS.len() + TOOL_KINDS.len() * TOOL_TIERS.len()
        );
    }
}
