//! Single-slot Bevy UI widget shared by the hotbar, grid, and held-cursor
//! renderers.
//!
//! Each slot is a fixed-size square with:
//!
//! - A border that subtly darkens when not selected and brightens when
//!   the [`SelectedHotbarSlot`] marker is set.
//! - A child node showing either the item icon (image) or a flat colour
//!   swatch (see [`crate::icons`]).
//! - A child [`Text`] showing the stack count when greater than one.
//!
//! Spawned via [`spawn_slot_widget`].  Each widget carries a
//! [`SlotKey`] component identifying which inventory slot it presents
//! and (for hotbar slots only) a [`SelectedMarker`] indicating
//! highlight state.

use bevy::prelude::*;
use dd40_inventory_core::prelude::{InventoryComponent, SelectedHotbarSlot};
use dd40_item_core::active_item::ItemStack;
use dd40_item_core::registry::{ItemId, ItemRegistry};

use crate::icons::{ItemIcon, ItemIconCache};

/// Default slot edge length in logical pixels (hotbar size).
pub const SLOT_SIZE: f32 = 40.0;
/// Slot edge length used inside the toggleable inventory grid panel.
pub const GRID_SLOT_SIZE: f32 = 96.0;
/// Pixel gap between adjacent slot widgets.
pub const SLOT_GAP: f32 = 4.0;

/// Border colour for an unselected slot.
const BORDER_IDLE: Color = Color::srgba(0.55, 0.55, 0.6, 0.9);
/// Border colour for the currently-selected hotbar slot.
const BORDER_SELECTED: Color = Color::srgba(1.0, 1.0, 1.0, 0.9);
/// Background of the slot itself (visible when the item has no icon and
/// the item-icon swatch is rendered behind a partly-transparent area).
const SLOT_BACKGROUND: Color = Color::srgba(0.25, 0.25, 0.28, 0.95);

/// Identifies which inventory slot a widget presents.
///
/// `character` is the owning [`Entity`] (the
/// `InventoryComponent` holder).  `slot` is the index into that
/// inventory.
#[derive(Component, Debug, Clone, Copy)]
pub struct SlotKey {
    /// Inventory holder this widget points at.
    pub character: Entity,
    /// Index into the holder's inventory.
    pub slot: u8,
}

/// Marker component placed on the widget that should render with the
/// "selected hotbar slot" border highlight.
#[derive(Component, Debug, Default)]
pub struct SelectedMarker;

/// Marker placed on the child node that renders the item icon, so the
/// update system can find and rewrite it without rebuilding the whole
/// widget hierarchy.
#[derive(Component)]
pub struct SlotIconNode;

/// Marker placed on the child text that shows the stack count.
#[derive(Component)]
pub struct SlotCountNode;

/// Spawns a slot widget under `parent` and returns its entity.
///
/// `size` is the edge length in logical pixels — pass [`SLOT_SIZE`] for
/// the hotbar or [`GRID_SLOT_SIZE`] for the inventory grid.
///
/// The widget is initially empty (no icon, no count).  The companion
/// [`sync_slot_widgets`] system keeps it in sync with the underlying
/// inventory each frame.
pub fn spawn_slot_widget(
    parent: &mut ChildSpawnerCommands,
    key: SlotKey,
    size: f32,
) -> Entity {
    let count_font_size = (size * 0.28).max(12.0);
    parent
        .spawn((
            Name::new("InventorySlot"),
            key,
            Node {
                width: Val::Px(size),
                height: Val::Px(size),
                margin: UiRect::all(Val::Px(SLOT_GAP / 2.0)),
                border: UiRect::all(Val::Px(2.0)),
                // Provide a positioned containing block for the
                // absolutely-positioned icon + count children, and
                // explicitly opt out of the parent row's flexbox so
                // sibling slots cannot collapse onto the same cell.
                position_type: PositionType::Relative,
                flex_shrink: 0.0,
                flex_grow: 0.0,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::FlexEnd,
                ..default()
            },
            BorderColor::all(BORDER_IDLE),
            BackgroundColor(SLOT_BACKGROUND),
        ))
        .with_children(|slot| {
            // Icon node: fills the slot, drawn behind the count text.
            // Anchors must be explicit — an absolute child with only
            // width/height (and no left/top/right/bottom) collapses to
            // the parent's content-box origin in bevy_ui.
            slot.spawn((
                Name::new("SlotIcon"),
                SlotIconNode,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(2.0),
                    top: Val::Px(2.0),
                    right: Val::Px(2.0),
                    bottom: Val::Px(2.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
                Pickable::IGNORE,
            ));
            // Count text: bottom-right corner, hidden when count <= 1.
            slot.spawn((
                Name::new("SlotCount"),
                SlotCountNode,
                Text::new(""),
                TextFont {
                    font_size: count_font_size,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(2.0),
                    bottom: Val::Px(0.0),
                    ..default()
                },
                Pickable::IGNORE,
            ));
        })
        .id()
}

/// Updates `icon` and `count` children of a slot widget to match
/// `stack`.  Pass `None` to render an empty slot.
pub fn update_slot_widget_children(
    icon_bg: &mut BackgroundColor,
    icon_node: &mut ImageNode,
    icon_node_marker: &mut Node,
    count_text: &mut Text,
    stack: Option<ItemStack>,
    items: &ItemRegistry,
    blocks: &dd40_core::block::BlockRegistry,
    cache: &mut ItemIconCache,
    asset_server: &AssetServer,
) {
    let _ = (icon_node, icon_node_marker, blocks, items, cache, asset_server);
    match stack {
        None => {
            *icon_bg = BackgroundColor(Color::NONE);
            count_text.0.clear();
        }
        Some(stack) => {
            // Icon rendering is folded into the per-widget icon system
            // because choosing between Image and Color requires
            // conditionally inserting/removing an ImageNode component,
            // which is awkward in a single helper.  The colour path is
            // applied here as a fast default; the system below upgrades
            // to an image when applicable.
            *icon_bg = BackgroundColor(Color::srgb(0.5, 0.5, 0.5));
            count_text.0 = if stack.count.get() > 1 {
                stack.count.get().to_string()
            } else {
                String::new()
            };
        }
    }
    let _ = stack;
}

/// Per-frame system that rewrites each slot widget's icon node and
/// count text from the underlying inventory.
///
/// The system queries every entity carrying a [`SlotKey`] (hotbar and
/// grid slots both use it) and resolves the icon through
/// [`ItemIconCache`].  Selected-slot highlighting is handled by a
/// separate observer in `hotbar.rs`.
pub fn sync_slot_widgets(
    mut slots: Query<(&SlotKey, &Children)>,
    mut icons: Query<
        (&mut BackgroundColor, Option<&mut ImageNode>),
        (With<SlotIconNode>, Without<SlotCountNode>),
    >,
    mut counts: Query<&mut Text, With<SlotCountNode>>,
    inventories: Query<&InventoryComponent>,
    items: Res<ItemRegistry>,
    blocks: Res<dd40_core::block::BlockRegistry>,
    asset_server: Res<AssetServer>,
    mut cache: ResMut<ItemIconCache>,
    mut commands: Commands,
) {
    for (key, children) in &mut slots {
        let stack = inventories
            .get(key.character)
            .ok()
            .and_then(|inv| inv.inventory().slot(key.slot as usize).copied());
        for child in children.iter() {
            if let Ok((mut bg, image_node)) = icons.get_mut(child) {
                match stack {
                    None => {
                        *bg = BackgroundColor(Color::NONE);
                        if image_node.is_some() {
                            commands.entity(child).remove::<ImageNode>();
                        }
                    }
                    Some(stack) => {
                        let icon = cache.get_or_resolve(stack.item, &items, &blocks, &asset_server);
                        match icon {
                            ItemIcon::Image(handle) => {
                                *bg = BackgroundColor(Color::WHITE);
                                commands.entity(child).insert(ImageNode::new(handle));
                            }
                            ItemIcon::Color(color) => {
                                *bg = BackgroundColor(color);
                                if image_node.is_some() {
                                    commands.entity(child).remove::<ImageNode>();
                                }
                            }
                        }
                    }
                }
            }
            if let Ok(mut text) = counts.get_mut(child) {
                text.0 = match stack {
                    Some(s) if s.count.get() > 1 => s.count.get().to_string(),
                    _ => String::new(),
                };
            }
        }
    }
}

/// System that toggles each slot widget's border colour based on the
/// presence of the [`SelectedMarker`] component.
pub fn sync_selection_border(mut slots: Query<(&mut BorderColor, Has<SelectedMarker>)>) {
    for (mut border, selected) in &mut slots {
        let target = if selected { BORDER_SELECTED } else { BORDER_IDLE };
        *border = BorderColor::all(target);
    }
}

/// Helper used by the hotbar to recompute which slot is selected.
pub fn refresh_selected_marker(
    commands: &mut Commands,
    slots: &Query<(Entity, &SlotKey, Has<SelectedMarker>), With<SlotKey>>,
    character: Entity,
    selected: &SelectedHotbarSlot,
) {
    let want = selected.0;
    for (entity, key, has_marker) in slots.iter() {
        if key.character != character {
            continue;
        }
        let should = key.slot == want;
        if should && !has_marker {
            commands.entity(entity).insert(SelectedMarker);
        } else if !should && has_marker {
            commands.entity(entity).remove::<SelectedMarker>();
        }
    }
}

/// Looks up the [`ItemId`] currently in `(character, slot)`, if any.
/// Used by the held-cursor and click translators.
pub fn slot_item(
    character: Entity,
    slot: u8,
    inventories: &Query<&InventoryComponent>,
) -> Option<ItemId> {
    inventories
        .get(character)
        .ok()
        .and_then(|inv| inv.inventory().slot(slot as usize).map(|s| s.item))
}
