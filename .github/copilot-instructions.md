# Copilot Instructions for dd40

## Project Overview

dd40 is an open-source Rust implementation of a Minecraft-inspired voxel game built with the Bevy game engine. The project is designed with a **modular, extensible architecture** where most functionality can be extended by adding new crates.

## Core Architectural Principles

### 1. Modular Crate-Based Structure

The project is organized as a Cargo workspace with 16 specialized crates in
a three-tier model (Foundation → Implementation → Binary):

**Tier 0 — Foundation** (types, components, system sets — no game logic):
- **`dd40_core`** — block registry, chunk pipeline, app state, tool system
- **`dd40_physics_core`** — physics types, components, `PhysicsSet`
- **`dd40_character_core`** — character types, `CharacterInput`, `MiningState`, `PlayerId`, `CharacterRenderSet`

**Tier 1 — Implementation** (systems, game behaviour):
- **`dd40_physics`** — gravity, block collision, character collision
- **`dd40_vanilla_palette`** — vanilla block/tool definitions (IDs 0–999)
- **`dd40_world`** — world generation
- **`dd40_chunk_storage`** — disk-backed chunk persistence
- **`dd40_renderer`** — greedy-mesh chunk renderer (replaces `BlockRenderingPlugin`)
- **`dd40_player_movement`** — keyboard/mouse → `CharacterInput`, first-person camera
- **`dd40_character_interaction`** — block targeting, mining, placement
- **`dd40_network`** — lightyear networking
- **`dd40_debug_ui`** — FPS overlay, orientation gizmo
- **`dd40_gui`** — in-game HUD
- **`dd40_player`** — convenience wrapper (tracked Tier 1 exception)

**Tier 2 — Binary**: `dd40_client`, `dd40_server`

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

**`BlockDefinition` is the single source of truth for everything the engine needs to know about a block type.**  All properties — rendering, physics, gameplay — must live on `BlockDefinition` so that `BlockRegistry` is the only resource callers need to consult.  Never store block-related data in a separate resource or component that must be kept in sync with the registry.

**Example of adding custom blocks via a new crate:**
```rust
use bevy::prelude::*;
use dd40_core::prelude::*;  // includes CollisionShape

pub const MY_CUSTOM_BLOCK: BlockId = BlockId(1000);
pub const MY_SLAB_BLOCK: BlockId = BlockId(1001);

fn register_my_blocks(mut registry: ResMut<BlockRegistry>, mut commands: Commands) {
    registry.register(
        BlockDefinition::new(MY_CUSTOM_BLOCK, "my_block")
            .with_color(Color::srgb(1.0, 0.5, 0.0))
            .with_solid(true)
            .with_renderable(true),
        // collision_shape defaults to CollisionShape::FullCube — no need to set it
        // for standard solid blocks.
        &mut commands,
    );

    registry.register(
        BlockDefinition::new(MY_SLAB_BLOCK, "my_slab")
            .with_color(Color::srgb(0.8, 0.6, 0.3))
            .with_solid(true)
            .with_renderable(true)
            .with_collision_shape(CollisionShape::Box {
                min: bevy::math::Vec3::ZERO,
                max: bevy::math::Vec3::new(1.0, 0.5, 1.0),
            }),
        &mut commands,
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
- `CorePlugin` — registers types, resources, messages, system-set ordering
- `PhysicsPlugin` — gravity, block collision, character collision
- `VanillaPalettePlugin` — registers vanilla blocks and tools
- `WorldPlugin` — handles world generation
- `RendererPlugin` — greedy-mesh chunk rendering (LOD-aware)
- `PlayerInputPlugin` — player movement, camera, block interaction
- `DebugUiPlugin` — FPS overlay, orientation gizmo

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

- **Language:** Rust (edition 2024)
- **Game Engine:** Bevy 0.18
- **Architecture Pattern:** ECS (Entity Component System)
- **Build System:** Cargo workspace

### Bevy 0.18 Specific Notes

- **Observer Pattern:** Bevy 0.18 uses the `On<EventType>` type (from `bevy::prelude`) for observer functions, not `Trigger<EventType>`.
  - Observer functions should have signature: `fn my_observer(trigger: On<MyEvent>)`
  - Access event data directly from the `On` type: `trigger.pos`, `trigger.field_name`
  - Register observers with `app.add_observer(my_observer)`
- **Events and Messages are two distinct concepts in Bevy 0.18** — see the "Bevy 0.18 API Notes" section below for the full breakdown.

## Bevy 0.18 API Notes

### Events vs Messages — Two Distinct Concepts

These are **not interchangeable**. Choose the right one based on how the data needs to be consumed.

---

#### Events — observed, immediate, targeted

Events are **observed directly** using Bevy's observer system. When an event is triggered, all registered observers run immediately, before the next system tick. They are not queued for later; there is no reader that polls them each frame.

- Define with `#[derive(Event)]`
- Trigger with `commands.trigger(MyEvent { .. })` or `commands.trigger_targets(MyEvent { .. }, entity)`
- Observe with `app.add_observer(my_observer_fn)`
- Observer function signature: `fn my_observer(trigger: On<MyEvent>)` — access event data directly via `trigger.field`
- **Do not** use `MessageReader` or `MessageWriter` with events
- **Use events for:** things that happen to a specific entity or that need an immediate response — e.g. block placed, player died, collision detected

**Example:**
```rust
#[derive(Event)]
pub struct BlockPlaced {
    pub pos: BlockPos,
    pub block_id: BlockId,
}

fn on_block_placed(trigger: On<BlockPlaced>) {
    info!("Block placed at {:?}", trigger.pos);
}

// In plugin:
app.add_observer(on_block_placed);

// Triggering:
commands.trigger(BlockPlaced { pos, block_id });
```

---

#### Messages — queued, polled by systems

Messages are **written into a queue** and **read by systems** on subsequent frames (or the same frame, depending on system ordering). They are not dispatched immediately; a system must actively drain the queue with `MessageReader`.

- Define with `#[derive(Message, Clone)]` from `bevy::ecs::message` (re-exported via `bevy::prelude::*`)
- Register with `app.add_message::<MyMessage>()`
- Write with `MessageWriter<T>` — use `writer.write(msg)` (**not** `.send()`)
- Read with `MessageReader<T>` — `reader.read()` returns an iterator of `&T`
- **Do not** use `add_observer` with messages
- **Use messages for:** data that flows between systems asynchronously — e.g. chunk load requests, network responses, background task results

**Example:**
```rust
#[derive(Message, Clone)]
pub struct ChunkReady {
    pub chunk: StorageChunk,
}

fn produce(mut writer: MessageWriter<ChunkReady>) {
    writer.write(ChunkReady { chunk });
}

fn consume(mut reader: MessageReader<ChunkReady>) {
    for msg in reader.read() {
        // handle msg
    }
}

// In plugin:
app.add_message::<ChunkReady>()
   .add_systems(Update, (produce, consume));
```

---

#### Quick-reference comparison

| | **Event** | **Message** |
|---|---|---|
| Derive | `#[derive(Event)]` | `#[derive(Message, Clone)]` |
| Register | _(not needed)_ | `app.add_message::<T>()` |
| Send / write | `commands.trigger(e)` | `writer.write(msg)` |
| Consume | `app.add_observer(fn)` | `MessageReader<T>` in a system |
| Timing | Immediate, before next tick | Queued, drained by systems |
| Targeting | Can target a specific entity | Broadcast to all readers |

---

- **`EventReader` / `EventWriter` / `add_event`**: These older Bevy APIs are **not available** in Bevy 0.18. Use the event observer pattern or the message system depending on your use case.
- **lightyear messages**: lightyear has its own separate message/replication system (registered via `app.register_message::<T>()`). These are entirely distinct from Bevy's `Message` system and use lightyear-specific channel/reader APIs.

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

### Adding an Observer for an Event (Bevy 0.18)

Use this pattern when something **happens** and you want an immediate reaction — e.g. a block being placed, an entity dying.

1. Define the event type with `#[derive(Event)]`
2. Create an observer function with `On<EventType>` as its parameter
3. Access event fields directly from the `On` parameter (e.g. `trigger.pos`)
4. Register with `app.add_observer(observer_function)`
5. Trigger the event with `commands.trigger(MyEvent { .. })`

**Example:**
```rust
#[derive(Event)]
pub struct BlockPlaced {
    pub pos: BlockPos,
    pub block_id: BlockId,
}

fn on_block_placed(trigger: On<BlockPlaced>) {
    info!("Block placed at ({}, {}, {})",
          trigger.pos.x, trigger.pos.y, trigger.pos.z);
}

// In plugin:
app.add_observer(on_block_placed);

// Elsewhere:
commands.trigger(BlockPlaced { pos, block_id });
```

### Sending a Message Between Systems (Bevy 0.18)

Use this pattern when data needs to be **queued** and **consumed by a system** — e.g. a background thread finishing a chunk load.

1. Define the message type with `#[derive(Message, Clone)]`
2. Register with `app.add_message::<MyMessage>()`
3. Write with `MessageWriter<T>` using `writer.write(msg)`
4. Read with `MessageReader<T>` using `reader.read()`

**Example:**
```rust
#[derive(Message, Clone)]
pub struct ChunkReady {
    pub chunk: StorageChunk,
}

fn on_chunk_loaded(mut writer: MessageWriter<ChunkReady>, /* ... */) {
    writer.write(ChunkReady { chunk });
}

fn apply_loaded_chunks(
    mut reader: MessageReader<ChunkReady>,
    mut storage: ResMut<BlockStorage>,
) {
    for msg in reader.read() {
        storage.set_chunk(msg.chunk.clone());
    }
}

// In plugin:
app.add_message::<ChunkReady>()
   .add_systems(Update, (on_chunk_loaded, apply_loaded_chunks));
```

### Creating a New Plugin

1. Define a public `Plugin` struct
2. Implement the `Plugin` trait with a `build` method
3. Add systems, resources, and events in the `build` method
4. Document the plugin's purpose and requirements
5. Export the plugin from the crate's `lib.rs`

### Extending the Block System

1. Define block ID constants starting at 1000+
2. Create registration function that accesses `ResMut<BlockRegistry>` and `Commands`
3. Set **all** block properties on `BlockDefinition` — rendering, physics, gameplay. Never use a separate resource or side-channel to store block data; `BlockRegistry` is the single source of truth
4. For irregular collision shapes (slabs, stairs, lecterns) set `.with_collision_shape(CollisionShape::Box { .. })` — the shape must fit within the 1×1×1 cell
5. Non-solid blocks (air, flowers, torches) must use `.with_collision_shape(CollisionShape::None)` so the physics solver skips them
6. Add registration system to `BlockRegistrySet`
7. Document block properties and intended use
8. Create plugin to encapsulate the block type(s)

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
