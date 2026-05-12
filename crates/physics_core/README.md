# dd40_physics_core

Tier 0 foundation crate. Defines all physics types, components, and system
sets used across the dd40 codebase. Contains no game logic — only the shared
vocabulary that physics behaviour speaks.

## Module overview

```
src/
├── lib.rs
├── plugin.rs          — PhysicsCorePlugin
├── prelude.rs         — re-exports of all stable public types
├── components.rs      — PhysicsBody, CharacterPosition, Velocity, GravityScale,
│                        Grounded, Impulse, CharacterCollider, Aabb
├── resources/
│   ├── mod.rs         — PhysicsConfig (gravity, ground_friction, air_friction,
│   │                    terminal_velocity)
│   └── spatial_cache.rs — CharacterSpatialCache
└── system_sets.rs     — PhysicsSet (InputSync → Integrate → BlockCollision →
                         CharacterCollision → Finalise)
```

## Dependencies (dd40)

`dd40_core`
