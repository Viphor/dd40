use bevy::ecs::message::Message;
use serde::{Deserialize, Serialize};

use crate::chunk::{Chunk, ChunkPos};

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
