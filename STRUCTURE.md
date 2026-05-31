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
| `dd40_inventory_core` | Pure-data `Inventory` + `InventoryComponent` (entity-keyed) + `BlockInventory` (block-cell, `BlockData`) + `DropItems` message + `CharacterInventoryExt` builder extension | `dd40_core`, `dd40_item_core` |
| `dd40_rng` | Pluggable `GameRng` resource, `RngPlugin` | — |
| `dd40_loot_core` | `LootTable`, `LootEntry`, `LootMode`; `BlockData` impl so tables can attach to `BlockDefinition` | `dd40_core`, `dd40_item_core` |
| `dd40_loose_item_core` | `LooseItem`, `DespawnTimer`, `PickupCooldown`, `LooseItemConfig`, `LooseItemSet` — vocabulary for items lying in the world | `dd40_core`, `dd40_item_core` |
| `dd40_input_core` | Shared `bevy_enhanced_input` action vocabulary, the `OnFoot` context, and the `InputTranslationSet` cross-crate ordering anchor (vocabulary only — no bindings, no systems) | — |

### Tier 1 — Implementation

| Crate | Role | Depends on (dd40) |
|---|---|---|
| `dd40_physics` | Gravity integration, block collision, character collision | `dd40_core`, `dd40_physics_core` |
| `dd40_integration_character_physics` | Bridges `CharacterInput` → physics `Impulse` (the only crate that knows about both `dd40_character_core` and `dd40_physics_core`) | `dd40_core`, `dd40_character_core`, `dd40_physics_core` |
| `dd40_vanilla_palette` | Vanilla block/tool definitions (IDs 0–999); attaches `LootTable` block-data | `dd40_core`, `dd40_item_core`, `dd40_loot_core` |
| `dd40_world` | World generation (generic over `WorldGenerator` trait) | `dd40_core` |
| `dd40_chunk_storage` | Disk-backed chunk persistence (bincode v1) | `dd40_core` |
| `dd40_renderer` | Greedy-mesh renderer, async mesh tasks, LOD | `dd40_core`, `dd40_physics_core` |
| `dd40_player_input` | Client-only: BEI bindings, `FreeCam`/`LocalUi` contexts, mouse-look (Controller mode), free-cam look + movement, pause/mode observers, and the `Action<T>` → `CharacterInput` translator (placed in `InputTranslationSet`) | `dd40_core`, `dd40_physics_core`, `dd40_character_core`, `dd40_input_core`, `dd40_item_core` |
| `dd40_character_interaction` | Block targeting, mining, placement for any `Character` entity | `dd40_core`, `dd40_physics_core`, `dd40_character_core` |
| `dd40_network` | lightyear client-server networking (feature-gated). Wire input is `ActionState<PlayerInput>` (lightyear `input_native`); the client bridges `CharacterInput → ActionState<PlayerInput>` after the translator runs (ordered via `InputTranslationSet`). Server does not load BEI. Also provides `ClientInventoryNetworkPlugin` (forwards local `SlotInteraction` as `NetSlotInteraction` over `InventoryChannel`) and `ServerInventoryNetworkPlugin` (drains incoming `NetSlotInteraction`, resolves the controlled `Character` via lightyear's `ControlledBy`, re-emits `SlotInteraction` for the apply system). Replicates `InventoryComponent` and `HeldStackComponent` server→client. | `dd40_core`, `dd40_physics_core`, `dd40_character_core`, `dd40_input_core`, `dd40_inventory_core` |
| `dd40_debug_ui` | FPS overlay, orientation gizmo, custom debug elements | `dd40_core` |
| `dd40_gui` | In-game HUD with no character coupling (crosshair) | `dd40_core` |
| `dd40_character_gui` | Visuals keyed off character vocabulary: targeted-block highlight, mining break overlay | `dd40_core`, `dd40_character_core` |
| `dd40_loot` | Server-only: turns accepted `ChunkChange::Remove` into `DropItems` messages, consulting cell-data and `BlockDefinition`-level `LootTable`s with `placeable`-item fallback | `dd40_core`, `dd40_item_core`, `dd40_inventory_core`, `dd40_loot_core`, `dd40_rng` |
| `dd40_loose_items` | Server-only: drains `DropItems` into spawned `LooseItem` entities (with physics body), ticks `DespawnTimer` / `PickupCooldown`, merges same-item stacks on `BodyBodyContact`, registers `LooseItemPersister` so loose items survive restart | `dd40_core`, `dd40_physics_core`, `dd40_item_core`, `dd40_inventory_core`, `dd40_loose_item_core` |
| `dd40_integration_loose_item_pickup` | Server-only: only crate where `LooseItem` and `InventoryComponent` meet — subscribes to `BodyBodyContact` and grants stacks to the closest eligible character | `dd40_core`, `dd40_character_core`, `dd40_inventory_core`, `dd40_item_core`, `dd40_loose_item_core`, `dd40_physics_core` |
| `dd40_loose_item_render` | Client-only: spinning, bobbing cube visual per `LooseItem` with placeable-block colour fallback | `dd40_core`, `dd40_item_core`, `dd40_loose_item_core` |
| `dd40_inventory` | Both sides: `InventoryPlugin` (selection / hotbar / `ActiveItem`) + `InventoryRulesPlugin` (apply system, **server-only** in networked builds; client-only in single-player) | `dd40_core`, `dd40_character_core`, `dd40_item_core`, `dd40_inventory_core`, `dd40_input_core` |
| `dd40_inventory_gui` | Client-only: hotbar widget, toggleable inventory grid window, per-slot widget with icon cache + colour fallback, held-stack cursor follower, click → `SlotInteraction` translator (forwarded to server by `dd40_network::ClientInventoryNetworkPlugin`) | `dd40_core`, `dd40_character_core`, `dd40_item_core`, `dd40_inventory_core`, `dd40_input_core` |

### Tier 2 — Binary

| Crate | Plugins wired |
|---|---|
| `dd40_client` | `CorePlugin`, `PhysicsPlugin`, `VanillaPalettePlugin`, `PlayerInputPlugin`, `RendererPlugin`, `ClientNetworkPlugin`, `DebugUiPlugin`, `GuiPlugin`, `CharacterGuiPlugin`, `LooseItemRenderPlugin`, `InventoryPlugin`, `InventoryGuiPlugin` |
| `dd40_server` | `CorePlugin`, `GracefulShutdownPlugin`, `PhysicsPlugin`, `IntegrationCharacterPhysicsPlugin`, `VanillaPalettePlugin`, `DiskStoragePlugin`, `WorldPlugin`, `CharacterInteractionPlugin`, `LootPlugin`, `LooseItemsPlugin`, `LooseItemPickupPlugin`, `ServerNetworkPlugin` |

---

## Crate details

### `dd40_core`

Foundation crate. Supplies the shared vocabulary every other crate speaks:
block types, the registry, chunk data structures, app/game state, tool system,
the cross-crate entity-persistence trait, the headless-binary shutdown helper,
and all messages that flow between subsystems.

```
src/
├── lib.rs               — public re-exports and prelude
├── plugin.rs            — CorePlugin (system-set ordering, message registration,
│                           initialises EntityPersisterRegistry)
├── state.rs             — AppState, GameState
├── loading.rs           — LoadingPlugin, LoadingTracker, LoadingSet
├── common.rs            — log_plugin() helper
├── debug.rs             — DebugInfo component
├── macros.rs            — ensure_plugins! macro
├── tools.rs             — ToolKindId, ToolTierId, ToolRegistry, ToolRegistrySet,
│                          mining_duration()
├── persistence.rs       — EntityPersister trait, EntityPersisterRegistry resource,
│                          PersistedEntity payload
├── graceful_shutdown.rs — GracefulShutdownPlugin: Ctrl-C / SIGTERM → AppExit for
│                          headless binaries with no windowing layer
├── block/
│   ├── mod.rs           — Block, BlockId, BlockPos, BlockCoord, CollisionShape
│   ├── registry.rs      — BlockDefinition, BlockRegistry, BlockRegistrySet
│   └── events.rs        — PlaceBlockRequest, BlockPlaced, BlockRemoved, BlockChanged,
│                          StartMiningRequest, AbortMiningRequest, MineBlockRequest
├── chunk/
│   ├── mod.rs           — Chunk, ChunkPos, CHUNK_SIZE_* constants
│   ├── cache.rs         — ChunkCache, ChunkCachePlugin
│   └── events.rs        — GenerateChunk, RequestChunk, ChunkReady
└── world/
    └── mod.rs           — WorldGenerationSet system set
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
├── components.rs      — PhysicsBody, PhysicsPosition, Velocity, GravityScale,
│                        Grounded, Impulse, PhysicsCollider, Aabb
├── resources/
│   ├── mod.rs         — PhysicsConfig (gravity, ground_friction, air_friction,
│   │                    terminal_velocity)
│   └── spatial_cache.rs — PhysicsSpatialCache
└── system_sets.rs     — PhysicsSet (InputSync → Integrate → BlockCollision →
                         BodyCollision → Finalise)
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
`PhysicsBody`, which silently leaves `PhysicsPosition` at `Vec3::ZERO`.

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
`dd40_inventory` provide the storage and selection systems.

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

### `dd40_inventory_core`

Foundation crate. Defines a passive inventory container that any
character entity can carry: the `Inventory` component, the
`InventoryChanged` entity event with per-slot diffs, and the
`CharacterInventoryExt` builder extension. Contains no hotbar,
selection, equipment, or UI logic — a future Tier 1 inventory-interaction
crate is expected to drain `RequestActiveItem` from `dd40_item_core`
using `Inventory::find_slot`.

```
src/
├── lib.rs
├── plugin.rs        — InventoryCorePlugin
├── prelude.rs       — re-exports of all stable public types
├── inventory.rs     — Inventory (Component), InventoryChanged (EntityEvent),
│                       SlotChange, InsertError, find_slot
└── character_ext.rs — CharacterInventoryExt: blanket on AddExtra
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
└── body_collision.rs     — body-vs-body push-apart (PhysicsCollider entities)
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
pipeline via `GenerateChunk` messages, flushes the in-memory `ChunkCache`
to disk on `AppExit`, and hosts the per-chunk **entity sidecar** layer —
the dispatch + I/O half of [`dd40_core::persistence::EntityPersister`].

```
src/
├── lib.rs                 — plugin wiring, channel newtypes, dispatch/collect systems
├── plugin.rs              — DiskStoragePlugin (also wires entity persistence + on-exit savers)
├── provider.rs            — DiskChunkProvider (async file I/O via crossbeam channels)
├── chunk_save_on_exit.rs  — saves every cached chunk on AppExit (idempotent Last-schedule system)
├── entity_persistence.rs  — EntityPersistenceConfig resource, load_entities_for_ready_chunks,
│                            save_entities_on_exit, save_all_entities; SAVE_ENTITIES_ENV
├── entity_sidecar.rs      — entities_X_Y_Z.bin file format (magic + version + coords + bincode body),
│                            EntitySidecarError
└── serialization/
    ├── mod.rs             — versioned entry point
    └── v1.rs              — version-1 bincode format
```

---

### `dd40_renderer`

Greedy-mesh chunk renderer. Listens for `ChunkReady` messages and produces
optimised Bevy meshes off the main thread. LOD is anchored to
`PhysicsPosition` (from `dd40_physics_core`).

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

Owns the client-side input pipeline (BEI bindings, contexts, mouse-look,
free-cam, pause/mode observers) **and** the headless `Action<T>` →
`CharacterInput` translator that runs on both client and server.

```
src/
├── lib.rs
├── plugin.rs          — PlayerInputPlugin (client), PlayerInputTranslationPlugin (translator)
├── bindings.rs        — spawn_local_player_input_tree (Added<Player>)
├── contexts.rs        — FreeCam, LocalUi (client-only)
├── translation.rs     — Action<T> → CharacterInput (headless-safe, both sides)
├── state.rs           — PlayerMode state transitions
└── systems.rs         — camera, cursor, RMB dispatch, free-cam, mouse-look
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
               + LooseItemRenderPlugin
```

---

### `dd40_server`

Default server binary. Configuration only.  Adds `GracefulShutdownPlugin`
so Ctrl-C / SIGTERM trigger the `Last`-schedule chunk and entity sidecar
flushes instead of yanking the process.

```
src/
└── main.rs   — MinimalPlugins + CorePlugin + GracefulShutdownPlugin
               + PhysicsPlugin + IntegrationCharacterPhysicsPlugin
               + VanillaPalettePlugin + DiskStoragePlugin + WorldPlugin
               + CharacterInteractionPlugin + LootPlugin + LooseItemsPlugin
               + LooseItemPickupPlugin + ServerNetworkPlugin
```

---

### `dd40_loose_item_core`

Foundation crate.  The shared vocabulary for every system that touches
ground items: the `LooseItem` component, the per-item `DespawnTimer` and
`PickupCooldown`, the tunable `LooseItemConfig`, and the
`LooseItemSet` ordering (`Spawn` → `Attract` → `Resolve` → `Lifecycle`)
that downstream crates anchor against.

```
src/
├── lib.rs
├── plugin.rs       — LooseItemCorePlugin (registers types, inits config, configures set)
├── prelude.rs
├── components.rs   — LooseItem, DespawnTimer, PickupCooldown
├── resources.rs    — LooseItemConfig (default_lifetime, default_pickup_cooldown,
│                     attraction_radius, attraction_strength, merge_contact_duration)
└── system_sets.rs  — LooseItemSet
```

---

### `dd40_loose_items`

Server-only Tier-1 crate.  Drains
[`DropItems`](dd40_inventory_core::drop::DropItems) into spawned
`LooseItem` entities, merges same-item stacks that stay in contact for
`LooseItemConfig::merge_contact_duration`, ticks lifetimes, and
registers `LooseItemPersister` with the
[`EntityPersisterRegistry`](dd40_core::persistence::EntityPersisterRegistry)
so loose items survive a server restart.

```
src/
├── lib.rs
├── plugin.rs     — LooseItemsPlugin (also auto-registers LooseItemPersister)
├── spawn.rs      — loose_item_bundle (single source of truth for required components),
│                   spawn_loose_items, tick_lifetimes
├── merge.rs      — MergeAccumulator, accumulate_and_merge_loose_items
└── persister.rs  — LooseItemPersister, LooseItemPayload::V1, LOOSE_ITEM_KIND
```

---

### `dd40_integration_loose_item_pickup`

Server-only Tier-1 integration crate.  The only place in the workspace
where `LooseItem` and `InventoryComponent` meet — keeps both
foundations decoupled.  Listens for
[`BodyBodyContact`](dd40_physics_core::messages::BodyBodyContact),
selects the closest eligible character (lowest `Entity` id breaks
exact ties) with a free or stackable slot, and grants the stack.

```
src/
├── lib.rs       — public re-exports
├── plugin.rs    — LooseItemPickupPlugin
├── pickup.rs    — contact handler, candidate ranking, inventory grant
└── attract.rs   — magnetises eligible loose items toward nearby characters
                   inside LooseItemConfig::attraction_radius
```

---

### `dd40_loose_item_render`

Client-only Tier-1 crate.  Attaches a spinning, slowly bobbing cube
visual to every replicated `LooseItem`.  Colour resolves through a
fallback chain: custom item render (TODO) → placeable block colour →
neutral billboard.

```
src/
├── lib.rs
└── plugin.rs     — LooseItemRenderPlugin: spawns child-cube visuals,
                    drives bob + spin, resolves colour via the fallback chain
```
