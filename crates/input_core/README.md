# dd40_input_core

Tier 0 foundation crate. Defines the **shared vocabulary** of
`bevy_enhanced_input` action types and the networked `OnFoot` context
that flows across the wire via lightyear.

This crate is intentionally small and binding-agnostic — it does not
spawn entities, install bindings, or react to input. Consumers:

- `dd40_player_input` provides the client-side bindings and the
  translator from action state → `CharacterInput`.
- `dd40_network` registers `OnFoot` with lightyear so action values
  replicate from client → server.

## Module overview

```
src/
├── lib.rs       — re-exports
├── plugin.rs    — InputCorePlugin: registers actions for reflection
├── contexts.rs  — OnFoot (networked, Serde-serializable)
└── actions.rs   — Move, Jump, Sprint, Attack, Place, Interact,
                   CameraRotation (networked) + Look, Pause,
                   ToggleFreeCam, FreeCamUp, FreeCamDown, RmbPress
                   (client-local)
```

## Dependencies (dd40)

None. Depends only on `bevy` and `bevy_enhanced_input`.
