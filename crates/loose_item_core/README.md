# dd40_loose_item_core

Foundation vocabulary for **loose items** — item stacks that exist in
the world as standalone entities (dropped from kills, thrown by
players, scattered from broken chests).

See the crate-level Rust docs (`cargo doc -p dd40_loose_item_core
--open`) for the authoritative API. In short:

- `LooseItem { stack }` is the marker component carrying the item
  payload. Attach it to any entity to make it behave like a "ground
  item" — physics moves it, the pickup integration grants it to
  touching characters, and the merge system combines same-id stacks
  after a brief contact period.
- `DespawnTimer(Timer)` controls the time-to-live (default 5 minutes).
  When two stacks merge, the resulting entity inherits the **larger**
  input's timer.
- `PickupCooldown(Timer)` is a short post-spawn / post-drop gate
  (default 500 ms) preventing instant re-pickup.
- `LooseItemConfig` is the server-side defaults resource
  (`default_lifetime`, `default_pickup_cooldown`, `attraction_radius`).
- `LooseItemSet` orders the four pipeline stages: `Spawn → Attract →
  Resolve → Lifecycle`. Downstream crates anchor their systems here.
- `LooseItemCorePlugin` registers all of the above; it has no game
  systems.

Implementation crates that build on top of this:

- `dd40_loose_items` — spawning, despawn timer, merge.
- `dd40_integration_loose_item_pickup` — pickup + attraction
  (integration crate, only place that touches both `LooseItem` and
  `Inventory`).
