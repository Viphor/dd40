# dd40_player

Player input, camera, and block-interaction crate for dd40. Handles
keyboard/mouse input, translates it into `CharacterInput` on the player entity,
follows the player with a camera, and provides block targeting and placement.

Depends only on `dd40_core`. Replacing this crate with a custom player
controller (e.g., for a top-down game or an NPC-only server) requires only
swapping the plugin in `dd40_client`.

## Module overview

```
src/
├── lib.rs                        — PlayerInputPlugin, player spawning, camera follow, input mapping
└── block_interaction/
    ├── mod.rs                    — BlockInteractionPlugin, BlockInteractionConfig, public re-exports
    ├── targeting.rs              — Ray-cast block targeting; TargetedBlock resource, BlockFace enum
    └── placement.rs              — Block placement and removal; HeldBlock resource
```
