# dd40 Modular Architecture Refactor — Spec

## Objective

Make dd40 fully plug-and-play. A developer building a custom client or server
should only need to add the implementation plugins they want; all foundation
dependencies are automatically satisfied with sensible defaults. Any subsystem
— physics, character control, block interaction — is independently swappable
by writing a single new crate that depends only on the relevant vocabulary
crates.

This spec drives three concrete changes:

1. **Tier split**: Extract physics and character vocabulary out of `dd40_core`
   into dedicated vocabulary crates (`dd40_physics_core`,
   `dd40_character_core`).
2. **System decomposition**: Move systems that belong to implementation crates
   into dedicated crates (`dd40_physics`, `dd40_player_movement`,
   `dd40_character_interaction`).
3. **Auto-plugin pattern**: Every plugin checks for its direct runtime
   dependencies and adds them with defaults when missing.

---

## Dependency Rules (updated)

The previous single rule ("every non-core crate depends only on `dd40_core`")
is replaced with a three-tier model:

### Tier 0 — Foundation crates
`dd40_core`, `dd40_physics_core`, `dd40_character_core`

- Contain types, components, system sets, resources, and messages — no game
  behaviour.
- May depend on other foundation crates (acyclically) and external libraries
  only.
- Must implement `Default` on every plugin so they can be auto-added.

### Tier 1 — Vocabulary crates *(deprecated term: previously "non-core crates")*
Same as foundation crates above. Use "foundation" going forward.

### Tier 2 — Implementation crates
`dd40_physics`, `dd40_vanilla_palette`, `dd40_world`,
`dd40_chunk_storage`, `dd40_renderer`, `dd40_character_interaction`,
`dd40_player_movement`, `dd40_network`, `dd40_debug_ui`, `dd40_gui`

- Contain systems, asset loading, and concrete behaviour.
- May depend on **any** foundation crates and external libraries.
- Must **not** depend on other implementation crates — only binary crates do
  that.
- Must check for all direct runtime dependencies in `Plugin::build` and add
  them with defaults when missing (see Auto-Plugin Pattern below).

### Tier 3 — Binary crates
`dd40_client`, `dd40_server`

- May depend on any dd40 crate.
- Wire together the desired set of implementation plugins.
- The ideal binary `main.rs` should be a flat list of implementation plugins
  — no foundation plugins needed because auto-plugin handles them.

---

## Auto-Plugin Pattern

Every implementation plugin checks **all of its direct runtime dependencies**,
not only those it trusts its Cargo dependency to inject. This prevents silent
breakage if a dependency's own auto-plugin behaviour changes.

### `ensure_plugins!` macro

The check boilerplate is encapsulated in a `macro_rules!` macro defined in
`dd40_core` and re-exported from every foundation crate's prelude:

```rust
/// Checks each listed plugin and adds it with `Default::default()` if not
/// already present.  Always call this at the top of `Plugin::build`.
#[macro_export]
macro_rules! ensure_plugins {
    ($app:expr, $($plugin:ty),+ $(,)?) => {
        $(
            if !$app.is_plugin_added::<$plugin>() {
                $app.add_plugins(<$plugin>::default());
            }
        )+
    };
}
```

`ensure_plugins!` is the **only** approved way to write the auto-plugin check
— never write the `if !app.is_plugin_added` block by hand.

With the macro, a typical implementation plugin looks like:

```rust
impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin, PhysicsCorePlugin);
        // add systems ...
    }
}
```

Foundation plugins apply the same rule:

```rust
impl Plugin for PhysicsCorePlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin);
        // register types, configure system sets ...
    }
}
```

The only plugins exempt from this rule are `CorePlugin` itself (nothing to
check) and binary crates (they explicitly manage their own plugin list).

---

## New Crate Layout

```
Foundation
  dd40_core              — AppState, LoadingTracker, BlockRegistry, chunk
                           pipeline messages; infrastructure only
  dd40_physics_core      — Aabb, CollisionShape*, Velocity, Impulse,
                           GravityScale, CharacterPosition,
                           TentativePosition, Grounded, PhysicsBody,
                           CharacterCollider, PhysicsConfig, PhysicsSet
  dd40_character_core    — Player, Character, CharacterInput,
                           MovementSpeed, JumpImpulse, CharacterBuilder,
                           CharacterBundle, CharacterRenderSet, SpawnPosition

Implementation
  dd40_physics           — Integration, BlockCollision, CharacterCollision
                           systems (moved from dd40_core::character::physics)
  dd40_vanilla_palette   — Vanilla block/tool definitions (unchanged)
  dd40_world             — WorldGenerator trait + flat generator (unchanged)
  dd40_chunk_storage     — Disk-backed chunk persistence (unchanged)
  dd40_character_interaction — Block targeting, placement, mining systems,
                           generic over Character (moved from dd40_player)
  dd40_player_movement   — Keyboard/mouse → CharacterInput, first-person
                           camera systems (moved from dd40_player)
  dd40_renderer          — Greedy mesh, LOD (unchanged; reads PlayerPosition
                           from character_core once LOD inconsistency fixed)
  dd40_network           — lightyear networking (unchanged)
  dd40_debug_ui          — FPS overlay (unchanged)
  dd40_gui               — HUD (unchanged)

Binary
  dd40_client            — wires implementation plugins for the playable client
  dd40_server            — wires implementation plugins for the headless server
  dd40_player            — thin convenience plugin: re-exports
                           PlayerMovementPlugin + CharacterInteractionPlugin
                           for the standard first-person player experience
                           (kept so existing code that adds PlayerPlugin
                           continues to work)
```

*`CollisionShape` stays in `dd40_core` as part of `BlockDefinition`. Physics
systems read it from the `BlockRegistry`. This avoids making `dd40_core`
depend on `dd40_physics_core`.*

---

## Character System Generalisation

All systems that currently gate on `With<Player>` and deal with generic
character capabilities (block targeting, mining, placement, movement
application) are rewritten to gate on `With<Character>` instead.

The `Player` marker remains; it is used only by systems that are genuinely
player-exclusive (e.g. keyboard/mouse input, first-person camera lock).

A developer can give any entity the `Character` marker plus
`CharacterInteractionPlugin`'s required components and that entity will be
able to target and interact with blocks. No player-specific code is needed.

---

## Task Plan

Tasks are ordered so that later tasks always build on stable conventions.
Documentation changes come first so every subsequent code change follows the
new rules from the start.

### Phase 0 — Convention changes (no code moved)

**Task 0.1** Update `dd40-architecture` SKILL.md
- Replace the single dependency rule with the three-tier model
  (Foundation / Implementation / Binary).
- Add the Auto-Plugin Pattern section.
- Replace all occurrences of "non-core crates" with "implementation crates".

**Task 0.2** Update `CLAUDE.md` architecture section
- Expand the dependency rules table with the three tiers.
- Add a short Auto-Plugin Pattern example.

**Task 0.3** Update `INCONSISTENCIES.md`
- Add planned items for: physics in core (not yet extracted), character types
  in core (not yet extracted), player-gated systems (not yet generalised).
- Mark existing items 1–4 as "tracked in SPEC.md" so they don't duplicate.

---

### Phase 1 — Extract `dd40_physics_core` (foundation crate)

**Task 1.1** Create crate skeleton
- `crates/physics_core/Cargo.toml` depending on `dd40_core` + bevy.
- Empty `lib.rs` with module stubs.

**Task 1.2** Move physics vocabulary to `dd40_physics_core`
- Move: `Aabb`, `Velocity`, `Impulse`, `GravityScale`, `CharacterPosition`,
  `TentativePosition`, `Grounded`, `PhysicsBody`, `CharacterCollider`,
  `PhysicsConfig`, `PhysicsSet` from `dd40_core::character::physics`.
- `PhysicsCorePlugin`: registers all types, inserts `PhysicsConfig::default()`,
  configures `PhysicsSet` ordering; checks for `CorePlugin`.

**Task 1.3** Create `dd40_physics` (implementation crate)
- `crates/physics/Cargo.toml` depending on `dd40_physics_core` +
  `dd40_core` + bevy.
- Move `IntegrationPlugin`, `BlockCollisionPlugin`, `CharacterCollisionPlugin`
  here verbatim.
- `PhysicsPlugin`: calls `ensure_plugins!(app, CorePlugin, PhysicsCorePlugin)`,
  then composes the three system plugins.

**Task 1.4** Update `dd40_core`
- Remove `character::physics` module and all re-exports.
- Remove `PhysicsPlugin` from `CharacterPlugin`.
- Add `dd40_physics_core` as a dev-dependency only (for doc-tests if needed).
- Update `CharacterPlugin` to no longer add physics (physics is now
  opt-in via `PhysicsPlugin`).

**Task 1.5** Update consumers
- `dd40_client/main.rs`: add `PhysicsPlugin`.
- `dd40_server/main.rs`: add `PhysicsPlugin`.
- Fix any import paths in `dd40_network`, `dd40_renderer` that referenced
  physics types from `dd40_core`.

**Task 1.6** Verify — `cargo build --workspace` and `cargo test --workspace`
pass green.

---

### Phase 2 — Extract `dd40_character_core` (foundation crate)

**Task 2.1** Create crate skeleton
- `crates/character_core/Cargo.toml` depending on `dd40_core` +
  `dd40_physics_core` + bevy.

**Task 2.2** Move character vocabulary
- Move: `Player`, `Character`, `CharacterInput`, `MovementSpeed`,
  `JumpImpulse`, `CharacterBuilder`, `CharacterBundle`, `CharacterRenderSet`,
  `SpawnPosition` from `dd40_core::character`.
- `CharacterCorePlugin`: registers all types, configures
  `CharacterRenderSet` ordering; checks for `CorePlugin` and
  `PhysicsCorePlugin`.
- `CharacterControllerPlugin` moves here (it reads `CharacterInput` and writes
  to physics components — pure vocabulary wiring).

**Task 2.3** Update `dd40_core`
- Remove `character` module entirely.
- Remove `CharacterPlugin` from `CorePlugin`.
- `CorePlugin` now contains only: `BlockRegistry`, chunk pipeline messages,
  `AppState`, `GameState`, `LoadingTracker`, `StatesPlugin` check.

**Task 2.4** Update consumers
- `dd40_player`, `dd40_network`, `dd40_renderer`, `dd40_debug_ui` — update all
  imports from `dd40_core::character::*` to `dd40_character_core::*`.
- Add `dd40_character_core` to each consumer's `Cargo.toml`.

**Task 2.5** Verify — full workspace build + tests green.

---

### Phase 3 — Decompose `dd40_player` into implementation crates

**Task 3.1** Create `dd40_player_movement`
- `crates/player_movement/Cargo.toml` depending on `dd40_character_core` +
  `dd40_physics_core` + `dd40_core` + bevy.
- Move: `PlayerMode` state, `MouseSensitivity`, `CameraRotation`, and all
  input+camera systems from `dd40_player`.
- `PlayerMovementPlugin`: checks for `CorePlugin`, `PhysicsCorePlugin`,
  `CharacterCorePlugin`.

**Task 3.2** Create `dd40_character_interaction`
- `crates/character_interaction/Cargo.toml` depending on
  `dd40_character_core` + `dd40_physics_core` + `dd40_core` + bevy.
- Move: `BlockInteractionPlugin` and mining systems from `dd40_player`.
- Generalise: replace all `With<Player>` with `With<Character>` in these
  systems (targeting, mining, placement).
- Move `MiningState`, `TargetedBlock`, `HeldBlock`, `BlockInteractionConfig`,
  `BlockFace` here (resolves INCONSISTENCIES.md item 3).
- `CharacterInteractionPlugin`: checks for `CorePlugin`, `PhysicsCorePlugin`,
  `CharacterCorePlugin`.

**Task 3.3** Slim `dd40_player` to a convenience plugin
- `dd40_player` re-exports `PlayerMovementPlugin` + `CharacterInteractionPlugin`
  under a `PlayerPlugin` umbrella so existing code adding `PlayerPlugin`
  continues to work.
- `PlayerSpawnPlugin` stays in `dd40_player` (spawning a locally controlled
  player is player-specific).
- `dd40_player/Cargo.toml` depends on `dd40_player_movement` +
  `dd40_character_interaction` + `dd40_character_core`.

**Task 3.4** Verify — full workspace build + tests green.

---

### Phase 4 — Apply auto-plugin pattern to remaining implementation crates

**Task 4.1** `dd40_vanilla_palette` — add check for `CorePlugin`.

**Task 4.2** `dd40_world` — add check for `CorePlugin`; replace panic with
auto-add.

**Task 4.3** `dd40_chunk_storage` — add check for `CorePlugin`.

**Task 4.4** `dd40_renderer` — add checks for `CorePlugin`,
`PhysicsCorePlugin`, `CharacterCorePlugin`.

**Task 4.5** `dd40_network` (client + server features) — add checks for
`CorePlugin`, `PhysicsCorePlugin`, `CharacterCorePlugin`.

**Task 4.6** `dd40_debug_ui`, `dd40_gui` — add check for `CorePlugin`.

**Task 4.7** Simplify `dd40_client/main.rs` and `dd40_server/main.rs` — remove
any foundation plugins that are now guaranteed by auto-plugin.

**Task 4.8** Verify — full workspace build + tests green.

---

### Phase 5 — Fix INCONSISTENCIES.md items

**Task 5.1** Fix renderer LOD dependency on `dd40_player` (item 1)
- Add `PlayerPosition` or reuse `CharacterPosition` from `dd40_character_core`
  as the LOD anchor.
- Remove `dd40_player` from `dd40_renderer`'s dependencies.

**Task 5.2** Add `PlayerId(u64)` to `dd40_character_core` (item 2)
- Map `PeerId → PlayerId` in `dd40_network`.

**Task 5.3** Verify block crack animation path is unblocked (item 4)
- `MiningState` is now in `dd40_character_interaction`; renderer can read it.
- If renderer needs it, add `dd40_character_interaction` as a dev-dep or move
  `MiningState` to `dd40_character_core`.

---

## Code Style

### Plugin struct conventions

Every plugin in an implementation crate must:

```rust
#[derive(Default)]
pub struct PhysicsPlugin;  // or carry config fields

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        // 1. Ensure every direct runtime dependency is present
        ensure_plugins!(app, CorePlugin, PhysicsCorePlugin);

        // 2. Register types
        app.register_type::<SomeType>();

        // 3. Add systems / resources
        app.add_plugins((IntegrationPlugin, BlockCollisionPlugin, CharacterCollisionPlugin));
    }
}
```

Foundation plugin structs also derive `Default` and use `ensure_plugins!`:

```rust
#[derive(Default)]
pub struct PhysicsCorePlugin;

impl Plugin for PhysicsCorePlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin);
        // register types, configure system sets, insert PhysicsConfig
    }
}
```

### Crate `lib.rs` structure

```
pub mod components;     // all ECS types
pub mod plugin;         // plugin(s) only
pub mod prelude;        // re-exports for convenience
```

Foundation crates also expose a `system_sets` module for their `SystemSet`
definitions.

---

## Testing Strategy

- **Compilation tests**: `cargo build --workspace` must pass after every task.
  No functionality should regress between tasks.
- **Unit tests**: Each new crate must have at least one integration test that
  creates a minimal Bevy `App`, adds only the crate's plugin, and asserts that
  required resources are initialised (verifies the auto-plugin chain works).
- **Existing tests**: `cargo test --workspace` must remain green throughout.
- No mocking of internal systems; prefer a real but minimal `App` instance.

---

## Boundaries

**Always:**
- Derive `Default` on every plugin struct so it can be auto-added.
- Check for direct runtime dependencies in every `Plugin::build`, not just
  Cargo-level dependencies.
- Keep `CollisionShape` in `dd40_core` as part of `BlockDefinition` (avoids
  a circular foundation dependency).
- Update `STRUCTURE.md`, per-crate `README.md`, and `INCONSISTENCIES.md`
  when adding, moving, or removing public items.

**Ask first:**
- Renaming or removing public types that `dd40_player` currently re-exports
  (external downstream risk).
- Adding a new foundation-to-foundation dependency (check for cycles).
- Moving `CollisionShape` out of `dd40_core` (significant refactor, not in
  scope here).

**Never:**
- Let an implementation crate depend on another implementation crate.
- Add physics or character systems to `dd40_core` after the extraction is done.
- Write `if !app.is_plugin_added` by hand — always use `ensure_plugins!`.
- Skip the `ensure_plugins!` call to save lines — it is the invariant that
  makes the architecture plug-and-play.
- Use `EventReader`/`EventWriter`/`add_event` (Bevy 0.18 uses observers and
  messages; see `CLAUDE.md`).

---

## Versioned Chunk Cache

The chunk cache is fully versioned. Every `Chunk` carries a monotonic
`version: u64`, an unbounded `confirmed_history: VecDeque<(u64, ChunkChange)>`,
and a runtime-only `predicted: VecDeque<ChunkChange>` queue.

`ChunkChange` is the **single mutation type** for any chunk on either
client or server. New mutation kinds extend the enum; no new messages or
events are introduced.

```rust
pub enum ChunkChange {
    Place   { local: BlockLocal, block_id: BlockId },
    Remove  { local: BlockLocal },
    Replace { local: BlockLocal, new_block: BlockId },
}
```

All coordinates inside `Chunk` (data, history, predictions) are
**chunk-local**. A chunk has no global-world knowledge; `Chunk::position`
is metadata for the outer `HashMap<ChunkPos, Chunk>` only.

### Lifecycle

| Stage | Where | What happens |
|---|---|---|
| **Predict** | Client or server | Push a `ChunkChange` into the chunk's `predicted` queue and apply it locally. |
| **Commit** (server only) | `ChunkAuthorityPlugin` in `PostUpdate` | Drain `predicted` through `ChunkChangeValidator` chain. Apply survivors. Bump `version`. Append to `confirmed_history`. Broadcast `ChunkUpdate { base_version, changes, new_version }` to clients in range. |
| **Reconcile** (client) | On `ChunkUpdate` arrival | If `base_version == client_version`: walk `changes`; remove matching entries from `predicted`; emit `PredictionRejected` for any leftovers. If `base_version > client_version`: log warn, drop, re-request via `RequestChunk { pos, current_version }`. |
| **Notify** | Both | Fire local `ChunkChanged { pos, changes, new_version }` Bevy message. Renderer / audio / future systems subscribe to it. |

### Configurable knobs

| Resource | Default | Meaning |
|---|---|---|
| `MaxDeltaBehind(u16)` | `15` | If `current_version < server_version - MaxDeltaBehind`, the server replies with a `ChunkSnapshot` instead of a `ChunkUpdate` and emits `ChunkSnapshotFallback { pos, client_version, server_version }`. |
| `DD40_CHUNK_STORAGE__SAVE_HISTORY` (env var, read at startup) | `false` | When `true`, the disk writer uses `ChunkVersion::V1Versioned` and persists history. When `false`, it uses `ChunkVersion::V1` and drops history at save time (logged at `debug!`). |

### Why a registered validator chain

The commit pass uses `Vec<Box<dyn ChunkChangeValidator>>` rather than an
inlined match against built-in change kinds. This keeps domain-specific
checks out of `dd40_core`: e.g. a "no placing into a tile occupied by a
character" rule lives in a downstream crate that owns the relevant
queries, not in core. Same principle as `BlockRegistry` — runtime
registration over hard-coded behaviour.

### Networked vs local

| | Local-only (singleplayer) | Networked |
|---|---|---|
| Predict | yes | yes |
| Commit | `ChunkAuthorityPlugin` (added by server binary, but also valid for singleplayer where the binary owns authority) | server only |
| Reconcile | n/a (commit and predict are the same process) | client |
| Wire format | none | `RequestChunk { pos, current_version } → ChunkSnapshot \| ChunkUpdate` |

Adding `ChunkAuthorityPlugin` *is* the authority gate. There is no
`run_if`, no marker resource, no client-vs-server runtime check. The
client binary simply does not add the plugin.

### Errors are loud

Every rejected change in `commit_predicted_changes` logs at `warn!` on
the server. Every `PredictionRejected` logs at `warn!` on the client.
Silence on a rejection is always a bug.
