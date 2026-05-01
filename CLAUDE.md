# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build --workspace          # Build all crates
cargo run --bin dd40_client      # Run the game client (port 6969)
cargo run --bin dd40_server      # Run the headless server
cargo test --workspace           # Run all tests
cargo test -p dd40_core          # Run tests for a single crate
cargo doc --workspace --open     # Build and open API docs
```

The client has an optional `debug_network` feature flag that enables lightyear's network diagnostics UI:
```bash
cargo run --bin dd40_client --features debug_network
```

## Architecture

dd40 is a Minecraft-inspired voxel game (Bevy 0.18 ECS, lightyear 0.26 networking). It is a Cargo workspace of specialized crates. Crates are organised into **three tiers** — see `SPEC.md` and `.agents/skills/dd40-architecture/SKILL.md` for the full rules.

**Tier 0 — Foundation** (types, components, system sets — no game behaviour):
- Foundation crates depend only on other foundation crates and external libraries.
- Every plugin must derive `Default` so it can be auto-added.

**Tier 1 — Implementation** (systems, concrete behaviour):
- May depend on any foundation crates and external libraries.
- Must **not** depend on other implementation crates.
- Must call `ensure_plugins!(app, DepPlugin, ...)` at the start of every `Plugin::build`.

**Tier 2 — Binary** (`dd40_client`, `dd40_server`):
- May depend on any dd40 crate.
- Wire implementation plugins together; foundation plugins are auto-satisfied.

Known violations are tracked in `INCONSISTENCIES.md`. When fixing or extending
those areas, the fix is documented there.

### Auto-plugin pattern

```rust
impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin, PhysicsCorePlugin);
        // add systems ...
    }
}
```

Never write `if !app.is_plugin_added` by hand — always use `ensure_plugins!`.

### Crate roles

| Tier | Crate | Role |
|---|---|---|
| Foundation | `dd40_core` | Block registry, chunk pipeline messages, app state |
| Foundation | `dd40_physics_core` | Physics types, components, system sets *(planned)* |
| Foundation | `dd40_character_core` | Character types, input bridge, render sets *(planned)* |
| Implementation | `dd40_physics` | Integration, block collision, character collision systems *(planned)* |
| Implementation | `dd40_vanilla_palette` | Vanilla block/tool definitions (IDs 0–999) |
| Implementation | `dd40_world` | World generation (generic over `WorldGenerator` trait) |
| Implementation | `dd40_chunk_storage` | Disk-backed chunk persistence (bincode v1) |
| Implementation | `dd40_renderer` | Greedy-mesh renderer, async mesh tasks, LOD |
| Implementation | `dd40_player` | Convenience wrapper: movement + interaction + spawn |
| Implementation | `dd40_player_movement` | Keyboard/mouse → CharacterInput, first-person camera *(planned)* |
| Implementation | `dd40_character_interaction` | Block targeting, mining, placement for any Character *(planned)* |
| Implementation | `dd40_network` | lightyear client-server networking (feature-gated) |
| Implementation | `dd40_debug_ui` | FPS overlay, orientation gizmo |
| Implementation | `dd40_gui` | In-game HUD (crosshair) |
| Binary | `dd40_client` | Playable client binary |
| Binary | `dd40_server` | Headless server binary (port 6969) |

*Crates marked (planned) do not exist yet — see `SPEC.md` Phase 1–3.*

### Chunk pipeline

```
[any system] → RequestChunk (Message)
                    ↓
             dd40_chunk_storage
              ├─ found on disk → ChunkReady (Message)
              └─ missing → GenerateChunk (Message)
                                ↓
                          dd40_world → ChunkReady
                                          ↓
                             ChunkCachePlugin (core) caches it
                                          ↓
                               dd40_renderer meshes it
```

### System ordering (Startup)

`BlockRegistrySet` runs before `WorldGenerationSet`. All block registration must be in `BlockRegistrySet`; world generation must be in `WorldGenerationSet` or later.

## Bevy 0.18 API — Events vs Messages

These are **not interchangeable**. Getting this wrong is the most common source of bugs.

**Events** — immediate, observer-driven:
```rust
#[derive(Event)]
pub struct BlockPlaced { pub pos: BlockPos, pub block_id: BlockId }

fn on_block_placed(trigger: On<BlockPlaced>) { /* trigger.pos, trigger.block_id */ }

app.add_observer(on_block_placed);
commands.trigger(BlockPlaced { pos, block_id });
```

**Messages** — queued, polled by systems (for async results like chunk loads):
```rust
#[derive(Message, Clone)]
pub struct ChunkReady { pub chunk: StorageChunk }

fn produce(mut writer: MessageWriter<ChunkReady>) { writer.write(ChunkReady { chunk }); }
fn consume(mut reader: MessageReader<ChunkReady>) { for msg in reader.read() { /* ... */ } }

app.add_message::<ChunkReady>().add_systems(Update, (produce, consume));
```

`EventReader` / `EventWriter` / `add_event` do not exist in Bevy 0.18. lightyear messages (`app.register_message::<T>()`) are a separate, third system.

## Block System

`BlockDefinition` is the single source of truth for everything the engine knows about a block. Never store block data outside `BlockRegistry`. Vanilla blocks use IDs 0–999; custom blocks start at 1000.

```rust
registry.register(
    BlockDefinition::new(BlockId(1000), "my_block")
        .with_color(Color::srgb(1.0, 0.5, 0.0))
        .with_solid(true)
        .with_renderable(true),
    &mut commands,
);
```

Non-solid blocks require `.with_collision_shape(CollisionShape::None)`. Slabs/stairs use `CollisionShape::Box { min, max }` within the 1×1×1 cell.

## Documentation Standard

Every `pub` item must have a `///` doc comment explaining what it does, why it exists, constraints, and panics. `cargo doc` is the authoritative documentation source — READMEs are secondary. Do not write doc comments for private items unless the logic is genuinely non-obvious.

## Adding New Functionality

New features belong in a new crate or plugin, not bolted onto existing crates. When in doubt, create a new plugin. See `INCONSISTENCIES.md` for planned architectural clean-ups before introducing more coupling.

When encountering a bug: write a failing test first, fix it, confirm the test passes.
