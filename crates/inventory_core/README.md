# dd40_inventory_core

Tier 0 foundation crate. Defines the shared vocabulary for inventories:
the per-entity [`Inventory`] component, the [`InventoryChanged`] entity
event, and the [`CharacterInventoryExt`] builder extension. Contains no
hotbar, selection, equipment, or UI logic.

## Role in the architecture

`Inventory` is a passive container. Any character entity can carry one;
any system that needs to read or mutate items reaches for it through the
methods on the component. Pickup crates push stacks in via
`insert_stack`; crafting crates read with `count_of` and pull with
`take_slot_n`; HUD and network code subscribe to `InventoryChanged` (or
the corresponding `Changed<Inventory>` change-detection signal) to react
when contents shift.

The crate stays **out of** selection — `RequestActiveItem` /
`ActiveItem` from `dd40_item_core` are deliberately not handled here.
A future Tier 1 inventory-interaction crate will bridge the two by
draining `RequestActiveItem` and looking up matches via
[`Inventory::find_slot`].

If that interaction crate ever wants to live in this same module tree
(adding systems and observers), the crate will be promoted to
`dd40_inventory`. The public surface is intentionally narrow so that
rename will be a `sed` away.

## Module overview

```
src/
├── lib.rs
├── plugin.rs        — InventoryCorePlugin
├── prelude.rs       — re-exports of all stable public types
├── inventory.rs     — Inventory (Component), InventoryChanged (EntityEvent),
│                       SlotChange, InsertError, find_slot
└── character_ext.rs — CharacterInventoryExt: blanket on AddExtra
```

## Dependencies (dd40)

- `dd40_core` — `AddExtra`, `ensure_plugins!`, `BlockId`, `ToolKindId`,
  `ToolTierId`, `CorePlugin`.
- `dd40_item_core` — `ItemId`, `ItemStack`, `ItemSelector`,
  `ItemRegistry`, `ItemDefinition`, `ItemCorePlugin`.

No Tier 1 dependencies. No `dd40_character_core` dependency — the builder
extension is implemented via the `AddExtra` blanket so the test suite
uses an inline `TestBuilder` instead.

## Mutating the inventory

Every mutator on `Inventory` comes in two flavours:

- **Event-firing** (recommended): `insert_stack`, `insert_stack_strict`,
  `set_slot`, `take_slot`, `take_slot_n`. Each takes
  `&mut Commands` + `Entity` and triggers exactly one
  `InventoryChanged` event (carrying the per-slot diff in slot-index
  order) on success. No-op calls fire no event.
- **Silent** (`*_without_event`): same mutation, no event. Use during
  tests, pre-spawn population, or when batching changes that should
  surface as a single higher-level event.

The `slots` field is private; all writes go through these methods so the
event-firing invariant cannot be bypassed accidentally.
