use bevy::prelude::*;
use dd40_core::{
    block::{Block, CollisionShape, events::BlockPlaced},
    chunk::cache::ChunkCache,
    prelude::*,
};
use dd40_physics_core::prelude::{Aabb, CharacterPosition, CharacterSpatialCache};
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
/// 3. Check whether any character (player, NPC, or any other entity with a
///    [`CharacterPosition`] and [`Aabb`]) occupies the target cell.  Collidable
///    blocks (solid, slab, …) are rejected if they would overlap a character.
///    Non-collidable blocks (flowers, torches, …) are always allowed.
/// 4. Apply the placement directly to the cached [`Chunk`] so that subsequent reads
///    from other systems see the updated state immediately.
/// 5. Write a [`BlockPlaced`] Bevy message so that any other server systems that
///    observe block-placement events (e.g. `log_block_placed`) are notified.
/// 6. Broadcast a [`BlockPlacedMessage`] over [`BlockChannel`] to every connected
///    client whose [`ChunkRequests`] set currently contains the target [`ChunkPos`],
///    meaning that chunk is already loaded on that client.
pub(crate) fn receive_place_requests(
    registry: Res<BlockRegistry>,
    mut cache: ResMut<ChunkCache>,
    spatial_cache: Res<CharacterSpatialCache>,
    characters: Query<(&CharacterPosition, &Aabb)>,
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

            // Reject placements that would trap a character inside a collidable
            // block.  Non-collidable blocks (CollisionShape::None) can always be
            // placed freely since they have no physical presence.
            let held_block = Block::new(req.block_id);
            if !matches!(registry.collision_shape(&held_block), CollisionShape::None) {
                // The block cell's AABB in world space.  Our `Aabb` convention
                // places the origin at the bottom-centre, so for a 1×1×1 cell
                // the origin is at (x + 0.5, y, z + 0.5) with half-extents 0.5.
                let block_aabb = Aabb::new(0.5, 0.5, 0.5);
                let block_origin = Vec3::new(
                    req.pos.x as f32 + 0.5,
                    req.pos.y as f32,
                    req.pos.z as f32 + 0.5,
                );

                // Use the spatial cache to limit the check to characters that
                // share a chunk with the target cell, then run a precise AABB
                // test only on those candidates.
                let overlaps_character =
                    spatial_cache.candidates_for_block(req.pos).any(|entity| {
                        match characters.get(entity) {
                            Ok((char_pos, char_aabb)) => {
                                char_aabb.overlaps(char_pos.0, &block_aabb, block_origin)
                            }
                            Err(_) => false,
                        }
                    });

                if overlaps_character {
                    debug!(
                        "Server: ignoring PlaceBlockRequest at {} — cell occupied by a character",
                        req.pos
                    );
                    continue;
                }
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
