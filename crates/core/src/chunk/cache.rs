use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
};

use crate::chunk::{
    ChunkChange, ChunkPos,
    events::{ChunkPredicted, ChunkReady, RequestChunk},
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
    /// Index of chunks that currently have pending predicted changes.
    ///
    /// Maintained automatically by [`ChunkCache::push_predicted`]; consumed
    /// by the chunk-authority commit pass via [`ChunkCache::drain_dirty`].
    /// This is the **O(1)** alternative to scanning every chunk in the cache
    /// every frame looking for non-empty predicted queues — important once
    /// the loaded radius is large.
    dirty: HashSet<ChunkPos>,
    /// Buffered [`ChunkPredicted`] payloads queued by
    /// [`ChunkCache::push_predicted`], drained each frame by
    /// [`emit_predicted_events`] into a [`MessageWriter`]. Listeners (the
    /// renderer, audio, …) subscribe to the resulting messages rather than
    /// polling the cache.
    pending_predicted_events: Vec<(ChunkPos, ChunkChange)>,
}

impl ChunkCache {
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
            requested: HashSet::new(),
            waiting: HashSet::new(),
            dirty: HashSet::new(),
            pending_predicted_events: Vec::new(),
        }
    }

    /// Returns a reference to the cached chunk at `pos`, or `None` if it is
    /// not present in the cache.
    pub fn get(&self, pos: &ChunkPos) -> Option<&Chunk> {
        self.chunks.get(pos)
    }

    /// Returns a mutable reference to the cached chunk at `pos`, or `None` if
    /// it is not present in the cache.
    ///
    /// **Warning:** mutations performed via this handle do **not** mark the
    /// chunk dirty. To enqueue a predicted change, prefer
    /// [`ChunkCache::push_predicted`], which both forwards to
    /// [`Chunk::push_predicted`] and registers the chunk in the dirty index.
    pub fn get_mut(&mut self, pos: &ChunkPos) -> Option<&mut Chunk> {
        self.chunks.get_mut(pos)
    }

    /// Enqueue a predicted [`ChunkChange`] on the chunk at `pos`, capturing
    /// the prior block at the target cell, optimistically applying the
    /// change to the chunk's data, and marking the chunk dirty for the
    /// next authority commit pass.
    ///
    /// This is the **canonical entry point** for prediction. It guarantees
    /// the chunk lands in the dirty index in O(1), so the commit pass never
    /// has to scan the whole cache.
    ///
    /// Returns `true` if the chunk was present and the change was queued,
    /// `false` if there is no chunk at `pos` (caller should request the
    /// chunk first).
    pub fn push_predicted(&mut self, pos: ChunkPos, change: ChunkChange) -> bool {
        let Some(chunk) = self.chunks.get_mut(&pos) else {
            return false;
        };
        chunk.push_predicted(change);
        self.dirty.insert(pos);
        self.pending_predicted_events.push((pos, change));
        true
    }

    /// Manually mark a chunk as dirty.
    ///
    /// You should rarely need this — [`ChunkCache::push_predicted`] handles
    /// it for you. Useful only if you bypass the canonical predicted path
    /// (e.g. when applying confirmed history out-of-band on the client).
    pub fn mark_dirty(&mut self, pos: ChunkPos) {
        if self.chunks.contains_key(&pos) {
            self.dirty.insert(pos);
        }
    }

    /// Iterate over chunk positions that currently have pending predicted
    /// changes, without consuming the dirty index.
    pub fn dirty_chunks(&self) -> impl Iterator<Item = &ChunkPos> {
        self.dirty.iter()
    }

    /// Drain the dirty index, returning every position with pending
    /// predicted changes and leaving the index empty.
    ///
    /// The chunk-authority commit pass calls this exactly once per frame.
    pub fn drain_dirty(&mut self) -> bevy::platform::collections::hash_set::Drain<'_, ChunkPos> {
        self.dirty.drain()
    }

    /// Number of chunks currently flagged dirty.
    pub fn dirty_count(&self) -> usize {
        self.dirty.len()
    }

    /// Drains buffered [`ChunkPredicted`] payloads queued by
    /// [`ChunkCache::push_predicted`]. Called once per frame by
    /// [`emit_predicted_events`].
    pub fn drain_pending_predicted_events(
        &mut self,
    ) -> std::vec::Drain<'_, (ChunkPos, ChunkChange)> {
        self.pending_predicted_events.drain(..)
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
        let pos = chunk.position;
        if !chunk.predicted().is_empty() {
            self.dirty.insert(pos);
        }
        self.chunks.insert(pos, chunk);
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

/// Drains buffered predictions queued by [`ChunkCache::push_predicted`] and
/// publishes them as [`ChunkPredicted`] messages.
///
/// Runs in [`PostUpdate`] so every prediction queued during the same frame's
/// `Update` is emitted in a single batch. Listeners read the messages on the
/// next frame's [`PreUpdate`].
pub fn emit_predicted_events(
    mut cache: ResMut<ChunkCache>,
    mut writer: MessageWriter<ChunkPredicted>,
) {
    for (pos, change) in cache.drain_pending_predicted_events() {
        writer.write(ChunkPredicted { pos, change });
    }
}

pub struct ChunkCachePlugin;

impl Plugin for ChunkCachePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkCache>()
            .add_systems(PreUpdate, chunk_ready_listener)
            .add_systems(PostUpdate, (request_chunk_system, emit_predicted_events));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockId;
    use crate::chunk::{BlockLocal, Chunk, ChunkChange};
    use bevy::ecs::message::Messages;

    fn cell() -> BlockLocal {
        BlockLocal::new(1, 2, 3)
    }

    fn pos() -> ChunkPos {
        ChunkPos::new(0, 0)
    }

    fn build_app() -> App {
        let mut app = App::new();
        app.add_message::<ChunkPredicted>();
        app.add_message::<ChunkReady>();
        app.add_message::<RequestChunk>();
        app.add_plugins(ChunkCachePlugin);
        app
    }

    #[test]
    fn push_predicted_buffers_event_payload() {
        let mut cache = ChunkCache::new();
        cache.insert(Chunk::new(pos()));
        let change = ChunkChange::new_place(cell(), BlockId(1));
        assert!(cache.push_predicted(pos(), change));
        let drained: Vec<_> = cache.drain_pending_predicted_events().collect();
        assert_eq!(drained, vec![(pos(), change)]);
    }

    #[test]
    fn push_predicted_on_missing_chunk_buffers_nothing() {
        let mut cache = ChunkCache::new();
        let change = ChunkChange::new_remove(cell());
        assert!(!cache.push_predicted(pos(), change));
        assert_eq!(cache.drain_pending_predicted_events().count(), 0);
    }

    #[test]
    fn emit_predicted_events_publishes_message_per_prediction() {
        let mut app = build_app();
        {
            let mut cache = app.world_mut().resource_mut::<ChunkCache>();
            cache.insert(Chunk::new(pos()));
            assert!(cache.push_predicted(pos(), ChunkChange::new_place(cell(), BlockId(7))));
            assert!(cache.push_predicted(pos(), ChunkChange::new_remove(cell())));
        }

        app.update();

        let messages = app.world().resource::<Messages<ChunkPredicted>>();
        let mut reader = messages.get_cursor();
        let collected: Vec<_> = reader.read(messages).cloned().collect();
        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0].pos, pos());
        assert!(matches!(collected[0].change, ChunkChange::Place { .. }));
        assert!(matches!(collected[1].change, ChunkChange::Remove { .. }));
    }

    #[test]
    fn emit_predicted_events_drains_buffer() {
        let mut app = build_app();
        {
            let mut cache = app.world_mut().resource_mut::<ChunkCache>();
            cache.insert(Chunk::new(pos()));
            assert!(cache.push_predicted(pos(), ChunkChange::new_remove(cell())));
        }
        app.update();
        let cache = app.world().resource::<ChunkCache>();
        assert_eq!(cache.pending_predicted_events.len(), 0);
    }
}
