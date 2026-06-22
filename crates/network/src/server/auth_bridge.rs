use bevy::ecs::message::MessageWriter;
use bevy::prelude::*;
use dd40_identity_core::{AuthTokenReceived, AwaitingAuth};
use lightyear::prelude::MessageReceiver;

use crate::protocol::AuthToken;

/// Drains incoming [`AuthToken`] lightyear messages from connection entities
/// and re-emits them as local [`AuthTokenReceived`] Bevy messages for
/// `dd40_identity` to verify.
pub(crate) fn bridge_auth_tokens(
    mut connections: Query<(Entity, &mut MessageReceiver<AuthToken>), With<AwaitingAuth>>,
    mut writer: MessageWriter<AuthTokenReceived>,
) {
    for (entity, mut receiver) in &mut connections {
        for msg in receiver.receive() {
            info!(?entity, "auth bridge: forwarding auth token to verifier");
            writer.write(AuthTokenReceived {
                connection_entity: entity,
                token: msg.token.clone(),
            });
        }
    }
}
