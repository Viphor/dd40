# dd40 — Repository Structure

This document describes the role and internal layout of every crate in the
workspace. Keep it up to date whenever a crate is added, removed, or
significantly restructured. Per-crate `README.md` files contain the same
overview for quick navigation from an IDE; this file is the authoritative
single-page reference.

---

## Dependency rules

1. Every non-core crate may depend **only** on `dd40_core` and external
   libraries. No non-core crate may import another dd40 crate.
2. `dd40_core` may depend only on external libraries.
3. `dd40_client` and `dd40_server` are configuration crates — they are the
   only crates allowed to depend on multiple dd40 crates at once.

---

## Crate inventory

| Crate | Binary? | Role | Depends on (dd40) |
|---|---|---|---|
| `dd40_core` | — | Shared types, registry, physics, messages/events | — |
| `dd40_vanilla_palette` | — | Vanilla blocks, tool kinds, and tool tiers | `dd40_core` |
| `dd40_world` | — | World generation | `dd40_core` |
| `dd40_chunk_storage` | — | Disk-backed chunk persistence | `dd40_core` |
| `dd40_renderer` | — | Greedy-mesh chunk renderer, LOD | `dd40_core` ¹ |
| `dd40_player` | — | Player input, camera, block interaction, mining | `dd40_core` |
| `dd40_network` | — | lightyear transport, protocol, replication | `dd40_core` |
| `dd40_debug_ui` | — | Debug overlay (FPS, stats, orientation gizmo) | `dd40_core` |
| `dd40_gui` | — | In-game HUD (crosshair, etc.) | `dd40_core` |
| `dd40_client` | ✓ | Default playable client | all relevant |
| `dd40_server` | ✓ | Default headless server | all relevant |

¹ `dd40_renderer` currently also depends on `dd40_player` (inconsistency — see
`INCONSISTENCIES.md`).

---

## Crate details

### `dd40_core`

Foundation crate. Supplies the shared vocabulary every other crate speaks.
The physics engine is the single intentional piece of game logic here — almost
every character-related crate needs it, and extracting it into a separate crate
would just force every consumer to take an extra dependency anyway.

```
src/
├── lib.rs             — public re-exports and prelude
├── plugin.rs          — CorePlugin (ToolRegistry, system-set ordering)
├── state.rs           — AppState, GameState
├── loading.rs         — LoadingPlugin, LoadingTracker, LoadingSet
├── common.rs          — log_plugin() helper
├── debug.rs           — DebugInfo component
├── tools.rs           — ToolKindId, ToolTierId, ToolRegistry, ToolRegistrySet,
│                        EquippedTool, mining_duration()
├── block/
│   ├── mod.rs         — Block, BlockId, BlockPos, BlockCoord
│   ├── registry.rs    — BlockDefinition (toughness, preferred_tool, is_destructible),
│   │                    BlockRegistry, BlockRegistrySet
│   └── events.rs      — PlaceBlockRequest, BlockPlaced, BlockRemoved, BlockChanged,
│                        StartMiningRequest, AbortMiningRequest, MineBlockRequest
├── chunk/
│   ├── mod.rs         — Chunk, ChunkPos, CHUNK_SIZE_* constants
│   ├── cache.rs       — ChunkCache, ChunkCachePlugin
│   └── events.rs      — GenerateChunk, RequestChunk, ChunkReady
├── character/
│   ├── mod.rs         — Character, Player, MovementSpeed, JumpImpulse, SpawnPosition,
│   │                    CharacterBundle, CharacterRenderSet
│   ├── builder.rs     — CharacterBuilder
│   ├── controller.rs  — CharacterController, CharacterInput
│   ├── plugin.rs      — CharacterPlugin
│   └── physics/
│       ├── mod.rs            — PhysicsPlugin, PhysicsSet, CollisionShape, CharacterCollider,
│       │                       PhysicsBody, PhysicsConfig, Velocity, GravityScale,
│       │                       Grounded, Impulse, Aabb, CharacterPosition
│       ├── integration.rs    — gravity + velocity integration
│       ├── block_collision.rs — O(1) voxel AABB resolution
│       ├── character_collision.rs — character-vs-character push-apart
│       └── spatial_cache.rs  — CharacterSpatialCache
└── world/
    └── mod.rs         — WorldGenerationSet system set
```

---

### `dd40_vanilla_palette`

All vanilla game content: block definitions, tool kinds, and tool tiers.
Nothing in this crate is required by the engine — it is purely content that
ships with the default game configuration.  Modders can add their own palette
crate alongside this one without touching core.

```
src/
├── lib.rs       — VanillaPalettePlugin (composes VanillaToolsPlugin + VanillaBlocksPlugin)
├── blocks.rs    — VanillaBlocks constants, VanillaBlocksPlugin
│                  (stone, dirt, grass, sand, wood, leaves — with toughness and
│                   preferred_tool values; registered in BlockRegistrySet)
└── tools.rs     — VanillaToolKinds / VanillaToolTiers constants, VanillaToolsPlugin
                   (HAND, PICKAXE, AXE, SHOVEL, HOE, SHEARS + WOOD/STONE/IRON/DIAMOND/GOLD
                    tiers; registered in ToolRegistrySet)
```

---

### `dd40_world`

World generation. Generic over the generator type so the algorithm can be swapped
without touching this crate.

```
src/
├── lib.rs
├── plugin.rs          — WorldPlugin<G: WorldGenerator + Resource + Clone>
└── generators/
    ├── mod.rs         — WorldGenerator trait
    └── flat.rs        — FlatWorldGenerator (no Default — callers supply BlockId layers)
```

---

### `dd40_chunk_storage`

Disk-backed chunk persistence. Reads/writes chunks as binary files. Delegates
missing chunks to the generation pipeline via `GenerateChunk` messages.

```
src/
├── lib.rs             — plugin wiring, channel newtypes, dispatch/collect systems
├── plugin.rs          — DiskStoragePlugin
├── provider.rs        — DiskChunkProvider (async file I/O)
└── serialization/
    ├── mod.rs         — versioned entry point
    └── v1.rs          — version-1 bincode format
```

---

### `dd40_renderer`

Greedy-mesh chunk renderer. Listens for `ChunkReady` messages and produces
optimised Bevy meshes off the main thread.

```
src/
├── lib.rs
├── systems.rs         — dirty tracking, task spawning, task application
├── chunk_mesh.rs      — per-chunk meshing orchestrator
├── face_culling.rs    — visible-face determination
├── greedy_mesh.rs     — maximal-quad merging
├── mesh_builder.rs    — Bevy Mesh construction
├── mesh_task.rs       — MeshData, PendingMeshTasks
├── lod.rs             — LodLevel, LodConfig
└── render_state.rs    — per-chunk RenderState
```

---

### `dd40_player`

Player input, camera, and block interaction (including mining).

```
src/
├── lib.rs                     — PlayerInputPlugin, player spawning, camera follow, input mapping
└── block_interaction/
    ├── mod.rs                 — BlockInteractionPlugin, BlockInteractionConfig
    ├── targeting.rs           — TargetedBlock (pos + face + block_id), BlockFace, DDA ray-cast
    ├── placement.rs           — HeldBlock, placement
    └── mining.rs              — MiningState, update_mining, apply_removed_blocks
```

---

### `dd40_network`

lightyear-based networking. Feature-flagged `client`/`server`.

```
src/
├── lib.rs
├── protocol.rs        — shared protocol definitions (messages + directions)
├── shared/
│   ├── mod.rs
│   ├── character.rs
│   └── connection.rs  — SHARED_SETTINGS, address constants
├── client/
│   ├── mod.rs
│   ├── plugin.rs      — ClientNetworkPlugin
│   ├── connection.rs  — DDClient config
│   ├── character.rs   — frame interpolation, visual correction
│   ├── chunk_provider.rs
│   ├── block_placement.rs
│   ├── block_mining.rs — send_{start,abort,mine}_block; receive_removed_blocks
│   ├── loading.rs
│   └── spawn.rs
└── server/
    ├── mod.rs
    ├── plugin.rs      — ServerNetworkPlugin
    ├── connection.rs  — DDServer config, LinkConditioner
    ├── character.rs
    ├── chunk_provider.rs
    ├── chunk_requests.rs
    ├── block_placement.rs
    ├── block_mining.rs — MiningSession component; receive_{start,abort,mine}_block
    ├── user.rs
    └── spawn.rs       — WorldSpawnConfig, PlayerLocations
```

---

### `dd40_debug_ui`

Debug overlay.

```
src/
├── lib.rs               — DebugUiPlugin
├── custom.rs            — DebugUiElementRoot, custom element systems
└── orientation_gizmo.rs — OrientationGizmoPlugin
```

---

### `dd40_gui`

In-game HUD.

```
src/
├── lib.rs
├── plugin.rs  — GuiPlugin
└── crosshair.rs
```

---

### `dd40_client`

Default client binary. Configuration only.

```
src/
└── main.rs   — DefaultPlugins + CorePlugin + VanillaPalettePlugin + PlayerInputPlugin
               + RendererPlugin + ClientNetworkPlugin + DebugUiPlugin + GuiPlugin
```

---

### `dd40_server`

Default server binary. Configuration only.

```
src/
└── main.rs   — MinimalPlugins + CorePlugin + VanillaPalettePlugin + WorldPlugin
               + DiskStoragePlugin + ServerNetworkPlugin
```
