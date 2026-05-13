use bevy::prelude::*;
use dd40_core::chunk::events::ChunkSnapshotFallback;
use dd40_core::prelude::*;
use lightyear::prelude::{MessageReceiver, MessageSender};

use crate::{
    protocol::{ChunkChannel, ChunkSnapshot, ChunkUpdate},
    server::chunk_requests::ChunkRequests,
};

/// Receives [`RequestChunk`] messages from each client and resolves them
/// against the server's [`ChunkCache`] using the client's `current_version`:
///
/// - **Chunk not cached** → forward the request into the local
///   [`RequestChunk`] message queue so the world generator / storage
///   backend produces it. The connection is registered in
///   [`ChunkRequests`] so the corresponding [`ChunkSnapshot`] is later
///   forwarded by [`send_chunk_data`].
/// - **`current_version == 0`** → client has nothing; send the cached
///   chunk as a [`ChunkSnapshot`] (full snapshot).
/// - **`current_version == server_version`** → client is up to date;
///   no message is sent.
/// - **`current_version > server_version`** → client claims to be ahead;
///   warn and send a full snapshot to recover.
/// - **`current_version < server_version` and within
///   [`MaxDeltaBehind`]** → if [`Chunk::history_since`] returns a delta,
///   send it as a [`ChunkUpdate`]. Otherwise (history truncated or gap
///   too large) fire [`ChunkSnapshotFallback`] and send a full snapshot.
/// - **`current_version < server_version` but beyond
///   [`MaxDeltaBehind`]** → fire [`ChunkSnapshotFallback`] and send a
///   full snapshot.
#[allow(clippy::type_complexity)]
pub(crate) fn receive_chunk_requests(
    chunk_cache: Res<ChunkCache>,
    max_delta: Res<MaxDeltaBehind>,
    mut requests: MessageWriter<RequestChunk>,
    mut snapshot_fallback: MessageWriter<ChunkSnapshotFallback>,
    mut receivers: Query<(
        &mut MessageReceiver<RequestChunk>,
        &mut MessageSender<ChunkSnapshot>,
        &mut MessageSender<ChunkUpdate>,
        &mut ChunkRequests,
    )>,
) {
    for (mut receiver, mut snapshot_sender, mut update_sender, mut pending) in receivers.iter_mut()
    {
        if receiver.has_messages() {
            trace!("Receiving chunk requests from client");
        }
        for request in receiver.receive() {
            trace!(
                "Received chunk request at {} (client version {})",
                request.pos, request.current_version
            );

            let Some(chunk) = chunk_cache.get(&request.pos) else {
                pending.insert(request.pos);
                requests.write(request);
                continue;
            };

            let server_version = chunk.version();
            let client_version = request.current_version;

            if client_version == server_version {
                trace!(
                    "Client at chunk {} already at version {} — no reply needed",
                    request.pos, server_version
                );
                continue;
            }

            if client_version > server_version {
                warn!(
                    "Client claims chunk {} version {} > server {} — sending snapshot to resync",
                    request.pos, client_version, server_version
                );
                snapshot_sender.send::<ChunkChannel>(ChunkSnapshot {
                    chunk: chunk.clone(),
                });
                continue;
            }

            // Client is behind. Try to deliver a delta if it's within window.
            let within_window = client_version.saturating_add(max_delta.0 as u64) >= server_version;
            let delta = if within_window {
                chunk.history_since(client_version)
            } else {
                None
            };

            match delta {
                Some(history) if !history.is_empty() => {
                    let new_version = history.last().map(|(v, _)| *v).unwrap_or(server_version);
                    let changes: Vec<_> = history.into_iter().map(|(_, c)| c).collect();
                    trace!(
                        "Sending {} delta change(s) for chunk {} ({} -> {})",
                        changes.len(),
                        request.pos,
                        client_version,
                        new_version
                    );
                    update_sender.send::<ChunkChannel>(ChunkUpdate {
                        pos: request.pos,
                        base_version: client_version,
                        changes,
                        new_version,
                    });
                }
                Some(_) => {
                    // history_since returned empty despite version mismatch
                    // — defensive fallback (shouldn't happen if version
                    // arithmetic is correct).
                    snapshot_sender.send::<ChunkChannel>(ChunkSnapshot {
                        chunk: chunk.clone(),
                    });
                }
                None => {
                    snapshot_fallback.write(ChunkSnapshotFallback {
                        pos: request.pos,
                        client_version,
                        server_version,
                    });
                    debug!(
                        "Falling back to snapshot for chunk {} (client {} server {})",
                        request.pos, client_version, server_version
                    );
                    snapshot_sender.send::<ChunkChannel>(ChunkSnapshot {
                        chunk: chunk.clone(),
                    });
                }
            }
        }
    }
}

/// Forwards locally-produced [`ChunkReady`] messages (typically emitted by
/// the world generator or storage backend) to every connection that has a
/// pending request for the chunk's position. Sent as a [`ChunkSnapshot`]
/// over the wire.
pub(crate) fn send_chunk_data(
    mut reader: MessageReader<ChunkReady>,
    mut senders: Query<(&mut MessageSender<ChunkSnapshot>, &mut ChunkRequests)>,
) {
    let messages = reader.read().collect::<Vec<_>>();
    senders.par_iter_mut().for_each(|(mut sender, mut cache)| {
        for ready in messages.iter() {
            if cache.contains(&ready.chunk.position()) {
                trace!("Sending chunk data at {}", ready.chunk.position());
                sender.send::<ChunkChannel>(ChunkSnapshot {
                    chunk: ready.chunk.clone(),
                });
                cache.remove(&ready.chunk.position());
            }
        }
    });
}
