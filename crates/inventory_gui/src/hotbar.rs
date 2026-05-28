//! The always-visible hotbar at the bottom of the screen.
//!
//! [`ensure_hotbar_root`] spawns one row of [`HOTBAR_SIZE`] slot widgets
//! the first time it finds a [`Player`] that doesn't already have a
//! hotbar root attached.  The root is despawned automatically when the
//! player is despawned, via [`HotbarFor`] relationship semantics.

use bevy::prelude::*;
use dd40_character_core::components::Player;
use dd40_inventory_core::prelude::{HOTBAR_SIZE, SelectedHotbarSlot};

use crate::slot_widget::{
    SelectedMarker, SlotKey, refresh_selected_marker, spawn_slot_widget,
};

/// Marker on the bottom-bar root node.
#[derive(Component)]
pub struct HotbarRoot;

/// Relationship pointing back at the player this hotbar belongs to.
#[derive(Component, Debug, Clone, Copy)]
pub struct HotbarFor(pub Entity);

/// Spawns the hotbar root for every newly-added [`Player`].
pub fn ensure_hotbar_root(
    mut commands: Commands,
    players: Query<Entity, Added<Player>>,
    existing: Query<&HotbarFor>,
) {
    let already: bevy::platform::collections::HashSet<Entity> =
        existing.iter().map(|h| h.0).collect();
    for player in &players {
        if already.contains(&player) {
            continue;
        }
        commands
            .spawn((
                Name::new("HotbarRoot"),
                HotbarRoot,
                HotbarFor(player),
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(16.0),
                    left: Val::Percent(50.0),
                    margin: UiRect {
                        left: Val::Px(-(HOTBAR_SIZE as f32) * 22.0),
                        ..default()
                    },
                    flex_direction: FlexDirection::Row,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.4)),
                Pickable::IGNORE,
            ))
            .with_children(|root| {
                for slot in 0..HOTBAR_SIZE {
                    spawn_slot_widget(
                        root,
                        SlotKey {
                            character: player,
                            slot,
                        },
                    );
                }
            });
    }
}

/// Adds / removes the [`SelectedMarker`] on each hotbar slot to match
/// the local player's [`SelectedHotbarSlot`].
pub fn sync_hotbar_selection(
    mut commands: Commands,
    selected: Query<
        (Entity, &SelectedHotbarSlot),
        (With<Player>, Changed<SelectedHotbarSlot>),
    >,
    slots: Query<(Entity, &SlotKey, Has<SelectedMarker>), With<SlotKey>>,
) {
    for (player, sel) in &selected {
        refresh_selected_marker(&mut commands, &slots, player, sel);
    }
}
