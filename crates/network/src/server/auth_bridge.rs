use bevy::ecs::message::MessageWriter;
use bevy::prelude::*;
use dd40_identity_core::{AuthTokenReceived, AwaitingAuth};
use lightyear::prelude::MessageReceiver;

use crate::protocol::AuthToken;

/// Drains incoming [`AuthToken`] lightyear messages from connection entities
/// and re-emits them as local [`AuthTokenReceived`] Bevy messages.
///
/// This is the transport bridge: it decouples `dd40_identity` (which does JWT
/// verification) from the lightyear transport layer so neither crate needs to
/// depend on the other.
pub(crate) fn bridge_auth_tokens(
    mut connections: Query<(Entity, &mut MessageReceiver<AuthToken>), With<AwaitingAuth>>,
    mut writer: MessageWriter<AuthTokenReceived>,
) {
    for (entity, mut receiver) in &mut connections {
        for msg in receiver.receive() {
            writer.write(AuthTokenReceived {
                connection_entity: entity,
                token: msg.token.clone(),
            });
        }
    }
}
