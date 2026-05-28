# dd40_player_input

Tier 1 implementation crate. Owns the **client-side** input pipeline.

## Responsibilities

- Spawns `bevy_enhanced_input` action entities + bindings on the local
  player (`bindings.rs`) on `Added<Player>`. Includes `OnFoot`,
  `FreeCam`, and `LocalUi` contexts and their action entities.
- Defines client-only contexts `FreeCam` (free-cam movement) and
  `LocalUi` (cursor-locked UI: look, pause, toggle-freecam, RMB
  dispatch) — see `contexts.rs`. The character-action context `OnFoot`
  itself lives in `dd40_input_core` so any future input source can
  reuse the symbol.
- Translates BEI character action state into the per-tick
  [`CharacterInput`] (`translation.rs`). Runs in `FixedPreUpdate` inside
  [`dd40_input_core::system_sets::InputTranslationSet`] so the network
  bridge in `dd40_network` can order `.after(...)` it and ship the same
  tick's value.
- Drives the first-person camera, cursor lock, pause state, free-cam
  mode, and RMB → Place/Interact dispatch (`systems.rs`). Mouse-look is
  split by mode: `mouse_look` runs only in `PlayerMode::Controller` and
  drives both the player's [`CharacterFace`] rotation and
  [`CharacterInput::yaw`]/`pitch`; `free_cam_look` runs only in
  `PlayerMode::FreeCam` and rotates the camera entity directly,
  leaving the character face frozen.

This crate is **client-only** — it is added by `dd40_client` only,
never by `dd40_server`. The server does not evaluate BEI; it consumes
the replicated `ActionState<PlayerInput>` directly off the wire (see
`dd40_network`).

## Module overview

```
src/
├── lib.rs
├── plugin.rs       — PlayerInputPlugin (full client pipeline) +
│                     PlayerInputTranslationPlugin (translator only)
├── bindings.rs     — spawn_local_player_input_tree (Added<Player>),
│                     installs OnFoot/FreeCam/LocalUi actions + bindings
├── contexts.rs     — FreeCam, LocalUi (client-only contexts)
├── translation.rs  — apply_actions_to_character_input
│                     (Action<Move/Jump/…> → CharacterInput,
│                      placed in InputTranslationSet)
├── state.rs        — PlayerMode (Controller | FreeCam) state
└── systems.rs      — camera, cursor, RMB dispatch, mode-toggle observer,
                      mouse_look (Controller), free_cam_look (FreeCam),
                      free_cam_movement, sync_camera_to_face
```

## Dependencies (dd40)

`dd40_core`, `dd40_physics_core`, `dd40_character_core`,
`dd40_input_core`, `dd40_item_core`.

[`CharacterInput`]: ../dd40_character_core/controller/struct.CharacterInput.html
[`CharacterFace`]: ../dd40_character_core/face/struct.CharacterFace.html
[`dd40_input_core::system_sets::InputTranslationSet`]: ../dd40_input_core/system_sets/struct.InputTranslationSet.html
