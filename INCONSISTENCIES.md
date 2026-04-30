# Inconsistencies and Suggestions

This document records known deviations from the stated architecture and
suggestions for improvement. It is the planning backlog for architectural
clean-up — not a bug tracker.

---

## Inconsistencies

### 1. `dd40_renderer` depends on `dd40_player`

**Rule violated:** Non-core crates must depend only on `dd40_core`.

**Current state:** `dd40_renderer` imports `dd40_player` to read the player
world position for LOD (level-of-detail) distance calculations.

**Fix:** Add a lightweight `PlayerPosition` marker resource or component to
`dd40_core` (distinct from the physics `CharacterPosition`) that any crate can
write and the renderer can read. The renderer then depends only on `dd40_core`,
and `dd40_player` writes the value without the renderer knowing about it.

---

### 2. `PlayerLocations` is keyed by lightyear `PeerId`

**Rule violated:** Core game concepts should not be coupled to network identity.

**Current state:** `dd40_network::server::spawn::PlayerLocations` stores last-known
spawn positions keyed by `lightyear::prelude::PeerId`. This couples spawn
management to the transport layer, making it impossible to reuse for NPCs,
animals, or alternative transports.

**Fix:** Introduce a game-level identity type (e.g., `PlayerId(u64)`) in
`dd40_core` and key `PlayerLocations` (or a successor resource) by that type.
The network layer maps `PeerId -> PlayerId` at connection time. This also opens
the door to a future `dd40_spawn` crate that manages spawn points independently
of the network stack.

---

### 3. `MiningState` lives in `dd40_player`, not `dd40_core`

**Rule violated (partial):** `MiningState` is a resource that the HUD and
renderer will eventually need to read (for progress bars and block-crack
animations). If they read it directly, they'd need to depend on `dd40_player`,
violating the single-dependency rule.

**Current state:** `MiningState` is defined in
`dd40_player::block_interaction::mining` and is publicly exported.

**Fix:** Move `MiningState` to `dd40_core` so that `dd40_renderer`, `dd40_gui`,
and any custom HUD crate can read it without taking a dependency on
`dd40_player`. The mining system in `dd40_player` would then write the value as
it does today.

---

### 4. Block crack animation is unimplemented

**Current state:** The mining system tracks `progress` in `MiningState` (range
`0.0–1.0`) but no renderer or HUD currently visualises it. The crack texture
overlay familiar from Minecraft is not yet implemented.

**Fix:** Once `MiningState` is moved to `dd40_core` (see item 3), the renderer
can read `progress` and overlay a crack texture on the targeted block.

---

## Suggestions

### A. Extract `dd40_physics` as a standalone crate

The physics engine is already treated as a special case inside `dd40_core`. If
it grows significantly, or if users want to swap it out, consider extracting it
into `dd40_physics`. Every crate that currently reads physics types from core
would then add `dd40_physics` as a second dependency — acceptable because
physics is genuinely foundational.

### B. Add a `WorldGenerator` trait re-export to `dd40_core`

`WorldGenerator` is currently defined in `dd40_world::generators`. A crate that
only wants to implement a custom generator must depend on `dd40_world`, pulling
in the flat-generator code as dead weight. Moving the trait to `dd40_core`
would let custom generators depend on core alone.

### C. Formalise the `ChunkProvider` contract in `dd40_core`

The chunk request/response contract (`RequestChunk` -> `ChunkReady`) is already
defined in `dd40_core`, but there is no explicit `ChunkProvider` trait. A trait
would let tooling and documentation surface the contract clearly and make it
easier to write and test alternative backends.

### D. Add a loading-screen crate

`LoadingTracker` in `dd40_core` tracks async initialisation but there is no
crate that renders a loading screen against it. A `dd40_loading_screen` crate
would complete the loop without adding game-logic dependencies to core.
