use bevy::prelude::*;
use dd40_core::{
    block::{Block, events::BlockPlaced},
    chunk::cache::ChunkCache,
    prelude::*,
};
use lightyear::prelude::{MessageReceiver, MessageSender};

use crate::{
    protocol::{BlockChannel, PlaceBlockRequest},
    server::chunk_requests::ChunkRequests,
};

/// Server-side system that processes incoming [`PlaceBlockRequest`] messages from clients.
///
/// For each request received from a connected client the system will:
///
/// 1. Resolve the target block inside the [`ChunkCache`] using the request position.
/// 2. Check [`BlockRegistry::is_replaceable`] — requests targeting non-replaceable blocks are
///    silently dropped (the server is authoritative; the client will not see its
///    optimistic placement confirmed).
/// 3. Apply the placement directly to the cached [`Chunk`] so that subsequent reads
///    from other systems see the updated state immediately.
/// 4. Write a [`BlockPlaced`] Bevy message so that any other server systems that
///    observe block-placement events (e.g. `log_block_placed`) are notified.
/// 5. Broadcast a [`BlockPlacedMessage`] over [`BlockChannel`] to every connected
///    client whose [`ChunkRequests`] set currently contains the target [`ChunkPos`],
///    meaning that chunk is already loaded on that client.
pub(crate) fn receive_place_requests(
    registry: Res<BlockRegistry>,
    mut cache: ResMut<ChunkCache>,
    mut placed_writer: MessageWriter<BlockPlaced>,
    mut receivers: Query<(&mut MessageReceiver<PlaceBlockRequest>, &ChunkRequests)>,
    mut broadcasters: Query<(&mut MessageSender<BlockPlaced>, &ChunkRequests)>,
) {
    // Collect valid, accepted requests first so we can borrow `cache` mutably
    // without conflicting with the immutable borrow needed for the replaceability
    // check.
    let mut accepted: Vec<PlaceBlockRequest> = Vec::new();

    for (mut receiver, _chunk_requests) in receivers.iter_mut() {
        for req in receiver.receive() {
            let chunk_pos = req.pos.chunk_pos();
            let local = req.pos.chunk_local();

            // Guard: y must be non-negative before casting to usize.
            if local.y < 0 {
                debug!(
                    "Server: ignoring PlaceBlockRequest at {} — y={} is below world floor",
                    req.pos, local.y
                );
                continue;
            }

            // Look up the current block at the target position.
            let current_block = cache
                .get(&chunk_pos)
                .and_then(|chunk| chunk.get(local.x as usize, local.y as usize, local.z as usize));

            let Some(block) = current_block else {
                debug!(
                    "Server: ignoring PlaceBlockRequest at {} — chunk {} not loaded",
                    req.pos, chunk_pos
                );
                continue;
            };

            // Server-authoritative replaceability check.
            if !registry.is_replaceable(&block) {
                debug!(
                    "Server: ignoring PlaceBlockRequest at {} — block {:?} is not replaceable",
                    req.pos, block.block_id
                );
                continue;
            }

            accepted.push(req);
        }
    }

    for req in accepted {
        let chunk_pos = req.pos.chunk_pos();
        let local = req.pos.chunk_local();

        // Apply the placement to the authoritative chunk cache.
        if let Some(chunk) = cache.get_mut(&chunk_pos) {
            chunk.set(
                local.x as usize,
                local.y as usize,
                local.z as usize,
                Block::new(req.block_id),
            );
        }

        // Notify internal server systems (e.g. the existing log_block_placed reader).
        placed_writer.write(BlockPlaced {
            pos: req.pos,
            block_id: req.block_id,
            placer: None,
        });

        // Broadcast to every client that currently has the affected chunk loaded.
        let mut broadcast_count = 0usize;
        for (mut sender, _client_chunks) in broadcasters.iter_mut() {
            // Currently we don't have any method to track which clients have which chunks loaded, so we'll just broadcast to everyone.
            // Once we have the position of each player's camera we can compute which chunk(s) they are looking at and only broadcast to those clients.
            // if client_chunks.contains(&chunk_pos) {
            sender.send::<BlockChannel>(BlockPlaced {
                pos: req.pos,
                block_id: req.block_id,
                placer: None,
            });
            broadcast_count += 1;
            // }
        }

        trace!(
            "Server: placing {:?} at {} — broadcasting to {} clients",
            req.block_id, req.pos, broadcast_count
        );
    }
}
