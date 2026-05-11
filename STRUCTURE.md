# dd40 — Repository Structure

This document describes the role and internal layout of every crate in the
workspace. Keep it up to date whenever a crate is added, removed, or
significantly restructured. Per-crate `README.md` files contain the same
overview for quick navigation from an IDE; this file is the authoritative
single-page reference.

---

## Three-tier dependency model

| Tier | Description | May depend on |
|---|---|---|
| **Tier 0 — Foundation** | Types, components, system sets — no game behaviour | Other foundation crates, external libraries |
| **Tier 1 — Implementation** | Systems and concrete game behaviour | Any foundation crates, external libraries. Must call `ensure_plugins!` |
| **Tier 2 — Binary** | Client and server binaries | Any dd40 crate |
**Tier 1 crates must not depend on other Tier 1 crates.** If two implementation
crates need to share data, that data belongs in a foundation crate.

There are currently no tracked exceptions to this rule.

---

## Crate inventory

### Tier 0 — Foundation

| Crate | Role | Depends on (dd40) |
|---|---|---|
| `dd40_core` | Block registry, chunk pipeline, app state, tools, messages | — |
| `dd40_physics_core` | Physics types, components, system sets | `dd40_core` |
| `dd40_character_core` | Character types, input bridge, `MiningState`, `TargetedBlock`, `PlayerId`, render sets | `dd40_core` |
| `dd40_item_core` | Item registry, `ActiveItem`, `RequestActiveItem`, `ActiveItemChanged` | `dd40_core` |

### Tier 1 — Implementation

| Crate | Role | Depends on (dd40) |
|---|---|---|
| `dd40_physics` | Gravity integration, block collision, character collision | `dd40_core`, `dd40_physics_core` |
| `dd40_integration_character_physics` | Bridges `CharacterInput` → physics `Impulse` (the only crate that knows about both `dd40_character_core` and `dd40_physics_core`) | `dd40_core`, `dd40_character_core`, `dd40_physics_core` |
| `dd40_vanilla_palette` | Vanilla block/tool definitions (IDs 0–999) | `dd40_core` |
| `dd40_world` | World generation (generic over `WorldGenerator` trait) | `dd40_core` |
| `dd40_chunk_storage` | Disk-backed chunk persistence (bincode v1) | `dd40_core` |
| `dd40_renderer` | Greedy-mesh renderer, async mesh tasks, LOD | `dd40_core`, `dd40_physics_core` |
| `dd40_player_input` | Keyboard/mouse → `CharacterInput`, first-person camera, `PlayerMode` | `dd40_core`, `dd40_physics_core`, `dd40_character_core` |
| `dd40_character_interaction` | Block targeting, mining, placement for any `Character` entity | `dd40_core`, `dd40_physics_core`, `dd40_character_core` |
| `dd40_network` | lightyear client-server networking (feature-gated) | `dd40_core`, `dd40_physics_core`, `dd40_character_core` |
| `dd40_debug_ui` | FPS overlay, orientation gizmo, custom debug elements | `dd40_core` |
| `dd40_gui` | In-game HUD with no character coupling (crosshair) | `dd40_core` |
| `dd40_character_gui` | Visuals keyed off character vocabulary: targeted-block highlight, mining break overlay | `dd40_core`, `dd40_character_core` |

### Tier 2 — Binary

| Crate | Plugins wired |
|---|---|
| `dd40_client` | `CorePlugin`, `PhysicsPlugin`, `VanillaPalettePlugin`, `PlayerInputPlugin`, `RendererPlugin`, `ClientNetworkPlugin`, `DebugUiPlugin`, `GuiPlugin` |
| `dd40_server` | `CorePlugin`, `PhysicsPlugin`, `VanillaPalettePlugin`, `DiskStoragePlugin`, `WorldPlugin`, `ServerNetworkPlugin` |

---

## Crate details

### `dd40_core`

Foundation crate. Supplies the shared vocabulary every other crate speaks:
block types, the registry, chunk data structures, app/game state, tool system,
and all messages that flow between subsystems.

```
src/
├── lib.rs             — public re-exports and prelude
├── plugin.rs          — CorePlugin (system-set ordering, message registration)
├── state.rs           — AppState, GameState
├── loading.rs         — LoadingPlugin, LoadingTracker, LoadingSet
├── common.rs          — log_plugin() helper
├── debug.rs           — DebugInfo component
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
│   ├── cache.rs       — ChunkCache, ChunkCachePlugin
│   └── events.rs      — GenerateChunk, RequestChunk, ChunkReady
└── world/
    └── mod.rs         — WorldGenerationSet system set
```

---

### `dd40_physics_core`

Foundation crate. Defines all physics types, components, and system sets.
No game logic — only the shared vocabulary for physics behaviour.

```
src/
├── lib.rs
├── plugin.rs          — PhysicsCorePlugin
├── prelude.rs         — re-exports of all stable public types
├── components.rs      — PhysicsBody, CharacterPosition, Velocity, GravityScale,
│                        Grounded, Impulse, CharacterCollider, Aabb
├── resources/
│   ├── mod.rs         — PhysicsConfig (gravity, ground_friction, air_friction,
│   │                    terminal_velocity)
│   └── spatial_cache.rs — CharacterSpatialCache
└── system_sets.rs     — PhysicsSet (InputSync → Integrate → BlockCollision →
                         CharacterCollision → Finalise)
```

---

### `dd40_character_core`

Foundation crate. Defines character-related types, the input bridge,
`MiningState`, `TargetedBlock`, `PlayerId`, the per-character face anchor,
and the render-frame system set.

```
src/
├── lib.rs
├── plugin.rs          — CharacterCorePlugin
├── prelude.rs         — re-exports of all stable public types
├── components.rs      — Character, Player, PlayerId, MovementSpeed, JumpImpulse,
│                        SpawnPosition
├── bundles.rs         — CharacterBundle (incl. MiningState, TargetedBlock)
├── builder.rs         — CharacterBuilder (spawn / attach attach a face child)
├── controller.rs      — CharacterController, CharacterInput (types only;
│                        the apply_character_controller system lives in
│                        dd40_integration_character_physics)
├── face.rs            — CharacterFace, CameraRotation, MouseSensitivity,
│                        DEFAULT_FACE_OFFSET — eye/head anchor that lives on
│                        a child entity of every Character
├── mining_state.rs    — MiningState (per-character Component)
├── targeted_block.rs  — TargetedBlock (per-character Component), BlockFace
└── system_sets.rs     — CharacterRenderSet (FrameInterpolation → CameraSync)
```

#### `CharacterBuilder` and the extension-trait pattern

`CharacterBuilder` is the **only** sanctioned way to spawn a character.
Every spawn site (single-player, server, predicted client) goes through
it.  Bypassing the builder risks forgetting to insert `Transform` before
`PhysicsBody`, which silently leaves `CharacterPosition` at `Vec3::ZERO`.

The builder owns three in-crate methods (which only need types from
`dd40_character_core` itself):

- `with_player()` — adds the `Player` marker.
- `with_controller()` — adds `(CharacterInput, CharacterController, JumpImpulse)`.
- `with_extra(|e| ...)` / `add_extra(|e| ...)` — pushes an arbitrary
  insertion closure onto the builder.

External capability crates extend the builder via **extension traits
implemented as a blanket impl on any `T: AddExtra`**.  This lets a crate
add a `with_*()` method to `CharacterBuilder` without any of the
character-core crates needing to depend on it.  The pattern:

```rust
// In your capability crate (depends on dd40_core only):
use dd40_core::builder_extra::AddExtra;

pub trait CharacterFooExt: Sized {
    fn with_foo(self, cfg: FooConfig) -> Self;
}

impl<T: AddExtra> CharacterFooExt for T {
    fn with_foo(mut self, cfg: FooConfig) -> Self {
        self.add_extra(move |e| { e.insert((Foo, cfg)); });
        self
    }
}
```

Existing extension traits in the workspace:

| Crate | Trait | Methods |
|---|---|---|
| `dd40_physics_core` | `CharacterPhysicsExt` | `with_physics()`, `with_physics_config(cfg)` |
| `dd40_network` (server) | `CharacterServerNetworkExt` | `with_server_replication(client_id, spawn_pos, owner)` |
| `dd40_network` (client) | `CharacterClientNetworkExt` | `with_predicted_local_player(initial_pos)` |

A typical full chain:

```rust
CharacterBuilder::new("Player")
    .transform(Transform::from_translation(spawn_pos))
    .with_physics()
    .with_controller()
    .with_player()
    .spawn(&mut commands);
```

---

### `dd40_item_core`

Foundation crate. Defines the item registry, the per-character
`ActiveItem` component, and the inventory-facing messages
(`RequestActiveItem`, `ActiveItemChanged`).  Contains no game logic and
no inventory layout — implementation crates such as
`dd40_vanilla_inventory` provide the storage and selection systems.

```
src/
├── lib.rs
├── plugin.rs        — ItemCorePlugin
├── prelude.rs       — re-exports of all stable public types
├── registry.rs      — ItemId, ItemDefinition, ItemRegistry, ItemRegistrySet,
│                       ToolBehavior
├── active_item.rs   — ActiveItem (per-character Component), ItemStack
└── messages.rs      — RequestActiveItem (Message), ActiveItemChanged (Event),
                        ItemSelector
```

---

### `dd40_physics`

Implementation crate. Contains all physics simulation systems:
gravity integration, block-collision resolution, and character-vs-character
push-apart. Inserts a `TentativePosition` component (internal to this crate)
on every `PhysicsBody` entity via an observer.

```
src/
├── lib.rs
├── plugin.rs             — PhysicsPlugin (wires sub-plugins; ensure_plugins!)
├── integration.rs        — gravity + velocity → tentative position
├── block_collision.rs    — O(1) voxel AABB resolution
└── character_collision.rs — character-vs-character push-apart
```

---

### `dd40_vanilla_palette`

All vanilla game content: block definitions, tool kinds, and tool tiers.
Nothing here is required by the engine — it is purely content that ships
with the default game configuration.

```
src/
├── lib.rs       — VanillaPalettePlugin (composes VanillaToolsPlugin + VanillaBlocksPlugin)
├── blocks.rs    — VanillaBlocks constants, VanillaBlocksPlugin
└── tools.rs     — VanillaToolKinds / VanillaToolTiers constants, VanillaToolsPlugin
```

---

### `dd40_world`

World generation. Generic over the generator type so the algorithm can be
swapped without touching this crate.

```
src/
├── lib.rs
├── plugin.rs          — WorldPlugin<G: WorldGenerator + Resource + Clone>
└── generators/
    ├── mod.rs         — WorldGenerator trait
    └── flat.rs        — FlatWorldGenerator
```

---

### `dd40_chunk_storage`

Disk-backed chunk persistence. Delegates missing chunks to the generation
pipeline via `GenerateChunk` messages.

```
src/
├── lib.rs             — plugin wiring, channel newtypes, dispatch/collect systems
├── plugin.rs          — DiskStoragePlugin
├── provider.rs        — DiskChunkProvider (async file I/O via crossbeam channels)
└── serialization/
    ├── mod.rs         — versioned entry point
    └── v1.rs          — version-1 bincode format
```

---

### `dd40_renderer`

Greedy-mesh chunk renderer. Listens for `ChunkReady` messages and produces
optimised Bevy meshes off the main thread. LOD is anchored to
`CharacterPosition` (from `dd40_physics_core`).

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

### `dd40_player_input`

Translates keyboard and mouse input into `CharacterInput` on the player entity,
drives the first-person camera, and manages the `PlayerMode` state.

```
src/
├── lib.rs
├── plugin.rs          — PlayerInputPlugin
├── components.rs      — PlayerMode, CameraRotation, MouseSensitivity
├── state.rs           — PlayerMode state transitions
└── systems.rs         — input mapping, camera follow systems
```

---

### `dd40_character_interaction`

Block targeting (DDA ray-cast), mining, and placement for any `Character`
entity. Re-exports `MiningState`, `TargetedBlock`, and `BlockFace` from
`dd40_character_core` for backwards compatibility.

```
src/
├── lib.rs             — CharacterInteractionPlugin, public re-exports
├── plugin.rs          — system wiring, ensure_plugins!
├── targeting.rs       — DDA ray-cast, BlockInteractionConfig
├── placement.rs       — block placement (reads ActiveItem)
└── mining.rs          — mining state update, block removal
```

---

### `dd40_network`

lightyear-based networking, feature-gated `client`/`server`.

```
src/
├── lib.rs
├── protocol.rs        — shared protocol (messages + directions)
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
│   ├── block_mining.rs
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
    ├── block_mining.rs — MiningSession component
    ├── user.rs
    └── spawn.rs       — WorldSpawnConfig, PlayerLocations
```

---

### `dd40_debug_ui`

Debug overlay with FPS counter, orientation gizmo, and a host for custom
`DebugInfo` elements.

```
src/
├── lib.rs               — DebugUiPlugin
├── custom.rs            — DebugUiElementRoot, custom element systems
└── orientation_gizmo.rs — OrientationGizmoPlugin
```

---

### `dd40_gui`

In-game HUD with no character coupling. Visuals that depend on
character vocabulary (e.g. the targeted-block highlight) live in
`dd40_character_gui` instead.

```
src/
├── lib.rs
├── plugin.rs  — GuiPlugin
└── crosshair.rs
```

---

### `dd40_character_gui`

Gizmo and HUD rendering for character-related state: the targeted-block
highlight and the mining break overlay. Wired into `dd40_client` only
— never the headless server.

```
src/
├── lib.rs
├── plugin.rs           — CharacterGuiPlugin
└── block_highlight.rs  — BlockHighlightConfig + draw_targeted_block_highlight
                          (outline + mining break animation)
```

---

### `dd40_client`

Default client binary. Configuration only.

```
src/
└── main.rs   — DefaultPlugins + CorePlugin + PhysicsPlugin + VanillaPalettePlugin
               + PlayerInputPlugin + RendererPlugin + ClientNetworkPlugin
               + DebugUiPlugin + GuiPlugin + CharacterGuiPlugin
```

---

### `dd40_server`

Default server binary. Configuration only.

```
src/
└── main.rs   — MinimalPlugins + CorePlugin + PhysicsPlugin + VanillaPalettePlugin
               + DiskStoragePlugin + WorldPlugin + ServerNetworkPlugin
```
