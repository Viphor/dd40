//! Hover tooltip that names the item under the cursor in the hotbar or
//! inventory grid.
//!
//! [`sync_tooltip`] scans every [`SlotKey`] widget for the one currently
//! hovered.  If that slot holds a stack, the tooltip is positioned near
//! the cursor and its text is set to the item's display name (and stack
//! count when greater than one).  When no slot is hovered, or the
//! hovered slot is empty, the tooltip is hidden.
//!
//! The tooltip itself is a single singleton entity spawned lazily on
//! first hover.  It uses `Pickable::IGNORE` so it never blocks the
//! click translator from reading slot interactions underneath it.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use dd40_inventory_core::prelude::InventoryComponent;
use dd40_item_core::registry::ItemRegistry;

use crate::slot_widget::SlotKey;

/// Marker on the singleton tooltip node.
#[derive(Component)]
pub struct TooltipNode;

/// Horizontal pixel offset from the cursor to the tooltip's top-left.
const CURSOR_OFFSET_X: f32 = 14.0;
/// Vertical pixel offset from the cursor to the tooltip's top-left.
const CURSOR_OFFSET_Y: f32 = 18.0;

/// Reads slot widget [`Interaction`] state, finds the hovered slot, and
/// updates the tooltip's text and position to describe its item.
///
/// Runs in `Update` alongside the rest of the inventory GUI systems.
pub fn sync_tooltip(
    mut commands: Commands,
    slots: Query<(&Interaction, &SlotKey)>,
    inventories: Query<&InventoryComponent>,
    items: Res<ItemRegistry>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut tooltip: Query<
        (Entity, &mut Node, &mut Visibility, &Children),
        With<TooltipNode>,
    >,
    mut texts: Query<&mut Text>,
) {
    let Ok(window) = windows.single() else {
        return;
    };

    // Find the first hovered slot, if any.
    let mut hovered: Option<&SlotKey> = None;
    for (interaction, key) in &slots {
        if matches!(interaction, Interaction::Hovered | Interaction::Pressed) {
            hovered = Some(key);
            break;
        }
    }

    let description = hovered.and_then(|key| {
        let stack = inventories
            .get(key.character)
            .ok()?
            .inventory()
            .slot(key.slot as usize)
            .copied()?;
        let name = items
            .get(stack.item)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| format!("item#{}", stack.item.0));
        Some(if stack.count.get() > 1 {
            format!("{name} ×{}", stack.count.get())
        } else {
            name
        })
    });

    let Some(cursor) = window.cursor_position() else {
        // Cursor gone → hide.
        for (_, _, mut vis, _) in &mut tooltip {
            *vis = Visibility::Hidden;
        }
        return;
    };

    match description {
        None => {
            for (_, _, mut vis, _) in &mut tooltip {
                *vis = Visibility::Hidden;
            }
        }
        Some(text) => {
            if let Some((_, mut node, mut vis, children)) = tooltip.iter_mut().next() {
                node.left = Val::Px(cursor.x + CURSOR_OFFSET_X);
                node.top = Val::Px(cursor.y + CURSOR_OFFSET_Y);
                *vis = Visibility::Visible;
                for child in children.iter() {
                    if let Ok(mut t) = texts.get_mut(child) {
                        if t.0 != text {
                            t.0 = text.clone();
                        }
                    }
                }
            } else {
                commands
                    .spawn((
                        Name::new("InventoryTooltip"),
                        TooltipNode,
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(cursor.x + CURSOR_OFFSET_X),
                            top: Val::Px(cursor.y + CURSOR_OFFSET_Y),
                            padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.85)),
                        BorderColor::all(Color::srgba(0.6, 0.6, 0.65, 0.9)),
                        Visibility::Visible,
                        Pickable::IGNORE,
                        ZIndex(1000),
                    ))
                    .with_children(|t| {
                        t.spawn((
                            Text::new(text),
                            TextFont {
                                font_size: 14.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                            Pickable::IGNORE,
                        ));
                    });
            }
        }
    }
}
