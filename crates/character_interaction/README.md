# dd40_character_interaction

Tier 1 implementation crate. Provides block targeting (DDA ray-cast), mining,
and block placement for any `Character` entity — not just the local player.

Re-exports `MiningState` from `dd40_character_core` as part of its public API.

## Module overview

```
src/
├── lib.rs          — CharacterInteractionPlugin, public re-exports
├── plugin.rs       — system wiring, ensure_plugins!
├── targeting.rs    — TargetedBlock, BlockFace, DDA ray-cast
├── placement.rs    — block placement (reads ActiveItem)
└── mining.rs       — mining state update, block removal
```

## Dependencies (dd40)

`dd40_core`, `dd40_physics_core`, `dd40_character_core`
