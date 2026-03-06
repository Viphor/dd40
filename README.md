# dd40
An open source Rust implementation of a MineCraft inspired game

## DISCLAIMER:
This is all for shits and giggles. Don't read too much into it.
If you like the project, don't hesitate to open an issue or clone the repo.

## Features

### Extensible Block System with Registry Pattern

The block system uses a **registry pattern** that allows any crate to register new block types dynamically. This means you can add new blocks by creating a plugin or separate crate without modifying the core code!

**System Ordering Guarantee:** The project uses Bevy `SystemSet`s to ensure all block registrations complete before any world generation runs. See [`SYSTEM_ORDERING.md`](SYSTEM_ORDERING.md) for details.

**Simple Example:**
```rust
use bevy::prelude::*;
use dd40_core::{Block, BlockPos, VanillaBlocks};

fn spawn_blocks(mut commands: Commands) {
    // Just spawn a block with its position - rendering happens automatically!
    commands.spawn((
        Block::new(VanillaBlocks::STONE),
        BlockPos::new(10, 64, 10),
    ));
    
    // Spawn multiple blocks
    commands.spawn((Block::new(VanillaBlocks::GRASS), BlockPos::new(11, 64, 10)));
    commands.spawn((Block::new(VanillaBlocks::DIRT), BlockPos::new(12, 64, 10)));
}
```

The system automatically:
- Creates and attaches mesh components
- Assigns appropriate materials based on block type
- Positions blocks at the correct world coordinates
- Updates rendering when block types change
- Removes rendering components when blocks are set to Air

### Adding Custom Blocks

Create your own blocks in a separate crate or plugin:

```rust
use bevy::prelude::*;
use dd40_core::{BlockDefinition, BlockId, BlockRegistry, BlockRegistrySet};

// Define your custom block IDs (start at 1000+ to avoid conflicts)
pub const COPPER_ORE: BlockId = BlockId(1000);
pub const EMERALD_ORE: BlockId = BlockId(1001);

fn register_custom_blocks(mut registry: ResMut<BlockRegistry>) {
    registry.register(
        BlockDefinition::new(COPPER_ORE, "copper_ore")
            .with_color(Color::srgb(0.8, 0.5, 0.3))
            .with_solid(true)
            .with_renderable(true)
    );
    
    registry.register(
        BlockDefinition::new(EMERALD_ORE, "emerald_ore")
            .with_color(Color::srgb(0.2, 0.9, 0.4))
            .with_solid(true)
            .with_renderable(true)
    );
}

pub struct CustomBlocksPlugin;

impl Plugin for CustomBlocksPlugin {
    fn build(&self, app: &mut App) {
        // Register in BlockRegistrySet to ensure it runs before world generation
        app.add_systems(Startup, register_custom_blocks.in_set(BlockRegistrySet));
    }
}
```

Then just add your plugin:
```rust
app.add_plugins(CustomBlocksPlugin);
```

The rendering system will automatically create materials and render your new blocks!

### Vanilla Block Types

Built-in blocks available out of the box:
- `VanillaBlocks::AIR` - Transparent, non-solid
- `VanillaBlocks::STONE` - Gray solid block
- `VanillaBlocks::DIRT` - Brown solid block
- `VanillaBlocks::GRASS` - Green solid block
- `VanillaBlocks::SAND` - Tan solid block
- `VanillaBlocks::WOOD` - Brown wooden block
- `VanillaBlocks::LEAVES` - Dark green foliage

For more details on the block system, see [`crates/world/README.md`](crates/world/README.md).

## Project Structure

- `crates/core` - Core types and data structures
  - Block registry system (BlockId, BlockDefinition, BlockRegistry)
  - Chunk and position types (ChunkPos, BlockPos)
  - Core plugin for reflection and resource setup
  - Vanilla blocks registration
- `crates/world` - World generation and block rendering
  - Chunk generation system
  - Automatic block rendering plugin
  - Block spawning utilities
- `crates/player` - Player controller and movement
- `crates/debug_ui` - Debug UI elements
  - FPS counter with color-coded performance indicators
  - Extensible debug overlay system
- `crates/client` - Game client with rendering
- `crates/server` - Headless server for multiplayer

## Building and Running

```bash
# Run the client
cargo run --bin dd40_client

# Run the server
cargo run --bin dd40_server

# Run tests
cargo test --workspace

# Run with optimizations
cargo run --bin dd40_client --release
```

## Quick Start

### 1. Clone and Build
```bash
git clone <repository-url>
cd dd40
cargo build --release
```

### 2. Run the Client
```bash
cargo run --bin dd40_client
```

You should see a 3D world with colored blocks rendered. Use WASD to move, Space/Shift for up/down.

### 3. Add Your First Custom Block

Create a new file or add to `crates/client/src/main.rs`:

```rust
use bevy::prelude::*;
use dd40_core::{BlockDefinition, BlockId, BlockRegistry, BlockRegistrySet, WorldGenerationSet};

// Define your custom block ID (use 1000+ to avoid conflicts)
pub const MY_BLOCK: BlockId = BlockId(1000);

// Register your block
fn register_my_block(mut registry: ResMut<BlockRegistry>) {
    registry.register(
        BlockDefinition::new(MY_BLOCK, "my_custom_block")
            .with_color(Color::srgb(1.0, 0.0, 1.0))  // Magenta
            .with_solid(true)
            .with_renderable(true)
    );
}

// Spawn your block in the world
fn spawn_my_block(mut commands: Commands) {
    use dd40_core::BlockPos;
    use dd40_world::spawn_block;
    
    spawn_block(&mut commands, MY_BLOCK, BlockPos::new(8, 65, 8));
}
```

Then add to your app:
```rust
app.add_systems(Startup, register_my_block.in_set(BlockRegistrySet))
   .add_systems(Startup, spawn_my_block.in_set(WorldGenerationSet));
```

That's it! Your custom magenta block will appear in the world with automatic rendering. The `SystemSet`s guarantee your block is registered before world generation runs.

## Architecture Highlights

### Block Registry Pattern
- **Extensible**: New blocks can be added by any crate
- **Efficient**: Uses u16 IDs with O(1) lookups
- **Flexible**: Supports up to 65,536 different block types
- **Type-safe**: Block definitions are validated at registration
- **Ordered**: SystemSets guarantee registration completes before world generation

### System Ordering with SystemSets
- **`BlockRegistrySet`**: All block registrations run here during startup
- **`WorldGenerationSet`**: All world generation runs here, after all blocks are registered
- **Plugin-safe**: Works correctly regardless of plugin load order
- **Foolproof**: Prevents using unregistered blocks during world generation

See [`SYSTEM_ORDERING.md`](SYSTEM_ORDERING.md) for detailed usage guide and examples.

### Automatic Rendering
- **Zero boilerplate**: Just spawn `Block + BlockPos`, rendering happens automatically
- **Memory efficient**: Shared meshes and materials
- **Change detection**: Only updates modified blocks
- **Dynamic materials**: New blocks get materials created on-the-fly

### Modular Design
- **Plugin-based**: Each subsystem is a Bevy plugin
- **Loosely coupled**: Crates depend only on core types
- **Testable**: Comprehensive unit tests for core functionality

## Contributing

Contributions are welcome! The registry pattern makes it easy to:
- Add new block types
- Create texture packs
- Implement custom rendering
- Add block behaviors
- Create mod systems

## License

See [LICENSE](LICENSE) for details.
