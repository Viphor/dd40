# dd40

An open-source Rust implementation of a Minecraft-inspired voxel game.

> **DISCLAIMER:** This is all for shits and giggles. Don't read too much into it.
> If you like the project, don't hesitate to open an issue or clone the repo.

For the full crate inventory and architecture overview, see **[STRUCTURE.md](STRUCTURE.md)**.

---

## Building and running the default game

**Prerequisites:** Rust stable toolchain (see [rustup.rs](https://rustup.rs)).

```bash
# Clone
git clone https://github.com/Viphor/dd40.git
cd dd40

# Build everything
cargo build --workspace

# Run the default client
cargo run --bin dd40_client

# Run the default server (headless, starts on port 6969)
cargo run --bin dd40_server

# Run all tests
cargo test --workspace
```

The client and server use lightyear for networking. Start the server first,
then launch the client — it will connect automatically.

---

## Writing your own client or server

Every subsystem in dd40 is an independently swappable crate organised in a
three-tier model (foundation → implementation → binary). You can use any subset
of them, replace any one of them, or write your own from scratch.

### Minimal custom server example

```rust
use bevy::prelude::*;
use dd40_core::plugin::CorePlugin;
use dd40_world::{WorldPlugin, generators::flat::FlatWorldGenerator};

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(CorePlugin)
        .add_plugins(WorldPlugin::new(FlatWorldGenerator::default()))
        .run();
}
```

### Adding custom blocks

```rust
use bevy::prelude::*;
use dd40_core::prelude::*;

pub const MY_BLOCK: BlockId = BlockId(1000); // 1000+ for custom blocks

fn register_my_blocks(mut registry: ResMut<BlockRegistry>, mut commands: Commands) {
    registry.register(
        BlockDefinition::new(MY_BLOCK, "my_block")
            .with_color(bevy::color::Color::srgb(1.0, 0.5, 0.0)),
        &mut commands,
    );
}

pub struct MyBlocksPlugin;

impl Plugin for MyBlocksPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_my_blocks.in_set(BlockRegistrySet));
    }
}
```

See the `examples/` directory for more complete worked examples, and the
`docs/` directory for per-system API documentation.

---

## Repository layout

See [STRUCTURE.md](STRUCTURE.md) for the full crate breakdown.

```
crates/
  core/                  — block registry, chunk pipeline, state (dd40_core)         [Tier 0]
  physics_core/          — physics types, components, system sets (dd40_physics_core) [Tier 0]
  character_core/        — character types, input bridge, render sets (dd40_character_core) [Tier 0]
  physics/               — gravity, block collision, char collision (dd40_physics)    [Tier 1]
  vanilla_palette/       — vanilla block/tool definitions (dd40_vanilla_palette)      [Tier 1]
  world/                 — world generation (dd40_world)                              [Tier 1]
  chunk_storage/         — disk-backed chunk persistence (dd40_chunk_storage)         [Tier 1]
  renderer/              — greedy-mesh chunk renderer (dd40_renderer)                 [Tier 1]
  player_movement/       — keyboard/mouse → CharacterInput, camera (dd40_player_movement) [Tier 1]
  character_interaction/ — block targeting, mining, placement (dd40_character_interaction) [Tier 1]
  network/               — lightyear networking (dd40_network)                        [Tier 1]
  debug_ui/              — debug overlay (dd40_debug_ui)                              [Tier 1]
  gui/                   — in-game HUD (dd40_gui)                                     [Tier 1]
  player/                — convenience wrapper: movement + interaction (dd40_player)  [Tier 1*]
  client/                — default client binary (dd40_client)                        [Tier 2]
  server/                — default server binary (dd40_server)                        [Tier 2]
docs/            — per-system API documentation with examples
examples/        — runnable example programs
```

*`dd40_player` is a tracked Tier 1 exception — see `INCONSISTENCIES.md`.

---

## License

See [LICENSE](LICENSE).
