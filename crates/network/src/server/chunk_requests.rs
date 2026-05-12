use std::time::Duration;

use bevy::{platform::collections::HashSet, prelude::*};
use dd40_core::prelude::*;
use lightyear::prelude::{LinkOf, MessageReceiver, MessageSender, ReplicationSender, SendUpdatesMode};

use crate::protocol::{ChunkSnapshot, ChunkUpdate, PlayerSpawnLocation, RequestSpawn};

/// Tracks which chunk positions have already been requested for a given client
/// connection so that the chunk pipeline never issues duplicate loads.
#[derive(Component, Deref, DerefMut, Debug, Default, Reflect)]
pub(crate) struct ChunkRequests(HashSet<ChunkPos>);

/// Observer that fires when lightyear adds a [`LinkOf`] component to an entity,
/// signalling that a new client connection is ready.
///
/// Inserts:
/// - [`ReplicationSender`] — required for lightyear to send replicated entity
///   updates to this client.  Without it, no component replication reaches the
///   client even if entities carry [`Replicate`].
/// - [`MessageSender`] / [`MessageReceiver`] components for each message type
///   used by the chunk and spawn pipelines.
/// - [`ChunkRequests`] — deduplicates chunk load requests per connection.
///
/// [`Replicate`]: lightyear::prelude::Replicate
pub(crate) fn add_message_handlers(trigger: On<Add, LinkOf>, mut commands: Commands) {
    let entity = trigger.entity;
    debug!("New client connection on entity {:?}", entity);

    commands.entity(entity).insert((
        // Send entity-component updates every 100 ms, since the last ack.
        ReplicationSender::new(
            Duration::from_millis(100),
            SendUpdatesMode::SinceLastAck,
            false,
        ),
        MessageSender::<ChunkSnapshot>::default(),
        MessageSender::<ChunkUpdate>::default(),
        MessageSender::<PlayerSpawnLocation>::default(),
        MessageReceiver::<RequestSpawn>::default(),
        MessageReceiver::<RequestChunk>::default(),
        ChunkRequests::default(),
        Name::new("Client"),
    ));
}
