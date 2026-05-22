# dd40_physics

Tier 1 implementation crate. Contains all physics simulation systems:
gravity integration, block-collision resolution (O(1) voxel AABB), and
body-vs-body push-apart between `PhysicsCollider` entities.

A `TentativePosition` component (internal to this crate) is inserted on every
`PhysicsBody` entity via an observer on component addition.

## Module overview

```
src/
├── lib.rs
├── plugin.rs              — PhysicsPlugin (wires sub-plugins; ensure_plugins!)
├── integration.rs         — gravity + velocity → tentative position
├── block_collision.rs     — O(1) voxel AABB resolution
└── body_collision.rs      — body-vs-body push-apart (PhysicsCollider entities)
```

## Dependencies (dd40)

`dd40_core`, `dd40_physics_core`
