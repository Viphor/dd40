# dd40_item_core

Tier 0 foundation crate. Defines the shared vocabulary for items: the
[`ItemRegistry`] of [`ItemDefinition`]s, the per-character [`ActiveItem`]
component, and the inventory-facing messages
([`RequestActiveItem`], [`ActiveItemChanged`]). Contains no game logic and
no inventory layout.

## Role in the architecture

`ActiveItem` is the **single seam** that lets inventory crates be swapped
out without touching gameplay code. Mining, placement, and any future
"use item" system read `ActiveItem` on the character; an inventory crate
(`dd40_vanilla_inventory`, hypothetical `dd40_multi_equip`) writes it.
The two messages — `RequestActiveItem` (queued; inventory drains it) and
`ActiveItemChanged` (emitted by the inventory; HUD/network observe it) —
let policy crates such as `dd40_auto_tool_swap` plug in without
depending on any specific inventory implementation.

## Module overview

```
src/
├── lib.rs
├── plugin.rs        — ItemCorePlugin
├── prelude.rs       — re-exports of all stable public types
├── registry.rs      — ItemId, ItemDefinition, ItemRegistry, ItemRegistrySet,
│                       ToolBehavior
├── active_item.rs   — ActiveItem (per-character Component), ItemStack
└── messages.rs      — RequestActiveItem (Message), ActiveItemChanged (Event),
                        ItemSelector
```

## Dependencies (dd40)

`dd40_core` (for `BlockId`, `ToolKindId`, `ToolTierId`, and the
`CorePlugin` auto-add).

## ID allocation

- `1..1000` reserved for vanilla items (`dd40_vanilla_palette`).
- `1000..` for modded items.

"Empty" is *not* an `ItemId` — inventory slots and `ActiveItem` use
`Option<ItemStack>` to express emptiness so the type system enforces
that every `ItemId` in scope refers to a real item.
