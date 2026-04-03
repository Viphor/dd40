use bevy::prelude::*;
use dd40_core::prelude::*;

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
    // Initialize the spatial index resource
    commands.insert_resource(BlockSpatialIndex::default());
    // Initialize the block statistics resource
    commands.insert_resource(BlockStatistics::default());
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
/// Only renders blocks that have at least one air-adjacent face (occlusion culling).
pub fn spawn_block_rendering(
    mut commands: Commands,
    rendering_assets: Option<Res<BlockRenderingAssets>>,
    registry: Res<BlockRegistry>,
    mut spatial_index: ResMut<BlockSpatialIndex>,
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
            // Make sure air blocks are not in the spatial index
            spatial_index.remove(block_pos.x, block_pos.y, block_pos.z);
            continue;
        }

        // Add this block to the spatial index
        spatial_index.insert(block_pos.x, block_pos.y, block_pos.z);

        // Skip blocks that are completely surrounded by other solid blocks (occlusion culling)
        if !spatial_index.has_air_neighbor(block_pos) {
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
/// Also handles re-evaluation of neighboring blocks when a block is added or removed.
pub fn update_block_rendering(
    rendering_assets: Option<Res<BlockRenderingAssets>>,
    registry: Res<BlockRegistry>,
    spatial_index: Res<BlockSpatialIndex>,
    mut blocks_query: Query<
        (
            Entity,
            &Block,
            &BlockPos,
            Option<&mut MeshMaterial3d<StandardMaterial>>,
        ),
        (Changed<Block>,),
    >,
    mut commands: Commands,
) {
    let Some(assets) = rendering_assets else {
        return;
    };

    for (entity, block, block_pos, material) in blocks_query.iter_mut() {
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

        // Check if block should be rendered based on air neighbors
        let should_render = block_def.is_renderable && spatial_index.has_air_neighbor(block_pos);
        let has_rendering = material.is_some();

        match (should_render, has_rendering) {
            (true, true) => {
                // Block should be rendered and has rendering - update material if needed
                if let Some(new_material) = assets.materials.get(&block.block_id) {
                    if let Some(mut mat) = material {
                        *mat = MeshMaterial3d(new_material.clone());
                    }
                }
            }
            (true, false) => {
                // Block should be rendered but doesn't have rendering - add it
                if let Some(material) = assets.materials.get(&block.block_id) {
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
            (false, true) => {
                // Block should not be rendered but has rendering - remove it
                commands
                    .entity(entity)
                    .remove::<BlockEntity>()
                    .remove::<Mesh3d>()
                    .remove::<MeshMaterial3d<StandardMaterial>>();
            }
            (false, false) => {
                // Block should not be rendered and doesn't have rendering - nothing to do
            }
        }
    }
}

/// System that re-evaluates neighboring blocks when a block changes.
/// This ensures that blocks that were hidden become visible when neighbors are removed,
/// and blocks that become hidden are culled when neighbors are added.
pub fn update_neighbor_rendering(
    changed_blocks: Query<(&Block, &BlockPos), Changed<Block>>,
    registry: Res<BlockRegistry>,
    spatial_index: Res<BlockSpatialIndex>,
    mut all_blocks: Query<(Entity, &Block, &BlockPos, Has<BlockEntity>)>,
    mut commands: Commands,
    rendering_assets: Option<Res<BlockRenderingAssets>>,
) {
    let Some(assets) = rendering_assets else {
        return;
    };

    // Collect all positions that need re-evaluation
    let mut positions_to_check = HashSet::new();
    for (_block, block_pos) in changed_blocks.iter() {
        // Add neighbors of changed blocks
        for neighbor_pos in spatial_index.get_neighbor_positions(block_pos) {
            positions_to_check.insert(neighbor_pos);
        }
    }

    // Re-evaluate each affected position
    for (entity, block, block_pos, has_rendering) in all_blocks.iter_mut() {
        let pos_tuple = (block_pos.x, block_pos.y, block_pos.z);
        if !positions_to_check.contains(&pos_tuple) {
            continue;
        }

        let Some(block_def) = registry.get(block.block_id) else {
            continue;
        };

        let should_render = block_def.is_renderable && spatial_index.has_air_neighbor(block_pos);

        match (should_render, has_rendering) {
            (true, false) => {
                // Should render but doesn't - add rendering
                if let Some(material) = assets.materials.get(&block.block_id) {
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
            (false, true) => {
                // Should not render but does - remove rendering
                commands
                    .entity(entity)
                    .remove::<BlockEntity>()
                    .remove::<Mesh3d>()
                    .remove::<MeshMaterial3d<StandardMaterial>>();
            }
            _ => {
                // Already in correct state
            }
        }
    }
}

/// Registers new block materials when new blocks are added to the registry.
/// This allows dynamic registration of blocks after startup.
pub fn update_block_materials(
    event: On<BlockRegistryUpdate>,
    mut rendering_assets: Option<ResMut<BlockRenderingAssets>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    registry: Res<BlockRegistry>,
) {
    let Some(ref mut assets) = rendering_assets else {
        return;
    };

    let id = event.block_id;

    if let Some(definition) = registry.get(id) {
        if !definition.is_renderable && assets.materials.contains_key(&id) {
            assets.materials.remove(&id);
        } else if definition.is_renderable {
            let material = materials.add(StandardMaterial {
                base_color: definition.color,
                ..default()
            });
            assets.materials.insert(id, material);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatial_index_has_block_at() {
        let mut index = BlockSpatialIndex::default();
        assert!(!index.has_block_at(0, 0, 0));

        index.insert(0, 0, 0);
        assert!(index.has_block_at(0, 0, 0));

        index.remove(0, 0, 0);
        assert!(!index.has_block_at(0, 0, 0));
    }

    #[test]
    fn spatial_index_air_neighbor_isolated_block() {
        let mut index = BlockSpatialIndex::default();
        let pos = BlockPos::new(0, 0, 0);

        // Single isolated block should have air neighbors
        index.insert(pos.x, pos.y, pos.z);
        assert!(index.has_air_neighbor(&pos));
    }

    #[test]
    fn spatial_index_air_neighbor_fully_enclosed() {
        let mut index = BlockSpatialIndex::default();
        let pos = BlockPos::new(0, 0, 0);

        // Add center block
        index.insert(pos.x, pos.y, pos.z);

        // Add all 6 neighbors
        index.insert(pos.x + 1, pos.y, pos.z); // East
        index.insert(pos.x - 1, pos.y, pos.z); // West
        index.insert(pos.x, pos.y + 1, pos.z); // Up
        index.insert(pos.x, pos.y - 1, pos.z); // Down
        index.insert(pos.x, pos.y, pos.z + 1); // North
        index.insert(pos.x, pos.y, pos.z - 1); // South

        // Fully enclosed block should NOT have air neighbors
        assert!(!index.has_air_neighbor(&pos));
    }

    #[test]
    fn spatial_index_air_neighbor_partial_enclosure() {
        let mut index = BlockSpatialIndex::default();
        let pos = BlockPos::new(0, 0, 0);

        // Add center block
        index.insert(pos.x, pos.y, pos.z);

        // Add only 5 out of 6 neighbors (missing Up)
        index.insert(pos.x + 1, pos.y, pos.z); // East
        index.insert(pos.x - 1, pos.y, pos.z); // West
        index.insert(pos.x, pos.y - 1, pos.z); // Down
        index.insert(pos.x, pos.y, pos.z + 1); // North
        index.insert(pos.x, pos.y, pos.z - 1); // South

        // Block with one air neighbor should be rendered
        assert!(index.has_air_neighbor(&pos));
    }

    #[test]
    fn spatial_index_get_neighbor_positions() {
        let index = BlockSpatialIndex::default();
        let pos = BlockPos::new(5, 10, 15);

        let neighbors = index.get_neighbor_positions(&pos);
        assert_eq!(neighbors.len(), 6);

        // Check all 6 neighbors are present
        assert!(neighbors.contains(&(6, 10, 15))); // East
        assert!(neighbors.contains(&(4, 10, 15))); // West
        assert!(neighbors.contains(&(5, 11, 15))); // Up
        assert!(neighbors.contains(&(5, 9, 15))); // Down
        assert!(neighbors.contains(&(5, 10, 16))); // North
        assert!(neighbors.contains(&(5, 10, 14))); // South
    }

    #[test]
    fn spatial_index_negative_coordinates() {
        let mut index = BlockSpatialIndex::default();
        let pos = BlockPos::new(-10, -5, -3);

        index.insert(pos.x, pos.y, pos.z);
        assert!(index.has_block_at(-10, -5, -3));
        assert!(index.has_air_neighbor(&pos));

        // Add all neighbors
        for neighbor in index.get_neighbor_positions(&pos) {
            index.insert(neighbor.0, neighbor.1, neighbor.2);
        }

        assert!(!index.has_air_neighbor(&pos));
    }

    #[test]
    fn spatial_index_edge_case_large_coordinates() {
        let mut index = BlockSpatialIndex::default();
        let pos = BlockPos::new(1000000, 500000, -1000000);

        index.insert(pos.x, pos.y, pos.z);
        assert!(index.has_air_neighbor(&pos));
    }

    #[test]
    fn occlusion_culling_reduces_rendered_blocks() {
        // This test verifies that blocks surrounded by other blocks are not rendered
        // Simulate a 3x3x3 cube where only the outer shell should be rendered

        let mut index = BlockSpatialIndex::default();
        let mut should_render_count = 0;
        let mut total_blocks = 0;

        // Create a 3x3x3 cube
        for x in 0..3 {
            for y in 0..3 {
                for z in 0..3 {
                    index.insert(x, y, z);
                    total_blocks += 1;
                }
            }
        }

        // Check which blocks have air neighbors (should be rendered)
        for x in 0..3 {
            for y in 0..3 {
                for z in 0..3 {
                    let pos = BlockPos::new(x, y, z);
                    if index.has_air_neighbor(&pos) {
                        should_render_count += 1;
                    }
                }
            }
        }

        // In a 3x3x3 cube, only the outer shell (26 blocks) should be rendered
        // The center block (1,1,1) is completely surrounded
        assert_eq!(total_blocks, 27);
        assert_eq!(should_render_count, 26);

        // Verify the center block is not rendered
        let center = BlockPos::new(1, 1, 1);
        assert!(!index.has_air_neighbor(&center));
    }

    #[test]
    fn occlusion_culling_surface_blocks_always_rendered() {
        let mut index = BlockSpatialIndex::default();

        // Create a flat surface at y=0
        for x in 0..10 {
            for z in 0..10 {
                index.insert(x, 0, z);
            }
        }

        // All blocks at y=0 should have air neighbors (above them)
        for x in 0..10 {
            for z in 0..10 {
                let pos = BlockPos::new(x, 0, z);
                assert!(
                    index.has_air_neighbor(&pos),
                    "Block at ({}, 0, {}) should have air neighbor",
                    x,
                    z
                );
            }
        }
    }

    #[test]
    fn occlusion_culling_hollow_structure() {
        let mut index = BlockSpatialIndex::default();

        // Create a hollow 5x5x5 cube (walls only)
        for x in 0..5 {
            for y in 0..5 {
                for z in 0..5 {
                    // Only place blocks on the outer shell
                    if x == 0 || x == 4 || y == 0 || y == 4 || z == 0 || z == 4 {
                        index.insert(x, y, z);
                    }
                }
            }
        }

        // All placed blocks should have at least one air neighbor
        // (either facing outward or inward)
        for x in 0..5 {
            for y in 0..5 {
                for z in 0..5 {
                    if x == 0 || x == 4 || y == 0 || y == 4 || z == 0 || z == 4 {
                        let pos = BlockPos::new(x, y, z);
                        assert!(
                            index.has_air_neighbor(&pos),
                            "Wall block at ({}, {}, {}) should have air neighbor",
                            x,
                            y,
                            z
                        );
                    }
                }
            }
        }
    }
}
