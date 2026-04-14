use bevy::{platform::collections::HashSet, prelude::*};
use dd40_core::prelude::*;
use lightyear::prelude::{LinkOf, MessageReceiver, MessageSender};

use crate::protocol::{PlaceBlockRequest, PlayerSpawnLocation, RequestSpawn};

/// Tracks which chunk positions have already been requested for a given client
/// connection so that the chunk pipeline never issues duplicate loads.
#[derive(Component, Deref, DerefMut, Debug, Default, Reflect)]
pub(crate) struct ChunkRequests(HashSet<ChunkPos>);

/// Observer that fires when lightyear adds a [`LinkOf`] component to an entity,
/// signalling that a new client connection is ready.
///
/// Attaches all required [`MessageSender`] and [`MessageReceiver`] components
/// to the connection entity so that the chunk and spawn pipelines can communicate
/// with the client, and inserts a [`NewClientMarker`] so that
/// [`send_spawn_location`](crate::server::spawn::send_spawn_location) processes
/// this connection exactly once on the next frame.
pub(crate) fn add_message_handlers(trigger: On<Add, LinkOf>, mut commands: Commands) {
    let entity = trigger.entity;
    debug!("New client connection on entity {:?}", entity);

    commands.entity(entity).insert((
        MessageSender::<ChunkReady>::default(),
        MessageSender::<BlockPlaced>::default(),
        MessageSender::<PlayerSpawnLocation>::default(),
        MessageReceiver::<RequestSpawn>::default(),
        MessageReceiver::<RequestChunk>::default(),
        MessageReceiver::<PlaceBlockRequest>::default(),
        ChunkRequests::default(),
        Name::new("Client"),
    ));
}
