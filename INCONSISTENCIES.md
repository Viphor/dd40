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

### 3. Vanilla blocks live in `dd40_core`

**Rule violated:** `dd40_core` should contain no game content, only engine
infrastructure.

**Current state:** `dd40_core::vanilla_blocks` registers the standard block
types (stone, dirt, grass, etc.) during startup.

**Planned fix (owner-acknowledged):** Move vanilla blocks to a dedicated crate
(e.g., `dd40_vanilla`) that depends on `dd40_core` and registers itself in
`BlockRegistrySet`. `dd40_core` would then only register `BlockId::AIR` (which
is an engine invariant, not content).

---

### 4. Root `Cargo.toml` package depends on `dd40_world` and `dd40_network`

**Rule violated:** Non-configuration packages should not depend on multiple
dd40 crates.

**Current state:** The workspace root defines a `[package]` section that
depends on `dd40_core`, `dd40_world`, `dd40_debug_ui`, and `dd40_network` in
order to host examples. This means the workspace root behaves like an
unofficial third configuration crate.

**Fix:** Move the examples into a dedicated workspace member (e.g.,
`crates/examples`) that is allowed to depend on multiple dd40 crates, or
promote each example to its own mini-crate. Remove the `[package]` section
from the workspace root so `Cargo.toml` is purely a workspace manifest.

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
