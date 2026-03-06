use bevy::prelude::*;

/// System set for block registration systems.
/// All block registrations should run in this set during Startup.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockRegistrySet;

/// System set for world generation systems.
/// All world generation should run in this set, after BlockRegistrySet.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorldGenerationSet;

pub mod vanilla_blocks;
pub use vanilla_blocks::{register_vanilla_blocks, setup_vanilla_blocks, VanillaBlocks};

/// A unique identifier for a block type.
/// Uses a u16 to allow up to 65,536 different block types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component, Reflect)]
pub struct BlockId(pub u16);

impl BlockId {
    /// Air block (ID 0) - always registered by default.
    pub const AIR: BlockId = BlockId(0);
}

/// Definition of a block type, containing all its properties.
#[derive(Debug, Clone, Reflect)]
pub struct BlockDefinition {
    /// Unique identifier for this block type.
    pub id: BlockId,
    /// Human-readable name for this block.
    pub name: String,
    /// Whether this block is solid (blocks movement/light).
    pub is_solid: bool,
    /// Whether this block should be rendered.
    pub is_renderable: bool,
    /// Color to use for rendering (placeholder until textures are added).
    pub color: Color,
}

impl BlockDefinition {
    /// Creates a new block definition.
    pub fn new(id: BlockId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            is_solid: true,
            is_renderable: true,
            color: Color::WHITE,
        }
    }

    /// Sets whether this block is solid.
    pub fn with_solid(mut self, is_solid: bool) -> Self {
        self.is_solid = is_solid;
        self
    }

    /// Sets whether this block is renderable.
    pub fn with_renderable(mut self, is_renderable: bool) -> Self {
        self.is_renderable = is_renderable;
        self
    }

    /// Sets the color for this block.
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

/// Registry that stores all registered block types.
#[derive(Resource, Default, Reflect)]
pub struct BlockRegistry {
    blocks: Vec<BlockDefinition>,
}

impl BlockRegistry {
    /// Creates a new block registry with the default Air block.
    pub fn new() -> Self {
        let mut registry = Self { blocks: Vec::new() };

        // Always register Air as the first block (ID 0)
        registry.register(
            BlockDefinition::new(BlockId::AIR, "air")
                .with_solid(false)
                .with_renderable(false)
                .with_color(Color::srgba(0.0, 0.0, 0.0, 0.0)),
        );

        registry
    }

    /// Registers a new block type. Returns the assigned BlockId.
    /// If a block with the same ID already exists, it will be replaced.
    pub fn register(&mut self, definition: BlockDefinition) -> BlockId {
        let id = definition.id;

        // Ensure we have enough space
        while self.blocks.len() <= id.0 as usize {
            // Fill gaps with placeholder air blocks
            let placeholder_id = BlockId(self.blocks.len() as u16);
            self.blocks.push(
                BlockDefinition::new(placeholder_id, format!("unknown_{}", placeholder_id.0))
                    .with_solid(false)
                    .with_renderable(false),
            );
        }

        self.blocks[id.0 as usize] = definition;
        id
    }

    /// Registers a new block type with auto-assigned ID.
    pub fn register_auto(&mut self, mut definition: BlockDefinition) -> BlockId {
        let id = BlockId(self.blocks.len() as u16);
        definition.id = id;
        self.blocks.push(definition);
        id
    }

    /// Gets a block definition by ID.
    pub fn get(&self, id: BlockId) -> Option<&BlockDefinition> {
        self.blocks.get(id.0 as usize)
    }

    /// Returns an iterator over all registered blocks.
    pub fn iter(&self) -> impl Iterator<Item = &BlockDefinition> {
        self.blocks.iter()
    }
}

/// A single block, storing its type.
#[derive(Debug, Clone, Copy, Component, Reflect)]
pub struct Block {
    pub block_id: BlockId,
}

impl Default for Block {
    fn default() -> Self {
        Self {
            block_id: BlockId::AIR,
        }
    }
}

impl Block {
    pub fn new(block_id: BlockId) -> Self {
        Self { block_id }
    }

    /// Checks if this block is solid by looking it up in the registry.
    pub fn is_solid(&self, registry: &BlockRegistry) -> bool {
        registry
            .get(self.block_id)
            .map(|def| def.is_solid)
            .unwrap_or(false)
    }

    /// Checks if this block is renderable by looking it up in the registry.
    pub fn is_renderable(&self, registry: &BlockRegistry) -> bool {
        registry
            .get(self.block_id)
            .map(|def| def.is_renderable)
            .unwrap_or(false)
    }
}

/// Width (X) of a chunk in blocks.
pub const CHUNK_SIZE_X: usize = 16;
/// Height (Y) of a chunk in blocks.
pub const CHUNK_SIZE_Y: usize = 256;
/// Depth (Z) of a chunk in blocks.
pub const CHUNK_SIZE_Z: usize = 16;

/// Position of a chunk in chunk-space coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component, Reflect)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
}

/// Global integer block position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component, Reflect)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Returns the chunk-space position that contains this block.
    pub fn chunk_pos(&self) -> ChunkPos {
        ChunkPos {
            x: self.x.div_euclid(CHUNK_SIZE_X as i32),
            z: self.z.div_euclid(CHUNK_SIZE_Z as i32),
        }
    }
}

/// Bevy plugin that registers core types with the reflection system.
pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(BlockRegistry::new())
            .register_type::<BlockId>()
            .register_type::<Block>()
            .register_type::<BlockRegistry>()
            .register_type::<ChunkPos>()
            .register_type::<BlockPos>()
            .configure_sets(
                Startup,
                (BlockRegistrySet, WorldGenerationSet.after(BlockRegistrySet)),
            )
            .add_systems(Startup, setup_vanilla_blocks.in_set(BlockRegistrySet));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_registry_air() {
        let registry = BlockRegistry::new();
        let air = registry.get(BlockId::AIR).unwrap();
        assert_eq!(air.name, "air");
        assert!(!air.is_solid);
        assert!(!air.is_renderable);
    }

    #[test]
    fn block_registry_register() {
        let mut registry = BlockRegistry::new();

        let stone_id = registry.register(
            BlockDefinition::new(BlockId(1), "stone").with_color(Color::srgb(0.5, 0.5, 0.5)),
        );

        assert_eq!(stone_id, BlockId(1));
        let stone = registry.get(stone_id).unwrap();
        assert_eq!(stone.name, "stone");
        assert!(stone.is_solid);
    }

    #[test]
    fn block_registry_auto_register() {
        let mut registry = BlockRegistry::new();

        let stone_id = registry.register_auto(
            BlockDefinition::new(BlockId(0), "stone") // ID will be overwritten
                .with_color(Color::srgb(0.5, 0.5, 0.5)),
        );

        assert_eq!(stone_id, BlockId(1)); // Auto-assigned after air
        let stone = registry.get(stone_id).unwrap();
        assert_eq!(stone.name, "stone");
    }

    #[test]
    fn block_is_solid() {
        let mut registry = BlockRegistry::new();
        let stone_id =
            registry.register_auto(BlockDefinition::new(BlockId(0), "stone").with_solid(true));

        let air_block = Block::new(BlockId::AIR);
        let stone_block = Block::new(stone_id);

        assert!(!air_block.is_solid(&registry));
        assert!(stone_block.is_solid(&registry));
    }

    #[test]
    fn block_pos_chunk_pos() {
        let pos = BlockPos::new(17, 64, -1);
        let chunk = pos.chunk_pos();
        assert_eq!(chunk, ChunkPos::new(1, -1));
    }
}
