# Copilot Instructions for dd40

## Project Overview

dd40 is an open-source Rust implementation of a Minecraft-inspired voxel game built with the Bevy game engine. The project is designed with a **modular, extensible architecture** where most functionality can be extended by adding new crates.

## Core Architectural Principles

### 1. Modular Crate-Based Structure

The project is organized as a Cargo workspace with multiple specialized crates:

- **`dd40_core`** - Core types, block registry system, vanilla blocks, and reflection setup
- **`dd40_world`** - World generation, chunk management, and automatic block rendering
- **`dd40_player`** - Player controller and movement systems
- **`dd40_debug_ui`** - Debug UI elements (FPS counter, debug overlays)
- **`dd40_client`** - Game client with rendering
- **`dd40_server`** - Headless server for multiplayer

**Most logic should be able to be extended through adding new crates.** This applies to:
- Block types (via the block registry system)
- World generation algorithms
- Rendering systems
- Gameplay mechanics
- UI elements
- Network protocols

### 2. Extensible Block Registry System

The block registry is a **core extensibility mechanism** that allows any crate to register new block types dynamically:

- Uses `BlockRegistry` resource to store block definitions
- Block IDs are `u16` values (supporting up to 65,536 types)
- Vanilla blocks use IDs 0-999, custom blocks should use 1000+
- Registration happens during `Startup` schedule in the `BlockRegistrySet` system set
- World generation runs in `WorldGenerationSet`, which is ordered **after** `BlockRegistrySet`

**Example of adding custom blocks via a new crate:**
```rust
use bevy::prelude::*;
use dd40_core::{BlockDefinition, BlockId, BlockRegistry, BlockRegistrySet};

pub const MY_CUSTOM_BLOCK: BlockId = BlockId(1000);

fn register_my_blocks(mut registry: ResMut<BlockRegistry>) {
    registry.register(
        BlockDefinition::new(MY_CUSTOM_BLOCK, "my_block")
            .with_color(Color::srgb(1.0, 0.5, 0.0))
            .with_solid(true)
            .with_renderable(true)
    );
}

pub struct MyBlocksPlugin;

impl Plugin for MyBlocksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_my_blocks.in_set(BlockRegistrySet));
    }
}
```

### 3. Plugin-Based Architecture

Every subsystem is a Bevy plugin:
- `CorePlugin` - Registers reflection types and core resources
- `WorldPlugin` - Handles world generation
- `BlockRenderingPlugin` - Automatic block rendering
- `PlayerPlugin` - Player spawning and control
- `DebugUiPlugin` - Debug UI elements

New functionality should be added as plugins to maintain modularity.

### 4. System Ordering with SystemSets

The project uses Bevy `SystemSet`s to guarantee correct execution order:

- **`BlockRegistrySet`** - All block registration systems run here during startup
- **`WorldGenerationSet`** - All world generation runs here, **after** block registration
- This ensures blocks are registered before any world generation attempts to use them

When adding new systems that depend on block registration, use `.in_set(WorldGenerationSet)` or configure appropriate ordering.

### 5. Automatic Rendering System

The rendering system is designed to be **zero-boilerplate** and automatic:

- Spawn a `Block` component with a `BlockPos` component
- The `BlockRenderingPlugin` automatically creates meshes, materials, and transforms
- Updates happen automatically when block types change
- Removing blocks or setting them to Air removes rendering components

This design allows world generation and gameplay code to focus on logic, not rendering.

## Development Guidelines

### Before Writing Code

**If any of the requirements are ambiguous, then ask clarifying questions before writing any code.**

Don't make assumptions about:
- Block behavior specifications
- World generation parameters
- Rendering requirements
- Network protocol details
- UI/UX expectations

Ask first, code second.

### After Writing Code

**After you finish writing any code, list the edge cases and suggest test cases to cover them.**

Consider edge cases such as:
- Invalid block IDs or positions
- Chunk boundaries
- Concurrent access to shared resources
- Plugin load order dependencies
- Extreme values (very large coordinates, negative positions, etc.)
- Empty or uninitialized state
- Resource cleanup on despawn

Suggest specific test cases with inputs and expected outputs.

### When Encountering Bugs

**When you encounter a bug, start by writing a test that reproduces it, then fix it until the test passes.**

Process:
1. Write a failing test that demonstrates the bug
2. Run the test to confirm it fails
3. Fix the implementation
4. Run the test to confirm it passes
5. Consider if additional tests are needed for related edge cases

### Learning from Mistakes

**Every time I correct you, reflect on what you did wrong and come up with a plan to never make the same mistake.**

After a correction:
1. Acknowledge the specific mistake
2. Explain why it was wrong
3. Describe what the correct approach should have been
4. Create a mental checklist to avoid repeating the error

## Documentation Standards

### Prefer Code Documentation Over READMEs

**I don't care about long READMEs. I much prefer well documented code. If it is not in the Rust doc, then I won't read it.**

Guidelines:
- Every public item (`pub`) **must** have doc comments (`///`)
- Use `///` for public APIs, `//` for implementation details
- Doc comments should explain:
  - What the item does
  - Why it exists
  - How to use it (with examples for non-trivial cases)
  - Important constraints or invariants
  - Panics, errors, or edge cases
- Prefer `cargo doc` as the primary documentation source
- READMEs should only contain:
  - Quick start / getting started
  - Build instructions
  - High-level architecture overview (pointing to code docs for details)

**Example of good documentation:**
```rust
/// Registers a new block type in the global block registry.
///
/// Block IDs should be unique. Vanilla blocks use IDs 0-999.
/// Custom blocks should use IDs 1000 and above to avoid conflicts.
///
/// # Arguments
///
/// * `definition` - The block definition containing ID, name, and properties
///
/// # Panics
///
/// Panics if a block with the same ID is already registered.
///
/// # Examples
///
/// ```
/// use dd40_core::{BlockDefinition, BlockId, BlockRegistry};
/// use bevy::prelude::*;
///
/// fn register_custom_block(mut registry: ResMut<BlockRegistry>) {
///     let def = BlockDefinition::new(BlockId(1000), "copper_ore")
///         .with_color(Color::srgb(0.8, 0.5, 0.3))
///         .with_solid(true);
///     registry.register(def);
/// }
/// ```
pub fn register(&mut self, definition: BlockDefinition) {
    // implementation
}
```

### Code Comments

Use inline comments sparingly and only when:
- The code is doing something non-obvious
- There's a subtle invariant that needs to be maintained
- You're working around a limitation or bug
- The algorithm requires explanation

Don't comment obvious things:
```rust
// BAD: Obvious comment
// Increment the counter
counter += 1;

// GOOD: Explains non-obvious reasoning
// Use saturating add to prevent overflow when processing untrusted chunk coordinates
counter = counter.saturating_add(1);
```

## Technology Stack

- **Language:** Rust (edition 2021)
- **Game Engine:** Bevy 0.15
- **Architecture Pattern:** ECS (Entity Component System)
- **Build System:** Cargo workspace

## Testing Practices

- Write unit tests in the same file as the code (in `#[cfg(test)]` modules)
- Write integration tests in `tests/` directories
- Use descriptive test names that explain what is being tested
- Test both success paths and error conditions
- Consider property-based testing for complex logic
- Run tests with `cargo test --workspace`

## Common Patterns

### Adding a New System

1. Define the system function with appropriate Bevy query parameters
2. Add it to a plugin's `build` method
3. Use appropriate `SystemSet`s for ordering if needed
4. Document what the system does and when it runs

### Creating a New Plugin

1. Define a public `Plugin` struct
2. Implement the `Plugin` trait with a `build` method
3. Add systems, resources, and events in the `build` method
4. Document the plugin's purpose and requirements
5. Export the plugin from the crate's `lib.rs`

### Extending the Block System

1. Define block ID constants starting at 1000+
2. Create registration function that accesses `ResMut<BlockRegistry>`
3. Add registration system to `BlockRegistrySet`
4. Document block properties and intended use
5. Create plugin to encapsulate the block type(s)

### Extending World Generation

1. Create new system that queries or generates chunks
2. Add to `WorldGenerationSet` to ensure blocks are registered
3. Use the `BlockRegistry` to look up block IDs by name if needed
4. Document generation algorithm and parameters

### Extending Rendering

1. The automatic rendering system handles basic block rendering
2. For custom rendering, create systems that query blocks and add/modify rendering components
3. Run after `BlockRenderingPlugin` systems if you need to override defaults
4. Document rendering behavior and performance considerations

## Performance Considerations

- Bevy uses parallel system execution - avoid unnecessary mutable access to resources
- Use change detection (`Changed<T>`, `Added<T>`) to avoid redundant work
- Chunk-based processing is more efficient than per-block processing
- Shared meshes and materials reduce memory usage
- Profile before optimizing (`cargo flamegraph`, Bevy diagnostics)

## Git and Version Control

- Write clear, descriptive commit messages
- Keep commits focused on a single logical change
- Test before committing
- Update doc comments when changing public APIs

---

**Remember:** This project values modularity, extensibility, and well-documented code. When in doubt, create a new crate or plugin rather than adding complexity to existing ones. Document your code thoroughly in Rust doc comments, not in README files.