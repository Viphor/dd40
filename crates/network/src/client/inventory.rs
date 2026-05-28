//! Client-side bridge that forwards local [`SlotInteraction`] messages
//! over the wire as [`NetSlotInteraction`].
//!
//! The inventory GUI keeps publishing [`SlotInteraction`] to the local
//! Bevy bus exactly as it always has.  This system listens for those
//! local messages and shovels them onto the lightyear
//! [`InventoryChannel`].  The server side
//! ([`ServerInventoryNetworkPlugin`]) then resolves the controlling
//! character and re-emits a `SlotInteraction` onto its own local bus
//! so the apply system can consume it.
//!
//! No prediction.  The client takes the 1-RTT latency; renderer paths
//! are wired off replicated state (`InventoryComponent`,
//! `HeldStackComponent`) and update naturally when the server reply
//! arrives.
//!
//! [`ServerInventoryNetworkPlugin`]: crate::ServerInventoryNetworkPlugin

use bevy::prelude::*;
use dd40_inventory_core::slot_interaction::SlotInteraction;
use lightyear::prelude::MessageSender;

use crate::protocol::{InventoryChannel, NetSlotInteraction};

/// Forwards every local [`SlotInteraction`] to the server.
///
/// The `character` field on the local message is ignored — the server
/// recovers the target character from the connection's `ControlledBy`.
pub fn forward_slot_interactions(
    mut reader: MessageReader<SlotInteraction>,
    sender: Option<Single<&mut MessageSender<NetSlotInteraction>>>,
) {
    let Some(mut sender) = sender else {
        // No active connection yet — drop messages silently.
        reader.read().for_each(|_| {});
        return;
    };
    for msg in reader.read() {
        sender.send::<InventoryChannel>(NetSlotInteraction {
            kind: msg.kind.clone(),
        });
    }
}

/// Plugin that adds [`forward_slot_interactions`] to `Update`.
///
/// Add this on the client alongside [`ClientNetworkPlugin`].  The GUI
/// crate continues to publish [`SlotInteraction`] on the local bus;
/// this system is what actually delivers the click to the server.
///
/// [`ClientNetworkPlugin`]: crate::client::plugin::ClientNetworkPlugin
#[derive(Default)]
pub struct ClientInventoryNetworkPlugin;

impl Plugin for ClientInventoryNetworkPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, forward_slot_interactions);
    }
}
