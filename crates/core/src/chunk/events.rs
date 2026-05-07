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
#[derive(Message, Clone, Serialize, Deserialize)]
pub struct RequestChunk {
    pub pos: ChunkPos,
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
