# dd40_loot_core

Foundation vocabulary for the loot pipeline: `LootTable`, `LootEntry`,
`LootMode`.

See the crate-level Rust docs (`cargo doc -p dd40_loot_core --open`)
for the authoritative API. In short:

- `LootTable` is a sequence of `LootEntry` values with a `LootMode`
  that says how they combine when rolled.
- `LootEntry` covers `Fixed { item, count }`,
  `Range { item, min, max }`, and `Chance { item, count, probability }`.
- `LootTable::roll(&mut dyn RngCore) -> Vec<ItemStack>` is the only way
  to consume a table.
- `LootTable` implements `BlockData` so it can be attached to a
  `BlockDefinition` via `BlockDefinition::with_data(table)`.
- `LootCorePlugin` registers the type with the block-data type
  registry so it round-trips through cell-data serialisation.

The crate has no runtime systems. The Tier-1 [`dd40_loot`] crate is
what actually rolls tables and emits drops.

[`dd40_loot`]: ../loot
