# Inconsistencies and Suggestions

This document records known deviations from the stated architecture and
suggestions for improvement. It is the planning backlog for architectural
clean-up — not a bug tracker.

Active architectural work is planned in `SPEC.md`.

---

## Inconsistencies

### 1. `dd40_renderer` depends on `dd40_player`

**Rule violated:** Implementation crates must not depend on other implementation
crates.

**Current state:** `dd40_renderer` imports `dd40_player` to read the player
world position for LOD (level-of-detail) distance calculations.

**Fix (planned in SPEC.md Task 5.1):** Use `CharacterPosition` from
`dd40_physics_core` as the LOD anchor instead of querying `With<Player>`.
The renderer then depends only on foundation crates.

---

### 2. `PlayerLocations` is keyed by lightyear `PeerId`

**Rule violated:** Core game concepts should not be coupled to network identity.

**Current state:** `dd40_network::server::spawn::PlayerLocations` stores last-known
spawn positions keyed by `lightyear::prelude::PeerId`. This couples spawn
management to the transport layer, making it impossible to reuse for NPCs,
animals, or alternative transports.

**Fix (planned in SPEC.md Task 5.2):** Introduce `PlayerId(u64)` in
`dd40_character_core` and key `PlayerLocations` by that type. The network layer
maps `PeerId -> PlayerId` at connection time.

---

### 3. `MiningState` lives in `dd40_player`, not a foundation crate

**Rule violated (partial):** `MiningState` is a resource that the HUD and
renderer will eventually need to read. If they read it directly, they'd need to
depend on `dd40_player`, violating the implementation-crate rule.

**Current state:** `MiningState` is defined in
`dd40_player::block_interaction::mining` and is publicly exported.

**Fix (planned in SPEC.md Task 3.2 / 5.3):** Move `MiningState` to
`dd40_character_core` so any foundation or implementation crate can read it
without depending on `dd40_player`.

---

### 4. Block crack animation is unimplemented

**Current state:** The mining system tracks `progress` in `MiningState` (range
`0.0–1.0`) but no renderer or HUD currently visualises it.

**Fix (planned in SPEC.md Task 5.3):** Once `MiningState` is in
`dd40_character_core`, the renderer can read `progress` and overlay a crack
texture on the targeted block.

---

### 5. Physics systems live in `dd40_core` (should be `dd40_physics`)

**Rule violated:** Foundation crates should contain types and system sets only,
not concrete game systems.

**Current state:** `dd40_core::character::physics` contains `IntegrationPlugin`,
`BlockCollisionPlugin`, and `CharacterCollisionPlugin` — concrete systems that
implement game behaviour.

**Fix (planned in SPEC.md Phase 1):** Extract vocabulary to `dd40_physics_core`
and systems to `dd40_physics`.

---

### 6. Character types live in `dd40_core` (should be `dd40_character_core`)

**Rule violated:** `dd40_core` should be pure infrastructure; character
vocabulary belongs in its own foundation crate.

**Current state:** `Player`, `Character`, `CharacterInput`, `MovementSpeed`,
`JumpImpulse`, `CharacterBundle`, `CharacterRenderSet`, and
`CharacterController` are all in `dd40_core::character`.

**Fix (planned in SPEC.md Phase 2):** Move all character vocabulary to a new
`dd40_character_core` foundation crate.

---

### 7. Block interaction and movement systems are player-gated

**Rule violated:** Generic character capabilities (mining, block placement,
movement) should work for any entity with the `Character` marker, not only
entities with `Player`.

**Current state:** Systems in `dd40_player::block_interaction` query
`With<Player>`, preventing NPCs or other characters from using them.

**Fix (planned in SPEC.md Phase 3):** Extract systems to
`dd40_character_interaction` and `dd40_player_movement`; change filters to
`With<Character>` where applicable.

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
