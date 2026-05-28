//! Cursor-following node that renders the currently-held stack while
//! the inventory grid is open.
//!
//! The held stack is owned by [`HeldStackComponent`] on the local
//! [`Player`] entity (replicated from the server).  When it is
//! non-empty, [`sync_held_cursor`] spawns (or shows) a small
//! ImageNode that tracks the mouse cursor.  When the stack is cleared
//! the node is hidden.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use dd40_character_core::components::Player;
use dd40_core::block::BlockRegistry;
use dd40_inventory_core::prelude::HeldStackComponent;
use dd40_item_core::registry::ItemRegistry;

use crate::icons::{ItemIcon, ItemIconCache};
use crate::slot_widget::SLOT_SIZE;

/// Marker on the floating cursor node.
#[derive(Component)]
pub struct HeldCursorNode;

/// Spawns the cursor node on first run, then each frame moves it to the
/// cursor position and updates its icon to reflect the local player's
/// [`HeldStackComponent`].
pub fn sync_held_cursor(
    mut commands: Commands,
    player_held: Query<&HeldStackComponent, With<Player>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    items: Res<ItemRegistry>,
    blocks: Res<BlockRegistry>,
    asset_server: Res<AssetServer>,
    block_icons: Res<crate::icons::BlockIconAssets>,
    mut cache: ResMut<ItemIconCache>,
    mut node: Query<(Entity, &mut Node, &mut BackgroundColor), With<HeldCursorNode>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        for (entity, _, _) in &mut node {
            commands.entity(entity).insert(Visibility::Hidden);
        }
        return;
    };

    let stack = match player_held.single() {
        Ok(held) if held.is_some() => held.0.unwrap(),
        _ => {
            for (entity, _, _) in &mut node {
                commands.entity(entity).insert(Visibility::Hidden);
            }
            return;
        }
    };

    let icon = cache.get_or_resolve(stack.item, &items, &blocks, &asset_server, &block_icons);

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
                *bg = BackgroundColor(Color::NONE);
                commands.entity(entity).insert(ImageNode {
                    image: handle,
                    image_mode: NodeImageMode::Stretch,
                    ..default()
                });
            }
            ItemIcon::Color(color) => {
                *bg = BackgroundColor(color);
                commands.entity(entity).remove::<ImageNode>();
            }
        }
    }
}
