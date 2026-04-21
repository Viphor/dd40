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

Every subsystem in dd40 is an independently swappable crate. All non-core
crates depend only on `dd40_core`; you can use any subset of them, replace any
one of them, or write your own from scratch.

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
  core/          — shared types, block registry, physics (dd40_core)
  world/         — world generation (dd40_world)
  chunk_storage/ — disk-backed chunk persistence (dd40_chunk_storage)
  renderer/      — greedy-mesh chunk renderer (dd40_renderer)
  player/        — player input, camera, block interaction (dd40_player)
  network/       — lightyear networking (dd40_network)
  debug_ui/      — debug overlay (dd40_debug_ui)
  gui/           — in-game HUD (dd40_gui)
  client/        — default client binary (dd40_client)
  server/        — default server binary (dd40_server)
docs/            — per-system API documentation with examples
examples/        — runnable example programs
```

---

## License

See [LICENSE](LICENSE).
