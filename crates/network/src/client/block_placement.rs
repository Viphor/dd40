use bevy::prelude::*;
use dd40_core::{
    block::{Block, events::BlockPlaced},
    chunk::cache::ChunkCache,
    prelude::PlaceBlockRequest,
};
use lightyear::prelude::{MessageReceiver, MessageSender};

use crate::protocol::BlockChannel;

/// Client-side system that drains incoming [`BlockPlacedMessage`]s from the
/// server and applies each one to the local [`ChunkCache`].
///
/// For every message received the system:
/// 1. Looks up the target chunk in the cache.
/// 2. If the chunk is present, writes the new block into it.
/// 3. Writes a [`BlockPlaced`] Bevy message so that local rendering, audio,
///    and other reactive systems can respond without coupling to the network
///    layer.
///
/// # Ordering
///
/// This system is registered in [`PostUpdate`] so that the cache mutations are
/// visible to rendering systems that run later in the same frame.
///
/// # Missing chunks
///
/// If the target chunk is not yet loaded in the cache the block update is
/// silently dropped. The server is authoritative; the correct block data will
/// arrive when the chunk is eventually loaded.
pub(crate) fn receive_placed_blocks(
    mut receiver: Single<&mut MessageReceiver<BlockPlaced>>,
    mut cache: ResMut<ChunkCache>,
    mut placed: MessageWriter<BlockPlaced>,
) {
    for msg in receiver.receive() {
        trace!("Client: received BlockPlacedMessage at {}", msg.pos);

        let chunk_pos = msg.pos.chunk_pos();
        let local = msg.pos.chunk_local();

        if let Some(chunk) = cache.get_mut(&chunk_pos) {
            // Guard against negative Y before casting to usize.
            if local.y >= 0 {
                chunk.set(
                    local.x as usize,
                    local.y as usize,
                    local.z as usize,
                    Block::new(msg.block_id),
                );
            }
        }

        placed.write(BlockPlaced {
            pos: msg.pos,
            block_id: msg.block_id,
            placer: None,
        });
    }
}

pub(crate) fn send_place_requests(
    mut receiver: MessageReader<PlaceBlockRequest>,
    mut broadcasters: Query<&mut MessageSender<PlaceBlockRequest>>,
) {
    let messages = receiver.read().cloned().collect::<Vec<PlaceBlockRequest>>();
    for mut broadcaster in broadcasters.iter_mut() {
        for req in messages.iter() {
            trace!(
                "Client: sending PlaceBlockRequest for {} to server",
                req.pos
            );
            broadcaster.send::<BlockChannel>(req.clone());
        }
    }
}
