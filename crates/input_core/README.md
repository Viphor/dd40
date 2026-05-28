# dd40_input_core

Tier 0 foundation crate. Defines the **shared vocabulary** of
`bevy_enhanced_input` action types and the `OnFoot` context.

This crate is intentionally small and binding-agnostic — it does not
spawn entities, install bindings, or react to input. The only consumer
is `dd40_player_input` (client-side bindings + translator from action
state → `CharacterInput`).

The actions defined here are **client-side only**. The wire protocol
between client and server is `ActionState<PlayerInput>` (lightyear
`input_native`), produced by the bridge in `dd40_network`. The server
does not evaluate BEI.

## Module overview

```
src/
├── lib.rs         — re-exports
├── plugin.rs      — InputCorePlugin: installs EnhancedInputPlugin once
├── contexts.rs    — OnFoot (the on-foot character input context)
├── actions.rs     — Move, Jump, Sprint, Attack, Place, Interact (character)
│                    + Look, Pause, ToggleFreeCam, FreeCamUp, FreeCamDown,
│                    RmbPress (UI/camera)
└── system_sets.rs — InputTranslationSet (cross-crate ordering anchor for
                     systems that translate BEI action state into
                     CharacterInput; both dd40_player_input and dd40_network
                     reference it)
```

## Dependencies (dd40)

`dd40_core` only. Otherwise depends on `bevy` and `bevy_enhanced_input`.
