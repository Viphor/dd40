# dd40_character_core

Tier 0 foundation crate. Defines character-related types, the input bridge,
`MiningState`, `PlayerId`, and the render-frame system set. Contains no game
logic — only the shared vocabulary for character behaviour.

## Module overview

```
src/
├── lib.rs
├── plugin.rs          — CharacterCorePlugin
├── prelude.rs         — re-exports of all stable public types
├── components.rs      — Character, Player, PlayerId, MovementSpeed, JumpImpulse,
│                        SpawnPosition
├── bundles.rs         — CharacterBundle
├── builder.rs         — CharacterBuilder
├── controller.rs      — CharacterController, CharacterControllerPlugin, CharacterInput
├── mining_state.rs    — MiningState
└── system_sets.rs     — CharacterRenderSet (FrameInterpolation → CameraSync)
```

## Dependencies (dd40)

`dd40_core`, `dd40_physics_core`
