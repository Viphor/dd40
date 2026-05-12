//! Item identifiers, definitions, and the [`ItemRegistry`] resource.
//!
//! # Registry model
//!
//! Items are stored in [`ItemRegistry`] and addressed by opaque integer IDs
//! ([`ItemId`]).  The pattern is intentionally identical to
//! [`BlockRegistry`][dd40_core::block::registry::BlockRegistry] and
//! [`ToolRegistry`][dd40_core::tools::ToolRegistry] so callers learn the
//! convention once.
//!
//! # ID allocation
//!
//! - `ItemId(0)` is reserved for [`ItemId::EMPTY`] — the "no item" sentinel.
//! - Vanilla items registered by `dd40_vanilla_palette` use IDs `1..1000`.
//! - Custom modded items should use IDs `1000` and above to avoid clashing
//!   with future vanilla additions.
//!
//! # System ordering
//!
//! Item-registration systems run in [`ItemRegistrySet`].  This set has no
//! enforced ordering relative to
//! [`BlockRegistrySet`][dd40_core::block::registry::BlockRegistrySet] or
//! [`ToolRegistrySet`][dd40_core::tools::ToolRegistrySet] because items
//! reference blocks and tools by *id* — those IDs are valid as soon as their
//! constants are defined, regardless of registration order.  An item whose
//! `placeable` block is not yet registered still works; the placement system
//! will simply find no definition until the block is registered.

use bevy::prelude::*;
use dd40_core::block::BlockId;
use dd40_core::tools::{ToolKindId, ToolTierId};
use serde::{Deserialize, Serialize};

/// System set for item-registration systems.
///
/// Add registration systems here during `Startup`:
///
/// ```ignore
/// app.add_systems(Startup, my_register_items.in_set(ItemRegistrySet));
/// ```
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemRegistrySet;

/// Opaque index into the [`ItemRegistry`].
///
/// `ItemId(0)` is the engine invariant [`ItemId::EMPTY`] — the
/// "no item" sentinel used by empty inventory slots and uninitialised
/// [`ActiveItem`][crate::active_item::ActiveItem]s.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect, Serialize, Deserialize, Default,
)]
pub struct ItemId(pub u16);

impl ItemId {
    /// The "no item" sentinel — used by empty inventory slots.
    pub const EMPTY: Self = Self(0);
}

impl std::fmt::Display for ItemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ItemId({})", self.0)
    }
}

/// Tool-related properties of an item.
///
/// Attach to an [`ItemDefinition`] via [`ItemDefinition::with_tool`] when the
/// item should grant a mining-speed bonus.  The mining system reads these
/// fields and feeds them to
/// [`mining_duration`][dd40_core::tools::mining_duration].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect, Serialize, Deserialize)]
pub struct ToolBehavior {
    /// The tool kind (e.g. `VanillaToolKinds::PICKAXE`).
    pub kind: ToolKindId,
    /// The material tier (e.g. `VanillaToolTiers::IRON`).
    pub tier: ToolTierId,
}

/// Definition of an item type.
///
/// This is the single source of truth for everything the engine needs to know
/// about an item — name, stack size, optional tool behaviour, optional
/// placement block.  All properties live here so that [`ItemRegistry`] is the
/// only resource callers need to consult.
#[derive(Debug, Clone, Reflect)]
pub struct ItemDefinition {
    /// Unique identifier for this item.
    pub id: ItemId,
    /// Human-readable name (e.g. `"iron_pickaxe"`).
    pub name: String,
    /// Maximum number of items that may share a single stack.
    ///
    /// `1` for non-stackable items (tools), `64` for typical stackable
    /// resources.  Defaults to `64`.
    pub max_stack: u16,
    /// Tool behaviour: when `Some`, holding this item as the
    /// [`ActiveItem`][crate::active_item::ActiveItem] grants the mining-speed
    /// bonus described by the contained kind/tier pair.
    pub tool: Option<ToolBehavior>,
    /// Placement target: when `Some`, "use item" actions place this block at
    /// the targeted face when the targeted voxel is replaceable.
    pub placeable: Option<BlockId>,
}

impl ItemDefinition {
    /// Creates a new item definition with sensible defaults.
    ///
    /// | Field        | Default    |
    /// |--------------|------------|
    /// | `max_stack`  | `64`       |
    /// | `tool`       | `None`     |
    /// | `placeable`  | `None`     |
    pub fn new(id: ItemId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            max_stack: 64,
            tool: None,
            placeable: None,
        }
    }

    /// Sets the maximum stack size.
    pub fn with_max_stack(mut self, max_stack: u16) -> Self {
        self.max_stack = max_stack;
        self
    }

    /// Marks this item as a tool of the given kind and tier.
    pub fn with_tool(mut self, kind: ToolKindId, tier: ToolTierId) -> Self {
        self.tool = Some(ToolBehavior { kind, tier });
        self
    }

    /// Marks this item as placeable, producing the given block on use.
    pub fn with_placeable(mut self, block: BlockId) -> Self {
        self.placeable = Some(block);
        self
    }
}

/// Registry that stores all registered item types.
///
/// This resource is inserted by [`ItemCorePlugin`][crate::plugin::ItemCorePlugin]
/// with the [`ItemId::EMPTY`] sentinel pre-populated.  Vanilla items are
/// registered by `dd40_vanilla_palette`; modded items may be registered by
/// any crate during [`ItemRegistrySet`].
#[derive(Resource, Default, Reflect)]
pub struct ItemRegistry {
    items: Vec<ItemDefinition>,
}

impl ItemRegistry {
    /// Creates a new registry with the [`ItemId::EMPTY`] sentinel pre-registered.
    pub fn new() -> Self {
        let mut registry = Self { items: Vec::new() };
        registry.insert_definition(ItemDefinition::new(ItemId::EMPTY, "empty").with_max_stack(0));
        registry
    }

    /// Inserts a definition, filling any gap in the dense ID array with
    /// `unknown_<n>` placeholders.  Returns the assigned [`ItemId`].
    fn insert_definition(&mut self, definition: ItemDefinition) -> ItemId {
        let id = definition.id;
        while self.items.len() <= id.0 as usize {
            let placeholder_id = ItemId(self.items.len() as u16);
            self.items.push(
                ItemDefinition::new(placeholder_id, format!("unknown_{}", placeholder_id.0))
                    .with_max_stack(0),
            );
        }
        self.items[id.0 as usize] = definition;
        id
    }

    /// Registers a new item type at its declared [`ItemId`].
    ///
    /// If the slot is already occupied it will be replaced — this matches
    /// [`BlockRegistry`][dd40_core::block::registry::BlockRegistry]'s
    /// behaviour and lets palettes deterministically reserve known IDs.
    pub fn register(&mut self, definition: ItemDefinition) -> ItemId {
        self.insert_definition(definition)
    }

    /// Registers a new item with an auto-assigned ID (the next free slot).
    ///
    /// The `id` field on the supplied definition is ignored.
    pub fn register_auto(&mut self, mut definition: ItemDefinition) -> ItemId {
        let id = ItemId(self.items.len() as u16);
        definition.id = id;
        self.items.push(definition);
        id
    }

    /// Looks up an item definition by ID.
    pub fn get(&self, id: ItemId) -> Option<&ItemDefinition> {
        self.items.get(id.0 as usize)
    }

    /// Iterates over every registered item definition, in ID order.
    pub fn iter(&self) -> impl Iterator<Item = &ItemDefinition> {
        self.items.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_sentinel_is_pre_registered() {
        let registry = ItemRegistry::new();
        let empty = registry.get(ItemId::EMPTY).expect("EMPTY sentinel exists");
        assert_eq!(empty.name, "empty");
        assert_eq!(empty.max_stack, 0);
    }

    #[test]
    fn register_at_declared_id_fills_gap_with_placeholders() {
        let mut registry = ItemRegistry::new();
        let id = registry.register(ItemDefinition::new(ItemId(3), "stone"));
        assert_eq!(id, ItemId(3));
        assert_eq!(registry.get(ItemId(3)).unwrap().name, "stone");
        assert!(registry.get(ItemId(1)).unwrap().name.starts_with("unknown_"));
        assert!(registry.get(ItemId(2)).unwrap().name.starts_with("unknown_"));
    }

    #[test]
    fn register_auto_uses_next_free_slot() {
        let mut registry = ItemRegistry::new();
        let id = registry.register_auto(ItemDefinition::new(ItemId(0), "ignored"));
        assert_eq!(id, ItemId(1));
        assert_eq!(registry.get(id).unwrap().name, "ignored");
    }

    #[test]
    fn builders_set_optional_fields() {
        let def = ItemDefinition::new(ItemId(1), "iron_pickaxe")
            .with_max_stack(1)
            .with_tool(ToolKindId(1), ToolTierId(2))
            .with_placeable(BlockId(7));
        assert_eq!(def.max_stack, 1);
        assert_eq!(def.tool.unwrap().kind, ToolKindId(1));
        assert_eq!(def.tool.unwrap().tier, ToolTierId(2));
        assert_eq!(def.placeable, Some(BlockId(7)));
    }
}
