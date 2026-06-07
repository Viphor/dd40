# Configuration System

dd40 uses a layered TOML configuration system provided by the `dd40_config` crate.
Any crate — including third-party mods — can add its own config section without
modifying `dd40_config`.

---

## Config File Locations

Files are loaded in priority order. Later layers win; earlier keys survive if the
later layer doesn't define them.

| Priority | Location |
|---|---|
| 1 (lowest) | Compiled-in `Default` impl on each section struct |
| 2 | Platform config dir: `~/.config/dd40/config.toml` (Linux/macOS), `%APPDATA%\dd40\config.toml` (Windows) |
| 3 | Binary-adjacent `./config.toml` in the working directory |
| 4 | Path from `DD40_CONFIG` env var |
| 5 (highest) | Per-key env var overrides: `DD40_<SECTION>__<KEY>=<VALUE>` |

Missing files are silently skipped. Each file that is opened is logged at `INFO`.

**The writable save target** is the highest-priority file that actually exists (or
that the binary can create). This is where [`save_config_section`] writes.

---

## Example `config.toml`

```toml
[network]
host            = "127.0.0.1"  # client: server address to connect to
port            = 6969          # server: listen port; client: connect port
private_key     = ""            # Netcode.io auth key (32 comma-separated bytes)
render_distance = 8             # server: chunk broadcast radius in chunks

[chunk_storage]
save_history  = false
save_entities = false

[texture_pack]
search_paths = []   # extra pack-root directories, appended after programmatic paths
```

All keys are optional. Missing keys fall back to the section struct's `Default`.

---

## Env Var Overrides

Any `DD40_<SECTION>__<KEY>=<VALUE>` environment variable overrides the
corresponding TOML key. Values are auto-coerced:

| Value format | TOML type |
|---|---|
| `1 \| true \| yes \| on` (case-insensitive) | `bool = true` |
| `0 \| false \| no \| off` (case-insensitive) | `bool = false` |
| Any `i64`-parseable string | `integer` |
| Any `f64`-parseable string | `float` |
| Everything else | `string` |

### Examples

```bash
DD40_NETWORK__HOST=192.168.1.10       # overrides [network] host
DD40_NETWORK__PORT=7000               # overrides [network] port
DD40_NETWORK__RENDER_DISTANCE=16      # overrides [network] render_distance
DD40_CHUNK_STORAGE__SAVE_HISTORY=true
DD40_CONFIG=/path/to/my-config.toml  # use a different config file entirely
```

### Legacy aliases

| Old env var | Canonical replacement |
|---|---|
| `DD40_PRIVATE_KEY` | `DD40_NETWORK__PRIVATE_KEY` |

Using a legacy alias logs a `WARN` and still works.

---

## Adding Config to Your Crate

### 1. Implement `ConfigSection`

```rust
use dd40_config::ConfigSection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct MyModConfig {
    pub spawn_rate: f64,
    pub enabled: bool,
}

impl Default for MyModConfig {
    fn default() -> Self {
        Self { spawn_rate: 1.0, enabled: true }
    }
}

impl ConfigSection for MyModConfig {
    // Must match the TOML section key and the SECTION part of DD40_<SECTION>__<KEY>.
    const SECTION: &'static str = "my_mod";
}
```

The `SECTION` constant determines:
- The TOML table key: `[my_mod]` in `config.toml`.
- The env var prefix: `DD40_MY_MOD__*`.

### 2. Read config in your plugin

Add `dd40_config` as a dependency and call `raw.section::<T>()` in a `Startup`
system (or in `PreStartup` if other systems depend on your resource):

```rust
use bevy::prelude::*;
use dd40_config::RawConfig;

fn init_config(raw: Res<RawConfig>, mut commands: Commands) {
    commands.insert_resource(raw.section::<MyModConfig>());
}
```

No env var handling is needed — `dd40_config` already applied all `DD40_MY_MOD__*`
overrides before `RawConfig` was inserted.

### 3. Save changes back to disk (optional)

```rust
use dd40_config::{ConfigDisk, save_config_section};

fn on_settings_saved(disk: Res<ConfigDisk>, cfg: Res<MyModConfig>) {
    if let Err(e) = save_config_section(&disk, &*cfg) {
        warn!("could not save config: {e}");
    }
}
```

`save_config_section` is **layer-aware and round-trip safe**:
- Only keys that differ from lower-priority layers (or were already in the write
  target) are written — unchanged values stay authoritative in the lower-layer file.
- Unknown sections and unknown keys within the updated section are preserved.
- Writes atomically via a temp file + rename.

---

## Section Naming Convention

| Crate | Config struct | `SECTION` | TOML key | Env var prefix |
|---|---|---|---|---|
| `dd40_network` | `NetworkConfig` | `"network"` | `[network]` | `DD40_NETWORK__` |
| `dd40_chunk_storage` | `ChunkStorageConfig` | `"chunk_storage"` | `[chunk_storage]` | `DD40_CHUNK_STORAGE__` |
| `dd40_texture_pack` | `TexturePackTomlConfig` | `"texture_pack"` | `[texture_pack]` | `DD40_TEXTURE_PACK__` |

Use the crate name segment as the section name (replacing hyphens/double-underscores
with single underscores). This keeps env vars and TOML keys consistent and
predictable.

---

## Deep Merge Behaviour

When two config file layers both define keys in the same section, the keys are
**merged** (not replaced). A later layer's key wins, but keys only in an earlier
layer survive.

Example:

```toml
# Layer 2 (platform config dir):
[network]
render_distance = 8
fallback_key = "from_layer_2"

# Layer 3 (binary-adjacent):
[network]
render_distance = 16
```

Result:

```toml
[network]
render_distance = 16        # layer 3 wins
fallback_key = "from_layer_2"  # only in layer 2 — survives
```

---

## Round-trip Save Example

Suppose `~/.config/dd40/config.toml` (layer 2) has:

```toml
[network]
render_distance = 8

[my_mod]
spawn_rate = 2.0
```

And `./config.toml` (layer 3, the write target) is empty.

After calling `save_config_section(&disk, &NetworkConfig { render_distance: 16 })`:

```toml
# ./config.toml — only the changed value is written
[network]
render_distance = 16
```

The `[my_mod]` section and the base `render_distance = 8` in layer 2 are both
untouched. If you later manually change layer 2 to `render_distance = 12`, the
layer-3 file still wins with `16` — only the keys you explicitly save are recorded
in the override file.
