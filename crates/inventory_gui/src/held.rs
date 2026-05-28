//! Cursor-following node that renders the currently-held stack while
//! the inventory grid is open.
//!
//! The held stack is owned by [`HeldStack`] in `dd40_inventory_core`.
//! When it is non-empty, [`sync_held_cursor`] spawns (or shows) a small
//! ImageNode that tracks the mouse cursor.  When the stack is cleared
//! the node is hidden.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use dd40_core::block::BlockRegistry;
use dd40_inventory_core::prelude::HeldStack;
use dd40_item_core::registry::ItemRegistry;

use crate::icons::{ItemIcon, ItemIconCache};
use crate::slot_widget::SLOT_SIZE;

/// Marker on the floating cursor node.
#[derive(Component)]
pub struct HeldCursorNode;

/// Spawns the cursor node on first run, then each frame moves it to the
/// cursor position and updates its icon to reflect [`HeldStack`].
pub fn sync_held_cursor(
    mut commands: Commands,
    held: Res<HeldStack>,
    windows: Query<&Window, With<PrimaryWindow>>,
    items: Res<ItemRegistry>,
    blocks: Res<BlockRegistry>,
    asset_server: Res<AssetServer>,
    mut cache: ResMut<ItemIconCache>,
    mut node: Query<(Entity, &mut Node, &mut BackgroundColor), With<HeldCursorNode>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        // Cursor outside the window — hide the node.
        for (entity, _, _) in &mut node {
            commands.entity(entity).insert(Visibility::Hidden);
        }
        return;
    };

    if held.is_empty() {
        for (entity, _, _) in &mut node {
            commands.entity(entity).insert(Visibility::Hidden);
        }
        return;
    }
    let stack = held.0.unwrap();

    let icon = cache.get_or_resolve(stack.item, &items, &blocks, &asset_server);

    if node.is_empty() {
        commands.spawn((
            Name::new("HeldCursor"),
            HeldCursorNode,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(SLOT_SIZE - 8.0),
                height: Val::Px(SLOT_SIZE - 8.0),
                left: Val::Px(cursor.x),
                top: Val::Px(cursor.y),
                ..default()
            },
            BackgroundColor(Color::NONE),
            Visibility::Visible,
            Pickable::IGNORE,
        ));
        return;
    }

    for (entity, mut n, mut bg) in &mut node {
        n.left = Val::Px(cursor.x);
        n.top = Val::Px(cursor.y);
        commands.entity(entity).insert(Visibility::Visible);
        match icon.clone() {
            ItemIcon::Image(handle) => {
                *bg = BackgroundColor(Color::WHITE);
                commands.entity(entity).insert(ImageNode::new(handle));
            }
            ItemIcon::Color(color) => {
                *bg = BackgroundColor(color);
                commands.entity(entity).remove::<ImageNode>();
            }
        }
    }
}
