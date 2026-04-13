use super::BlockId;
use bevy::{
    color::Color,
    ecs::{event::Event, resource::Resource, schedule::SystemSet, system::Commands},
    reflect::Reflect,
};
use serde::{Deserialize, Serialize};

/// System set for block registration systems.
/// All block registrations should run in this set during Startup.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockRegistrySet;

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
    /// Whether this block can be replaced by a placement action (e.g. air, water).
    /// Blocks where `is_replaceable` is `true` do not need to be broken before a
    /// new block can be placed in their voxel.
    pub is_replaceable: bool,
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
            is_replaceable: false,
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

    /// Sets whether this block can be replaced by a placement action (e.g. air, water).
    /// Blocks where `is_replaceable` is `true` do not need to be broken before a new block
    /// can be placed in their voxel.
    pub fn with_replaceable(mut self, is_replaceable: bool) -> Self {
        self.is_replaceable = is_replaceable;
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
        registry.insert_definition(
            BlockDefinition::new(BlockId::AIR, "air")
                .with_solid(false)
                .with_renderable(false)
                .with_color(Color::srgba(0.0, 0.0, 0.0, 0.0))
                .with_replaceable(true),
        );

        registry
    }

    /// Internal function for making sure that the definition is inserted correctly
    fn insert_definition(&mut self, definition: BlockDefinition) -> BlockId {
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

    /// Registers a new block type. Returns the assigned BlockId.
    /// If a block with the same ID already exists, it will be replaced.
    pub fn register(&mut self, definition: BlockDefinition, commands: &mut Commands) -> BlockId {
        let id = self.insert_definition(definition);
        commands.trigger(BlockRegistryUpdate { block_id: id });
        id
    }

    /// Registers a new block type without triggering a [`BlockRegistryUpdate`]
    /// event.
    ///
    /// This is intended for contexts where [`Commands`] is unavailable, such
    /// as inside async compute tasks that need a lightweight copy of the
    /// registry for solidity / renderability checks.  Callers are responsible
    /// for ensuring that any systems which observe [`BlockRegistryUpdate`] are
    /// not affected by the missing event.
    ///
    /// Returns the assigned [`BlockId`].
    pub fn register_without_event(&mut self, definition: BlockDefinition) -> BlockId {
        self.insert_definition(definition)
    }

    /// Registers a new block type with auto-assigned ID.
    pub fn register_auto(
        &mut self,
        mut definition: BlockDefinition,
        commands: &mut Commands,
    ) -> BlockId {
        let id = BlockId(self.blocks.len() as u16);
        definition.id = id;
        self.blocks.push(definition);
        commands.trigger(BlockRegistryUpdate { block_id: id });
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

#[derive(Event, Debug, Clone, Serialize, Deserialize)]
pub struct BlockRegistryUpdate {
    pub block_id: BlockId,
}

#[cfg(test)]
mod tests {
    use crate::block::Block;
    use bevy::{
        app::App,
        prelude::{Commands, ResMut},
    };

    use super::*;

    #[test]
    fn block_registry_air() {
        let registry = BlockRegistry::new();
        let air = registry.get(BlockId::AIR).unwrap();
        assert_eq!(air.name, "air");
        assert!(!air.is_solid);
        assert!(!air.is_renderable);
    }

    /// Test system that registers a block and stores the ID in a resource
    fn register_stone_system(mut registry: ResMut<BlockRegistry>, mut commands: Commands) {
        let stone_id = registry.register(
            BlockDefinition::new(BlockId(1), "stone").with_color(Color::srgb(0.5, 0.5, 0.5)),
            &mut commands,
        );
        commands.insert_resource(TestBlockId(stone_id));
    }

    /// Test system that auto-registers a block and stores the ID in a resource
    fn register_stone_auto_system(mut registry: ResMut<BlockRegistry>, mut commands: Commands) {
        let stone_id = registry.register_auto(
            BlockDefinition::new(BlockId(0), "stone") // ID will be overwritten
                .with_color(Color::srgb(0.5, 0.5, 0.5)),
            &mut commands,
        );
        commands.insert_resource(TestBlockId(stone_id));
    }

    /// Resource to hold block ID for testing
    #[derive(Resource)]
    struct TestBlockId(BlockId);

    #[test]
    fn block_registry_register() {
        // Setup app
        let mut app = App::new();

        // Add BlockRegistry resource
        app.insert_resource(BlockRegistry::new());

        // Add BlockRegistryUpdate message
        //app.::<BlockRegistryUpdate>();

        // Add test system
        app.add_systems(bevy::app::Update, register_stone_system);

        // Run systems
        app.update();

        // Check resulting changes
        let registry = app.world().resource::<BlockRegistry>();
        let stone_id = app.world().resource::<TestBlockId>().0;

        assert_eq!(stone_id, BlockId(1));
        let stone = registry.get(stone_id).unwrap();
        assert_eq!(stone.name, "stone");
        assert!(stone.is_solid);

        // Check that BlockRegistryUpdate message was sent
        //let messages = app.world().resource::<Messages<BlockRegistryUpdate>>();
        //let mut cursor = messages.get_cursor();
        //let update = cursor.read(messages).next().unwrap();
        //assert_eq!(update.block_id, BlockId(1));
    }

    #[test]
    fn block_registry_auto_register() {
        // Setup app
        let mut app = App::new();

        // Add BlockRegistry resource
        app.insert_resource(BlockRegistry::new());

        // Add BlockRegistryUpdate message
        //app.add_message::<BlockRegistryUpdate>();

        // Add test system
        app.add_systems(bevy::app::Update, register_stone_auto_system);

        // Run systems
        app.update();

        // Check resulting changes
        let registry = app.world().resource::<BlockRegistry>();
        let stone_id = app.world().resource::<TestBlockId>().0;

        assert_eq!(stone_id, BlockId(1)); // Auto-assigned after air
        let stone = registry.get(stone_id).unwrap();
        assert_eq!(stone.name, "stone");

        // Check that BlockRegistryUpdate message was sent
        // let messages = app.world().resource::<Messages<BlockRegistryUpdate>>();
        // let mut cursor = messages.get_cursor();
        // let update = cursor.read(messages).next().unwrap();
        // assert_eq!(update.block_id, BlockId(1));
    }

    #[test]
    fn block_is_solid() {
        // Setup app
        let mut app = App::new();

        // Add BlockRegistry resource
        app.insert_resource(BlockRegistry::new());

        // Add BlockRegistryUpdate message
        // app.add_message::<BlockRegistryUpdate>();

        // Add test system
        app.add_systems(bevy::app::Update, register_stone_auto_system);

        // Run systems
        app.update();

        // Check block solidity
        let registry = app.world().resource::<BlockRegistry>();
        let stone_id = app.world().resource::<TestBlockId>().0;

        let air_block = Block::new(BlockId::AIR);
        let stone_block = Block::new(stone_id);

        assert!(!air_block.is_solid(registry));
        assert!(stone_block.is_solid(registry));
    }
}
