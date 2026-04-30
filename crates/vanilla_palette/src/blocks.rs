//! Vanilla block definitions.
//!
//! Provides [`VanillaBlocksPlugin`] (registers in [`BlockRegistrySet`]) and
//! the [`VanillaBlocks`] constant struct for access to vanilla [`BlockId`]
//! values elsewhere.
//!
//! # Block IDs
//!
//! | Constant              | ID |
//! |-----------------------|----|
//! | `VanillaBlocks::AIR`  | 0  |
//! | `VanillaBlocks::STONE`| 1  |
//! | `VanillaBlocks::DIRT` | 2  |
//! | `VanillaBlocks::GRASS`| 3  |
//! | `VanillaBlocks::SAND` | 4  |
//! | `VanillaBlocks::WOOD` | 5  |
//! | `VanillaBlocks::LEAVES`| 6 |
//!
//! Custom-crate blocks should start at ID `1000` to leave room for future
//! vanilla additions.
//!
//! [`BlockRegistrySet`]: dd40_core::block::registry::BlockRegistrySet

use bevy::prelude::*;
use dd40_core::{
    block::{BlockId, registry::{BlockDefinition, BlockRegistrySet}},
    prelude::*,
};

use crate::tools::VanillaToolKinds;

// ── Constants ─────────────────────────────────────────────────────────────────

/// [`BlockId`] constants for the vanilla blocks.
pub struct VanillaBlocks;

impl VanillaBlocks {
    /// Air — the engine invariant (registered by `CorePlugin`, re-exported here
    /// for convenience).
    pub const AIR: BlockId = BlockId(0);
    /// Stone — mined with a pickaxe.
    pub const STONE: BlockId = BlockId(1);
    /// Dirt — mined with a shovel.
    pub const DIRT: BlockId = BlockId(2);
    /// Grass — mined with a shovel.
    pub const GRASS: BlockId = BlockId(3);
    /// Sand — mined with a shovel.
    pub const SAND: BlockId = BlockId(4);
    /// Wood (log) — mined with an axe.
    pub const WOOD: BlockId = BlockId(5);
    /// Leaves — mined with shears (or bare hands, slowly).
    pub const LEAVES: BlockId = BlockId(6);
}

// ── Registration system ───────────────────────────────────────────────────────

fn register_vanilla_blocks(mut registry: ResMut<BlockRegistry>, mut commands: Commands) {
    // Air (ID 0) is already registered by CorePlugin — skip it.

    registry.register(
        BlockDefinition::new(VanillaBlocks::STONE, "stone")
            .with_color(Color::srgb(0.5, 0.5, 0.5))
            .with_solid(true)
            .with_renderable(true)
            .with_toughness(1.5)
            .with_preferred_tool(VanillaToolKinds::PICKAXE),
        &mut commands,
    );

    registry.register(
        BlockDefinition::new(VanillaBlocks::DIRT, "dirt")
            .with_color(Color::srgb(0.6, 0.4, 0.2))
            .with_solid(true)
            .with_renderable(true)
            .with_toughness(0.5)
            .with_preferred_tool(VanillaToolKinds::SHOVEL),
        &mut commands,
    );

    registry.register(
        BlockDefinition::new(VanillaBlocks::GRASS, "grass")
            .with_color(Color::srgb(0.2, 0.8, 0.2))
            .with_solid(true)
            .with_renderable(true)
            .with_toughness(0.6)
            .with_preferred_tool(VanillaToolKinds::SHOVEL),
        &mut commands,
    );

    registry.register(
        BlockDefinition::new(VanillaBlocks::SAND, "sand")
            .with_color(Color::srgb(0.9, 0.85, 0.6))
            .with_solid(true)
            .with_renderable(true)
            .with_toughness(0.5)
            .with_preferred_tool(VanillaToolKinds::SHOVEL),
        &mut commands,
    );

    registry.register(
        BlockDefinition::new(VanillaBlocks::WOOD, "wood")
            .with_color(Color::srgb(0.55, 0.35, 0.2))
            .with_solid(true)
            .with_renderable(true)
            .with_toughness(2.0)
            .with_preferred_tool(VanillaToolKinds::AXE),
        &mut commands,
    );

    registry.register(
        BlockDefinition::new(VanillaBlocks::LEAVES, "leaves")
            .with_color(Color::srgb(0.1, 0.6, 0.1))
            .with_solid(true)
            .with_renderable(true)
            .with_toughness(0.2)
            .with_preferred_tool(VanillaToolKinds::SHEARS),
        &mut commands,
    );
}

// ── Plugin ────────────────────────────────────────────────────────────────────

/// Plugin that registers all vanilla block types during [`BlockRegistrySet`].
///
/// Added automatically by [`VanillaPalettePlugin`]; you can also add it
/// directly if you only want the vanilla blocks without the vanilla tools.
///
/// [`VanillaPalettePlugin`]: crate::VanillaPalettePlugin
pub struct VanillaBlocksPlugin;

impl Plugin for VanillaBlocksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_vanilla_blocks.in_set(BlockRegistrySet));
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::tools::ToolRegistry;

    fn make_test_app() -> App {
        let mut app = App::new();
        app.insert_resource(BlockRegistry::new());
        app.insert_resource(ToolRegistry::new());
        // Register shears kind so preferred_tool references are valid
        app.add_systems(Startup, register_vanilla_blocks.in_set(BlockRegistrySet));
        app.configure_sets(Startup, BlockRegistrySet);
        app
    }

    #[test]
    fn vanilla_blocks_registered() {
        let mut app = make_test_app();
        app.update();

        let registry = app.world().resource::<BlockRegistry>();

        let stone = registry.get(VanillaBlocks::STONE).unwrap();
        assert_eq!(stone.name, "stone");
        assert!(stone.is_solid);
        assert!(stone.is_renderable);
        assert!(stone.is_destructible);
        assert!((stone.toughness - 1.5).abs() < 1e-6);

        let leaves = registry.get(VanillaBlocks::LEAVES).unwrap();
        assert_eq!(leaves.name, "leaves");
        assert!((leaves.toughness - 0.2).abs() < 1e-6);
    }

    #[test]
    fn all_vanilla_block_ids_exist() {
        let mut app = make_test_app();
        app.update();

        let registry = app.world().resource::<BlockRegistry>();
        for id in [
            VanillaBlocks::AIR,
            VanillaBlocks::STONE,
            VanillaBlocks::DIRT,
            VanillaBlocks::GRASS,
            VanillaBlocks::SAND,
            VanillaBlocks::WOOD,
            VanillaBlocks::LEAVES,
        ] {
            assert!(registry.get(id).is_some(), "Block {:?} not registered", id);
        }
    }

    #[test]
    fn air_is_not_destructible() {
        let registry = BlockRegistry::new();
        let air = registry.get(VanillaBlocks::AIR).unwrap();
        assert!(!air.is_destructible);
    }
}
