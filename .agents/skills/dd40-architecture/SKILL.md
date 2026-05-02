---
name: dd40-architecture
description: >
  Expert guide to the dd40 voxel game architecture — crate responsibilities,
  dependency rules, and code placement decisions. Invoke this skill whenever
  you are planning a feature that spans more than one file, deciding which crate
  new code belongs in, moving or reorganizing code between crates, adding a new
  crate, or updating cross-crate interfaces. Also invoke it for any task that
  involves adding blocks, world generators, rendering logic, networking,
  UI elements, physics behaviour, player controls, or any other game system —
  even if the user does not explicitly mention architecture.
---

# dd40 Architecture Skill

dd40 is an open-source, Minecraft-inspired voxel game built with Bevy 0.18.
The guiding design goal is **modularity**: anyone should be able to swap out any
subsystem — chunk storage, world generation, UI, player controller, renderer,
physics — by writing a single replacement crate that depends only on the
relevant foundation crates, without touching the rest of the codebase.

## Where to look things up

- **Current crate inventory and module breakdown**: `STRUCTURE.md` at the repo root.
- **What a specific crate does and how it is organised**: the `README.md` inside
  that crate's directory.
- **Public events, messages, resources, and components**: the `docs/` folder at
  the repo root, organized by system.
- **Known deviations from these rules and planned fixes**: `INCONSISTENCIES.md`.
- **Planned architectural work**: `SPEC.md` at the repo root.

Always read the relevant `README.md` and `STRUCTURE.md` before planning work that
touches more than one file.

---

## The architectural rules

### Three-tier dependency model

Crates are organised into three tiers. Each tier has strict rules about what it
may depend on.

#### Tier 0 — Foundation crates
`dd40_core`, `dd40_physics_core`, `dd40_character_core`

- Contain **types, components, system sets, resources, and messages only** —
  no concrete game behaviour.
- May depend on other foundation crates (acyclically) and external libraries.
- Every plugin struct must derive `Default` so it can be auto-added.

#### Tier 1 — Implementation crates
`dd40_physics`, `dd40_vanilla_palette`, `dd40_world`, `dd40_chunk_storage`,
`dd40_renderer`, `dd40_character_interaction`, `dd40_player_movement`,
`dd40_network`, `dd40_debug_ui`, `dd40_gui`

- Contain **systems, asset loading, and concrete behaviour**.
- May depend on **any** foundation crates and external libraries.
- Must **not** depend on other implementation crates.
- Must use `ensure_plugins!` at the top of every `Plugin::build` to
  auto-satisfy direct runtime dependencies (see below).

**Exception — `dd40_player`:** This crate is a convenience wrapper that
composes `dd40_player_movement` + `dd40_character_interaction` and is the only
place that needs both physics types and interaction types simultaneously (for
`update_debug_info`). It intentionally depends on two Tier 1 crates and is
tracked in `INCONSISTENCIES.md`.

#### Tier 2 — Binary crates
`dd40_client`, `dd40_server`

- May depend on any dd40 crate.
- Wire together the chosen implementation plugins.
- The ideal binary `main.rs` is a flat list of implementation plugins — no
  foundation plugins needed, because `ensure_plugins!` handles them.

### Rule 2 — BlockDefinition is the single source of truth

All block properties — rendering, physics, gameplay — live on `BlockDefinition`.
Never store block-related data in a separate resource that must be kept in sync
with `BlockRegistry`.

### Rule 3 — `CollisionShape` stays in `dd40_core`

`CollisionShape` is part of `BlockDefinition`. Moving it would make `dd40_core`
depend on `dd40_physics_core`, creating a circular foundation dependency.

---

## The `ensure_plugins!` auto-plugin pattern

Every implementation (and foundation) plugin must satisfy its direct runtime
dependencies automatically, never assuming the caller added them. Use the
`ensure_plugins!` macro from `dd40_core`:

```rust
impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        // Satisfy every direct runtime dependency, in any order.
        dd40_core::ensure_plugins!(app, CorePlugin, PhysicsCorePlugin);

        app.add_plugins((IntegrationPlugin, BlockCollisionPlugin, CharacterCollisionPlugin));
    }
}
```

Foundation plugins apply the same rule:

```rust
impl Plugin for PhysicsCorePlugin {
    fn build(&self, app: &mut App) {
        dd40_core::ensure_plugins!(app, CorePlugin);
        // register types, configure system sets ...
    }
}
```

**Rules:**
- List every plugin your `build` method **directly** reads resources or system
  sets from — not just the Cargo dependency, not just what you know the dep
  transitively checks.
- Never write `if !app.is_plugin_added` by hand; always use `ensure_plugins!`.
- `CorePlugin` itself is exempt (it is the root — nothing to check).
- Binary crates are exempt (they manage their plugin list explicitly).

---

## Code placement

Ask: "What does this code need to read or write?"

- Needs only its own types and bevy → stays in the same crate.
- Needs types from one foundation crate → belongs in an implementation crate
  that depends on that foundation crate.
- Needs types from two implementation crates → belongs in a binary, or requires
  a new abstraction in a foundation crate.

Consult `STRUCTURE.md` for the current role of each crate.

---

## Documentation maintenance

After every change that adds, moves, or removes public items in a crate:

1. **Per-crate `README.md`** — update purpose paragraph and module overview if
   the structure changed.
2. **`STRUCTURE.md`** — keep the crate table and module breakdown accurate.
3. **`docs/`** — update or create the relevant system doc if any public
   events, messages, resources, or components were affected.
4. **`INCONSISTENCIES.md`** — update if the change introduces or resolves an
   architectural deviation.

The rule of thumb: someone reading only `STRUCTURE.md` should be able to tell
where any given piece of functionality lives; someone reading a crate's
`README.md` should be able to understand what it does without reading the source.
