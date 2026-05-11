# dd40_player

Convenience wrapper crate that composes `PlayerInputPlugin` (from
`dd40_player_input`) and `CharacterInteractionPlugin` (from
`dd40_character_interaction`) into three focused plugins. This is a Tier 1
implementation crate and a tracked architectural exception — it depends on
other Tier 1 crates (`dd40_player_input`, `dd40_character_interaction`).
See `INCONSISTENCIES.md`.

Replacing the player controller (e.g., for a top-down game) requires only
swapping `PlayerControlsPlugin` in `dd40_client`.

## Plugins

| Plugin | Role |
|---|---|
| `PlayerPlugin` | Composes spawn + controls; convenience entry point for single-player |
| `PlayerControlsPlugin` | Wires `PlayerInputPlugin` + `CharacterInteractionPlugin` + debug info |
| `PlayerSpawnPlugin` | Spawns the player entity with physics, collider, and input components |

## Module overview

```
src/
└── lib.rs   — PlayerPlugin, PlayerControlsPlugin, PlayerSpawnPlugin
```

## Dependencies (dd40)

`dd40_core`, `dd40_physics_core`, `dd40_character_core`,
`dd40_player_input`, `dd40_character_interaction`
