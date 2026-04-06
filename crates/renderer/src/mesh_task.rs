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

use bevy::prelude::*;
use bevy::tasks::Task;
use dd40_core::chunk::ChunkPos;

use crate::lod::LodLevel;

// ── MeshData ──────────────────────────────────────────────────────────────────

/// The raw output produced by an off-thread chunk meshing task.
///
/// Returned by the [`Task`] spawned in [`systems::spawn_mesh_tasks`] and
/// consumed by [`systems::apply_mesh_tasks`] to upload the mesh to the GPU.
///
/// # All-air chunks
///
/// When the chunk contains only air (or all faces are fully occluded) no
/// geometry is produced.  In that case `mesh` is `None` and `apply_mesh_tasks`
/// will skip spawning a mesh entity for this chunk.
///
/// [`systems::spawn_mesh_tasks`]: crate::systems::spawn_mesh_tasks
/// [`systems::apply_mesh_tasks`]: crate::systems::apply_mesh_tasks
pub struct MeshData {
    /// The chunk whose mesh was built.
    pub pos: ChunkPos,
    /// The LOD level at which the mesh was built.
    ///
    /// Stored so that [`ChunkRenderState`] can be updated with the correct
    /// level after the task completes.
    ///
    /// [`ChunkRenderState`]: crate::render_state::ChunkRenderState
    pub lod: LodLevel,
    /// The finished mesh, or `None` when the chunk produced no visible
    /// geometry (all-air or fully occluded).
    pub mesh: Option<Mesh>,
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
}
