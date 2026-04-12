use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
};

use crate::chunk::{
    ChunkPos,
    events::{ChunkReady, RequestChunk},
};

use super::*;

#[derive(Resource, Default)]
pub struct ChunkCache {
    /// Map of chunk positions to cached chunk data.
    chunks: HashMap<ChunkPos, Chunk>,
    /// Requested chunks that have not yet been fulfilled by the provider.
    requested: HashSet<ChunkPos>,
    /// Chunks that have been requested but not yet fulfilled by the provider.
    waiting: HashSet<ChunkPos>,
}

impl ChunkCache {
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
            requested: HashSet::new(),
            waiting: HashSet::new(),
        }
    }

    /// Returns a reference to the cached chunk at `pos`, or `None` if it is
    /// not present in the cache.
    pub fn get(&self, pos: &ChunkPos) -> Option<&Chunk> {
        self.chunks.get(pos)
    }

    /// Returns a mutable reference to the cached chunk at `pos`, or `None` if
    /// it is not present in the cache.
    pub fn get_mut(&mut self, pos: &ChunkPos) -> Option<&mut Chunk> {
        self.chunks.get_mut(pos)
    }

    /// Requests the chunk at `pos` from the provider if it has not already been requested,
    /// and returns a reference to the cached chunk if it is present.
    pub fn request(&mut self, pos: ChunkPos) -> Option<&Chunk> {
        let res = self.chunks.get(&pos);
        if res.is_none() && !self.waiting.contains(&pos) {
            self.requested.insert(pos);
        }
        res
    }

    pub fn contains(&mut self, pos: &ChunkPos) -> bool {
        self.chunks.contains_key(pos) || self.requested.contains(pos)
    }

    /// Returns an iterator over all chunk positions currently loaded in the cache.
    ///
    /// Only returns positions for chunks whose data is fully loaded (i.e. present
    /// in the cache map). Chunks that are requested but not yet loaded are not
    /// included.
    pub fn iter_positions(&self) -> impl Iterator<Item = &ChunkPos> {
        self.chunks.keys()
    }

    /// Inserts a chunk into the cache, replacing any existing entry.
    ///
    /// NOTE: You should not call this method directly.
    /// Instead, send the message `ChunkReady` with the chunk to be inserted,
    /// and the `chunk_ready_listener` system will handle inserting it into the cache.
    ///
    /// Calling this method directly may lead to unexpected behavior.
    /// No guarantees are made for updating the rest of the systems.
    pub fn insert(&mut self, chunk: Chunk) {
        self.chunks.insert(chunk.position, chunk);
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}

/// System that listens for `ChunkReady` events and inserts the ready chunks into the cache.
pub fn chunk_ready_listener(mut cache: ResMut<ChunkCache>, mut events: MessageReader<ChunkReady>) {
    for event in events.read() {
        debug!(
            "Inserting chunk {:?} into the cache",
            event.chunk.position()
        );
        cache.insert(event.chunk.clone());
    }
}

pub fn request_chunk_system(mut cache: ResMut<ChunkCache>, mut mq: MessageWriter<RequestChunk>) {
    cache.requested.iter().for_each(|&pos| {
        debug!("Requesting chunk {:?} from provider", pos);
        mq.write(RequestChunk { pos });
    });
    let requested = cache.requested.drain().collect::<Vec<_>>();
    cache.waiting.extend(requested);
}

pub struct ChunkCachePlugin;

impl Plugin for ChunkCachePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkCache>()
            .add_systems(PreUpdate, chunk_ready_listener)
            .add_systems(PostUpdate, request_chunk_system);
    }
}
