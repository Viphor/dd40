use bevy::{platform::collections::HashSet, prelude::*};
use dd40_core::prelude::*;
use lightyear::prelude::MessageReceiver;

#[derive(Component, Deref, DerefMut, Debug, Default, Reflect)]
pub(crate) struct ChunkRequests(HashSet<ChunkPos>);

pub(crate) fn add_chunk_requests_cache(
    query: Query<Entity, Added<MessageReceiver<RequestChunk>>>,
    mut commands: Commands,
) {
    for entity in query.iter() {
        commands.entity(entity).insert(ChunkRequests::default());
    }
}
