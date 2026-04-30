//! Client-side block-mining network bridge.
//!
//! This module reads the three mining Bevy messages written by `dd40_player`
//! and forwards them to the server over lightyear's [`BlockChannel`].  It also
//! receives [`BlockRemoved`] messages from the server and applies them to the
//! local [`ChunkCache`], then re-emits a Bevy [`BlockRemoved`] message so that
//! other local systems (rendering, audio, HUD) can react without coupling to
//! the network layer.

use bevy::prelude::*;
use dd40_core::{
    block::{Block, BlockId, events::{AbortMiningRequest, BlockRemoved, MineBlockRequest, StartMiningRequest}},
    chunk::cache::ChunkCache,
};
use lightyear::prelude::{MessageReceiver, MessageSender};

use crate::protocol::BlockChannel;

// ── Client → Server ───────────────────────────────────────────────────────────

/// Drains [`StartMiningRequest`] Bevy messages and forwards them to the server.
pub(crate) fn send_start_mining(
    mut reader: MessageReader<StartMiningRequest>,
    mut senders: Query<&mut MessageSender<StartMiningRequest>>,
) {
    let messages: Vec<_> = reader.read().cloned().collect();
    for mut sender in senders.iter_mut() {
        for msg in messages.iter() {
            trace!("Client: sending StartMiningRequest at {}", msg.pos);
            sender.send::<BlockChannel>(msg.clone());
        }
    }
}

/// Drains [`AbortMiningRequest`] Bevy messages and forwards them to the server.
pub(crate) fn send_abort_mining(
    mut reader: MessageReader<AbortMiningRequest>,
    mut senders: Query<&mut MessageSender<AbortMiningRequest>>,
) {
    let messages: Vec<_> = reader.read().cloned().collect();
    for mut sender in senders.iter_mut() {
        for msg in messages.iter() {
            trace!("Client: sending AbortMiningRequest at {}", msg.pos);
            sender.send::<BlockChannel>(msg.clone());
        }
    }
}

/// Drains [`MineBlockRequest`] Bevy messages and forwards them to the server.
pub(crate) fn send_mine_block(
    mut reader: MessageReader<MineBlockRequest>,
    mut senders: Query<&mut MessageSender<MineBlockRequest>>,
) {
    let messages: Vec<_> = reader.read().cloned().collect();
    for mut sender in senders.iter_mut() {
        for msg in messages.iter() {
            trace!("Client: sending MineBlockRequest at {}", msg.pos);
            sender.send::<BlockChannel>(msg.clone());
        }
    }
}

// ── Server → Client ───────────────────────────────────────────────────────────

/// Receives [`BlockRemoved`] messages from the server and applies them to the
/// local [`ChunkCache`].
///
/// For every message received the system:
/// 1. Looks up the target chunk in the cache.
/// 2. If the chunk is present, sets the voxel to [`BlockId::AIR`].
/// 3. Writes a [`BlockRemoved`] Bevy message so that rendering, audio, and
///    other reactive systems can respond without coupling to the network layer.
///
/// # Ordering
///
/// Registered in [`PostUpdate`] so cache mutations are visible to rendering
/// systems later in the same frame.
///
/// # Missing chunks
///
/// If the target chunk is not loaded the update is silently dropped. The
/// correct block data will arrive when the chunk is eventually loaded.
pub(crate) fn receive_removed_blocks(
    mut receiver: Single<&mut MessageReceiver<BlockRemoved>>,
    mut cache: ResMut<ChunkCache>,
    mut removed: MessageWriter<BlockRemoved>,
) {
    for msg in receiver.receive() {
        trace!("Client: received BlockRemoved at {}", msg.pos);

        let chunk_pos = msg.pos.chunk_pos();
        let local = msg.pos.chunk_local();

        if local.y >= 0 {
            if let Some(chunk) = cache.get_mut(&chunk_pos) {
                chunk.set(
                    local.x as usize,
                    local.y as usize,
                    local.z as usize,
                    Block::new(BlockId::AIR),
                );
            }
        }

        removed.write(BlockRemoved {
            pos: msg.pos,
            previous_block_id: msg.previous_block_id,
            remover: None,
        });
    }
}
