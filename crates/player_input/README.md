# dd40_player_input

Tier 1 implementation crate. Translates keyboard and mouse input into
`CharacterInput` on the player entity, drives the first-person camera, and
manages the `PlayerMode` state machine (normal / flying / spectator).

## Module overview

```
src/
├── lib.rs
├── plugin.rs      — PlayerInputPlugin
├── components.rs  — PlayerMode, CameraRotation, MouseSensitivity
├── state.rs       — PlayerMode state transitions
└── systems.rs     — input mapping and camera follow systems
```

## Dependencies (dd40)

`dd40_core`, `dd40_physics_core`, `dd40_character_core`
