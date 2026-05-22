# Character and Physics System

Character components live in `dd40_character_core` and physics components live
in `dd40_physics_core`. Both are Tier 0 foundation crates — they define types
and system sets only, with no game logic.

---

## Character components

All character types are in `dd40_character_core::prelude`.

### `Character`
Marker component. Identifies any entity as a character (player or NPC).

### `Player`
Marker component. Identifies the locally controlled player entity.

### `PlayerId`
```rust
pub struct PlayerId(pub u64);
```
Stable identifier that survives network reconnects.

### `MovementSpeed`
```rust
pub struct MovementSpeed(pub f32);  // units per second; default 5.0
```

### `JumpImpulse`
```rust
pub struct JumpImpulse(pub f32);  // upward velocity on jump; default 8.0
```
Entities **without** this component cannot jump.

### `CharacterBundle`
Convenience bundle: `Character + MovementSpeed + Transform + Name`.

### `SpawnPosition`
```rust
pub struct SpawnPosition(pub Vec3);
```
Resource. Written by the network or spawn system to indicate where to place
the player entity.

---

## Character input

### `CharacterInput`
```rust
pub struct CharacterInput {
    pub movement: Vec2,  // local XZ direction (not normalised)
    pub pitch:    f32,   // camera pitch in radians
    pub yaw:      f32,   // camera yaw in radians
    pub jump:     bool,
    pub sprint:   bool,
}
```
Written by `dd40_player_input` (or the network layer for remote characters).
Read by the physics integration system each tick. Network systems that write
`CharacterInput` must do so in `PhysicsSet::InputSync`.

---

## Physics components

All physics types are in `dd40_physics_core::prelude`.

### `PhysicsBody`
Marker. Entities with this component participate in the physics simulation.

### `PhysicsPosition`
```rust
pub struct PhysicsPosition(pub Vec3);
```
The authoritative physics position. Distinct from `Transform`, which is the
visual-only output updated by frame interpolation in `Update`.

### `Velocity`
```rust
pub struct Velocity(pub Vec3);  // world-space, units per second
```

### `GravityScale`
```rust
pub struct GravityScale(pub f32);  // multiplier on global gravity; default 1.0
```

### `Grounded`
Marker component. Present when the character's feet are touching a solid surface.

### `Impulse`
```rust
pub struct Impulse(pub Vec3);
```
One-shot velocity addition applied on the next physics tick, then cleared.
Use this for jumps, knockback, and explosions.

### `PhysicsCollider`
```rust
pub struct PhysicsCollider { pub half_extents: Vec3 }
```
AABB collider dimensions (half-extents in each axis) used by block collision
and character-vs-character push-apart.

### `Aabb`
Axis-aligned bounding box helper. Used internally by the collision solver.

---

## Physics resources

All physics resources are in `dd40_physics_core::resources`.

### `PhysicsConfig`
```rust
pub struct PhysicsConfig {
    pub gravity:           f32,  // downward acceleration (default 20.0)
    pub ground_friction:   f32,  // horizontal damping when grounded (default 1.0)
    pub air_friction:      f32,  // horizontal damping when airborne (default 0.0002)
    pub terminal_velocity: f32,  // maximum fall speed (default 60.0)
}
```
Override by inserting this resource before `PhysicsCorePlugin`.

### `PhysicsSpatialCache`
Resource. A spatial index of all character AABB positions, rebuilt each physics
tick. Used by the character-vs-character push-apart stage.

---

## System sets

### `PhysicsSet`
Five ordered stages, all in `FixedUpdate` (from `dd40_physics_core::system_sets`):

| Stage | What runs there |
|---|---|
| `PhysicsSet::InputSync` | Network / remote input writes `CharacterInput` here |
| `PhysicsSet::Integrate` | Apply gravity + velocity → tentative position |
| `PhysicsSet::BlockCollision` | Sweep tentative position against the block grid; emit `BodyBlockContact` for every touching face |
| `PhysicsSet::BodyCollision` | Push overlapping physics colliders apart; emit `BodyBodyContact` for every overlap |
| `PhysicsSet::Finalise` | Write resolved `PhysicsPosition` back to `Transform` |

---

## Collision-contact messages

Both messages live in `dd40_physics_core::messages` and are registered by
`PhysicsCorePlugin`. They are written by `dd40_physics` once per
`FixedUpdate` tick and can be subscribed to by any system via
`MessageReader<T>`.

### `BodyBlockContact { body, block, normal, penetration }`

Emitted by `dd40_physics::block_collision::detect_block_contacts`
(parallel scan over `PhysicsBody` AABBs) for every body face that is
within 1 mm of a solid block face. `normal` points **from the block
toward the body** (e.g. `+Y` for a body resting on a block).

### `BodyBodyContact { a, b, normal, penetration }`

Emitted by `dd40_physics::body_collision::resolve_body_collisions`
whenever two physics colliders overlap. `a.index() < b.index()` is
guaranteed (use `BodyBodyContact::new` to construct correctly).
`normal` points **from `b` toward `a`**.

### `CharacterRenderSet`
Two ordered stages in `Update` (from `dd40_character_core::system_sets`):

| Stage | What runs there |
|---|---|
| `CharacterRenderSet::FrameInterpolation` | Write the smoothed visual `Transform` |
| `CharacterRenderSet::CameraSync` | Follow the smoothed `Transform` with the camera |

Both `dd40_network` and `dd40_player_input` import this set so they can
declare deterministic ordering without a direct dependency on each other.

---

## Collision shapes

Set via `BlockDefinition::with_collision_shape(shape)` when registering a block.
Shapes are defined in `dd40_core::prelude::CollisionShape`.

| Variant | When to use |
|---|---|
| `CollisionShape::FullCube` | Standard solid block (default) |
| `CollisionShape::Box { min, max }` | Partial-cell shape: slabs, stairs, etc. (cell-local [0,1] range) |
| `CollisionShape::None` | Non-solid: air, flowers, torches |

---

## Building a character entity

```rust
use dd40_character_core::prelude::*;
use dd40_physics_core::prelude::*;

fn spawn_character(mut commands: Commands) {
    CharacterBuilder::new("Alice")
        .with_position(Vec3::new(0.0, 74.0, 0.0))
        .with_movement_speed(MovementSpeed(6.0))
        .with_jump_impulse(JumpImpulse(9.0))
        .build(&mut commands);
}
```

See `crates/network/src/client/spawn.rs` and `crates/network/src/server/spawn.rs`
for full worked examples (the network crate is the only place that spawns
character entities now that the `dd40_player` wrapper has been removed).
