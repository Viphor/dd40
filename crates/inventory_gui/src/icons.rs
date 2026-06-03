//! Per-item icon cache with a placeable-block-colour fallback.
//!
//! The slot widget renders one of three things per item:
//!
//! 1. A PNG icon loaded from `ItemDefinition::icon_path` (when set).
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
use dd40_core::block::{BlockDefinition, BlockId, BlockRegistry};
use dd40_item_core::registry::{ItemId, ItemRegistry};

use crate::block_icon::build_block_icon;
#[cfg(feature = "textures")]
use crate::block_icon::build_block_icon_textured;

/// Pre-rendered isometric icons for every known [`BlockId`].
///
/// Populated by [`prerender_block_icons`] during `Startup` (after
/// [`dd40_core::plugin::BlockRegistrySet`]) and on any subsequent change
/// to the [`BlockRegistry`].  Avoids per-frame allocation in
/// [`ItemIconCache::get_or_resolve`].
#[derive(Resource, Default, Debug)]
pub struct BlockIconAssets {
    map: HashMap<BlockId, Handle<Image>>,
}

impl BlockIconAssets {
    /// Returns the pre-rendered icon for `block`, if one has been built.
    pub fn get(&self, block: BlockId) -> Option<Handle<Image>> {
        self.map.get(&block).cloned()
    }

    /// Builds (or rebuilds) icons for every block in `blocks`.
    ///
    /// When `atlas` is ready and the atlas image is available in `images`,
    /// blocks with [`dd40_texture_core::BlockTextures`] are rendered using
    /// their actual texture tiles (top / west / east faces).  All other
    /// blocks fall back to the procedural flat-colour cube.
    pub fn rebuild(
        &mut self,
        blocks: &BlockRegistry,
        images_assets: &mut Assets<Image>,
        #[cfg(feature = "textures")] atlas: &dd40_texture_core::BlockAtlas,
        #[cfg(feature = "textures")] images: &Assets<Image>,
    ) {
        self.map.clear();
        for def in blocks.iter() {
            let handle = images_assets.add(build_icon_for_block(
                def,
                #[cfg(feature = "textures")]
                atlas,
                #[cfg(feature = "textures")]
                images,
            ));
            self.map.insert(def.id, handle);
        }
    }
}

/// Builds the best available icon for a single block definition.
#[allow(unused_variables)]
fn build_icon_for_block(
    def: &dd40_core::block::BlockDefinition,
    #[cfg(feature = "textures")] atlas: &dd40_texture_core::BlockAtlas,
    #[cfg(feature = "textures")] images: &Assets<Image>,
) -> Image {
    #[cfg(feature = "textures")]
    if let Some(image) = try_build_textured_icon(def, atlas, images) {
        return image;
    }
    build_block_icon(def.color)
}

/// Attempts to build a textured isometric icon by sampling from the atlas.
///
/// Returns `None` when the atlas is not ready, the block has no
/// [`dd40_texture_core::BlockTextures`], or any required tile cannot be
/// extracted.
#[cfg(feature = "textures")]
fn try_build_textured_icon(
    def: &dd40_core::block::BlockDefinition,
    atlas: &dd40_texture_core::BlockAtlas,
    images: &Assets<Image>,
) -> Option<Image> {
    use dd40_texture_core::{BlockTextures, Face};

    if !atlas.is_ready() {
        return None;
    }
    let atlas_handle = atlas.texture(dd40_texture_core::AtlasId(0))?;
    let atlas_image = images.get(&atlas_handle)?;

    let textures = def.data::<BlockTextures>()?;

    // Extract top, west (left in isometric view), east (right) face tiles.
    let resolve_tile = |face: Face| -> Option<(Vec<u8>, u32, u32)> {
        let tex_ref = textures.get(face)?;
        let resolved = atlas.resolve(tex_ref)?;
        let pixels = resolved.uv.extract_tile_pixels(atlas_image)?;
        let tile_w = ((resolved.uv.max.x - resolved.uv.min.x) * atlas_image.width() as f32)
            .round() as u32;
        let tile_h = ((resolved.uv.max.y - resolved.uv.min.y) * atlas_image.height() as f32)
            .round() as u32;
        if tile_w == 0 || tile_h == 0 {
            return None;
        }
        Some((pixels, tile_w, tile_h))
    };

    let top = resolve_tile(Face::Top);
    let left = resolve_tile(Face::West);
    let right = resolve_tile(Face::East);

    // Only proceed if at least one face has a texture.
    if top.is_none() && left.is_none() && right.is_none() {
        return None;
    }

    Some(build_block_icon_textured(
        top.as_deref_with_size(),
        left.as_deref_with_size(),
        right.as_deref_with_size(),
        def.color,
    ))
}

#[cfg(feature = "textures")]
trait DerefWithSize {
    fn as_deref_with_size(&self) -> Option<(&[u8], u32, u32)>;
}

#[cfg(feature = "textures")]
impl DerefWithSize for Option<(Vec<u8>, u32, u32)> {
    fn as_deref_with_size(&self) -> Option<(&[u8], u32, u32)> {
        self.as_ref().map(|(p, w, h)| (p.as_slice(), *w, *h))
    }
}

/// Startup / change-driven system that fills [`BlockIconAssets`].
///
/// Re-runs automatically when the [`BlockRegistry`] changes.  With the
/// `textures` feature enabled it also re-runs when the [`BlockAtlas`]
/// changes (i.e. after the texture pack finishes loading).
pub fn prerender_block_icons(
    blocks: Res<BlockRegistry>,
    mut assets: ResMut<BlockIconAssets>,
    mut images: ResMut<Assets<Image>>,
    mut cache: ResMut<ItemIconCache>,
    #[cfg(feature = "textures")] atlas: Res<dd40_texture_core::BlockAtlas>,
    #[cfg(feature = "textures")] images_ro: Res<Assets<Image>>,
) {
    #[cfg(feature = "textures")]
    let needs_rebuild = blocks.is_changed() || atlas.is_changed() || assets.map.is_empty();
    #[cfg(not(feature = "textures"))]
    let needs_rebuild = blocks.is_changed() || assets.map.is_empty();

    if !needs_rebuild {
        return;
    }

    assets.rebuild(
        &blocks,
        &mut images,
        #[cfg(feature = "textures")]
        &atlas,
        #[cfg(feature = "textures")]
        &images_ro,
    );
    debug!(
        "prerender_block_icons: built {} block icons",
        assets.map.len()
    );
    cache.clear();
}

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
        block_icons: &BlockIconAssets,
    ) -> ItemIcon {
        if let Some(existing) = self.map.get(&item) {
            return existing.clone();
        }
        let resolved = resolve_icon(item, items, blocks, asset_server, block_icons);
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
/// Pure resolution of [`ItemIcon`] for a single item id.
///
/// Resolution order:
///
/// 1. If the item is **placeable** and a procedural block icon has been
///    pre-rendered, use that.  Placeable items always render as their
///    block's three-face isometric cube — the same visual the player
///    will see in-world.
/// 2. Otherwise, if the item declares an `icon_path`, load that PNG.
/// 3. Otherwise, fall back to the magenta missing-icon swatch.
///
/// Public so tests can exercise the fallback chain without spinning up
/// an `App`.  Production code should prefer
/// [`ItemIconCache::get_or_resolve`].
pub fn resolve_icon(
    item: ItemId,
    items: &ItemRegistry,
    blocks: &BlockRegistry,
    asset_server: &AssetServer,
    block_icons: &BlockIconAssets,
) -> ItemIcon {
    let def = items.get(item);
    if let Some(handle) = def
        .and_then(|d| d.placeable)
        .and_then(|b| block_icons.get(b))
    {
        return ItemIcon::Image(handle);
    }
    if let Some(path) = def.and_then(|d| d.icon_path.as_ref()) {
        return ItemIcon::Image(asset_server.load(path.clone()));
    }
    if def
        .and_then(|d| d.placeable)
        .and_then(|b| blocks.get(b))
        .map(BlockDefinition::clone_color)
        .is_some()
    {
        // Placeable block exists but its icon hasn't been pre-rendered
        // yet — fall back to magenta temporarily.  The next frame after
        // `prerender_block_icons` runs the cache is cleared and the
        // proper cube is picked up.
        return ItemIcon::Color(MISSING_ICON_COLOR);
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
