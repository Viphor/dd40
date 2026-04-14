use super::BlockId;
use bevy::{
    color::Color,
    ecs::{event::Event, resource::Resource, schedule::SystemSet, system::Commands},
    reflect::Reflect,
};
use serde::{Deserialize, Serialize};

use crate::character::physics::CollisionShape;

/// System set for block registration systems.
/// All block registrations should run in this set during Startup.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockRegistrySet;

/// Definition of a block type, containing all its properties.
///
/// This is the single source of truth for everything the engine needs to know
/// about a block type.  All properties — rendering, physics, gameplay — live
/// here so that [`BlockRegistry`] is the only resource callers need to
/// consult.
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
    /// The collision shape used by the physics solver for this block type.
    ///
    /// Defaults to [`CollisionShape::FullCube`] so that newly registered solid
    /// blocks behave correctly without any extra setup.  Non-solid blocks (air,
    /// torches, etc.) should use [`CollisionShape::None`].
    ///
    /// Use [`CollisionShape::Box`] for partial-cell shapes such as slabs,
    /// stairs, and lecterns — all coordinates are in cell-local space
    /// (`[0, 1]` range).
    pub collision_shape: CollisionShape,
}

impl BlockDefinition {
    /// Creates a new block definition with sensible defaults.
    ///
    /// | Field              | Default                        |
    /// |--------------------|--------------------------------|
    /// | `is_solid`         | `true`                         |
    /// | `is_renderable`    | `true`                         |
    /// | `color`            | [`Color::WHITE`]               |
    /// | `is_replaceable`   | `false`                        |
    /// | `collision_shape`  | [`CollisionShape::FullCube`]   |
    pub fn new(id: BlockId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            is_solid: true,
            is_renderable: true,
            color: Color::WHITE,
            is_replaceable: false,
            collision_shape: CollisionShape::FullCube,
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

    /// Sets the collision shape used by the physics solver for this block type.
    ///
    /// Use [`CollisionShape::None`] for non-solid blocks (air, flowers, etc.),
    /// [`CollisionShape::FullCube`] for standard opaque blocks, and
    /// [`CollisionShape::Box`] for partial-cell shapes like slabs or stairs.
    ///
    /// # Examples
    ///
    /// ```
    /// use bevy::math::Vec3;
    /// use dd40_core::prelude::*;
    /// use dd40_core::character::physics::CollisionShape;
    ///
    /// let slab = BlockDefinition::new(BlockId(1000), "oak_slab")
    ///     .with_collision_shape(CollisionShape::Box {
    ///         min: Vec3::ZERO,
    ///         max: Vec3::new(1.0, 0.5, 1.0),
    ///     });
    /// ```
    pub fn with_collision_shape(mut self, shape: CollisionShape) -> Self {
        self.collision_shape = shape;
        self
    }
}

/// Registry that stores all registered block types.
#[derive(Resource, Default, Reflect)]
pub struct BlockRegistry {
    blocks: Vec<BlockDefinition>,
}

impl BlockRegistry {
    /// Creates a new block registry with the default Air block pre-registered.
    pub fn new() -> Self {
        let mut registry = Self { blocks: Vec::new() };

        // Always register Air as the first block (ID 0).
        // Air has no collision, is not solid, and is not rendered.
        registry.insert_definition(
            BlockDefinition::new(BlockId::AIR, "air")
                .with_solid(false)
                .with_renderable(false)
                .with_color(Color::srgba(0.0, 0.0, 0.0, 0.0))
                .with_replaceable(true)
                .with_collision_shape(CollisionShape::None),
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

    /// Checks if the given block is solid by looking it up in the registry.
    ///
    /// Returns `false` if the block's ID is not registered.
    pub fn is_solid(&self, block: &super::Block) -> bool {
        self.get(block.block_id)
            .map(|def| def.is_solid)
            .unwrap_or(false)
    }

    /// Checks if the given block is renderable by looking it up in the registry.
    ///
    /// Returns `false` if the block's ID is not registered.
    pub fn is_renderable(&self, block: &super::Block) -> bool {
        self.get(block.block_id)
            .map(|def| def.is_renderable)
            .unwrap_or(false)
    }

    /// Checks if the given block can be replaced by a placement action (e.g. air, water)
    /// by looking it up in the registry.
    ///
    /// Blocks where this returns `true` do not need to be broken before a new block
    /// can be placed in their voxel.
    ///
    /// Returns `false` if the block's ID is not registered.
    pub fn is_replaceable(&self, block: &super::Block) -> bool {
        self.get(block.block_id)
            .map(|def| def.is_replaceable)
            .unwrap_or(false)
    }

    /// Returns the [`CollisionShape`] for the given block.
    ///
    /// This is the authoritative lookup used by the physics solver — no
    /// separate shape registry is needed.
    ///
    /// Returns [`CollisionShape::None`] when the block's ID is not registered,
    /// which is safe: unknown blocks are treated as passable.
    pub fn collision_shape(&self, block: &super::Block) -> CollisionShape {
        self.get(block.block_id)
            .map(|def| def.collision_shape.clone())
            .unwrap_or(CollisionShape::None)
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

        assert!(!registry.is_solid(&air_block));
        assert!(registry.is_solid(&stone_block));
    }
}
