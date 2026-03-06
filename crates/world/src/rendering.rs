use bevy::prelude::*;
use dd40_core::{Block, BlockId, BlockPos, BlockRegistry};
use std::collections::HashMap;

/// Marker component for entities that represent rendered blocks.
#[derive(Component)]
pub struct BlockEntity;

/// Resource that stores shared rendering assets for blocks.
#[derive(Resource)]
pub struct BlockRenderingAssets {
    /// Shared cube mesh used for all block types.
    pub cube_mesh: Handle<Mesh>,
    /// Materials for each block type, keyed by BlockId.
    pub materials: HashMap<BlockId, Handle<StandardMaterial>>,
}

/// Creates a unit cube mesh centered at the origin.
fn create_cube_mesh() -> Mesh {
    Cuboid::new(1.0, 1.0, 1.0).into()
}

/// Initializes block rendering assets (meshes and materials).
/// This runs after the BlockRegistry has been populated with blocks.
pub fn setup_block_rendering(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    registry: Res<BlockRegistry>,
) {
    let cube_mesh = meshes.add(create_cube_mesh());
    let mut block_materials = HashMap::new();

    // Create materials for all registered blocks
    for block_def in registry.iter() {
        if block_def.is_renderable {
            let material = materials.add(StandardMaterial {
                base_color: block_def.color,
                ..default()
            });
            block_materials.insert(block_def.id, material);
        }
    }

    commands.insert_resource(BlockRenderingAssets {
        cube_mesh,
        materials: block_materials,
    });
}

/// System that spawns rendering components for blocks that don't have them yet.
/// This runs whenever a Block component is added to an entity.
pub fn spawn_block_rendering(
    mut commands: Commands,
    rendering_assets: Option<Res<BlockRenderingAssets>>,
    registry: Res<BlockRegistry>,
    // Query for blocks that have a Block and BlockPos but no BlockEntity marker
    blocks_query: Query<(Entity, &Block, &BlockPos), (Without<BlockEntity>, Changed<Block>)>,
) {
    // Wait until rendering assets are loaded
    let Some(assets) = rendering_assets else {
        return;
    };

    for (entity, block, block_pos) in blocks_query.iter() {
        // Get block definition from registry
        let Some(block_def) = registry.get(block.block_id) else {
            warn!(
                "Block with unknown ID {:?} at {:?}",
                block.block_id, block_pos
            );
            continue;
        };

        // Skip non-renderable blocks (like Air)
        if !block_def.is_renderable {
            continue;
        }

        // Get the appropriate material for this block type
        let Some(material) = assets.materials.get(&block.block_id) else {
            warn!("No material found for block type: {}", block_def.name);
            continue;
        };

        //info!("Adding block rendering at {:?}", block_pos);

        // Add rendering components to the entity
        commands.entity(entity).insert((
            BlockEntity,
            Mesh3d(assets.cube_mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(
                block_pos.x as f32 + 0.5,
                block_pos.y as f32 + 0.5,
                block_pos.z as f32 + 0.5,
            ),
        ));
    }
}

/// System that updates block rendering when the block type changes.
pub fn update_block_rendering(
    rendering_assets: Option<Res<BlockRenderingAssets>>,
    registry: Res<BlockRegistry>,
    mut blocks_query: Query<
        (Entity, &Block, &mut MeshMaterial3d<StandardMaterial>),
        (With<BlockEntity>, Changed<Block>),
    >,
    mut commands: Commands,
) {
    let Some(assets) = rendering_assets else {
        return;
    };

    for (entity, block, mut material) in blocks_query.iter_mut() {
        // Get block definition from registry
        let Some(block_def) = registry.get(block.block_id) else {
            // Unknown block - remove rendering
            commands
                .entity(entity)
                .remove::<BlockEntity>()
                .remove::<Mesh3d>()
                .remove::<MeshMaterial3d<StandardMaterial>>();
            continue;
        };

        // If block is no longer renderable, remove rendering components
        if !block_def.is_renderable {
            commands
                .entity(entity)
                .remove::<BlockEntity>()
                .remove::<Mesh3d>()
                .remove::<MeshMaterial3d<StandardMaterial>>();
            continue;
        }

        // Update material if block type changed
        if let Some(new_material) = assets.materials.get(&block.block_id) {
            *material = MeshMaterial3d(new_material.clone());
        }
    }
}

/// Registers new block materials when new blocks are added to the registry.
/// This allows dynamic registration of blocks after startup.
pub fn update_block_materials(
    mut rendering_assets: Option<ResMut<BlockRenderingAssets>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    registry: Res<BlockRegistry>,
) {
    let Some(ref mut assets) = rendering_assets else {
        return;
    };

    // Check if there are any new blocks that don't have materials yet
    for block_def in registry.iter() {
        if block_def.is_renderable && !assets.materials.contains_key(&block_def.id) {
            let material = materials.add(StandardMaterial {
                base_color: block_def.color,
                ..default()
            });
            assets.materials.insert(block_def.id, material);
        }
    }
}

/// Plugin that handles block rendering.
pub struct BlockRenderingPlugin;

impl Plugin for BlockRenderingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            setup_block_rendering.after(dd40_core::setup_vanilla_blocks),
        )
        .add_systems(
            Update,
            (
                update_block_materials,
                spawn_block_rendering,
                update_block_rendering,
            )
                .chain(),
        );
    }
}
