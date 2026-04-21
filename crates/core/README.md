# dd40_core

Foundation crate for the dd40 voxel game. Every other dd40 crate depends on
this one and only this one among the dd40 family. It contains no game logic
beyond the physics engine (an intentional exception — see below).

`dd40_core` supplies the shared vocabulary that all other crates speak:
block types and the registry, chunk data structures, character components,
the physics engine, the app/game state machine, the loading tracker, and
all the messages and events that flow between subsystems.

## Module overview

```
src/
├── lib.rs             — public re-exports and the `prelude` module
├── plugin.rs          — CorePlugin (registers types, resources, messages, system sets)
├── state.rs           — AppState and GameState enums
├── loading.rs         — LoadingPlugin, LoadingTracker, LoadingSet
├── common.rs          — log_plugin() helper
├── debug.rs           — DebugInfo component (hook for debug overlays)
├── vanilla_blocks.rs  — VanillaBlocks constants and setup_vanilla_blocks system (planned: move to a separate crate)
│
├── block/
│   ├── mod.rs         — Block, BlockId, BlockPos, BlockCoord
│   ├── registry.rs    — BlockDefinition, BlockRegistry, BlockRegistrySet
│   └── events.rs      — PlaceBlockRequest, BlockPlaced, BlockRemoved, BlockChanged
│
├── chunk/
│   ├── mod.rs         — Chunk, ChunkPos, CHUNK_SIZE_* constants
│   ├── cache.rs       — ChunkCache resource, ChunkCachePlugin
│   └── events.rs      — GenerateChunk, RequestChunk, ChunkReady
│
├── character/
│   ├── mod.rs         — Character, Player, MovementSpeed, JumpImpulse, SpawnPosition, CharacterBundle, CharacterRenderSet
│   ├── builder.rs     — CharacterBuilder helper
│   ├── controller.rs  — CharacterController, CharacterInput
│   ├── plugin.rs      — CharacterPlugin
│   └── physics/
│       ├── mod.rs            — PhysicsPlugin, PhysicsSet, CollisionShape, CharacterCollider, PhysicsBody, PhysicsConfig, Velocity, GravityScale, Grounded, Impulse, Aabb, CharacterPosition
│       ├── integration.rs    — gravity and velocity integration
│       ├── block_collision.rs — O(1) voxel AABB resolution
│       ├── character_collision.rs — character-vs-character push-apart
│       └── spatial_cache.rs  — CharacterSpatialCache
│
└── world/
    └── mod.rs         — WorldGenerationSet system set
```

## Physics engine exception

The physics engine lives here rather than in a separate crate because almost
every character-related crate needs to understand collision shapes and movement.
It is the single intentional piece of game logic in `dd40_core`.
