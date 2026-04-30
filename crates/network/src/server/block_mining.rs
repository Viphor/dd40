//! Server-side block-mining validation.
//!
//! Receives the three mining messages from each client and enforces server-
//! authoritative timing:
//!
//! 1. [`StartMiningRequest`] — records a [`MiningSession`] component on the
//!    connection entity with the current server time and the expected duration
//!    (computed from the same [`mining_duration`] formula used by the client).
//! 2. [`AbortMiningRequest`] — removes the session component.
//! 3. [`MineBlockRequest`] — validates that enough time has elapsed, that the
//!    targeted block is still intact, and that the block is destructible; then
//!    removes the block and broadcasts [`BlockRemoved`] to all clients.
//!
//! # Session lifecycle
//!
//! [`MiningSession`] lives as a component on the lightyear connection entity.
//! When the client disconnects, Bevy despawns that entity automatically,
//! removing the session with it — no explicit cleanup is needed.
//!
//! # Latency tolerance
//!
//! The server accepts a [`MineBlockRequest`] up to [`MINING_LATENCY_TOLERANCE`]
//! seconds *before* the required duration has fully elapsed.  This compensates
//! for round-trip latency without allowing meaningful cheating.
//!
//! [`mining_duration`]: dd40_core::tools::mining_duration

use bevy::prelude::*;
use dd40_core::{
    block::{Block, BlockId, events::{AbortMiningRequest, BlockRemoved, MineBlockRequest, StartMiningRequest}},
    chunk::cache::ChunkCache,
    prelude::*,
    tools::{ToolRegistry, mining_duration},
};
use lightyear::prelude::{MessageReceiver, MessageSender};

use crate::server::chunk_requests::ChunkRequests;
use crate::protocol::BlockChannel;

/// How many seconds before the full required duration the server will still
/// accept a [`MineBlockRequest`].  Compensates for network round-trip latency.
const MINING_LATENCY_TOLERANCE: f32 = 0.3;

// ── Session component ─────────────────────────────────────────────────────────

/// Records an in-progress mining action for one connected client.
///
/// This component is inserted on the lightyear connection entity when a
/// [`StartMiningRequest`] is received and removed when an [`AbortMiningRequest`]
/// arrives or the connection entity is despawned (client disconnect).
#[derive(Component, Debug, Clone)]
pub(crate) struct MiningSession {
    /// The block the client claimed to start mining.
    pub pos: BlockPos,
    /// Server time (in seconds since startup) when the session was created.
    pub started_at: f32,
    /// How long (seconds) the client must mine this block with the declared tool.
    ///
    /// Computed server-side using [`mining_duration`] to prevent clients from
    /// manipulating this value.
    pub required_duration: f32,
}

// ── Systems ───────────────────────────────────────────────────────────────────

/// Receives [`StartMiningRequest`] from each client and records a
/// [`MiningSession`] on the connection entity.
///
/// Silently rejects requests targeting blocks that are not destructible or are
/// not present in the loaded cache.
pub(crate) fn receive_start_mining(
    registry: Res<BlockRegistry>,
    tool_registry: Res<ToolRegistry>,
    cache: Res<ChunkCache>,
    time: Res<Time>,
    mut commands: Commands,
    mut receivers: Query<(Entity, &mut MessageReceiver<StartMiningRequest>)>,
) {
    for (entity, mut receiver) in receivers.iter_mut() {
        for req in receiver.receive() {
            let chunk_pos = req.pos.chunk_pos();
            let local = req.pos.chunk_local();

            if local.y < 0 {
                continue;
            }

            let current_block = cache
                .get(&chunk_pos)
                .and_then(|chunk| chunk.get(local.x as usize, local.y as usize, local.z as usize));

            let Some(block) = current_block else {
                debug!("Server: StartMiningRequest at {} ignored — chunk not loaded", req.pos);
                continue;
            };

            // Reject air / replaceable blocks.
            if registry.is_replaceable(&block) {
                debug!("Server: StartMiningRequest at {} ignored — block is replaceable", req.pos);
                continue;
            }

            let Some(block_def) = registry.get(block.block_id) else {
                continue;
            };

            if !block_def.is_destructible {
                debug!("Server: StartMiningRequest at {} ignored — block is indestructible", req.pos);
                continue;
            }

            let Some(required_duration) = mining_duration(block_def, &req.tool, &tool_registry) else {
                continue;
            };

            debug!(
                "Server: mining session started at {} (required {:.2}s)",
                req.pos, required_duration
            );

            commands.entity(entity).insert(MiningSession {
                pos: req.pos,
                started_at: time.elapsed_secs(),
                required_duration,
            });
        }
    }
}

/// Receives [`AbortMiningRequest`] from each client and removes the
/// [`MiningSession`] component from the connection entity.
pub(crate) fn receive_abort_mining(
    mut commands: Commands,
    mut receivers: Query<(Entity, &mut MessageReceiver<AbortMiningRequest>, Option<&MiningSession>)>,
) {
    for (entity, mut receiver, maybe_session) in receivers.iter_mut() {
        for req in receiver.receive() {
            if maybe_session.is_some() {
                debug!("Server: mining aborted at {}", req.pos);
                commands.entity(entity).remove::<MiningSession>();
            }
        }
    }
}

/// Receives [`MineBlockRequest`] from each client and, after validation,
/// removes the block and broadcasts [`BlockRemoved`] to all clients.
///
/// # Validation steps
///
/// 1. A [`MiningSession`] must exist on the connection entity at the same `pos`.
/// 2. `elapsed >= required_duration - MINING_LATENCY_TOLERANCE`.
/// 3. The block at `pos` must still be a non-replaceable, destructible block.
pub(crate) fn receive_mine_block(
    registry: Res<BlockRegistry>,
    _tool_registry: Res<ToolRegistry>,
    mut cache: ResMut<ChunkCache>,
    time: Res<Time>,
    mut commands: Commands,
    mut removed_writer: MessageWriter<BlockRemoved>,
    mut receivers: Query<(Entity, &mut MessageReceiver<MineBlockRequest>, Option<&MiningSession>, &ChunkRequests)>,
    mut broadcasters: Query<(&mut MessageSender<BlockRemoved>, &ChunkRequests)>,
) {
    let mut accepted: Vec<(Entity, BlockPos, BlockId)> = Vec::new();

    for (entity, mut receiver, maybe_session, _) in receivers.iter_mut() {
        for req in receiver.receive() {
            let Some(session) = maybe_session else {
                debug!("Server: MineBlockRequest at {} ignored — no active session", req.pos);
                continue;
            };

            // Position must match the active session.
            if session.pos != req.pos {
                debug!("Server: MineBlockRequest at {} ignored — session is for {}", req.pos, session.pos);
                continue;
            }

            // Timing check (with latency tolerance).
            let elapsed = time.elapsed_secs() - session.started_at;
            if elapsed < session.required_duration - MINING_LATENCY_TOLERANCE {
                debug!(
                    "Server: MineBlockRequest at {} rejected — only {:.2}s elapsed, need {:.2}s",
                    req.pos, elapsed, session.required_duration
                );
                continue;
            }

            // Verify the block is still there and still destructible.
            let chunk_pos = req.pos.chunk_pos();
            let local = req.pos.chunk_local();

            if local.y < 0 {
                continue;
            }

            let current_block = cache
                .get(&chunk_pos)
                .and_then(|c| c.get(local.x as usize, local.y as usize, local.z as usize));

            let Some(block) = current_block else {
                continue;
            };

            if registry.is_replaceable(&block) {
                debug!("Server: MineBlockRequest at {} ignored — block already removed", req.pos);
                continue;
            }

            let Some(block_def) = registry.get(block.block_id) else {
                continue;
            };

            if !block_def.is_destructible {
                continue;
            }

            accepted.push((entity, req.pos, block.block_id));
        }
    }

    for (entity, pos, previous_block_id) in accepted {
        let chunk_pos = pos.chunk_pos();
        let local = pos.chunk_local();

        // Remove the session component.
        commands.entity(entity).remove::<MiningSession>();

        // Apply to the authoritative cache.
        if let Some(chunk) = cache.get_mut(&chunk_pos) {
            chunk.set(
                local.x as usize,
                local.y as usize,
                local.z as usize,
                Block::new(BlockId::AIR),
            );
        }

        // Notify local server systems.
        removed_writer.write(BlockRemoved {
            pos,
            previous_block_id,
            remover: Some(entity),
        });

        // Broadcast to all connected clients.
        let mut broadcast_count = 0usize;
        for (mut sender, _) in broadcasters.iter_mut() {
            sender.send::<BlockChannel>(BlockRemoved {
                pos,
                previous_block_id,
                remover: None,
            });
            broadcast_count += 1;
        }

        debug!(
            "Server: block {:?} removed at {} — broadcasting to {} clients",
            previous_block_id, pos, broadcast_count
        );
    }
}
