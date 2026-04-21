# App and Game States

State management is defined in `dd40_core::state` and `dd40_core::loading`.

---

## States

### `AppState`
Top-level application state. Registered by `CorePlugin`.

| Variant | When |
|---|---|
| `AppState::Loading` | Initial state; waits for `LoadingTracker` to empty |
| `AppState::Menu` | Main menu (not yet used in default client) |
| `AppState::Playing` | Active gameplay |

Transition from `Loading` to `Playing` happens automatically when
`LoadingTracker` is empty.

### `GameState`
In-game sub-state. Active only while `AppState::Playing`.

| Variant | When |
|---|---|
| `GameState::Running` | Normal gameplay |
| `GameState::Paused` | Game paused |

---

## Loading system

### `LoadingTracker`
```rust
pub struct LoadingTracker { /* named pending items */ }
```
Resource. Tracks named async initialisation items.

- `tracker.add("key", "description")` — register a pending item during startup
- `tracker.remove("key")` — mark it complete

When the tracker becomes empty, `LoadingPlugin` transitions `AppState` from
`Loading` to `Playing`.

### `LoadingSet`
System set (during `Startup`). Register loading items in this set so they are
guaranteed to run before the first completion check.

```rust
fn register_my_item(mut tracker: ResMut<LoadingTracker>) {
    tracker.add("my_crate:ready", "Waiting for my crate…");
}

impl Plugin for MyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_my_item.in_set(LoadingSet));
    }
}
```

`dd40_network` uses `LoadingTracker` to hold the `Playing` state until the
server connection is established. See `crates/network/src/client/loading.rs`.

---

## `DebugInfo` component

```rust
pub struct DebugInfo { /* section, key/value pairs */ }
```
Attach to any entity to surface runtime data in `dd40_debug_ui`'s debug overlay
without taking a dependency on that crate.

```rust
commands.spawn(DebugInfo::new("Physics")
    .with("velocity", "0,0,0")
    .with("grounded", "true"));
```

The `dd40_debug_ui` crate detects `DebugInfo` components automatically and
creates the corresponding text elements in the overlay.
