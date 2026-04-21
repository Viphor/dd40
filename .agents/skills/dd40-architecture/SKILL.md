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
subsystem — chunk storage, world generation, UI, player controller, renderer —
by writing their own crate that only talks to `dd40_core`, without touching the
rest of the codebase.

## Where to look things up

- **Current crate inventory and module breakdown**: `STRUCTURE.md` at the repo root.
- **What a specific crate does and how it is organised**: the `README.md` inside
  that crate's directory.
- **Public events, messages, resources, and components**: the `docs/` folder at
  the repo root, organized by system.
- **Known deviations from these rules and planned fixes**: `INCONSISTENCIES.md`.

Always read the relevant `README.md` and `STRUCTURE.md` before planning work that
touches more than one file.

---

## The architectural rules

### 1. One shared foundation

Every non-core dd40 crate may depend only on `dd40_core` and external libraries.
No non-core crate may import another dd40 crate. This keeps each feature crate
independently swappable — you can replace any one of them without touching the
others.

### 2. Core is vocabulary, not logic

`dd40_core` contains types, components, events, messages, system sets, and
shared resources. It enforces no game behaviour beyond the physics engine, which
is an intentional exception (see `STRUCTURE.md` for the reasoning).

### 3. Client and server are configurations

`dd40_client` and `dd40_server` are the only crates allowed to depend on
multiple dd40 crates at once. Their job is to wire together the right set of
plugins for the default game experience, nothing more.

### 4. BlockDefinition is the single source of truth

All block properties — rendering, physics, gameplay — live on `BlockDefinition`.
Never store block-related data in a separate resource that must be kept in sync
with `BlockRegistry`.

---

## Code placement

When you are unsure which crate a piece of code belongs in, ask: "Can this be
implemented with only `dd40_core` and external libraries?" If yes, it is a
candidate for that feature crate. If it inherently needs two non-core crates, it
belongs in client/server or in a new abstraction in `dd40_core`.

Consult `STRUCTURE.md` for the current role of each crate and use those roles as
the deciding factor.

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
