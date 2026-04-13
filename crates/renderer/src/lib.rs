//! `dd40_renderer` — greedy-mesh chunk renderer for the dd40 voxel game.
//!
//! This crate implements a full rendering pipeline for the world's voxel chunks:
//!
//! 1. **Face culling** ([`face_culling`]) — determines which of a block's six
//!    faces are visible by checking whether the adjacent block is air or
//!    non-solid.
//! 2. **Greedy meshing** ([`greedy_mesh`]) — merges adjacent same-type visible
//!    faces on each axis-aligned slice into maximal rectangles, dramatically
//!    reducing triangle count.
//! 3. **Mesh building** ([`mesh_builder`]) — converts the merged quads into a
//!    Bevy [`Mesh`] with positions, normals, UVs, and vertex colors.
//! 4. **Chunk orchestration** ([`chunk_mesh`]) — drives face culling and greedy
//!    meshing for a full chunk at a configurable [`LodLevel`].
//! 5. **LOD support** ([`lod`]) — three detail levels selected by Chebyshev
//!    distance from the player, configured via [`LodConfig`].
//! 6. **Render state** ([`render_state`]) — tracks per-chunk mesh entities and
//!    dirty flags so meshes are only rebuilt when data changes.
//! 7. **Async mesh tasks** ([`mesh_task`]) — [`MeshData`] output type and
//!    [`PendingMeshTasks`] queue that connect the two-system async pipeline.
//! 8. **Bevy systems** ([`systems`]) — wires everything together: listens for
//!    [`ChunkReady`] messages, updates LOD levels, spawns off-thread meshing
//!    tasks, and applies completed tasks to the ECS.
//!
//! # Quick start
//!
//! Add [`RendererPlugin`] to your Bevy [`App`] after [`CorePlugin`]:
//!
//! ```ignore
//! use bevy::prelude::*;
//! use dd40_core::plugin::CorePlugin;
//! use dd40_renderer::RendererPlugin;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .add_plugins(CorePlugin)
//!         .add_plugins(RendererPlugin)
//!         .run();
//! }
//! ```
//!
//! # LOD configuration
//!
//! LOD distance thresholds can be overridden by inserting a [`LodConfig`]
//! resource **before** [`RendererPlugin`] is added, or mutating it at runtime:
//!
//! ```ignore
//! app.insert_resource(dd40_renderer::lod::LodConfig {
//!     lod1_distance: 6,
//!     lod2_distance: 12,
//! });
//! ```
//!
//! # Async meshing pipeline
//!
//! Mesh building is performed off the main thread via Bevy's
//! [`AsyncComputeTaskPool`].  Two systems collaborate:
//!
//! - [`spawn_mesh_tasks`] — runs in [`RebuildChunksSet`], iterates dirty
//!   chunks, clones chunk data and pre-collected block colors, and spawns a
//!   [`Task<MeshData>`] for each.
//! - [`apply_mesh_tasks`] — also in [`RebuildChunksSet`] (after
//!   [`spawn_mesh_tasks`]), polls the [`PendingMeshTasks`] queue and uploads
//!   any finished meshes to [`Assets<Mesh>`].
//!
//! [`CorePlugin`]: dd40_core::plugin::CorePlugin
//! [`ChunkReady`]: dd40_core::chunk::events::ChunkReady
//! [`LodLevel`]: crate::lod::LodLevel
//! [`LodConfig`]: crate::lod::LodConfig
//! [`MeshData`]: crate::mesh_task::MeshData
//! [`PendingMeshTasks`]: crate::mesh_task::PendingMeshTasks
//! [`Mesh`]: bevy::render::mesh::Mesh
//! [`AsyncComputeTaskPool`]: bevy::tasks::AsyncComputeTaskPool
//! [`spawn_mesh_tasks`]: crate::systems::spawn_mesh_tasks
//! [`apply_mesh_tasks`]: crate::systems::apply_mesh_tasks

pub mod chunk_mesh;
pub mod face_culling;
pub mod greedy_mesh;
pub mod lod;
pub mod mesh_builder;
pub mod mesh_task;
pub mod render_state;
pub mod systems;

use bevy::prelude::*;
use dd40_core::prelude::{AppState, GameState};

use lod::LodConfig;
use mesh_task::PendingMeshTasks;
use render_state::ChunkRenderState;
use systems::{
    apply_mesh_tasks, mark_dirty_on_block_change, mark_dirty_on_chunk_ready, spawn_mesh_tasks,
    update_lod_levels,
};

/// System set label for the LOD update pass.
///
/// [`update_lod_levels`] runs in this set, before [`RebuildChunksSet`].
/// Insert custom systems that modify [`ChunkRenderState`] into this set if
/// they need to influence the rebuild pass in the same frame.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct UpdateLodSet;

/// System set label for the mesh rebuild pass.
///
/// Both [`spawn_mesh_tasks`] and [`apply_mesh_tasks`] run in this set (in that
/// order), after [`UpdateLodSet`].
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct RebuildChunksSet;

/// Bevy plugin that registers all renderer systems and resources.
///
/// # Requirements
///
/// [`CorePlugin`] (from `dd40_core`) must be added before this plugin, because:
/// - [`BlockRegistry`] must exist as a resource.
/// - [`ChunkCache`] must exist as a resource (added by [`ChunkCachePlugin`]).
/// - The [`ChunkReady`] message type must be registered.
///
/// # What this plugin adds
///
/// - [`ChunkRenderState`] resource (default-initialized).
/// - [`PendingMeshTasks`] resource (default-initialized).
/// - [`LodConfig`] resource (default-initialized, unless already present).
/// - [`mark_dirty_on_chunk_ready`] in `PreUpdate`.
/// - [`update_lod_levels`] in `Update` (inside [`UpdateLodSet`]).
/// - [`spawn_mesh_tasks`] in `Update` (inside [`RebuildChunksSet`], after
///   [`UpdateLodSet`]).
/// - [`apply_mesh_tasks`] in `Update` (inside [`RebuildChunksSet`], after
///   [`spawn_mesh_tasks`]).
///
/// All `Update` systems run only while in [`AppState::Playing`] and
/// [`GameState::Running`].
///
/// [`CorePlugin`]: dd40_core::plugin::CorePlugin
/// [`BlockRegistry`]: dd40_core::block::BlockRegistry
/// [`ChunkCache`]: dd40_core::chunk::cache::ChunkCache
/// [`ChunkCachePlugin`]: dd40_core::chunk::cache::ChunkCachePlugin
/// [`ChunkReady`]: dd40_core::chunk::events::ChunkReady
pub struct RendererPlugin;

impl Plugin for RendererPlugin {
    fn build(&self, app: &mut App) {
        // Insert resources only if they haven't been added already so the
        // caller can override them before adding the plugin.
        app.init_resource::<ChunkRenderState>();
        app.init_resource::<PendingMeshTasks>();

        if !app.world().contains_resource::<LodConfig>() {
            app.insert_resource(LodConfig::default());
        }

        // Configure system set ordering within Update.
        app.configure_sets(
            Update,
            (UpdateLodSet, RebuildChunksSet.after(UpdateLodSet))
                .run_if(in_state(AppState::Playing).and(in_state(GameState::Running))),
        );

        // PreUpdate: react to new chunk data as early as possible.
        app.add_systems(
            PreUpdate,
            (mark_dirty_on_chunk_ready, mark_dirty_on_block_change),
        );

        // Update: LOD recalculation then async mesh task dispatch and apply.
        app.add_systems(
            Update,
            update_lod_levels
                .in_set(UpdateLodSet)
                .run_if(in_state(AppState::Playing).and(in_state(GameState::Running))),
        );
        app.add_systems(
            Update,
            (spawn_mesh_tasks, apply_mesh_tasks)
                .chain()
                .in_set(RebuildChunksSet)
                .run_if(in_state(AppState::Playing).and(in_state(GameState::Running))),
        );
    }
}
