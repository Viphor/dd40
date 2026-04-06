use bevy::prelude::*;
use dd40_core::prelude::*;
use lightyear::prelude::{MessageReceiver, MessageSender};

use crate::protocol::ChunkChannel;

pub(crate) fn send_chunk_requests(
    mut requests: MessageReader<RequestChunk>,
    mut sender: Single<&mut MessageSender<RequestChunk>>,
) {
    for request in requests.read() {
        trace!("Requesting chunk at {}", request.pos);
        sender.send::<ChunkChannel>(request.clone());
    }
}

pub(crate) fn receive_chunk_data(
    mut ready: MessageWriter<ChunkReady>,
    mut receiver: Single<&mut MessageReceiver<ChunkReady>>,
) {
    for chunk in receiver.receive() {
        trace!("Received chunk at {}", chunk.chunk.position());
        ready.write(chunk);
    }
}
