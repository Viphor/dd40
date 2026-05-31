//! [`SlotInteraction`] — UI-to-rules message describing a player intent
//! to mutate an inventory slot.
//!
//! The seam: inventory GUI crates never mutate
//! [`Inventory`][crate::inventory::Inventory] directly.  Instead they
//! describe what the player tried to do and leave the resolution to
//! whichever inventory rules crate is wired in.
//!
//! Producers: GUI crates (e.g. `dd40_inventory_gui`).
//! Consumers: rules crates (e.g. `dd40_inventory`).
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
/// Named for **player intent**, not for any specific input device or
/// cursor state.  A keyboard/mouse GUI inspects the local
/// [`HeldStackComponent`][crate::held_stack::HeldStackComponent] and
/// translates each click into the right intent variant before sending;
/// a gamepad, touch, or accessibility binding maps its own gestures
/// onto the same intents.  The rules layer never branches on input
/// device.
///
/// Each variant is also strictly typed in *what state it operates on*:
/// `Take*` always reads from `slot` into the cursor and is a no-op
/// when the cursor is already non-empty; `Place*` always writes from
/// the cursor into `slot` and is a no-op when the cursor is empty.
/// This is intentional — sending the wrong intent for the current
/// cursor state must not silently flip the operation around, because
/// the server may have a different view of the cursor than the
/// client.  When in doubt the server treats it as a no-op.
///
/// Derives `Serialize` + `Deserialize` so it can travel as the payload
/// of a network message (the `Entity` in [`SlotInteraction`] is
/// resolved server-side from lightyear's `ControlledBy`, so it does
/// not need to be serialized).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotInteractionKind {
    /// Pick up the full stack at `slot` into the cursor.  No-op if
    /// the cursor is already non-empty or the slot is empty.
    TakeAll {
        /// Index into [`InventoryComponent`][crate::component::InventoryComponent].
        slot: u8,
    },
    /// Deposit the held stack into `slot`.  Merges into a matching
    /// stack up to `max_stack` (leftover stays in the cursor), swaps
    /// when the items differ, or simply fills an empty slot.  No-op
    /// when the cursor is empty.
    PlaceAll {
        /// Index into [`InventoryComponent`][crate::component::InventoryComponent].
        slot: u8,
    },
    /// Pick up ceil(slot.count / 2) into the cursor.  No-op if the
    /// cursor is already non-empty or the slot is empty.
    TakeHalf {
        /// Index into [`InventoryComponent`][crate::component::InventoryComponent].
        slot: u8,
    },
    /// Deposit a single item from the cursor into `slot`.  Increments
    /// a matching stack (up to `max_stack`), swaps when the items
    /// differ, or starts a new stack in an empty slot.  No-op when
    /// the cursor is empty.
    PlaceOne {
        /// Index into [`InventoryComponent`][crate::component::InventoryComponent].
        slot: u8,
    },
    /// Move the full stack at `slot` to the opposite inventory
    /// section (hotbar ↔ main), merging into matching stacks first
    /// and then filling the first empty slot.
    QuickTransfer {
        /// Index into [`InventoryComponent`][crate::component::InventoryComponent].
        slot: u8,
    },
    /// Drop whatever the player is currently holding into the world.
    /// The rules crate consumes the held stack and emits
    /// [`DropItems`][crate::drop::DropItems].
    DropHeld,
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
                        kind: SlotInteractionKind::TakeAll { slot: 3 },
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
        assert_eq!(captured[0], SlotInteractionKind::TakeAll { slot: 3 });
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
