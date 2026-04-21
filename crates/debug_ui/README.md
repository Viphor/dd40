# dd40_debug_ui

Debug overlay crate for dd40. Provides an in-game heads-up display of
performance and world statistics, an orientation gizmo, and an extensible
system for any other crate to push custom debug information into the overlay
without depending on this crate.

Depends only on `dd40_core`. Replace this crate with a custom debug UI by
swapping the plugin in `dd40_client`.

## Module overview

```
src/
├── lib.rs               — DebugUiPlugin: FPS counter, chunk stats, custom element host
├── custom.rs            — DebugUiElementRoot marker, spawn_custom_debug_ui / update_custom_debug_ui systems
└── orientation_gizmo.rs — OrientationGizmoPlugin: axis gizmo in the bottom-right corner
```

## Extensibility

Any crate (or game code) can surface runtime data in the overlay by attaching a
`DebugInfo` component (from `dd40_core::debug`) to any entity. The
`spawn_custom_debug_ui` and `update_custom_debug_ui` systems in this crate
detect those components automatically and create or update the corresponding
text elements. No dependency on `dd40_debug_ui` is required from the crate
providing the data.
