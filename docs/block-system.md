# Block System

The block system is defined in `dd40_core::block`. Vanilla block and tool
definitions live in `dd40_vanilla_palette`. Any crate can register new block
types by adding a system to `BlockRegistrySet`.

---

## Types and components

### `BlockId`
```rust
pub struct BlockId(pub u16);
```
Unique identifier for a block type. Up to 65,536 types are supported.
Vanilla blocks use IDs 0–999. Custom blocks should start at 1000.

- `BlockId::AIR` is always `BlockId(0)` and is registered automatically.

### `Block`
```rust
pub struct Block { pub block_id: BlockId }
```
Component attached to any entity that represents a block in the world.

### `BlockPos`
```rust
pub struct BlockPos { pub x: i32, pub y: i32, pub z: i32 }
```
Global integer block position. Implements `From<&Transform>` and provides
`chunk_pos()` and `chunk_local()` helpers.

### `BlockDefinition`
The single source of truth for a block type's properties.

| Field | Type | Default | Description |
|---|---|---|---|
| `id` | `BlockId` | — | Unique ID |
| `name` | `String` | — | Human-readable name |
| `is_solid` | `bool` | `true` | Blocks movement and light |
| `is_renderable` | `bool` | `true` | Should be rendered |
| `color` | `Color` | `WHITE` | Render colour (until textures land) |
| `is_replaceable` | `bool` | `false` | Can be overwritten by placement |
| `collision_shape` | `CollisionShape` | `FullCube` | Physics collision shape |

### `BlockRegistry`
```rust
pub struct BlockRegistry { /* ... */ }
```
Resource. Maps `BlockId -> BlockDefinition`. Registered in `CorePlugin`.

Key methods:
- `register(def, &mut commands)` — register a block type; panics on duplicate ID
- `get(id)` — look up a definition by ID
- `is_replaceable(block)` — check if a block instance can be replaced
- `get_collision_shape(id)` — look up the physics shape for a block

---

## System sets

### `BlockRegistrySet`
All block registration systems must run in this set (during `Startup`). The
world generation set is ordered **after** this set, so blocks are always
registered before generation begins.

```rust
app.add_systems(Startup, register_my_blocks.in_set(BlockRegistrySet));
```

---

## Messages (events)

All messages are registered by `CorePlugin` via `app.add_message::<T>()`.

### `PlaceBlockRequest`
Sent by player/UI systems to request placing a block. Travels to the server
(via network) for authoritative validation before `BlockPlaced` is emitted.

```rust
pub struct PlaceBlockRequest { pub pos: BlockPos, pub block_id: BlockId }
```

### `BlockPlaced`
Emitted when a block placement has been confirmed. The `ChunkCache` is updated
**before** this message is written, so listeners can immediately query the new
block.

```rust
pub struct BlockPlaced {
    pub pos: BlockPos,
    pub block_id: BlockId,
    pub placer: Option<Entity>,
}
```

### `BlockRemoved`
Emitted when a block is broken or explicitly removed.

```rust
pub struct BlockRemoved {
    pub pos: BlockPos,
    pub previous_block_id: BlockId,
    pub remover: Option<Entity>,
}
```

### `BlockChanged`
Emitted when a block changes type in-place (e.g. water freezing to ice).

```rust
pub struct BlockChanged {
    pub pos: BlockPos,
    pub old_block_id: BlockId,
    pub new_block_id: BlockId,
}
```

---

## Registering a custom block

```rust
use bevy::prelude::*;
use dd40_core::prelude::*;

pub const MY_BLOCK: BlockId = BlockId(1000);
pub const MY_SLAB:  BlockId = BlockId(1001);

fn register_my_blocks(mut registry: ResMut<BlockRegistry>, mut commands: Commands) {
    registry.register(
        BlockDefinition::new(MY_BLOCK, "my_block")
            .with_color(Color::srgb(1.0, 0.5, 0.0))
            .with_solid(true),
        &mut commands,
    );

    registry.register(
        BlockDefinition::new(MY_SLAB, "my_slab")
            .with_color(Color::srgb(0.8, 0.6, 0.3))
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

See also `crates/vanilla_palette/src/blocks.rs` for the full vanilla block
registration as a worked example.
