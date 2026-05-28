//! [`SlotInteraction`] — UI-to-rules message describing a player intent
//! to mutate an inventory slot.
//!
//! The seam: inventory GUI crates never mutate
//! [`Inventory`][crate::inventory::Inventory] directly.  Instead they
//! describe what the player tried to do and leave the resolution to
//! whichever inventory rules crate is wired in.
//!
//! Producers: GUI crates (e.g. `dd40_inventory_gui`).
//! Consumers: rules crates (e.g. `dd40_vanilla_inventory`).
//!
//! # Slot indexing
//!
//! `slot` is an index into the recipient's
//! [`InventoryComponent`][crate::component::InventoryComponent] — the
//! flat slot layout that both hotbar (`0..HOTBAR_SIZE`) and main
//! inventory (`HOTBAR_SIZE..`) share.  Out-of-range indices are a
//! protocol error and consumers will log + drop them.

use bevy::prelude::{Entity, Message};
use serde::{Deserialize, Serialize};

/// A player-initiated slot mutation request.
#[derive(Message, Clone, Debug)]
pub struct SlotInteraction {
    /// The character whose inventory the interaction targets.
    pub character: Entity,
    /// What the player tried to do.
    pub kind: SlotInteractionKind,
}

/// Concrete interaction variants.
///
/// Mirrors the Minecraft conventions the GUI v1 spec calls for:
/// left-click swap/pickup/drop, right-click split/place-one,
/// shift-click move-to-other-half, drag-out-of-window drop.
///
/// Derives `Serialize` + `Deserialize` so it can travel as the payload
/// of a network message (the `Entity` in [`SlotInteraction`] is
/// resolved server-side from lightyear's `ControlledBy`, so it does
/// not need to be serialized).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotInteractionKind {
    /// Primary click on a slot: pick up the stack, drop the held
    /// stack, or swap.
    LeftClick {
        /// Index into [`InventoryComponent`][crate::component::InventoryComponent].
        slot: u8,
    },
    /// Secondary click on a slot: pick up half the stack (rounded up)
    /// when not holding; otherwise place one item from the cursor.
    RightClick {
        /// Index into [`InventoryComponent`][crate::component::InventoryComponent].
        slot: u8,
    },
    /// Shift + primary click on a slot: move the full stack between
    /// hotbar and main areas, into the first compatible target slot
    /// on the opposite side.
    ShiftClick {
        /// Index into [`InventoryComponent`][crate::component::InventoryComponent].
        slot: u8,
    },
    /// Player released the cursor outside any slot widget while
    /// holding a stack.  The rules crate consumes the held stack and
    /// emits [`DropItems`][crate::drop::DropItems].
    DropOutside,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use bevy::prelude::*;

    #[test]
    fn writer_reader_round_trip() {
        let mut app = App::new();
        app.add_message::<SlotInteraction>();

        #[derive(Resource, Default)]
        struct Captured(Vec<SlotInteractionKind>);
        app.init_resource::<Captured>();

        app.add_systems(
            Update,
            (
                |mut w: MessageWriter<SlotInteraction>| {
                    w.write(SlotInteraction {
                        character: Entity::from_raw_u32(7).unwrap(),
                        kind: SlotInteractionKind::LeftClick { slot: 3 },
                    });
                },
                |mut r: MessageReader<SlotInteraction>, mut c: ResMut<Captured>| {
                    for ev in r.read() {
                        c.0.push(ev.kind.clone());
                    }
                },
            )
                .chain(),
        );

        app.update();
        let captured = &app.world().resource::<Captured>().0;
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0], SlotInteractionKind::LeftClick { slot: 3 });
    }

    #[test]
    fn message_resource_exists_after_registration() {
        let mut app = App::new();
        app.add_message::<SlotInteraction>();
        assert!(
            app.world()
                .get_resource::<Messages<SlotInteraction>>()
                .is_some()
        );
    }
}
