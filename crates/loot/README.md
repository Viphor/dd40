# dd40_loot

Server-authoritative loot system. Turns accepted
`ChunkChange::Remove` events into `DropItems` messages.

See the crate-level Rust docs (`cargo doc -p dd40_loot --open`) for the
authoritative API. In short:

- `LootPlugin` is **server-only**. The dd40 client never adds it.
- A `snapshot_remove_targets` system runs in
  `ChunkAuthoritySet::Validate` to capture the prior block id and any
  `BlockInventory` contents before the authority commit overwrites the
  cell.
- After commit, `emit_loot_drops` (in `LootSet::EmitDrops`) reads the
  `ChunkChanged` message, resolves a loot table for each removed cell,
  rolls it, appends any inventory contents, and emits a single
  `DropItems` per cell.
- Loot is resolved in this order:
  1. cell-data `LootTable`
  2. `BlockDefinition` default `LootTable`
  3. the placeable item that maps to the removed block
- Cells that had a `BlockInventory` get the inventory cleared via a
  predicted `CellDataChange` that commits next frame.

No item-entity spawner is included yet — `DropItems` is the seam that
a future spawner will consume.
