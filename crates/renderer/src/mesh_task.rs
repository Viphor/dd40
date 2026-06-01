//! Types for off-thread chunk mesh building.
//!
//! The async meshing pipeline is split into two stages:
//!
//! 1. **Spawn** ([`systems::spawn_mesh_tasks`]) — for each dirty chunk, clone
//!    the chunk data and pre-collect the block colors needed for meshing, then
//!    dispatch a [`bevy::tasks::Task`] on the [`AsyncComputeTaskPool`].
//! 2. **Apply** ([`systems::apply_mesh_tasks`]) — poll completed tasks each
//!    frame, upload finished meshes to [`Assets<Mesh>`], and spawn/update
//!    mesh entities.
//!
//! This module owns the two shared types that connect those two systems:
//! [`MeshData`] (the task output) and [`PendingMeshTasks`] (the task queue).
//!
//! [`AsyncComputeTaskPool`]: bevy::tasks::AsyncComputeTaskPool
//! [`systems::spawn_mesh_tasks`]: crate::systems::spawn_mesh_tasks
//! [`systems::apply_mesh_tasks`]: crate::systems::apply_mesh_tasks

use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy::tasks::Task;
use dd40_core::chunk::ChunkPos;

use crate::lod::LodLevel;

// ── MeshData ──────────────────────────────────────────────────────────────────

/// Which material the apply pass should pair a [`ChunkMeshPart`] with.
///
/// Untextured is the colour-only fallback (used today, and when the
/// `textures` feature is off or the atlas is not yet ready).  The
/// textured variants — currently just [`Self::AtlasStatic`] — carry
/// the data the apply pass needs to construct the right
/// [`BlockAtlasMaterial`](crate::textures::material::BlockAtlasMaterial)
/// instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChunkMaterialKind {
    /// Use a shared colour-only `StandardMaterial::default()`.  Vertex
    /// colour drives the appearance.
    Untextured,
    /// Sample the block atlas at `atlas_layer` with the alpha mode
    /// implied by `render_layer`.
    #[cfg(feature = "textures")]
    AtlasStatic {
        /// Which atlas to sample.
        atlas_id: dd40_texture_core::AtlasId,
        /// Array layer.
        atlas_layer: u32,
        /// Composition pass.
        render_layer: dd40_texture_core::RenderLayer,
        /// Whether the per-vertex colour tints the sampled texel.
        tinted: bool,
    },
}

/// One piece of a chunk's mesh + the material it wants.
///
/// A chunk produces one [`ChunkMeshPart`] when the colour-only path is
/// active, or up to one part per bucket when the textured path is
/// active.  Each part becomes a child of the chunk's root entity.
pub struct ChunkMeshPart {
    /// The mesh itself.
    pub mesh: Mesh,
    /// Which material the apply pass should pair it with.
    pub material: ChunkMaterialKind,
}

/// The raw output produced by an off-thread chunk meshing task.
///
/// Returned by the [`Task`] spawned in
/// [`spawn_mesh_tasks`](crate::systems::spawn_mesh_tasks) and consumed
/// by [`apply_mesh_tasks`](crate::systems::apply_mesh_tasks) to upload
/// the meshes to the GPU.
///
/// # All-air chunks
///
/// `parts` is empty when the chunk produced no visible geometry
/// (all-air or fully occluded).
pub struct MeshData {
    /// The chunk whose mesh was built.
    pub pos: ChunkPos,
    /// The LOD level at which the mesh was built.
    pub lod: LodLevel,
    /// One mesh + material per render bucket.  Empty when the chunk
    /// has no visible geometry.
    pub parts: Vec<ChunkMeshPart>,
}

// ── PendingMeshTasks ──────────────────────────────────────────────────────────

/// Bevy resource that holds all in-flight chunk mesh-building tasks.
///
/// [`systems::spawn_mesh_tasks`] pushes a new [`Task<MeshData>`] here for
/// every dirty chunk.  [`systems::apply_mesh_tasks`] polls the Vec each frame,
/// drains completed tasks, and removes them from the list.
///
/// # Ordering guarantee
///
/// The Vec is polled in order but results may complete out of order depending
/// on thread scheduling.  The apply system handles this gracefully by only
/// acting on tasks that report [`Poll::Ready`].
///
/// [`systems::spawn_mesh_tasks`]: crate::systems::spawn_mesh_tasks
/// [`systems::apply_mesh_tasks`]: crate::systems::apply_mesh_tasks
/// [`Poll::Ready`]: std::task::Poll::Ready
#[derive(Resource, Default)]
pub struct PendingMeshTasks {
    /// In-flight tasks.  Each entry is a handle to a background computation
    /// that will eventually yield a [`MeshData`].
    pub tasks: Vec<Task<MeshData>>,
    /// Set of chunk positions that currently have an in-flight task.
    ///
    /// Used by [`spawn_mesh_tasks`] to deduplicate: when a chunk is marked
    /// dirty while a previous mesh task for the same chunk is still
    /// running, the new spawn is skipped (and the dirty flag is left set
    /// so the chunk is retried next frame). Without this guard, multiple
    /// tasks for the same chunk could complete out of order, letting a
    /// stale task overwrite a fresh one and leave a stale mesh on screen.
    ///
    /// Entries are inserted in [`spawn_mesh_tasks`] when a task is
    /// dispatched and removed in [`apply_mesh_tasks`] when the matching
    /// task completes.
    ///
    /// [`spawn_mesh_tasks`]: crate::systems::spawn_mesh_tasks
    /// [`apply_mesh_tasks`]: crate::systems::apply_mesh_tasks
    pub in_flight: HashSet<ChunkPos>,
}
