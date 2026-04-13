use bevy::{platform::collections::HashSet, prelude::*};
use dd40_core::prelude::*;
use lightyear::prelude::{LinkOf, MessageReceiver, MessageSender};

use crate::protocol::{BlockPlacedMessage, PlaceBlockRequest};

#[derive(Component, Deref, DerefMut, Debug, Default, Reflect)]
pub(crate) struct ChunkRequests(HashSet<ChunkPos>);

pub(crate) fn add_message_handlers(trigger: On<Add, LinkOf>, mut commands: Commands) {
    let entity = trigger.entity;
    debug!("Adding ChunkRequests cache to entity {:?}", entity);
    commands.entity(entity).insert((
        MessageSender::<ChunkReady>::default(),
        MessageSender::<BlockPlacedMessage>::default(),
        MessageReceiver::<RequestChunk>::default(),
        MessageReceiver::<PlaceBlockRequest>::default(),
        ChunkRequests::default(),
        Name::new("Client"),
    ));
}
