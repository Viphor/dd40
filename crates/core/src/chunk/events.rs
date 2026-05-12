use bevy::ecs::message::Message;
use serde::{Deserialize, Serialize};

use crate::chunk::{Chunk, ChunkChange, ChunkPos};

/// Event telling the world generator to generate a chunk at the given position.
/// The world generator should respond with a `ChunkReady` event when the chunk is ready.
#[derive(Message, Clone, Serialize, Deserialize)]
pub struct GenerateChunk {
    pub pos: ChunkPos,
}

/// Sent when some system wants a chunk to be loaded/generated.
///
/// `current_version` carries the chunk version the requester already has
/// cached, or `0` if none. The server uses this to decide between sending
/// a [`ChunkSnapshot`](crate::chunk::events) (full chunk) and a
/// [`ChunkChanged`] delta:
///
/// - `0` → always reply with a snapshot (the requester has nothing).
/// - `> server_version` → log + reply with a snapshot (requester is ahead).
/// - `< server_version` and within `MaxDeltaBehind` → reply with the
///   missing changes as a delta.
/// - Otherwise → reply with a snapshot (the gap is too large or history
///   has been truncated).
///
/// Local-only requesters (e.g. `dd40_chunk_storage`) ignore this field
/// since they always materialise the chunk from disk or generation.
#[derive(Message, Clone, Serialize, Deserialize)]
pub struct RequestChunk {
    /// Position of the chunk being requested.
    pub pos: ChunkPos,
    /// The version the requester already has cached, or `0` if none.
    pub current_version: u64,
}

/// Sent when a chunk is ready to be inserted into [`ChunkCache`].
#[derive(Message, Clone, Serialize, Deserialize)]
pub struct ChunkReady {
    pub chunk: Chunk,
}

/// Local Bevy message broadcast on **both client and server** every time a
/// chunk's authoritative state changes — once per `commit_predicted_changes`
/// pass on the server, and once per applied `ChunkUpdate` on the client.
///
/// Subscribed by the renderer (to remesh), audio (to play place/break
/// sounds), redstone, fire, etc. New downstream listeners go here rather
/// than as additional dedicated messages.
#[derive(Message, Clone, Debug)]
pub struct ChunkChanged {
    /// Chunk that changed.
    pub pos: ChunkPos,
    /// Changes applied, in commit order. Coordinates are chunk-local.
    pub changes: Vec<ChunkChange>,
    /// Authoritative chunk version after these changes were applied.
    pub new_version: u64,
}

/// Local Bevy message fired every time a predicted [`ChunkChange`] is queued
/// on a chunk via
/// [`ChunkCache::push_predicted`](crate::chunk::cache::ChunkCache::push_predicted).
///
/// Optimistic listeners — first and foremost the renderer — subscribe to this
/// to remesh chunks the same frame as the local prediction, without waiting
/// for the authority commit pass.
///
/// `change.local()` already disambiguates the affected cell; the chunk is
/// `pos`. The optimistic block value is already written into the chunk's
/// `data` by the time this message is read, so listeners can read directly
/// from the [`ChunkCache`](crate::chunk::cache::ChunkCache).
#[derive(Message, Clone, Debug)]
pub struct ChunkPredicted {
    /// Chunk that received the prediction.
    pub pos: ChunkPos,
    /// Predicted change. Coordinates are chunk-local.
    pub change: ChunkChange,
}

/// Local Bevy message fired on a client when a locally-predicted change is
/// rejected by the server (i.e. the server's authoritative `ChunkUpdate`
/// did not contain a matching change).
///
/// The change is also logged at `warn!` level — listeners are optional and
/// exist for UI hooks (e.g. flashing the held-tool icon, replaying a
/// "block won't go there" sound).
#[derive(Message, Clone, Debug)]
pub struct PredictionRejected {
    /// Chunk the rejected prediction targeted.
    pub pos: ChunkPos,
    /// The predicted change that was rejected.
    pub change: ChunkChange,
}

/// Local Bevy message fired on the **server** every time the server falls
/// back to sending a full snapshot instead of a delta because the client's
/// requested version is more than `MaxDeltaBehind` versions stale (or the
/// chunk's history has been truncated).
///
/// Not an error — this is the documented bandwidth/space tradeoff. Future
/// analysis tooling can subscribe to this to surface "clients are
/// reconnecting too far behind" patterns.
#[derive(Message, Clone, Debug)]
pub struct ChunkSnapshotFallback {
    /// Chunk the request was for.
    pub pos: ChunkPos,
    /// Version the client said it had.
    pub client_version: u64,
    /// Version the server has.
    pub server_version: u64,
}
