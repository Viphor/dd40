use bevy::ecs::message::Message;
use bevy::prelude::Entity;

/// Local Bevy message emitted by the network bridge when a client sends an
/// [`AuthToken`][dd40_network::AuthToken] over the wire.
///
/// `dd40_identity` consumes this message to verify the JWT without depending
/// on the lightyear transport layer.
#[derive(Message, Clone, Debug)]
pub struct AuthTokenReceived {
    /// The lightyear connection entity that sent the token.
    pub connection_entity: Entity,
    /// Raw JWT string as sent by the client.
    pub token: String,
}
