# dd40_client

Default playable client binary for dd40. This crate is a **configuration**, not
a feature — its only job is to assemble the correct set of plugins from the
other dd40 crates into a runnable game client.

As a configuration crate, `dd40_client` is intentionally allowed to depend on
all relevant dd40 crates. It is not intended to be imported as a library.

## What it wires together

| Plugin | Provided by |
|---|---|
| `CorePlugin` | `dd40_core` |
| `PlayerInputPlugin` | `dd40_player` |
| `RendererPlugin` | `dd40_renderer` |
| `ClientNetworkPlugin` | `dd40_network` |
| `DebugUiPlugin` | `dd40_debug_ui` |
| `GuiPlugin` | `dd40_gui` |

## Module overview

```
src/
└── main.rs   — App construction: DefaultPlugins + all dd40 client plugins
```

## Building and running

```bash
cargo run --bin dd40_client
```

Enable verbose network debug output:

```bash
cargo run --bin dd40_client --features debug_network
```
