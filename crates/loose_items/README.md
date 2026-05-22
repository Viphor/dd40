# dd40_loose_items

Tier 1 (Implementation). Server-side spawner and lifecycle for
loose items dropped on the ground.

## Responsibility

- Drain `DropItems` messages and spawn one `LooseItem` entity per
  stack, splitting oversize stacks against
  `ItemDefinition::max_stack`.
- Each spawned entity carries the full physics-body bundle
  (`PhysicsBody`, `PhysicsCollider`, `Aabb`, `Velocity`,
  `GravityScale`) plus the foundation components from
  `dd40_loose_item_core` (`LooseItem`, `DespawnTimer`,
  `PickupCooldown`).
- Tick `DespawnTimer` and `PickupCooldown` every frame and despawn
  entities whose lifetime has elapsed.

## What lives elsewhere

- Pickup, merging, attraction, replication, visuals, and
  persistence are **separate crates** that read the same
  foundation components.
- Scatter / spread on drop is the **emitter's** responsibility
  (e.g. `dd40_loot` adds random velocity to `DropItems.velocity`).

## Usage

Add `LooseItemsPlugin` to the **server** binary only:

```rust
use dd40_loose_items::LooseItemsPlugin;
app.add_plugins(LooseItemsPlugin);
```

Clients receive loose items through replication and never spawn
them locally.
