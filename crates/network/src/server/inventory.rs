//! Server-side bridge that turns wire-form [`NetSlotInteraction`]
//! messages into local [`SlotInteraction`] messages so the
//! `dd40_inventory` apply system can consume them unchanged.
//!
//! The wire form deliberately omits the target `Entity` (it would be
//! the client's local id, meaningless on the server).  Instead the
//! controlling `Character` is resolved here, using lightyear's
//! [`ControlledBy`] component which the server attaches to every
//! character at spawn (see
//! [`CharacterServerNetworkExt::with_server_replication`]).
//!
//! [`CharacterServerNetworkExt::with_server_replication`]: crate::character_ext::CharacterServerNetworkExt::with_server_replication

use bevy::prelude::*;
use dd40_character_core::components::Character;
use dd40_inventory_core::set_active_slot::SetActiveSlot;
use dd40_inventory_core::slot_interaction::SlotInteraction;
use lightyear::prelude::{ControlledBy, MessageReceiver};

use crate::protocol::{NetSetActiveSlot, NetSlotInteraction};

/// Drains [`NetSlotInteraction`] from every connection, resolves the
/// owning [`Character`] via [`ControlledBy`], and re-emits a local
/// [`SlotInteraction`] for the apply system to consume.
///
/// If a connection has no associated character (e.g. the player has
/// not finished the spawn handshake yet) the message is dropped with
/// a `warn!` — clients should not be sending slot interactions before
/// they have a character.
pub fn forward_slot_interactions(
    mut connections: Query<(Entity, &mut MessageReceiver<NetSlotInteraction>)>,
    characters: Query<(Entity, &ControlledBy), With<Character>>,
    mut writer: MessageWriter<SlotInteraction>,
) {
    for (conn, mut receiver) in connections.iter_mut() {
        let messages: Vec<NetSlotInteraction> = receiver.receive().collect();
        if messages.is_empty() {
            continue;
        }
        let Some((character, _)) = characters.iter().find(|(_, cb)| cb.owner == conn) else {
            warn!(
                "Dropping {} NetSlotInteraction(s) from connection {:?} — no controlled Character yet",
                messages.len(),
                conn
            );
            continue;
        };
        for msg in messages {
            writer.write(SlotInteraction {
                character,
                kind: msg.kind,
            });
        }
    }
}

/// Drains [`NetSetActiveSlot`] from every connection, resolves the
/// owning [`Character`] via [`ControlledBy`], and re-emits a local
/// [`SetActiveSlot`] for the authoritative apply system to consume.
///
/// Same ownership-resolution contract as [`forward_slot_interactions`].
pub fn forward_set_active_slot(
    mut connections: Query<(Entity, &mut MessageReceiver<NetSetActiveSlot>)>,
    characters: Query<(Entity, &ControlledBy), With<Character>>,
    mut writer: MessageWriter<SetActiveSlot>,
) {
    for (conn, mut receiver) in connections.iter_mut() {
        let messages: Vec<NetSetActiveSlot> = receiver.receive().collect();
        if messages.is_empty() {
            continue;
        }
        let Some((character, _)) = characters.iter().find(|(_, cb)| cb.owner == conn) else {
            warn!(
                "Dropping {} NetSetActiveSlot(s) from connection {:?} — no controlled Character yet",
                messages.len(),
                conn
            );
            continue;
        };
        for msg in messages {
            writer.write(SetActiveSlot {
                character,
                slot: msg.slot,
            });
        }
    }
}

/// Plugin that adds [`forward_slot_interactions`] and
/// [`forward_set_active_slot`] to `Update`.
///
/// Add this on the server *after* `InventoryRulesPlugin` (from
/// `dd40_inventory`); that plugin owns the [`SlotInteraction`]
/// / [`SetActiveSlot`] message registrations and the apply systems.
#[derive(Default)]
pub struct ServerInventoryNetworkPlugin;

impl Plugin for ServerInventoryNetworkPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (forward_slot_interactions, forward_set_active_slot));
    }
}
