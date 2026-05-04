# dd40_core

Tier 0 foundation crate for the dd40 voxel game. Supplies the shared
vocabulary that all other crates speak: block types and the registry, chunk
data structures, the app/game state machine, the tool system, and all the
messages and events that flow between subsystems.

`dd40_core` contains no game logic. Physics types and character types were
extracted to `dd40_physics_core` and `dd40_character_core` respectively.

## Module overview

```
src/
├── lib.rs             — public re-exports and the `prelude` module
├── plugin.rs          — CorePlugin (registers types, resources, messages, system sets)
├── state.rs           — AppState and GameState enums
├── loading.rs         — LoadingPlugin, LoadingTracker, LoadingSet
├── common.rs          — log_plugin() helper
├── debug.rs           — DebugInfo component (hook for debug overlays)
├── macros.rs          — ensure_plugins! macro
├── tools.rs           — ToolKindId, ToolTierId, ToolRegistry, ToolRegistrySet,
│                        mining_duration()
├── block/
│   ├── mod.rs         — Block, BlockId, BlockPos, BlockCoord, CollisionShape
│   ├── registry.rs    — BlockDefinition, BlockRegistry, BlockRegistrySet
│   └── events.rs      — PlaceBlockRequest, BlockPlaced, BlockRemoved, BlockChanged,
│                        StartMiningRequest, AbortMiningRequest, MineBlockRequest
├── chunk/
│   ├── mod.rs         — Chunk, ChunkPos, CHUNK_SIZE_* constants
│   ├── cache.rs       — ChunkCache resource, ChunkCachePlugin
│   └── events.rs      — GenerateChunk, RequestChunk, ChunkReady
└── world/
    └── mod.rs         — WorldGenerationSet system set
```
