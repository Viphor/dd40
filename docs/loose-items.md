# Loose Items

Loose items are item stacks that exist in the world as standalone entities —
dropped from killed mobs, thrown by players, scattered from broken chests.
They are **server-authoritative**: the server owns spawning, physics,
merging, pickup, and despawn; clients only interpolate their position and
draw the visual.

The system is split across four crates so that the foundation vocabulary,
the producer, the inventory glue, and the visuals can each evolve
independently.

| Crate | Tier | Wired into | Responsibility |
|---|---|---|---|
| `dd40_loose_item_core` | 0 (foundation) | both | components, config, system-set ordering |
| `dd40_loose_items` | 1 | **server** | spawn, lifetime, merge, persistence |
| `dd40_integration_loose_item_pickup` | 1 | **server** | attraction + pickup into inventory |
| `dd40_loose_item_render` | 1 | **client** | spinning, bobbing cube visual |

---

## Foundation: `dd40_loose_item_core`

### Components

```rust
pub struct LooseItem {
    pub stack: ItemStack,   // ItemId + count
}

pub struct DespawnTimer(pub Timer);    // counts down toward removal
pub struct PickupCooldown(pub Timer);  // refuses pickup while > 0
```

A loose item entity additionally carries the standard physics body
(`PhysicsBody`, `PhysicsPosition`, `Velocity`, `PhysicsCollider`) so
that gravity, block collisions, and body-vs-body contacts apply to it
exactly like to a character.

### `LooseItemConfig`

Single resource holding every tunable for the system.  Defaults are tuned
for a Minecraft-like feel:

| Field | Default | Purpose |
|---|---|---|
| `default_lifetime` | **5 min** | initial value of `DespawnTimer` on spawn |
| `default_pickup_cooldown` | **500 ms** | initial value of `PickupCooldown`; prevents drop-and-instantly-re-pickup |
| `attraction_radius` | **1.5 m** | how close a character must be before items drift toward them; `0.0` disables |
| `attraction_strength` | **12.0 m/s²** | acceleration applied to attracted items, scaled by `(1 − dist / radius)` |
| `merge_contact_duration` | **1 s** | how long two same-item stacks must touch before merging |

Mutate the resource at startup — or at any time — to retune.

### `LooseItemSet`

```text
Spawn → Attract → Resolve → Lifecycle
```

All loose-item systems are ordered inside this set so downstream crates
can hang systems off a stable, named phase without inspecting individual
system names.

---

## Server: `dd40_loose_items`

### Spawning

[`DropItems`](dd40_inventory_core::drop::DropItems) is the **only** way
loose items enter the world.  The emitter (loot tables, block-breaking,
player throw, etc.) supplies the spawn point and initial velocity vector;
scatter, if any, is the emitter's job.  This keeps spawn control local
to the systems that know the intent.

`loose_item_bundle(stack, position, velocity, &config)` is the single
source of truth for the components a loose item needs.  Both the spawner
and the persistence restore path call into it so that newly dropped and
restored items are indistinguishable.

### Merging

A `BodyBodyContact` between two `LooseItem` entities carrying the same
`ItemId` accumulates contact time in a small in-system map.  Once that
time exceeds `merge_contact_duration`, the smaller stack is folded into
the larger one and despawned.  The surviving entity keeps the **larger**
`DespawnTimer` of the two so that merging never shortens an item's
remaining life.

### Lifetime

`DespawnTimer` ticks every frame inside `LooseItemSet::Lifecycle`.  When
it hits zero the entity is despawned.  `PickupCooldown` ticks the same
way; the pickup integration crate only considers items whose cooldown
has finished.

### Stuck resolution

If a block is placed onto a loose item, the physics block-collision
solver pushes the item to the nearest empty cell (in any direction,
not just upward).  This is handled by the standard collision-resolution
pass — there is no `UnstuckWhenOverlapping` marker component.

### Persistence

`LooseItemsPlugin` registers a `LooseItemPersister` with
[`EntityPersisterRegistry`](dd40_core::persistence::EntityPersisterRegistry).
Each persister handles one entity kind; this one captures
`(stack, position, velocity, despawn_timer, pickup_cooldown)` into a
versioned `LooseItemPayload::V1` enum.

Loose items are **bucketed by the chunk that contains their centre
point** (computed with `ChunkPos::from(&center)` so Y-stacked chunks
work transparently), saved into a sidecar file
`entities_X_Y_Z.bin` next to `chunk_X_Y_Z.bin`, and restored the next
time that chunk is loaded.  See [`chunk-system.md`](chunk-system.md)
for the chunk pipeline.

---

## Server: `dd40_integration_loose_item_pickup`

This is the **only** crate in the workspace where `LooseItem` and
`InventoryComponent` are both visible.  Either foundation can change
independently as long as this thin glue layer keeps up.

### Attraction

Every frame, the attraction system pulls eligible loose items toward
nearby characters with a free or stackable inventory slot.  The
acceleration is `attraction_strength × (1 − dist / attraction_radius)`,
applied to the item's `Velocity`.  Characters with full inventories
exert no pull.

### Pickup

When a `BodyBodyContact` fires between a character and a loose item
whose `PickupCooldown` has elapsed:

1. Build the candidate list of characters touching the item this tick.
2. Filter to characters with an `Inventory` that can accept the stack
   (`find_slot` returns `Some`).
3. **Rank by Euclidean distance to the item.**  Closest character wins.
4. **Tie-break by lowest `Entity` index** — only relevant when two
   characters are exactly the same distance, which essentially never
   happens with `f32` math but is defined so the outcome is
   deterministic.
5. Grant the stack to the winner.  Any leftover (slot overflowed)
   stays on the ground with a fresh `PickupCooldown`.

---

## Client: `dd40_loose_item_render`

`LooseItemRenderPlugin` attaches a child cube to every replicated
`LooseItem` and:

- spins it slowly around the world Y axis,
- bobs it gently up and down with a sine,
- raises the visual a touch off the floor so it doesn't z-fight with
  the block below.

The cube colour resolves through a fallback chain:

1. The item's own custom render — **TODO**: when `ItemDefinition`
   grows `mesh` / `texture` fields these will be honoured first.
2. The colour of the [placeable](dd40_item_core::registry::ItemDefinition::placeable)
   block the item maps to, looked up in `BlockRegistry`.
3. A neutral billboard colour as a last resort.

The render crate never mutates `LooseItem` state — it only reads
position and item id to produce visuals.

---

## Cross-references

- [`character-physics.md`](character-physics.md) — `PhysicsBody`,
  `PhysicsCollider`, `BodyBodyContact`, `BodyBlockContact`.
- [`chunk-system.md`](chunk-system.md) — chunk pipeline, sidecar
  loading, on-exit save.
- `dd40_core::persistence` — the `EntityPersister` trait every
  persistent-entity kind (loose items today; NPCs tomorrow) implements.
