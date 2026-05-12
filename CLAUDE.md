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
cargo fmt --workspace            # Format workspace (should be done before commits)
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
| Foundation | `dd40_physics_core` | Physics types, components, system sets |
| Foundation | `dd40_character_core` | Character types, input bridge, `MiningState`, `PlayerId`, render sets |
| Implementation | `dd40_physics` | Integration, block collision, character collision systems |
| Implementation | `dd40_vanilla_palette` | Vanilla block/tool definitions (IDs 0–999) |
| Implementation | `dd40_world` | World generation (generic over `WorldGenerator` trait) |
| Implementation | `dd40_chunk_storage` | Disk-backed chunk persistence (bincode v1) |
| Implementation | `dd40_renderer` | Greedy-mesh renderer, async mesh tasks, LOD anchored on `CharacterPosition` |
| Implementation | `dd40_player_input` | Keyboard/mouse → CharacterInput, first-person camera, `PlayerMode` state |
| Implementation | `dd40_character_interaction` | Block targeting, mining, placement for any `Character` entity |
| Implementation | `dd40_network` | lightyear client-server networking (feature-gated) |
| Implementation | `dd40_debug_ui` | FPS overlay, orientation gizmo |
| Implementation | `dd40_gui` | In-game HUD (crosshair) |
| Binary | `dd40_client` | Playable client binary |
| Binary | `dd40_server` | Headless server binary (port 6969) |

### Chunk pipeline

```
[any system] → RequestChunk { pos, current_version } (Message)
                    ↓
             dd40_chunk_storage
              ├─ found on disk → ChunkReady (Message)
              └─ missing → GenerateChunk (Message)
                                ↓
                          dd40_world → ChunkReady (version = 1)
                                          ↓
                             ChunkCachePlugin (core) caches it
                                          ↓
                               dd40_renderer meshes it
```

Networked clients additionally consume `ChunkSnapshot` (full chunk) or
`ChunkUpdate { base_version, changes, new_version }` (delta) on arrival —
see "Versioned chunk cache" below.

### System ordering (Startup)

`BlockRegistrySet` runs before `WorldGenerationSet`. All block registration must be in `BlockRegistrySet`; world generation must be in `WorldGenerationSet` or later.

### Versioned chunk cache

Every `Chunk` carries:
- `version: u64` — monotonic, bumped once per server-committed batch.
  Generator output is `version = 1`; `version = 0` means "client has
  nothing".
- `confirmed_history: VecDeque<(u64, ChunkChange)>` — every change ever
  committed to this chunk while it has been loaded. **Not capped.**
  Dropped on eviction (and persisted only when `DD40_CHUNK_STORAGE__SAVE_HISTORY=true`).
- `predicted: VecDeque<ChunkChange>` — runtime-only queue of
  locally-applied changes awaiting server confirmation. Both client and
  server mutate chunks by pushing into `predicted`.

`ChunkChange` is the single mutation type: `Place { local, block_id }`,
`Remove { local }`, `Replace { local, new_block }`. All coordinates are
**chunk-local** — a chunk has no global-world knowledge.

#### Authoritative commit (server-only)

`ChunkAuthorityPlugin` (added by the server binary, never the client)
runs `commit_predicted_changes` in `PostUpdate`. It drains `predicted`
through a registered chain of `ChunkChangeValidator`s, applies the
survivors, bumps `version`, appends them to `confirmed_history`, and
broadcasts `ChunkUpdate` to clients in range. Adding the plugin *is* the
authority gate — there is no `run_if` / marker resource.

#### Reconciliation (client)

When a `ChunkUpdate { base_version, changes, new_version }` arrives:
- `base_version == client_version` → walk `changes`; for each, scan
  `predicted` for an exact match (same `local`, same kind) and remove on
  first hit. Anything left in `predicted` is a **rejected prediction** →
  log a warning and fire `PredictionRejected { pos, change }`.
- `base_version > client_version` → client missed history. Log warn,
  drop the update, re-request via `RequestChunk { pos, current_version }`.

#### Snapshot fallback

The single configurable knob is `MaxDeltaBehind(u16)` (default 15).
If `current_version < server_version - MaxDeltaBehind`, the server
replies with a full `ChunkSnapshot` instead of a `ChunkUpdate` and emits
a local `ChunkSnapshotFallback { pos, client_version, server_version }`
message for analysis tooling.

#### Local notification

After every commit (server) or applied update (client), a Bevy
`ChunkChanged { pos, changes, new_version }` message is emitted.
Renderer, audio, and any future system (redstone, …) subscribe to it.
There is no `BlockPlaced` / `BlockRemoved` event — those were deleted
when the versioned cache landed.

#### Errors are loud

Any rejection inside the commit pass logs at `warn!` on the server.
Clients log at `warn!` on every `PredictionRejected`. Silence is never
the right behaviour for a rejected change.

#### Disk format

`dd40_chunk_storage` reads any known `ChunkVersion` (currently `V1` and
`V1Versioned`). The writer's choice is fixed at plugin startup from the
`DD40_CHUNK_STORAGE__SAVE_HISTORY` env var (truthy: `1|true|yes|on`,
default `false`):
- `false` → `ChunkVersion::V1` — block data + `version`. Confirmed
  history is dropped on save (logged at `debug!`).
- `true` → `ChunkVersion::V1Versioned` — block data + `version` +
  `confirmed_history`. Required for the server to serve delta updates
  after a restart.

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

## Logging

Use Bevy's structured log macros — `debug!`, `info!`, `warn!`, `error!` — for all diagnostic output. Never use `println!`, `eprintln!`, `print!`, or `dbg!` in production or test code. These produce unstructured output that bypasses the log filter and pollutes CI.

## Circular Dev-Dependency Rule

If crate A has `dd40_B` as a **dev-dependency** and `dd40_B` already depends on `dd40_A` at runtime, **do not write tests in A that use types from B**. Cargo compiles A twice (once as a library for B, once as the test binary), giving every type defined in A a different `TypeId`. ECS queries will find 0 entities even when the components are present.

The correct fix is to write those integration tests in B (where A is a regular dep), so A is only compiled once and `TypeId`s are consistent.

## Adding New Functionality

New features belong in a new crate or plugin, not bolted onto existing crates. When in doubt, create a new plugin. See `INCONSISTENCIES.md` for planned architectural clean-ups before introducing more coupling.

When encountering a bug: write a failing test first, fix it, confirm the test passes.

## Design Principle: Flexibility Over Convenience

**dd40 always favours flexibility and the ability to extend functionality from other crates.** This is especially true for code in `dd40_core` and other foundation crates: the entire point of this implementation is to be moddable and to let downstream crates change behaviour without having to fork the engine.

Concretely, when designing core systems:

- Prefer **extension points** (trait-object hooks, registries, validator chains, plugin-driven system sets) over hard-coded logic that downstream crates would have to fork to change.
- If a system has a single concrete behaviour today but is conceptually open-ended (e.g. "validate a chunk change", "decide what to do on death", "rank inventory slots"), expose it as a registered list of behaviours rather than inlining the one we happen to need.
- Accept a small amount of indirection cost for a large gain in extensibility. A `Vec<Box<dyn Validator>>` is fine.
- The cost of *not* doing this is that someone wanting to change the behaviour has to either fork the crate or carry an upstream patch. Both are unacceptable for the project's modding goals.

Concrete examples in the codebase:
- `BlockRegistry` is a runtime registry, not a hard-coded enum.
- The chunk authority's commit pass uses a registered chain of `ChunkChangeValidator`s, not an inlined match against built-in change types — so e.g. a character-collision check can live in a downstream crate that owns the relevant resources.
