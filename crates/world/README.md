# World Crate

This crate manages the voxel world, including chunk generation, block rendering, and occlusion culling.

## Block System Overview

The block system uses a **registry pattern** that allows any crate to register new block types dynamically. This design enables modular extensibility - you can add new blocks by creating a new crate without modifying the core code.

### Key Components

- **BlockId**: A unique numeric identifier (u16) for each block type
- **BlockDefinition**: Stores properties of a block (name, color, solidity, etc.)
- **BlockRegistry**: A resource that maps BlockId → BlockDefinition
- **Block**: Component that stores just the BlockId reference

## Automatic Block Rendering

When you add a block to the world, the rendering system automatically spawns all necessary components for visualization.

### How It Works

1. **Block Registration**: Block types are registered in the `BlockRegistry` during startup
2. **Block Spawning**: You spawn an entity with `Block` and `BlockPos` components
3. **Automatic Rendering**: The system detects the new block and automatically adds:
   - `BlockEntity` (marker component)
   - `Mesh3d` (shared cube mesh)
   - `MeshMaterial3d` (material with appropriate color)
   - `Transform` (positioned at block coordinates)
4. **Occlusion Culling**: Only blocks with at least one air-adjacent face are rendered

### Basic Usage

```rust
use bevy::prelude::*;
use dd40_core::{Block, BlockPos, VanillaBlocks};

fn spawn_blocks(mut commands: Commands) {
    // Spawn a stone block - rendering happens automatically!
    commands.spawn((
        Block::new(VanillaBlocks::STONE),
        BlockPos::new(10, 64, 10),
    ));
    
    // Or use the helper function
    use dd40_world::spawn_block;
    spawn_block(&mut commands, VanillaBlocks::GRASS, BlockPos::new(11, 64, 10));
}
```

### Dynamic Updates

If you change a block's type, the rendering system automatically updates:

```rust
fn change_block_type(mut query: Query<&mut Block>) {
    for mut block in &mut query {
        // Changing the block ID automatically updates the rendering
        block.block_id = VanillaBlocks::DIRT;
    }
}
```

## Adding Custom Blocks

The registry pattern allows any crate to add new blocks. Here's how:

### Option 1: Register During Startup (Simple)

Create a system that registers your blocks:

```rust
use bevy::prelude::*;
use dd40_core::{BlockDefinition, BlockId, BlockRegistry};

// Define your block IDs (use high numbers to avoid conflicts)
pub const COPPER_ORE: BlockId = BlockId(1000);
pub const TIN_ORE: BlockId = BlockId(1001);

fn register_ore_blocks(mut registry: ResMut<BlockRegistry>) {
    registry.register(
        BlockDefinition::new(COPPER_ORE, "copper_ore")
            .with_color(Color::srgb(0.8, 0.5, 0.3))
            .with_solid(true)
            .with_renderable(true)
    );
    
    registry.register(
        BlockDefinition::new(TIN_ORE, "tin_ore")
            .with_color(Color::srgb(0.6, 0.6, 0.7))
            .with_solid(true)
            .with_renderable(true)
    );
}

// Add to your plugin
impl Plugin for OrePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_ore_blocks);
    }
}
```

### Option 2: Auto-Assign IDs (Alternative)

If you don't care about specific IDs, let the registry assign them:

```rust
use bevy::prelude::*;
use dd40_core::{BlockDefinition, BlockId, BlockRegistry};

#[derive(Resource)]
pub struct CustomBlocks {
    pub copper_ore: BlockId,
    pub tin_ore: BlockId,
}

fn register_custom_blocks(mut commands: Commands, mut registry: ResMut<BlockRegistry>) {
    let copper_ore = registry.register_auto(
        BlockDefinition::new(BlockId(0), "copper_ore") // ID will be auto-assigned
            .with_color(Color::srgb(0.8, 0.5, 0.3))
    );
    
    let tin_ore = registry.register_auto(
        BlockDefinition::new(BlockId(0), "tin_ore")
            .with_color(Color::srgb(0.6, 0.6, 0.7))
    );
    
    // Store the IDs for later use
    commands.insert_resource(CustomBlocks { copper_ore, tin_ore });
}
```

### Option 3: Create a Separate Crate

Create a new crate (e.g., `dd40_ores`) that depends on `dd40_core`:

**Cargo.toml:**
```toml
[dependencies]
bevy = "0.15"
dd40_core = { path = "../core" }
```

**lib.rs:**
```rust
use bevy::prelude::*;
use dd40_core::{BlockDefinition, BlockId, BlockRegistry};

pub const COPPER_ORE: BlockId = BlockId(1000);
pub const DIAMOND_ORE: BlockId = BlockId(1001);

fn register_ores(mut registry: ResMut<BlockRegistry>) {
    registry.register(
        BlockDefinition::new(COPPER_ORE, "copper_ore")
            .with_color(Color::srgb(0.8, 0.5, 0.3))
    );
    
    registry.register(
        BlockDefinition::new(DIAMOND_ORE, "diamond_ore")
            .with_color(Color::srgb(0.4, 0.7, 0.9))
    );
}

pub struct OrePlugin;

impl Plugin for OrePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_ores);
    }
}
```

Then in your client or server, just add the plugin:
```rust
app.add_plugins(dd40_ores::OrePlugin);
```

## Vanilla Blocks

The core library provides these default blocks:

```rust
use dd40_core::VanillaBlocks;

// Block IDs:
VanillaBlocks::AIR     // ID 0 - Transparent, non-solid
VanillaBlocks::STONE   // ID 1 - Gray solid block
VanillaBlocks::DIRT    // ID 2 - Brown solid block
VanillaBlocks::GRASS   // ID 3 - Green solid block
VanillaBlocks::SAND    // ID 4 - Tan solid block
VanillaBlocks::WOOD    // ID 5 - Brown wooden block
VanillaBlocks::LEAVES  // ID 6 - Dark green foliage
```

## Occlusion Culling

The rendering system includes **automatic occlusion culling** to significantly improve performance by only rendering blocks that have at least one air-adjacent face.

### How It Works

1. **Spatial Index**: A `BlockSpatialIndex` resource tracks all non-air block positions using a `HashSet`
2. **Neighbor Detection**: Before rendering a block, the system checks all 6 face-adjacent neighbors (up, down, north, south, east, west)
3. **Visibility Check**: A block is only rendered if at least one neighbor is air or unoccupied
4. **Dynamic Updates**: When blocks are added or removed, neighboring blocks are automatically re-evaluated

### Performance Impact

Example results for a 3×3×3 cube (27 blocks):
- **Without culling**: 27 blocks rendered
- **With culling**: 26 blocks rendered (center block is completely surrounded and culled)

In realistic terrain with underground caves:
- Blocks completely surrounded by stone are **not rendered**
- Only surface-facing blocks use GPU resources
- Significant performance improvement in large worlds

### Block Statistics

The `BlockStatistics` resource tracks block counts in real-time:

```rust
use dd40_world::BlockStatistics;

fn display_stats(stats: Res<BlockStatistics>) {
    println!("Loaded blocks: {}", stats.loaded_blocks);
    println!("Rendered blocks: {}", stats.rendered_blocks);
    let culled = stats.loaded_blocks - stats.rendered_blocks;
    println!("Culled blocks: {}", culled);
}
```

This resource is automatically updated by the rendering system and can be accessed by other crates (such as the debug UI).

## Architecture

### Resources
- **BlockRegistry**: Stores all registered block definitions
- **BlockRenderingAssets**: Stores shared meshes and materials
- **BlockSpatialIndex**: Tracks all non-air block positions for occlusion culling
- **BlockStatistics**: Tracks loaded and rendered block counts

### Systems
- **setup_vanilla_blocks**: Registers default blocks (runs in Startup)
- **setup_block_rendering**: Creates shared cube mesh and materials for all registered blocks
- **spawn_block_rendering**: Adds rendering components to new blocks (with occlusion culling)
- **update_block_rendering**: Updates rendering when blocks change
- **update_neighbor_rendering**: Re-evaluates neighbors when blocks change
- **update_block_materials**: Creates materials for newly registered blocks
- **update_block_statistics**: Updates the BlockStatistics resource

### Components
- **Block**: The block type (just stores BlockId)
- **BlockPos**: Global block position
- **BlockEntity**: Marker for rendered blocks

## Performance Notes

- **Occlusion culling**: Blocks completely surrounded by other blocks are not rendered
- All blocks share a single cube mesh to reduce memory usage
- Materials are created once per block type and reused
- Only renderable blocks have rendering components spawned
- The system uses Bevy's change detection to only update modified blocks
- Registry lookups are O(1) using direct array indexing
- Spatial index uses O(1) HashSet lookups for neighbor checks
- Supports up to 65,536 different block types (u16)
- Block statistics are efficiently updated via query counting

## Future Extensions

The registry pattern and rendering system make it easy to add:
- Custom textures (extend BlockDefinition with texture handles)
- Custom meshes (stairs, slabs, etc.)
- Block states (rotation, waterlogged, etc.)
- Custom rendering logic per block type
- Network synchronization metadata
- Custom block behaviors and interactions
- **Greedy meshing**: Combine adjacent identical blocks into larger meshes
- **Face-level culling**: Cull individual faces instead of entire blocks
- **Frustum culling**: Don't render chunks outside camera view
- **Level of Detail (LOD)**: Simpler meshes for distant chunks