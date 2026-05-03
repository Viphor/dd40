# dd40_player

Convenience wrapper crate that composes `PlayerMovementPlugin` and
`CharacterInteractionPlugin` into three focused plugins. This is a Tier 1
implementation crate and a tracked architectural exception — it depends on
other Tier 1 crates (`dd40_player_movement`, `dd40_character_interaction`).
See `INCONSISTENCIES.md`.

Replacing the player controller (e.g., for a top-down game) requires only
swapping `PlayerInputPlugin` in `dd40_client`.

## Plugins

| Plugin | Role |
|---|---|
| `PlayerPlugin` | Composes movement + interaction; convenience entry point |
| `PlayerInputPlugin` | Wires `PlayerMovementPlugin` + `CharacterInteractionPlugin` + debug info |
| `PlayerSpawnPlugin` | Spawns the player entity with physics, collider, and input components |

## Module overview

```
src/
└── lib.rs   — PlayerPlugin, PlayerInputPlugin, PlayerSpawnPlugin
```

## Dependencies (dd40)

`dd40_core`, `dd40_physics_core`, `dd40_character_core`,
`dd40_player_movement`, `dd40_character_interaction`
