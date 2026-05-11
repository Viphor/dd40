# Inconsistencies and Suggestions

This document records known deviations from the stated architecture and
suggestions for improvement. It is the planning backlog for architectural
clean-up — not a bug tracker.

Active architectural work is planned in `SPEC.md`.

---

## Open Inconsistencies

### 1. `dd40_player` depends on other implementation crates

**Rule violated:** Implementation crates must not depend on other implementation
crates.

**Current state:** `dd40_player` depends on `dd40_player_movement` and
`dd40_character_interaction`, both of which are Tier 1 implementation crates.

**Rationale for keeping:** `dd40_player` is an intentional convenience wrapper
that composes the two movement/interaction plugins plus the local player spawn
and debug-info systems. It holds the only code that needs *both* physics types
(`Velocity`, `Impulse`) and interaction types (`MiningState`) simultaneously —
specifically the `update_debug_info` system. Splitting that system further would
create more coupling, not less.

**Planned resolution:** If `update_debug_info` is moved to a HUD crate that
depends on both interaction and physics foundation crates directly, `dd40_player`
could be deleted and callers would compose the plugins themselves.

---

### 2. ~~Block crack animation is unimplemented~~ — resolved

**Resolution:** `dd40_character_gui::block_highlight::draw_targeted_block_highlight`
now draws a gizmo break overlay whose scale and colour interpolate with
`MiningState::Mining { progress, .. }`. The pure helper
`break_overlay_for_progress` makes the easing curve unit-testable. A
textured crack overlay is still future work, but the previous "no
visualisation at all" gap is closed.

---

## Suggestions

### A. Add a `WorldGenerator` trait re-export to `dd40_core`

`WorldGenerator` is currently defined in `dd40_world::generators`. A crate that
only wants to implement a custom generator must depend on `dd40_world`, pulling
in the flat-generator code as dead weight. Moving the trait to `dd40_core`
would let custom generators depend on core alone.

### B. Formalise the `ChunkProvider` contract in `dd40_core`

The chunk request/response contract (`RequestChunk` -> `ChunkReady`) is already
defined in `dd40_core`, but there is no explicit `ChunkProvider` trait. A trait
would let tooling and documentation surface the contract clearly and make it
easier to write and test alternative backends.

### C. Add a loading-screen crate

`LoadingTracker` in `dd40_core` tracks async initialisation but there is no
crate that renders a loading screen against it. A `dd40_loading_screen` crate
would complete the loop without adding game-logic dependencies to core.

### D. Key `PlayerLocations` by `PlayerId` instead of `PeerId`

`dd40_network::server::spawn::PlayerLocations` stores last-known spawn positions
keyed by lightyear's `PeerId`, coupling spawn management to the transport layer.
`PlayerId(u64)` now exists in `dd40_character_core`. Migrating the key type
would let the spawn system be reused for NPCs or alternative transports.

---

## Resolved (archived)

| # | Description | Resolved in |
|---|---|---|
| — | `dd40_renderer` depended on `dd40_player` for LOD anchor | SPEC.md Task 5.1 — renderer now uses `CharacterPosition` from `dd40_physics_core` |
| — | `MiningState` lived in `dd40_player` | SPEC.md Task 5.3 — moved to `dd40_character_core::mining_state` |
| — | `MiningState` was a global `Resource` (singleton bug) | core-rewrite — converted to a `Component` on the `Character` entity, attached via `CharacterBundle` |
| — | `TargetedBlock` was a global `Resource` in `dd40_character_interaction` (singleton bug) | core-rewrite — moved to `dd40_character_core::targeted_block` and converted to a `Component` |
| — | `update_targeted_block` queried `Camera3d`, blocking server-side targeting on headless servers | core-rewrite — added `dd40_character_core::face::CharacterFace` child entity; targeting now reads the local player's face `GlobalTransform` |
| — | `HeldBlock` was a global `Resource` (singleton bug) coupling placement to a single block | core-rewrite — deleted; placement now reads the placeable block from each character's `ActiveItem` via `ItemRegistry` |
| — | `EquippedTool` newtype wrapped `(ToolKindId, ToolTierId)` and was never attached to any entity | core-rewrite — collapsed into raw primitives; mining reads tool kind/tier from each character's `ActiveItem` |
| — | Physics systems lived in `dd40_core` | SPEC.md Phase 1 — extracted to `dd40_physics_core` + `dd40_physics` |
| — | Character types lived in `dd40_core` | SPEC.md Phase 2 — extracted to `dd40_character_core` |
| — | Block interaction and movement systems were player-gated | SPEC.md Phase 3 — `dd40_character_interaction` and `dd40_player_movement` created, filters changed to `With<Character>` |
| — | `PlayerId(u64)` did not exist | SPEC.md Task 5.2 — added to `dd40_character_core::components` |
| — | `BlockPlaced` / `BlockRemoved` events were broadcast ad-hoc per change kind | versioned-chunk-cache — replaced by the unified `ChunkChange` enum and the local `ChunkChanged { pos, changes, new_version }` message emitted by the authority commit pass / client reconciler |
| — | `PlaceBlockRequest` / `StartMiningRequest` / `AbortMiningRequest` / `MineBlockRequest` lived as separate lightyear messages | versioned-chunk-cache — replaced by predicted `ChunkChange`s on the chunk itself; clients push, the server's authority plugin commits and broadcasts `ChunkUpdate` |
| — | `ChunkPos` was 2D, blocking vertical chunk splits in serialization paths | versioned-chunk-cache (Phase 7.5) — added `y` axis throughout; on-disk filenames are `chunk_X_Y_Z.bin`, physics + raycast walk Y boundaries |
| — | Disk format was unversioned | versioned-chunk-cache (Phase 6) — `ChunkVersion::V1` and `V1Versioned` introduced; reader auto-detects, writer chooses via `DD40_CHUNK_STORAGE__SAVE_HISTORY` env var |
