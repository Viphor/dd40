# dd40_server

Default headless server binary for dd40. This crate is a **configuration**, not
a feature — its only job is to assemble the correct set of plugins from the
other dd40 crates into a runnable dedicated server.

As a configuration crate, `dd40_server` is intentionally allowed to depend on
all relevant dd40 crates. It does not depend on any rendering or windowing
crates. It is not intended to be imported as a library.

## What it wires together

| Plugin | Provided by |
|---|---|
| `CorePlugin` | `dd40_core` |
| `WorldPlugin` | `dd40_world` |
| `DiskStoragePlugin` | `dd40_chunk_storage` |
| `ServerNetworkPlugin` | `dd40_network` |

## Module overview

```
src/
└── main.rs   — App construction: MinimalPlugins + all dd40 server plugins
```

## Building and running

```bash
cargo run --bin dd40_server
```
