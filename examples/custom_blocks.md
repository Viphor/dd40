# Custom Blocks Example

This example demonstrates how to create and register custom blocks in dd40 using the block registry pattern.

## Simple Custom Block Module

Here's a complete example of creating a module with custom ore blocks:

```rust
// custom_ores.rs
use bevy::prelude::*;
use dd40_core::{BlockDefinition, BlockId, BlockRegistry};

/// Custom ore block IDs (using high numbers to avoid conflicts)
pub struct OreBlocks;

impl OreBlocks {
    pub const COPPER_ORE: BlockId = BlockId(1000);
    pub const TIN_ORE: BlockId = BlockId(1001);
    pub const SILVER_ORE: BlockId = BlockId(1002);
    pub const GOLD_ORE: BlockId = BlockId(1003);
    pub const DIAMOND_ORE: BlockId = BlockId(1004);
}

/// System that registers ore blocks into the block registry
fn register_ore_blocks(mut registry: ResMut<BlockRegistry>) {
    // Copper ore - orange/brown color
    registry.register(
        BlockDefinition::new(OreBlocks::COPPER_ORE, "copper_ore")
            .with_color(Color::srgb(0.8, 0.5, 0.3))
            .with_solid(true)
            .with_renderable(true)
    );

    // Tin ore - light gray color
    registry.register(
        BlockDefinition::new(OreBlocks::TIN_ORE, "tin_ore")
            .with_color(Color::srgb(0.6, 0.6, 0.7))
            .with_solid(true)
            .with_renderable(true)
    );

    // Silver ore - shiny gray
    registry.register(
        BlockDefinition::new(OreBlocks::SILVER_ORE, "silver_ore")
            .with_color(Color::srgb(0.75, 0.75, 0.75))
            .with_solid(true)
            .with_renderable(true)
    );

    // Gold ore - golden yellow
    registry.register(
        BlockDefinition::new(OreBlocks::GOLD_ORE, "gold_ore")
            .with_color(Color::srgb(1.0, 0.84, 0.0))
            .with_solid(true)
            .with_renderable(true)
    );

    // Diamond ore - cyan/blue
    registry.register(
        BlockDefinition::new(OreBlocks::DIAMOND_ORE, "diamond_ore")
            .with_color(Color::srgb(0.4, 0.7, 0.9))
            .with_solid(true)
            .with_renderable(true)
    );

    info!("Registered {} custom ore blocks", 5);
}

/// Plugin that adds custom ore blocks to the game
pub struct OreBlocksPlugin;

impl Plugin for OreBlocksPlugin {
    fn build(&self, app: &mut App) {
        // Register blocks during startup, after vanilla blocks
        app.add_systems(Startup, register_ore_blocks);
    }
}

/// Example system that spawns ore blocks in the world
fn spawn_ore_examples(mut commands: Commands) {
    use dd40_core::BlockPos;
    use dd40_world::spawn_block;

    // Spawn a row of ore blocks
    spawn_block(&mut commands, OreBlocks::COPPER_ORE, BlockPos::new(0, 65, 0));
    spawn_block(&mut commands, OreBlocks::TIN_ORE, BlockPos::new(1, 65, 0));
    spawn_block(&mut commands, OreBlocks::SILVER_ORE, BlockPos::new(2, 65, 0));
    spawn_block(&mut commands, OreBlocks::GOLD_ORE, BlockPos::new(3, 65, 0));
    spawn_block(&mut commands, OreBlocks::DIAMOND_ORE, BlockPos::new(4, 65, 0));
}
```

## Using Auto-Assigned IDs

If you don't want to manage IDs manually, use `register_auto`:

```rust
use bevy::prelude::*;
use dd40_core::{BlockDefinition, BlockId, BlockRegistry};

#[derive(Resource)]
pub struct DecorativeBlocks {
    pub marble: BlockId,
    pub obsidian: BlockId,
    pub crystal: BlockId,
}

fn register_decorative_blocks(
    mut commands: Commands, 
    mut registry: ResMut<BlockRegistry>
) {
    // Let the registry auto-assign IDs
    let marble = registry.register_auto(
        BlockDefinition::new(BlockId(0), "marble")
            .with_color(Color::srgb(0.95, 0.95, 0.98))
    );

    let obsidian = registry.register_auto(
        BlockDefinition::new(BlockId(0), "obsidian")
            .with_color(Color::srgb(0.1, 0.05, 0.15))
    );

    let crystal = registry.register_auto(
        BlockDefinition::new(BlockId(0), "crystal")
            .with_color(Color::srgb(0.8, 0.9, 1.0))
    );

    // Store the IDs as a resource for later use
    commands.insert_resource(DecorativeBlocks {
        marble,
        obsidian,
        crystal,
    });

    info!("Registered decorative blocks: marble={:?}, obsidian={:?}, crystal={:?}", 
          marble, obsidian, crystal);
}

pub struct DecorativeBlocksPlugin;

impl Plugin for DecorativeBlocksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_decorative_blocks);
    }
}
```

## Creating a Separate Crate

For larger block sets, create a separate crate:

**Directory structure:**
```
dd40/
├── crates/
│   ├── core/
│   ├── world/
│   ├── client/
│   └── custom_blocks/  ← New crate
│       ├── Cargo.toml
│       └── src/
│           └── lib.rs
```

**crates/custom_blocks/Cargo.toml:**
```toml
[package]
name = "dd40_custom_blocks"
version = "0.1.0"
edition = "2021"

[dependencies]
bevy = "0.15"
dd40_core = { path = "../core" }
```

**crates/custom_blocks/src/lib.rs:**
```rust
use bevy::prelude::*;
use dd40_core::{BlockDefinition, BlockId, BlockRegistry};

/// Gem block IDs
pub struct GemBlocks;

impl GemBlocks {
    pub const RUBY: BlockId = BlockId(2000);
    pub const EMERALD: BlockId = BlockId(2001);
    pub const SAPPHIRE: BlockId = BlockId(2002);
    pub const AMETHYST: BlockId = BlockId(2003);
}

fn register_gem_blocks(mut registry: ResMut<BlockRegistry>) {
    registry.register(
        BlockDefinition::new(GemBlocks::RUBY, "ruby_block")
            .with_color(Color::srgb(0.9, 0.1, 0.2))
            .with_solid(true)
            .with_renderable(true)
    );

    registry.register(
        BlockDefinition::new(GemBlocks::EMERALD, "emerald_block")
            .with_color(Color::srgb(0.2, 0.9, 0.4))
            .with_solid(true)
            .with_renderable(true)
    );

    registry.register(
        BlockDefinition::new(GemBlocks::SAPPHIRE, "sapphire_block")
            .with_color(Color::srgb(0.2, 0.4, 0.9))
            .with_solid(true)
            .with_renderable(true)
    );

    registry.register(
        BlockDefinition::new(GemBlocks::AMETHYST, "amethyst_block")
            .with_color(Color::srgb(0.6, 0.3, 0.9))
            .with_solid(true)
            .with_renderable(true)
    );
}

pub struct GemBlocksPlugin;

impl Plugin for GemBlocksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_gem_blocks);
    }
}
```

**Using the custom crate in client/main.rs:**
```rust
use bevy::prelude::*;
use dd40_core::CorePlugin;
use dd40_world::WorldPlugin;
use dd40_custom_blocks::GemBlocksPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((
            CorePlugin,
            WorldPlugin,
            GemBlocksPlugin,  // ← Add your custom blocks plugin
        ))
        .run();
}
```

## Advanced: Non-Solid Blocks

Create transparent or non-solid blocks like glass or plants:

```rust
use bevy::prelude::*;
use dd40_core::{BlockDefinition, BlockId, BlockRegistry};

pub struct TransparentBlocks;

impl TransparentBlocks {
    pub const GLASS: BlockId = BlockId(3000);
    pub const ICE: BlockId = BlockId(3001);
    pub const WATER: BlockId = BlockId(3002);
}

fn register_transparent_blocks(mut registry: ResMut<BlockRegistry>) {
    // Glass - solid but could be made transparent with custom materials
    registry.register(
        BlockDefinition::new(TransparentBlocks::GLASS, "glass")
            .with_color(Color::srgba(0.8, 0.9, 1.0, 0.3))
            .with_solid(true)
            .with_renderable(true)
    );

    // Ice - semi-transparent blue
    registry.register(
        BlockDefinition::new(TransparentBlocks::ICE, "ice")
            .with_color(Color::srgba(0.7, 0.85, 1.0, 0.6))
            .with_solid(true)
            .with_renderable(true)
    );

    // Water - non-solid, blue
    registry.register(
        BlockDefinition::new(TransparentBlocks::WATER, "water")
            .with_color(Color::srgba(0.2, 0.4, 0.8, 0.5))
            .with_solid(false)  // Players can walk through
            .with_renderable(true)
    );
}
```

## Testing Your Custom Blocks

Add tests to verify your blocks are registered correctly:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::BlockRegistry;

    #[test]
    fn ore_blocks_register_correctly() {
        let mut registry = BlockRegistry::new();
        
        // Manually call the registration function
        register_ore_blocks(registry.as_mut());

        // Verify all blocks are registered
        let copper = registry.get(OreBlocks::COPPER_ORE).unwrap();
        assert_eq!(copper.name, "copper_ore");
        assert!(copper.is_solid);
        assert!(copper.is_renderable);

        let diamond = registry.get(OreBlocks::DIAMOND_ORE).unwrap();
        assert_eq!(diamond.name, "diamond_ore");
    }

    #[test]
    fn ore_block_ids_are_unique() {
        let ids = vec![
            OreBlocks::COPPER_ORE,
            OreBlocks::TIN_ORE,
            OreBlocks::SILVER_ORE,
            OreBlocks::GOLD_ORE,
            OreBlocks::DIAMOND_ORE,
        ];

        // Check for duplicates
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                assert_ne!(ids[i], ids[j], "Duplicate block ID found!");
            }
        }
    }
}
```

## Best Practices

1. **Use High IDs**: Start custom block IDs at 1000+ to avoid conflicts with vanilla blocks (0-999)

2. **Namespace Your IDs**: Group related blocks (e.g., ores: 1000-1999, gems: 2000-2999)

3. **Register in Startup**: Always register blocks in `Startup` systems, before blocks are spawned

4. **Test Your Blocks**: Write unit tests to verify registration

5. **Document Your Blocks**: Comment the purpose and appearance of each block

6. **Use Const IDs**: Define block IDs as constants for type safety and discoverability

7. **Create Plugins**: Wrap registration in plugins for modularity

## Common Patterns

### Batch Registration
```rust
fn register_colored_blocks(mut registry: ResMut<BlockRegistry>) {
    let colors = [
        (5000, "red_block", Color::srgb(0.9, 0.1, 0.1)),
        (5001, "green_block", Color::srgb(0.1, 0.9, 0.1)),
        (5002, "blue_block", Color::srgb(0.1, 0.1, 0.9)),
    ];

    for (id, name, color) in colors {
        registry.register(
            BlockDefinition::new(BlockId(id), name)
                .with_color(color)
                .with_solid(true)
                .with_renderable(true)
        );
    }
}
```

### Conditional Registration
```rust
fn register_debug_blocks(mut registry: ResMut<BlockRegistry>) {
    #[cfg(debug_assertions)]
    {
        registry.register(
            BlockDefinition::new(BlockId(9999), "debug_marker")
                .with_color(Color::srgb(1.0, 0.0, 1.0))
        );
    }
}
```

### Using Resources for Dynamic IDs
```rust
#[derive(Resource, Default)]
pub struct ModBlockIds {
    blocks: HashMap<String, BlockId>,
}

impl ModBlockIds {
    pub fn get(&self, name: &str) -> Option<BlockId> {
        self.blocks.get(name).copied()
    }
}
```
