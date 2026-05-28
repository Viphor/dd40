//! Per-item icon cache with a placeable-block-colour fallback.
//!
//! The slot widget renders one of three things per item:
//!
//! 1. A PNG icon loaded from [`ItemDefinition::icon_path`] (when set).
//! 2. The flat colour of the item's placeable block, looked up via
//!    [`BlockRegistry`] → [`BlockDefinition::color`].
//! 3. Magenta — a "missing icon" placeholder — when the item has neither
//!    an icon nor a placeable block.
//!
//! [`ItemIconCache`] memoises whichever of those resolves into a stable
//! [`ItemIcon`] enum so the widget update path is a cheap hash-map
//! lookup.  Entries are populated lazily on first request; if the
//! registries change the cache is cleared and rebuilt.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use dd40_core::block::{BlockDefinition, BlockRegistry};
use dd40_item_core::registry::{ItemId, ItemRegistry};

/// Resolved per-item icon — either a loaded image handle or a flat colour.
#[derive(Debug, Clone)]
pub enum ItemIcon {
    /// PNG icon resolved via [`AssetServer`].
    Image(Handle<Image>),
    /// Flat colour swatch.  Sourced either from the placeable block's
    /// definition or from the magenta missing-icon fallback.
    Color(Color),
}

/// The "missing icon" colour — a loud magenta used when neither an icon
/// path nor a placeable block colour can be resolved.
pub const MISSING_ICON_COLOR: Color = Color::srgb(1.0, 0.0, 1.0);

/// Cache mapping every queried [`ItemId`] to its resolved [`ItemIcon`].
///
/// Populated lazily by [`resolve_icon`].  Cleared whenever the item or
/// block registry is mutated so a freshly-registered item picks up the
/// right fallback on its next lookup.
#[derive(Resource, Default)]
pub struct ItemIconCache {
    map: HashMap<ItemId, ItemIcon>,
}

impl ItemIconCache {
    /// Returns the icon for `item`, computing and inserting it on first
    /// request.  Subsequent calls are O(1).
    pub fn get_or_resolve(
        &mut self,
        item: ItemId,
        items: &ItemRegistry,
        blocks: &BlockRegistry,
        asset_server: &AssetServer,
    ) -> ItemIcon {
        if let Some(existing) = self.map.get(&item) {
            return existing.clone();
        }
        let resolved = resolve_icon(item, items, blocks, asset_server);
        self.map.insert(item, resolved.clone());
        resolved
    }

    /// Clears all cached entries.  Call when a registry mutation may
    /// have invalidated previously-resolved fallbacks.
    pub fn clear(&mut self) {
        self.map.clear();
    }
}

/// Pure resolution of [`ItemIcon`] for a single item id.
///
/// Public so tests can exercise the fallback chain without spinning up
/// an `App`.  Production code should prefer
/// [`ItemIconCache::get_or_resolve`].
pub fn resolve_icon(
    item: ItemId,
    items: &ItemRegistry,
    blocks: &BlockRegistry,
    asset_server: &AssetServer,
) -> ItemIcon {
    let def = items.get(item);
    if let Some(path) = def.and_then(|d| d.icon_path.as_ref()) {
        return ItemIcon::Image(asset_server.load(path.clone()));
    }
    if let Some(block) = def
        .and_then(|d| d.placeable)
        .and_then(|b| blocks.get(b))
        .map(BlockDefinition::clone_color)
    {
        return ItemIcon::Color(block);
    }
    ItemIcon::Color(MISSING_ICON_COLOR)
}

trait CloneColor {
    fn clone_color(&self) -> Color;
}

impl CloneColor for BlockDefinition {
    fn clone_color(&self) -> Color {
        self.color
    }
}

#[cfg(test)]
mod tests {
    // Pure-fallback-chain tests live in inv-fd-integration-tests because
    // constructing a BlockRegistry + ItemRegistry + AssetServer requires
    // the full Bevy AssetPlugin, which an integration test can install
    // cheaply but a unit test cannot.
}
