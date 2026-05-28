//! Translates bevy_ui `Interaction` events on slot widgets into
//! [`SlotInteraction`] messages.
//!
//! Mouse-button discrimination uses [`ButtonInput<MouseButton>`] since
//! bevy_ui's `Interaction` enum doesn't carry which button caused the
//! press.  Shift modifier is read from [`ButtonInput<KeyCode>`].
//!
//! Drop-outside detection is handled by [`translate_drop_outside`]:
//! whenever the user releases the left mouse button while the local
//! player is holding a stack and no slot widget is hovered, a
//! `SlotInteraction::DropOutside` is emitted.

use bevy::prelude::*;
use dd40_character_core::components::Player;
use dd40_inventory_core::prelude::{HeldStackComponent, SlotInteraction, SlotInteractionKind};

use crate::slot_widget::SlotKey;

/// Per-slot click → [`SlotInteraction`] translator.
///
/// Fires once per press transition (`Interaction::Pressed` newly set on
/// a widget).  Holding the button does not auto-repeat.
pub fn translate_clicks(
    slots: Query<(&Interaction, &SlotKey), Changed<Interaction>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut writer: MessageWriter<SlotInteraction>,
) {
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let left = mouse.just_pressed(MouseButton::Left);
    let right = mouse.just_pressed(MouseButton::Right);
    if !left && !right {
        return;
    }
    for (interaction, key) in &slots {
        if !matches!(interaction, Interaction::Pressed) {
            continue;
        }
        let kind = if left && shift {
            SlotInteractionKind::ShiftClick { slot: key.slot }
        } else if left {
            SlotInteractionKind::LeftClick { slot: key.slot }
        } else if right {
            SlotInteractionKind::RightClick { slot: key.slot }
        } else {
            continue;
        };
        writer.write(SlotInteraction {
            character: key.character,
            kind,
        });
    }
}

/// Emits `SlotInteraction::DropOutside` when the player releases the
/// left mouse button while holding a stack and no slot widget is hovered.
pub fn translate_drop_outside(
    mouse: Res<ButtonInput<MouseButton>>,
    player: Query<(Entity, &HeldStackComponent), With<Player>>,
    hovered: Query<&Interaction, With<SlotKey>>,
    mut writer: MessageWriter<SlotInteraction>,
) {
    if !mouse.just_released(MouseButton::Left) {
        return;
    }
    let Ok((player, held)) = player.single() else {
        return;
    };
    if held.is_empty() {
        return;
    }
    let any_hovered = hovered
        .iter()
        .any(|i| matches!(i, Interaction::Hovered | Interaction::Pressed));
    if any_hovered {
        return;
    }
    writer.write(SlotInteraction {
        character: player,
        kind: SlotInteractionKind::DropOutside,
    });
}
