//! The toggleable inventory grid window.
//!
//! Toggle is driven by the `ToggleInventory` BEI action.  When
//! [`InventoryGuiOpen`] flips to `true`, [`toggle_grid`]
//! spawns a centred 3×9 grid of slot widgets for the local player's
//! inventory slots `9..36` and registers a [`UiWindow`] with
//! [`OpenUiWindows`] so the cursor is released.  When closed, the grid
//! root is despawned and the window is unregistered.
//!
//! v1 does not repeat the hotbar row inside the grid window — the
//! always-visible hotbar continues to serve that role.

use bevy::prelude::*;
use bevy_enhanced_input::prelude::{Action, ActionEvents};
use dd40_character_core::components::Player;
use dd40_core::ui_windows::{OpenUiWindows, UiWindow, UiWindowId};
use dd40_input_core::actions::ToggleInventory;
use dd40_inventory_core::prelude::InventoryComponent;

use crate::plugin::InventoryGuiOpen;
use crate::slot_widget::{GRID_SLOT_SIZE, SlotKey, spawn_slot_widget};

const GRID_COLS: u8 = 9;
const GRID_ROWS: u8 = 3;

/// Marker for the grid root node.
#[derive(Component)]
pub struct InventoryGridRoot;

/// Relationship pointing back at the player whose inventory is shown.
#[derive(Component, Debug, Clone, Copy)]
pub struct GridFor(pub Entity);

/// Window-id token used in [`OpenUiWindows`] when the grid is open.
#[derive(Default)]
struct InventoryWindow;

/// Reads `ToggleInventory` press events and flips [`InventoryGuiOpen`].
///
/// When the flag flips, this system also spawns or despawns the grid
/// root and updates [`OpenUiWindows`] so the cursor release reconciler
/// in `dd40_player_input` releases the mouse.
pub fn toggle_grid(
    mut commands: Commands,
    actions: Query<(&Action<ToggleInventory>, &ActionEvents)>,
    mut open: ResMut<InventoryGuiOpen>,
    mut windows: ResMut<OpenUiWindows>,
    players: Query<Entity, With<Player>>,
    existing: Query<Entity, With<InventoryGridRoot>>,
    inventories: Query<&InventoryComponent>,
) {
    let mut pressed = false;
    for (_, events) in &actions {
        if events.contains(ActionEvents::START) {
            pressed = true;
            break;
        }
    }
    if !pressed {
        return;
    }
    open.0 = !open.0;
    let id = UiWindowId::of::<InventoryWindow>();
    if open.0 {
        windows.insert(id, UiWindow::cursor_released());
        let Ok(player) = players.single() else {
            warn!("ToggleInventory pressed but no Player entity found");
            open.0 = false;
            windows.remove(id);
            return;
        };
        let capacity = inventories
            .get(player)
            .map(|i| i.inventory().capacity())
            .unwrap_or(0);
        spawn_grid(&mut commands, player, capacity);
    } else {
        windows.remove(id);
        for grid in &existing {
            commands.entity(grid).despawn();
        }
    }
}

fn spawn_grid(commands: &mut Commands, player: Entity, capacity: usize) {
    info!(
        "spawn_grid: player={:?} capacity={} (expecting {}+ for {}x{} grid)",
        player,
        capacity,
        dd40_inventory_core::prelude::HOTBAR_SIZE as usize + GRID_ROWS as usize * GRID_COLS as usize,
        GRID_COLS,
        GRID_ROWS,
    );
    commands
        .spawn((
            Name::new("InventoryGridOverlay"),
            InventoryGridRoot,
            GridFor(player),
            // Full-screen overlay so the grid can be centred with
            // flexbox alignment instead of brittle hardcoded margins.
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|overlay| {
            // Explicit panel size: 9 cols * (slot + gap) + 2*padding,
            // 3 rows similarly.  Sizing the panel by content was
            // collapsing it to ~zero in bevy_ui 0.18 because the slot
            // children are absolutely-positioned-bearing flex items in
            // a Center-aligned overlay; making the panel intrinsically
            // sized avoids the collapse entirely.
            let panel_pad = 12.0_f32;
            let cell = GRID_SLOT_SIZE + crate::slot_widget::SLOT_GAP;
            let panel_w = cell * GRID_COLS as f32 + panel_pad * 2.0;
            let panel_h = cell * GRID_ROWS as f32 + panel_pad * 2.0;
            overlay
                .spawn((
                    Name::new("InventoryGridPanel"),
                    Node {
                        width: Val::Px(panel_w),
                        height: Val::Px(panel_h),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(panel_pad)),
                        flex_shrink: 0.0,
                        flex_grow: 0.0,
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
                ))
                .with_children(|panel| {
                    let start = dd40_inventory_core::prelude::HOTBAR_SIZE;
                    for row in 0..GRID_ROWS {
                        panel
                            .spawn((
                                Name::new("GridRow"),
                                Node {
                                    width: Val::Px(cell * GRID_COLS as f32),
                                    height: Val::Px(cell),
                                    flex_direction: FlexDirection::Row,
                                    flex_shrink: 0.0,
                                    flex_grow: 0.0,
                                    ..default()
                                },
                            ))
                            .with_children(|row_node| {
                                for col in 0..GRID_COLS {
                                    let slot = start + row * GRID_COLS + col;
                                    if (slot as usize) >= capacity {
                                        continue;
                                    }
                                    spawn_slot_widget(
                                        row_node,
                                        SlotKey {
                                            character: player,
                                            slot,
                                        },
                                        GRID_SLOT_SIZE,
                                    );
                                }
                            });
                    }
                });
        });
}

/// No-op placeholder kept so `plugin.rs` can schedule a deterministic
/// system slot for future "rebuild grid on inventory resize" work.
pub fn ensure_grid_widgets() {}
