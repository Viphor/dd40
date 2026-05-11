use std::collections::VecDeque;
use std::fmt::Display;

use bevy::{
    ecs::component::Component,
    math::Vec3,
    prelude::{Deref, DerefMut},
    reflect::Reflect,
    transform::components::Transform,
};
use serde::{Deserialize, Serialize, ser::SerializeTuple};

use crate::block::{Block, BlockCoord, BlockId, BlockPos};

pub mod authority;
pub mod cache;
pub mod change;
pub mod config;
pub mod events;

pub use authority::{
    ChunkAuthorityAppExt, ChunkAuthorityPlugin, ChunkAuthoritySet,
    PendingChunkRejections, RejectReason, commit_predicted_changes,
    default_block_registry_validator,
};
pub use change::{BlockLocal, ChunkChange};
pub use config::MaxDeltaBehind;

/// Width (X) of a chunk in blocks.
pub const CHUNK_SIZE_X: usize = 16;
/// Height (Y) of a chunk in blocks.
pub const CHUNK_SIZE_Y: usize = 256;
/// Depth (Z) of a chunk in blocks.
pub const CHUNK_SIZE_Z: usize = 16;
/// Number of blocks in a chunk.
pub const CHUNK_SIZE: usize = CHUNK_SIZE_X * CHUNK_SIZE_Y * CHUNK_SIZE_Z;

/// Position of a chunk in the world, using chunk coordinates.
///
/// `x` and `z` index the horizontal chunk grid as you would expect.
/// `y` indexes the **vertical** chunk grid: today every chunk spans the
/// full build height so `y` is always `0`, but the field is reserved so
/// that a future split into 16×N×16 cubic chunks (for near-infinite build
/// height) is a non-breaking change to the on-disk format and the
/// network protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component, Reflect, Serialize, Deserialize)]
pub struct ChunkPos {
    pub x: BlockCoord,
    pub y: BlockCoord,
    pub z: BlockCoord,
}

impl ChunkPos {
    /// Constructs a chunk position from its three chunk-grid coordinates.
    ///
    /// All current callers pass `y = 0` because the world is a single
    /// vertical column today; the parameter is exposed so callers think
    /// about the third axis explicitly when (and if) it starts being used.
    pub fn new(x: BlockCoord, y: BlockCoord, z: BlockCoord) -> Self {
        Self { x, y, z }
    }

    /// Convert a chunk-local coordinate to a world-space [`BlockPos`].
    ///
    /// This is the inverse of [`BlockPos::chunk_pos`] +
    /// [`BlockPos::chunk_local`]: feeding the local coord of any block
    /// inside this chunk back through `block_pos` reconstructs its
    /// world-space position.
    ///
    /// Today `self.y` is always `0` and `local.y` carries the full
    /// world-Y in the range `[0, CHUNK_SIZE_Y)`, so the reconstruction
    /// reduces to `local.y`. The general formula
    /// `self.y * CHUNK_SIZE_Y + local.y` is forward-compatible with a
    /// future Y-split where `local.y` is bounded by the chunk's vertical
    /// extent instead of the entire build height.
    #[inline]
    pub fn block_pos(self, local: BlockLocal) -> BlockPos {
        BlockPos::new(
            self.x * (CHUNK_SIZE_X as BlockCoord) + local.x as BlockCoord,
            self.y * (CHUNK_SIZE_Y as BlockCoord) + local.y as BlockCoord,
            self.z * (CHUNK_SIZE_Z as BlockCoord) + local.z as BlockCoord,
        )
    }
}

impl Display for ChunkPos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {})", self.x, self.y, self.z)
    }
}

impl From<&BlockPos> for ChunkPos {
    fn from(value: &BlockPos) -> Self {
        Self {
            x: value.x.div_euclid(CHUNK_SIZE_X as BlockCoord),
            y: value.y.div_euclid(CHUNK_SIZE_Y as BlockCoord),
            z: value.z.div_euclid(CHUNK_SIZE_Z as BlockCoord),
        }
    }
}

impl From<BlockPos> for ChunkPos {
    fn from(value: BlockPos) -> Self {
        Self::from(&value)
    }
}

impl From<&Transform> for ChunkPos {
    fn from(value: &Transform) -> Self {
        Self {
            x: value.translation.x.div_euclid(CHUNK_SIZE_X as f32) as BlockCoord,
            y: value.translation.y.div_euclid(CHUNK_SIZE_Y as f32) as BlockCoord,
            z: value.translation.z.div_euclid(CHUNK_SIZE_Z as f32) as BlockCoord,
        }
    }
}

impl From<&Vec3> for ChunkPos {
    fn from(value: &Vec3) -> Self {
        Self {
            x: value.x.div_euclid(CHUNK_SIZE_X as f32) as BlockCoord,
            y: value.y.div_euclid(CHUNK_SIZE_Y as f32) as BlockCoord,
            z: value.z.div_euclid(CHUNK_SIZE_Z as f32) as BlockCoord,
        }
    }
}

#[derive(Clone, DerefMut, Deref)]
struct ChunkData([Block; CHUNK_SIZE]);

impl Serialize for ChunkData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_tuple(CHUNK_SIZE)?;
        for block in &self.0 {
            seq.serialize_element(block)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for ChunkData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ChunkDataVisitor;

        impl<'de> serde::de::Visitor<'de> for ChunkDataVisitor {
            type Value = ChunkData;

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut blocks = [Block::default(); CHUNK_SIZE];
                for i in 0..CHUNK_SIZE {
                    blocks[i] = seq
                        .next_element()?
                        .ok_or_else(|| serde::de::Error::invalid_length(i, &self))?;
                }
                Ok(ChunkData(blocks))
            }

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(
                    formatter,
                    "expecting to find {} number of blocks",
                    CHUNK_SIZE
                )
            }
        }

        deserializer.deserialize_tuple(CHUNK_SIZE, ChunkDataVisitor)
    }
}

/// A chunk-sized slab of block data, optionally populated.
///
/// The flat array is indexed as:
///   `index = local_x + local_z * CHUNK_SIZE_X + local_y * CHUNK_SIZE_X * CHUNK_SIZE_Z`
///
/// # Versioning
///
/// Every chunk carries a monotonically-increasing `version: u64` that the
/// authoritative server bumps once per applied [`ChunkChange`]. Clients use
/// the version to detect missed updates and request deltas via
/// `RequestChunk { current_version }`.
///
/// # Predicted vs confirmed
///
/// `predicted` is a queue of locally-issued, optimistic changes that have
/// not yet been acknowledged by the server. They are **applied immediately
/// to `data`** at push time so the local renderer reflects the optimistic
/// state, and dropped on rejection (which restores `data` from
/// `confirmed_history`).
///
/// `confirmed_history` is the authoritative log, paired with the version
/// each change produced. It is uncapped in memory; the [`ChunkCache`]
/// drops it on eviction. Storage backends may persist it (see
/// `dd40_chunk_storage`).
///
/// # World-position independence
///
/// `position` is **only** cache metadata used by the outer
/// `HashMap<ChunkPos, Chunk>` lookup. No change-log API or block accessor
/// reads it. A chunk can be physically moved to a new `ChunkPos` without
/// rewriting any of its inner data.
///
/// [`ChunkCache`]: crate::chunk::cache::ChunkCache
#[derive(Clone, Serialize, Deserialize)]
pub struct Chunk {
    position: ChunkPos,
    data: Box<ChunkData>,
    /// Server-authoritative monotonically increasing version. `0` means
    /// "freshly constructed, never authoritatively committed" — it is the
    /// signal a client uses to request a snapshot.
    version: u64,
    /// Confirmed change log, paired with the version each change produced.
    /// Uncapped in memory. Persisted only by storage backends that opt in.
    confirmed_history: VecDeque<(u64, ChunkChange)>,
    /// Runtime-only queue of locally-predicted changes paired with the
    /// pre-prediction value at the target cell. The pre-prediction value
    /// lets the authoritative server validate against the *original* state
    /// (not the optimistically-mutated `data`) and lets rejections roll
    /// back the cell precisely.
    ///
    /// Skipped by serde so it never crosses the wire or reaches disk.
    #[serde(skip)]
    predicted: VecDeque<PredictedChange>,
}

/// A locally-predicted change paired with the pre-prediction value at its
/// target cell.
///
/// `prior` is the block that occupied `change.local()` immediately before
/// `push_predicted` overwrote it. The server's commit pass uses this to
/// validate the change against the cell's true confirmed state — not the
/// optimistically-mutated `data`. On rejection, `prior` is written back.
#[derive(Debug, Clone, Copy)]
pub struct PredictedChange {
    /// The change the caller pushed.
    pub change: ChunkChange,
    /// Block that occupied the target cell before the change was applied.
    pub prior: Block,
}

impl Chunk {
    /// Creates a new chunk at `position`, pre-filled with `Block::default()` (air).
    ///
    /// The new chunk has `version = 0` and empty history / predicted queues.
    pub fn new(position: ChunkPos) -> Self {
        Self {
            position,
            data: Box::new(ChunkData([Block::default(); CHUNK_SIZE])),
            version: 0,
            confirmed_history: VecDeque::new(),
            predicted: VecDeque::new(),
        }
    }

    /// Returns the chunk's position in chunk coordinates.
    pub fn position(&self) -> ChunkPos {
        self.position
    }

    /// Sets the chunk's position metadata. The inner block data is **not**
    /// rewritten — the chunk is world-position independent.
    pub fn set_position(&mut self, position: ChunkPos) {
        self.position = position;
    }

    /// Returns the current chunk version.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Sets the chunk version directly, bypassing the predicted/confirmed
    /// queues.
    ///
    /// This is a low-level escape hatch for systems that author chunks
    /// outside the predict-and-commit flow — world generators (which stamp
    /// version `1` on first emission) and storage backends (which restore
    /// the version they persisted). Anything inside the live predict /
    /// commit pipeline must let [`Chunk::commit_accepted`] manage the
    /// version instead.
    pub fn set_version(&mut self, version: u64) {
        self.version = version;
    }

    /// Returns the queue of locally-predicted changes that have not yet
    /// been confirmed.
    pub fn predicted(&self) -> &VecDeque<PredictedChange> {
        &self.predicted
    }

    /// Returns the confirmed history.
    pub fn confirmed_history(&self) -> &VecDeque<(u64, ChunkChange)> {
        &self.confirmed_history
    }

    /// Appends `(version, change)` to the confirmed history without
    /// touching `data` or the chunk's own `version` field.
    ///
    /// This is a low-level escape hatch for storage backends that need to
    /// rehydrate a persisted history alongside an already-loaded data
    /// array. Inside the live predict / commit pipeline, history grows
    /// only via [`Chunk::commit_accepted`].
    pub fn push_confirmed_for_load(&mut self, version: u64, change: ChunkChange) {
        self.confirmed_history.push_back((version, change));
    }

    /// Returns the block at chunk-local coordinates, or `None` when the
    /// coordinates are out of range.
    pub fn get(&self, lx: usize, ly: usize, lz: usize) -> Option<Block> {
        if lx >= CHUNK_SIZE_X || ly >= CHUNK_SIZE_Y || lz >= CHUNK_SIZE_Z {
            return None;
        }
        Some(self.data[Self::index(lx, ly, lz)])
    }

    /// Returns the block at the chunk-local position `local`.
    pub fn get_local(&self, local: BlockLocal) -> Block {
        self.data[Self::index(local.x as usize, local.y as usize, local.z as usize)]
    }

    pub fn get_global(&self, pos: BlockPos) -> Option<Block> {
        if pos.x >= self.position.x
            || pos.x < self.position.x + CHUNK_SIZE_X as BlockCoord
            || pos.y >= CHUNK_SIZE_Y as BlockCoord
            || pos.z >= self.position.z
            || pos.z < self.position.z + CHUNK_SIZE_Z as BlockCoord
        {
            return None;
        };

        let local = pos.chunk_local();
        self.get(local.x as usize, local.y as usize, local.z as usize)
    }

    /// Sets the block at chunk-local coordinates. Does nothing when the
    /// coordinates are out of range.
    ///
    /// **Direct writes bypass the predicted/confirmed queues.** Prefer
    /// [`Chunk::push_predicted`] for any change that should be reconciled
    /// with the authoritative server. Direct `set` is appropriate only
    /// for world generation, snapshot loading, and rollback.
    pub fn set(&mut self, lx: usize, ly: usize, lz: usize, block: Block) {
        if lx >= CHUNK_SIZE_X || ly >= CHUNK_SIZE_Y || lz >= CHUNK_SIZE_Z {
            return;
        }
        self.data[Self::index(lx, ly, lz)] = block;
    }

    /// Sets the block at the chunk-local position `local`, bypassing the
    /// predicted/confirmed queues. See [`Chunk::set`].
    pub fn set_local(&mut self, local: BlockLocal, block: Block) {
        self.data[Self::index(local.x as usize, local.y as usize, local.z as usize)] = block;
    }

    /// Pushes a locally-predicted change onto the chunk and applies it to
    /// `data` immediately so the local renderer reflects the optimistic
    /// state.
    ///
    /// The pre-prediction value at the target cell is captured in the
    /// queue entry so the authoritative server can validate against the
    /// original state and roll back on rejection.
    ///
    /// The change is **not** added to `confirmed_history` and the chunk
    /// `version` is **not** bumped — both happen only when the server's
    /// authoritative commit pass acknowledges the change (or rejects it).
    ///
    /// # Prefer [`ChunkCache::push_predicted`]
    ///
    /// This is a low-level entry point that does **not** mark the chunk
    /// dirty in the [`ChunkCache`]'s dirty index. The chunk-authority
    /// commit pass relies on that index to know which chunks to walk —
    /// any prediction queued via this path will be invisible to it.
    ///
    /// External callers should use
    /// [`ChunkCache::push_predicted`](crate::chunk::cache::ChunkCache::push_predicted)
    /// instead, which forwards here and updates the dirty index in the
    /// same call. This method exists only for the cache's own use and a
    /// handful of unit tests that operate on standalone chunks.
    pub fn push_predicted(&mut self, change: ChunkChange) {
        let local = change.local();
        let prior = self.get_local(local);
        self.apply_change_to_data(&change);
        self.predicted.push_back(PredictedChange { change, prior });
    }

    /// Drains every queued predicted change without touching `data`,
    /// `version`, or `confirmed_history`. Returns the entries in
    /// FIFO order.
    pub fn take_predicted(&mut self) -> Vec<PredictedChange> {
        self.predicted.drain(..).collect()
    }

    /// Server-side: commit every predicted change as confirmed.
    ///
    /// Drains the predicted queue, bumps `version` once per change, and
    /// appends `(new_version, change)` to `confirmed_history`. Returns the
    /// committed `(version, change)` pairs in commit order so callers can
    /// broadcast a single `ChunkUpdate` per chunk.
    ///
    /// **Does no validation.** Callers that need validation should use
    /// [`Self::take_predicted`] + [`Self::commit_accepted`] instead.
    pub fn commit_predicted_all(&mut self) -> Vec<(u64, ChunkChange)> {
        let mut committed = Vec::with_capacity(self.predicted.len());
        while let Some(entry) = self.predicted.pop_front() {
            self.version += 1;
            self.confirmed_history.push_back((self.version, entry.change));
            committed.push((self.version, entry.change));
        }
        committed
    }

    /// Server-side: commit a pre-validated list of changes that are already
    /// reflected in `data` (because they were predicted and accepted).
    /// Bumps `version` once per change and appends to `confirmed_history`.
    ///
    /// Returns the committed `(version, change)` pairs in commit order.
    pub fn commit_accepted(&mut self, accepted: &[ChunkChange]) -> Vec<(u64, ChunkChange)> {
        let mut committed = Vec::with_capacity(accepted.len());
        for change in accepted {
            self.version += 1;
            self.confirmed_history.push_back((self.version, *change));
            committed.push((self.version, *change));
        }
        committed
    }

    /// Restores the cell at `local` to `prior`. Used by the server commit
    /// pass after a predicted change is rejected.
    pub fn rollback_to(&mut self, local: BlockLocal, prior: Block) {
        self.set_local(local, prior);
    }

    /// Client-side: apply a batch of confirmed changes coming from the
    /// server.
    ///
    /// `base_version` must equal the chunk's current `version`; otherwise
    /// the call returns `false` without mutating anything (the caller
    /// should re-request the chunk to resync). On success, each change is
    /// written to `data`, appended to `confirmed_history`, and `version`
    /// is bumped per change. Matching predictions are **not** removed
    /// here; the network reconciliation system handles that so it can
    /// emit `PredictionRejected` for the leftovers.
    pub fn apply_confirmed_changes(
        &mut self,
        base_version: u64,
        changes: &[ChunkChange],
    ) -> bool {
        if base_version != self.version {
            return false;
        }
        for change in changes {
            self.apply_change_to_data(change);
            self.version += 1;
            self.confirmed_history.push_back((self.version, *change));
        }
        true
    }

    /// Returns every confirmed change strictly after `version`, in commit
    /// order. Returns `None` when `version` is below the oldest entry in
    /// `confirmed_history` — meaning the history was truncated by eviction
    /// or the chunk was reloaded fresh, and the caller should send a full
    /// snapshot instead.
    ///
    /// `version == self.version` returns `Some(empty)` (caller is up to date).
    /// `version > self.version` returns `Some(empty)` and the caller is
    /// responsible for warning + snapshot fallback.
    pub fn history_since(&self, version: u64) -> Option<Vec<(u64, ChunkChange)>> {
        if version >= self.version {
            return Some(Vec::new());
        }
        match self.confirmed_history.front() {
            // `front().0` is the oldest version still in memory; if the
            // caller is even older we can't deliver a delta.
            Some(&(oldest, _)) if version + 1 < oldest => None,
            Some(_) => Some(
                self.confirmed_history
                    .iter()
                    .filter(|(v, _)| *v > version)
                    .copied()
                    .collect(),
            ),
            // Empty history with version > 0 only happens when the chunk was
            // loaded from non-versioned storage. We can't deliver a delta.
            None => None,
        }
    }

    /// Applies a single change to `data` without touching version/history/predicted.
    /// Out-of-range writes are silently dropped (mirrors [`Chunk::set`]).
    fn apply_change_to_data(&mut self, change: &ChunkChange) {
        match *change {
            ChunkChange::Place { local, block_id } | ChunkChange::Replace { local, new_block: block_id } => {
                self.set_local(local, Block::new(block_id));
            }
            ChunkChange::Remove { local } => {
                self.set_local(local, Block::new(BlockId::AIR));
            }
        }
    }

    /// Converts chunk-local coordinates to a flat array index.
    #[inline(always)]
    fn index(lx: usize, ly: usize, lz: usize) -> usize {
        lx + lz * CHUNK_SIZE_X + ly * CHUNK_SIZE_X * CHUNK_SIZE_Z
    }
}

impl std::fmt::Debug for Chunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Chunk")
            .field("position", &self.position)
            .field("version", &self.version)
            .field("predicted", &self.predicted.len())
            .field("history", &self.confirmed_history.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockId;

    fn lp(x: u8, y: u16, z: u8) -> BlockLocal {
        BlockLocal::new(x, y, z)
    }

    #[test]
    fn new_chunk_starts_at_version_zero_and_empty_queues() {
        let c = Chunk::new(ChunkPos::new(0, 0, 0));
        assert_eq!(c.version(), 0);
        assert!(c.predicted().is_empty());
        assert!(c.confirmed_history().is_empty());
    }

    #[test]
    fn chunk_pos_block_pos_is_inverse_of_block_pos_chunk_local() {
        // Roundtrip across positive, negative, and chunk-boundary chunks.
        for (cx, cz) in [(0, 0), (3, -2), (-5, -7), (12345, -67890)] {
            let chunk_pos = ChunkPos::new(cx, 0, cz);
            for (lx, ly, lz) in [
                (0u8, 0u16, 0u8),
                (15, 255, 15),
                (7, 64, 11),
            ] {
                let local = BlockLocal::new(lx, ly, lz);
                let world = chunk_pos.block_pos(local);
                assert_eq!(world.chunk_pos(), chunk_pos);
                let local_again = world.chunk_local();
                assert_eq!(local_again.x as u8, lx);
                assert_eq!(local_again.y as u16, ly);
                assert_eq!(local_again.z as u8, lz);
            }
        }
    }

    #[test]
    fn chunk_pos_block_pos_origin() {
        // The (0,0,0) local of chunk (3,5) is at world (48, 0, 80).
        let cp = ChunkPos::new(3, 0, 5);
        let bp = cp.block_pos(BlockLocal::new(0, 0, 0));
        assert_eq!(bp, BlockPos::new(48, 0, 80));
    }

    #[test]
    fn push_predicted_applies_to_data_immediately() {
        let mut c = Chunk::new(ChunkPos::new(0, 0, 0));
        c.push_predicted(ChunkChange::new_place(lp(1, 2, 3), BlockId(7)));
        assert_eq!(c.get_local(lp(1, 2, 3)).block_id, BlockId(7));
        assert_eq!(c.predicted().len(), 1);
        assert_eq!(c.version(), 0); // not yet committed
    }

    #[test]
    fn commit_predicted_all_drains_and_bumps_version() {
        let mut c = Chunk::new(ChunkPos::new(0, 0, 0));
        c.push_predicted(ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));
        c.push_predicted(ChunkChange::new_place(lp(1, 0, 0), BlockId(2)));

        let committed = c.commit_predicted_all();

        assert_eq!(committed.len(), 2);
        assert_eq!(committed[0].0, 1);
        assert_eq!(committed[1].0, 2);
        assert_eq!(c.version(), 2);
        assert!(c.predicted().is_empty());
        assert_eq!(c.confirmed_history().len(), 2);
    }

    #[test]
    fn apply_confirmed_changes_rejects_mismatched_base_version() {
        let mut c = Chunk::new(ChunkPos::new(0, 0, 0));
        c.push_predicted(ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));
        c.commit_predicted_all();
        // c.version == 1
        let bad = c.apply_confirmed_changes(0, &[ChunkChange::new_remove(lp(0, 0, 0))]);
        assert!(!bad);
        assert_eq!(c.version(), 1);
    }

    #[test]
    fn apply_confirmed_changes_writes_data_and_appends_history() {
        let mut c = Chunk::new(ChunkPos::new(0, 0, 0));
        let ok = c.apply_confirmed_changes(
            0,
            &[
                ChunkChange::new_place(lp(0, 0, 0), BlockId(1)),
                ChunkChange::new_remove(lp(0, 0, 0)),
            ],
        );
        assert!(ok);
        assert_eq!(c.version(), 2);
        assert_eq!(c.get_local(lp(0, 0, 0)).block_id, BlockId::AIR);
        assert_eq!(c.confirmed_history().len(), 2);
    }

    #[test]
    fn history_since_returns_empty_when_caller_is_current() {
        let mut c = Chunk::new(ChunkPos::new(0, 0, 0));
        c.push_predicted(ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));
        c.commit_predicted_all();
        assert_eq!(c.history_since(1).unwrap().len(), 0);
    }

    #[test]
    fn history_since_returns_delta() {
        let mut c = Chunk::new(ChunkPos::new(0, 0, 0));
        c.push_predicted(ChunkChange::new_place(lp(0, 0, 0), BlockId(1)));
        c.push_predicted(ChunkChange::new_place(lp(1, 0, 0), BlockId(2)));
        c.commit_predicted_all();

        let delta = c.history_since(0).unwrap();
        assert_eq!(delta.len(), 2);
        assert_eq!(delta[0].0, 1);
        assert_eq!(delta[1].0, 2);
    }

    #[test]
    fn history_since_returns_none_when_truncated() {
        // Simulate a chunk loaded from non-versioned storage: version > 0,
        // empty history.
        let mut c = Chunk::new(ChunkPos::new(0, 0, 0));
        c.version = 5;
        assert!(c.history_since(0).is_none());
        assert!(c.history_since(3).is_none());
        assert_eq!(c.history_since(5).unwrap().len(), 0);
    }

    #[test]
    fn position_is_metadata_only_data_survives_swap() {
        // Build a chunk at (0,0) with a distinctive block pattern.
        let mut c = Chunk::new(ChunkPos::new(0, 0, 0));
        c.push_predicted(ChunkChange::new_place(lp(2, 64, 5), BlockId(42)));
        c.commit_predicted_all();
        let snapshot_block = c.get_local(lp(2, 64, 5));
        let version = c.version();
        let history_len = c.confirmed_history().len();

        // Physically move the chunk to a different ChunkPos.
        c.set_position(ChunkPos::new(-7, 0, 13));

        // Block data, version, and history must be untouched. The chunk
        // has no global-world knowledge to invalidate.
        assert_eq!(c.get_local(lp(2, 64, 5)), snapshot_block);
        assert_eq!(c.version(), version);
        assert_eq!(c.confirmed_history().len(), history_len);
    }

    #[test]
    fn replace_change_applies_unconditionally() {
        let mut c = Chunk::new(ChunkPos::new(0, 0, 0));
        c.set_local(lp(0, 0, 0), Block::new(BlockId(99)));
        c.push_predicted(ChunkChange::new_replace(lp(0, 0, 0), BlockId(7)));
        assert_eq!(c.get_local(lp(0, 0, 0)).block_id, BlockId(7));
    }
}
