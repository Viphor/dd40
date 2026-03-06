use crate::{BlockDefinition, BlockId, BlockRegistry};
use bevy::prelude::*;

/// Block IDs for vanilla (built-in) blocks.
pub struct VanillaBlocks {
    pub air: BlockId,
    pub stone: BlockId,
    pub dirt: BlockId,
    pub grass: BlockId,
    pub sand: BlockId,
    pub wood: BlockId,
    pub leaves: BlockId,
}

impl VanillaBlocks {
    /// Standard vanilla block IDs.
    pub const AIR: BlockId = BlockId(0);
    pub const STONE: BlockId = BlockId(1);
    pub const DIRT: BlockId = BlockId(2);
    pub const GRASS: BlockId = BlockId(3);
    pub const SAND: BlockId = BlockId(4);
    pub const WOOD: BlockId = BlockId(5);
    pub const LEAVES: BlockId = BlockId(6);
}

/// Registers all vanilla (default) block types into the registry.
/// This should be called during app startup.
pub fn register_vanilla_blocks(registry: &mut BlockRegistry) -> VanillaBlocks {
    // Air is already registered by default in BlockRegistry::new()
    info!("Registering vanilla blocks");

    registry.register(
        BlockDefinition::new(VanillaBlocks::STONE, "stone")
            .with_color(Color::srgb(0.5, 0.5, 0.5))
            .with_solid(true)
            .with_renderable(true),
    );

    registry.register(
        BlockDefinition::new(VanillaBlocks::DIRT, "dirt")
            .with_color(Color::srgb(0.6, 0.4, 0.2))
            .with_solid(true)
            .with_renderable(true),
    );

    registry.register(
        BlockDefinition::new(VanillaBlocks::GRASS, "grass")
            .with_color(Color::srgb(0.2, 0.8, 0.2))
            .with_solid(true)
            .with_renderable(true),
    );

    registry.register(
        BlockDefinition::new(VanillaBlocks::SAND, "sand")
            .with_color(Color::srgb(0.9, 0.85, 0.6))
            .with_solid(true)
            .with_renderable(true),
    );

    registry.register(
        BlockDefinition::new(VanillaBlocks::WOOD, "wood")
            .with_color(Color::srgb(0.55, 0.35, 0.2))
            .with_solid(true)
            .with_renderable(true),
    );

    registry.register(
        BlockDefinition::new(VanillaBlocks::LEAVES, "leaves")
            .with_color(Color::srgb(0.1, 0.6, 0.1))
            .with_solid(true)
            .with_renderable(true),
    );

    VanillaBlocks {
        air: VanillaBlocks::AIR,
        stone: VanillaBlocks::STONE,
        dirt: VanillaBlocks::DIRT,
        grass: VanillaBlocks::GRASS,
        sand: VanillaBlocks::SAND,
        wood: VanillaBlocks::WOOD,
        leaves: VanillaBlocks::LEAVES,
    }
}

/// Bevy startup system that registers vanilla blocks.
pub fn setup_vanilla_blocks(mut registry: ResMut<BlockRegistry>) {
    register_vanilla_blocks(&mut registry);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vanilla_blocks_registered() {
        let mut registry = BlockRegistry::new();
        let vanilla = register_vanilla_blocks(&mut registry);

        assert_eq!(vanilla.stone, VanillaBlocks::STONE);

        let stone = registry.get(vanilla.stone).unwrap();
        assert_eq!(stone.name, "stone");
        assert!(stone.is_solid);
        assert!(stone.is_renderable);

        let grass = registry.get(vanilla.grass).unwrap();
        assert_eq!(grass.name, "grass");
        assert_eq!(grass.color, Color::srgb(0.2, 0.8, 0.2));
    }

    #[test]
    fn all_vanilla_blocks_exist() {
        let mut registry = BlockRegistry::new();
        let vanilla = register_vanilla_blocks(&mut registry);

        assert!(registry.get(vanilla.air).is_some());
        assert!(registry.get(vanilla.stone).is_some());
        assert!(registry.get(vanilla.dirt).is_some());
        assert!(registry.get(vanilla.grass).is_some());
        assert!(registry.get(vanilla.sand).is_some());
        assert!(registry.get(vanilla.wood).is_some());
        assert!(registry.get(vanilla.leaves).is_some());
    }
}
