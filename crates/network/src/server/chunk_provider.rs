use bevy::prelude::*;
use dd40_core::prelude::*;
use lightyear::prelude::{MessageReceiver, MessageSender};

use crate::{protocol::ChunkChannel, server::chunk_requests::ChunkRequests};

pub(crate) fn receive_chunk_requests(
    mut requests: MessageWriter<RequestChunk>,
    mut receivers: Query<(&mut MessageReceiver<RequestChunk>, &mut ChunkRequests)>,
) {
    for (mut receiver, mut cache) in receivers.iter_mut() {
        if receiver.has_messages() {
            trace!("Receiving chunk requests from client");
        }
        for request in receiver.receive() {
            trace!("Received chunk request at {}", request.pos);
            cache.insert(request.pos);
            requests.write(request);
        }
    }
}

pub(crate) fn send_chunk_data(
    mut reader: MessageReader<ChunkReady>,
    mut senders: Query<(&mut MessageSender<ChunkReady>, &mut ChunkRequests)>,
) {
    for ready in reader.read() {
        for (mut sender, mut cache) in senders.iter_mut() {
            if cache.contains(&ready.chunk.position()) {
                trace!("Sending chunk data at {}", ready.chunk.position());
                sender.send::<ChunkChannel>(ready.clone());
                cache.remove(&ready.chunk.position());
            }
        }
    }
}
