---
name: dd40-create-system
description: >
  Step-by-step guide for scaffolding a new dd40 system (Rust library crate).
  Invoke this skill whenever the user wants to create a new gameplay system,
  subsystem, or extension crate — whether it is a Tier 0 foundation crate
  (physics, redstone, items, weather primitives) or a Tier 1 implementation
  crate (flying, pistons, enchanting, fire spread). Also invoke it when the
  user asks about adding a feature crate, creating a new Bevy plugin crate for
  dd40, or structuring a new library that other crates may or may not build on.
  Handles tier classification, dependency detection, crate scaffolding with
  full boilerplate, workspace registration, and a build verification step.
---

# Create a dd40 System (Crate)

Scaffold a new Rust library crate for a dd40 system. Work through every step
in order, stopping to ask the user after each question. Do not create any
files until step 4.

---

## Step 1: Gather information

Ask the user these questions **one at a time**, waiting for answers before
moving on.

### 1a — What does this system do?

Get a short description of its purpose and what game behaviour it will
handle. Listen for: what it simulates or manages, what other systems or
developers will interact with it, and whether it is something others will
build upon.

### 1b — Foundation or implementation?

Explain the distinction and let the user decide. Here is how to frame it:

> A **Tier 0 (Foundation)** crate is the shared vocabulary for a domain — it
> defines the types, components, events, system sets, and primitive logic that
> *other crates will import*. Think of it as the language the rest of the
> codebase speaks when talking about this domain. Foundation crates have no
> game logic.
>
> A **Tier 1 (Implementation)** crate implements game behaviour *using* that
> vocabulary. Nothing else will ever import it; it communicates with the rest
> of the game only through the foundation types it reads and writes.

The practical test: *Will other crates need to `use` types defined in this
new crate?* If yes → Tier 0 (Foundation). If no → Tier 1 (Implementation).

Examples to share if the user is uncertain:
- Foundation: physics types (everyone needs `Velocity`, `Grounded`), redstone
  types (everyone needs `PowerLevel`, `RedstoneTick`)
- Implementation: flying (reads `Velocity`, writes nothing others consume),
  pistons (reads `PowerLevel`, moves blocks)

### 1c — Short name

Ask for the `<name>` portion of `dd40_<name>`. It must be lowercase
`snake_case`. Examples: `redstone`, `flying`, `weather`, `chunk_storage`.

---

## Step 2: Detect dependencies

You need to know which existing dd40 crates to add as dependencies.

The workspace currently has three tiers of crates:

**Tier 0 — Foundation** (types/components/sets only, no game logic):
- `dd40_core` — block registry, chunk pipeline, app state, tools
- `dd40_physics_core` — physics types, components, `PhysicsSet`
- `dd40_character_core` — character types, `CharacterInput`, `MiningState`, `PlayerId`, `CharacterRenderSet`

**Tier 1 — Implementation** (systems, game behaviour):
`dd40_physics`, `dd40_vanilla_palette`, `dd40_world`, `dd40_chunk_storage`,
`dd40_renderer`, `dd40_player_input`, `dd40_character_interaction`,
`dd40_network`, `dd40_debug_ui`, `dd40_gui`, `dd40_player`

**Tier 2 — Binary**: `dd40_client`, `dd40_server`

Rules when choosing dependencies:
1. Every new crate (Foundation or Implementation) should depend on `dd40_core`
   unless there is a clear reason not to.
2. Foundation crates may depend on other foundation crates — never circularly.
3. Implementation crates may depend on any foundation crates but **never on
   other implementation crates**.
4. `dd40_player` is a tracked exception to rule 3 — do not follow its example
   without explicitly noting the inconsistency in `INCONSISTENCIES.md`.

Steps:
1. Check the `[workspace]` section in the root `Cargo.toml`.
2. Think about which crates the new system genuinely needs.
3. Present the candidates: "Based on what you described, this system would
   likely depend on: [list]. Does that look right?"
4. Confirm the final list before using it in `Cargo.toml`.

---

## Step 3: Determine crate location

- **Workspace present**: place the crate at `crates/<name>/`.
- **No workspace**: ask the user where they would like the crate placed.

---

## Step 4: Scaffold the crate

Create the following file tree, substituting `<name>` and `<PluginName>`
throughout:

```
crates/<name>/
├── Cargo.toml
└── src/
    ├── lib.rs
    └── plugin.rs
```

### Cargo.toml

```toml
[package]
name = "dd40_<name>"
version = "0.1.0"
edition = "2024"

[dependencies]
bevy = { workspace = true, default-features = false, features = [] }
dd40_core = { workspace = true }
# dd40_<other_foundation> = { workspace = true }   ← add other confirmed foundation dependencies
```

If no workspace exists, replace `{ workspace = true }` with concrete versions:

```toml
bevy = { version = "0.18", default-features = false }
```

Dependency rules:
- Include only the dd40 crates confirmed in step 2.
- Never add a Tier 1 implementation crate as a dependency (unless this new
  crate is also a tracked exception like `dd40_player`).
- Leave the `features = []` list empty until the user asks to enable specific
  Bevy feature flags.

### src/lib.rs

```rust
//! <One-sentence description of what this crate provides.>
//!
//! # Overview
//!
//! <Two or three sentences: the system's role, what game behaviour it owns,
//! and what other crates or developers should reach for when working in
//! this domain.>
//!
//! # Usage
//!
//! Add [`plugin::<PluginName>`] to your [`App`] to enable this system:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_<name>::plugin::<PluginName>;
//!
//! App::new()
//!     .add_plugins(<PluginName>)
//!     .run();
//! ```

pub mod plugin;

// Re-export stable public API here once types are defined, for example:
// pub use <module>::<Type>;
```

### src/plugin.rs

For **Tier 1 (Implementation)** crates, always call `ensure_plugins!` at the
top of `build`:

```rust
//! Root plugin for the `dd40_<name>` crate.
//!
//! [`<PluginName>`] is the single entry point for this system. Add it to
//! your [`App`] once to register all components, resources, events, and
//! systems this crate provides.
//!
//! # Example
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_<name>::plugin::<PluginName>;
//!
//! App::new()
//!     .add_plugins(<PluginName>)
//!     .run();
//! ```

use bevy::prelude::*;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;

/// Plugin that registers all systems and types provided by `dd40_<name>`.
///
/// ## What this plugin sets up
///
/// - TODO: list components / resources / events registered here
/// - TODO: list system sets defined here
/// - TODO: list systems added here
#[derive(Default)]
pub struct <PluginName>;

impl Plugin for <PluginName> {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin); // add other dep plugins here
        // TODO: register components, resources, events, and systems
    }
}
```

For **Tier 0 (Foundation)** crates, omit `ensure_plugins!` if this is the
root foundation crate, or include it for the foundation crates this one
depends on.

**Plugin naming**: use `<NameInPascalCase>Plugin` — e.g., `RedstonePlugin`,
`FlyingPlugin`, `WeatherPlugin`.

**Plugin derive**: every plugin must `#[derive(Default)]` so it can be
auto-satisfied by the `ensure_plugins!` macro.

**Documentation rule**: every `pub` item — including `pub mod` declarations
and the plugin struct itself — must have a `///` doc comment. Generate doc
stubs even for placeholder items.

---

## Step 5: Register in the workspace (if applicable)

If a `[workspace]` section exists in the root `Cargo.toml`, add the crate
in two places:

```toml
# Under [workspace] members list:
"crates/<name>",

# Under [workspace.dependencies] — add only when another crate will
# immediately depend on it. Otherwise, add this entry when first needed.
dd40_<name> = { path = "crates/<name>" }
```

---

## Step 6: Verify the build

```bash
cargo build --workspace
```

Fix any compilation errors before moving on.

---

## Step 7: Suggest next steps

Tell the user what to add inside the new crate, based on the tier.

### Tier 0 (Foundation) — highest-value additions first

1. **System sets** with ordering guarantees (e.g. `<Name>Set`) so downstream
   crates can place their systems correctly relative to this one.
2. **Foundational types** — components, resources, and data structures that
   other crates will import (`#[derive(Component)]`, `#[derive(Resource)]`).
3. **Events and messages** that broadcast state changes to the rest of the
   codebase (`#[derive(Event)]`, `#[derive(Message, Clone)]`).
4. **Trait definitions** if the system is generic over behaviour (e.g. a
   `WorldGenerator` trait that world crates implement).
5. Re-export all stable public types from `lib.rs` via a `prelude` module so
   consumers can use `use dd40_<name>::prelude::*`.

### Tier 1 (Implementation) — highest-value additions first

1. **System functions** that implement the feature behaviour.
2. **Local-only components or resources** — marked clearly as internal, not
   intended for other crates to import.
3. **Plugin wiring** — `.add_systems(...)`, `.add_observer(...)`,
   `app.add_message::<T>()` inside the `Plugin::build` method.
4. **Ordering constraints** relative to the foundation system sets this crate
   builds on.

---

## Architectural rules

These are hard constraints. If any would be violated by the user's request,
stop and explain the problem before proceeding.

1. **Tier 1 (Implementation) crates never import other Tier 1 crates.** If
   two implementation systems need to share data, that data belongs in a
   foundation crate. The sole tracked exception is `dd40_player`.

2. **Tier 0 (Foundation) crates may depend on other foundation crates** —
   but never circularly. `dd40_core` is the root; every new crate is
   encouraged to depend on it.

3. **Tier 2 (Binary) crates are the only wiring points.** They are the only
   places allowed to depend on multiple Tier 1 crates simultaneously.

4. **All `pub` items must have `///` doc comments.** This includes modules,
   structs, traits, functions, and type aliases.

5. **The plugin always lives in `src/plugin.rs`**, exposed to the crate root
   via `pub mod plugin;` in `lib.rs`.

6. **Every plugin must `#[derive(Default)]`** so `ensure_plugins!` can
   auto-satisfy it.

7. **Every Tier 1 plugin must call `ensure_plugins!`** at the start of
   `build` for every foundation plugin it depends on.

8. **Crate names use `dd40_<name>`** when inside a dd40 workspace.
