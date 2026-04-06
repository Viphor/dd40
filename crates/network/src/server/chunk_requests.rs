use bevy::{platform::collections::HashSet, prelude::*};
use dd40_core::prelude::*;
use lightyear::prelude::{LinkOf, MessageReceiver, MessageSender};

#[derive(Component, Deref, DerefMut, Debug, Default, Reflect)]
pub(crate) struct ChunkRequests(HashSet<ChunkPos>);

pub(crate) fn add_chunk_requests_cache(trigger: On<Add, LinkOf>, mut commands: Commands) {
    let entity = trigger.entity;
    debug!("Adding ChunkRequests cache to entity {:?}", entity);
    commands.entity(entity).insert((
        MessageSender::<ChunkReady>::default(),
        MessageReceiver::<RequestChunk>::default(),
        ChunkRequests::default(),
        Name::new("Client"),
    ));
}
