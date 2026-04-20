use bevy::prelude::*;
use dd40_core::prelude::*;
use lightyear::prelude::{MessageReceiver, MessageSender};

use crate::{protocol::ChunkChannel, server::chunk_requests::ChunkRequests};

pub(crate) fn receive_chunk_requests(
    chunk_cache: Res<ChunkCache>,
    mut requests: MessageWriter<RequestChunk>,
    mut receivers: Query<(
        &mut MessageReceiver<RequestChunk>,
        &mut MessageSender<ChunkReady>,
        &mut ChunkRequests,
    )>,
) {
    for (mut receiver, mut sender, mut cache) in receivers.iter_mut() {
        if receiver.has_messages() {
            trace!("Receiving chunk requests from client");
        }
        for request in receiver.receive() {
            trace!("Received chunk request at {}", request.pos);
            if let Some(chunk) = chunk_cache.get(&request.pos) {
                trace!(
                    "Chunk at {} is already cached, sending immediately",
                    request.pos
                );
                sender.send::<ChunkChannel>(ChunkReady {
                    chunk: chunk.clone(),
                });
                continue;
            }
            cache.insert(request.pos);
            requests.write(request);
        }
    }
}

pub(crate) fn send_chunk_data(
    mut reader: MessageReader<ChunkReady>,
    mut senders: Query<(&mut MessageSender<ChunkReady>, &mut ChunkRequests)>,
) {
    let messages = reader.read().collect::<Vec<_>>();
    senders.par_iter_mut().for_each(|(mut sender, mut cache)| {
        for ready in messages.iter() {
            if cache.contains(&ready.chunk.position()) {
                trace!("Sending chunk data at {}", ready.chunk.position());
                sender.send::<ChunkChannel>((*ready).clone());
                cache.remove(&ready.chunk.position());
            }
        }
    });
}
