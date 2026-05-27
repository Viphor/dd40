# dd40_player_input

Tier 1 implementation crate. Owns the **client-side** input pipeline:

- Spawns `bevy_enhanced_input` action entities + bindings on the local
  player (`bindings.rs`).
- Defines client-only contexts `FreeCam` and `LocalUi` (`contexts.rs`).
  The networked `OnFoot` context lives in `dd40_input_core`.
- Translates BEI action state into the per-tick `CharacterInput`
  contract (`translation.rs`) — this runs on **both** client and server
  so prediction / rollback converge.
- Drives the first-person camera, cursor lock, pause toggle, free-cam
  mode, and RMB → Place/Interact dispatch (`systems.rs`).

Swapping the input crate touches this crate only; the network layer is
agnostic.

## Module overview

```
src/
├── lib.rs
├── plugin.rs       — PlayerInputPlugin, PlayerInputTranslationPlugin
├── bindings.rs     — spawn_local_player_input_tree (Added<Player>)
├── contexts.rs     — FreeCam, LocalUi
├── translation.rs  — Action<T> → CharacterInput (headless-safe)
├── state.rs        — PlayerMode state
└── systems.rs      — camera, cursor, RMB dispatch, free-cam, mouse-look
```

## Dependencies (dd40)

`dd40_core`, `dd40_physics_core`, `dd40_character_core`,
`dd40_input_core`, `dd40_item_core`.
