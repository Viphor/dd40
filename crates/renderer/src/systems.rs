//! Bevy systems that drive the chunk renderer each frame.
//!
//! # System overview
//!
//! | System                       | Schedule    | What it does                                           |
//! |------------------------------|-------------|--------------------------------------------------------|
//! [`mark_dirty_on_chunk_ready`]  | `PreUpdate` | Reads `ChunkReady` messages, marks chunks dirty        |
//! [`update_lod_levels`]          | `Update`    | Re-evaluates each chunk's LOD based on player pos      |
//! [`spawn_mesh_tasks`]           | `Update`    | Spawns off-thread meshing tasks for dirty chunks       |
//! [`apply_mesh_tasks`]           | `Update`    | Polls completed tasks, uploads meshes, spawns entities |
//!
//! [`update_lod_levels`] runs before [`spawn_mesh_tasks`], which runs before
//! [`apply_mesh_tasks`], all within [`RebuildChunksSet`].  This guarantees
//! LOD changes are flushed before tasks are spawned, and tasks are spawned
//! before results are consumed.
//!
//! # Off-thread meshing
//!
//! [`spawn_mesh_tasks`] avoids blocking the main thread by dispatching greedy
//! meshing and mesh building to the [`AsyncComputeTaskPool`].  Because
//! [`BlockRegistry`] is not `Send`, the system pre-collects the block colors
//! needed for meshing into a plain `HashMap<BlockId, [f32; 4]>` before
//! spawning the task.
//!
//! # Mesh entity structure
//!
//! Each chunk that has at least one visible face gets a single ECS entity with:
//! - [`Mesh3d`] — a handle to the generated [`Mesh`] asset
//! - [`MeshMaterial3d`] — a handle to a shared [`StandardMaterial`] that uses
//!   vertex colors (no texture, `vertex_colors = true`)
//! - [`Transform`] at the chunk's world-space origin
//! - [`GlobalTransform`]
//! - A [`ChunkMeshMarker`] component for easy querying / cleanup
//!
//! [`AsyncComputeTaskPool`]: bevy::tasks::AsyncComputeTaskPool
//! [`BlockRegistry`]: dd40_core::block::BlockRegistry

use std::collections::HashMap;

use bevy::{
    ecs::message::MessageReader,
    prelude::*,
    tasks::{AsyncComputeTaskPool, block_on, futures_lite::future},
};
use dd40_core::{
    block::{BlockId, BlockRegistry},
    chunk::events::{ChunkChanged, ChunkPredicted, ChunkReady},
    chunk::{ChunkPos, cache::ChunkCache},
};
use dd40_physics_core::prelude::CharacterPosition;

use crate::{
    chunk_mesh::build_chunk_quads,
    lod::{LodConfig, chebyshev_distance},
    mesh_builder::MeshBuilder,
    mesh_task::{MeshData, PendingMeshTasks},
    render_state::ChunkRenderState,
};

// ── Marker component ──────────────────────────────────────────────────────────

/// Marker component placed on every chunk mesh entity spawned by the renderer.
///
/// This allows other systems (and cleanup on despawn) to query specifically for
/// renderer-owned mesh entities.
#[derive(Component, Debug, Clone, Copy)]
pub struct ChunkMeshMarker {
    /// The chunk position this mesh entity belongs to.
    pub chunk_pos: ChunkPos,
}

// ── Systems ───────────────────────────────────────────────────────────────────

/// Reads incoming [`ChunkReady`] messages and marks the corresponding chunks
/// dirty in [`ChunkRenderState`].
///
/// Runs in `PreUpdate` so dirty flags are set before the `Update` rebuild pass.
pub fn mark_dirty_on_chunk_ready(
    mut reader: MessageReader<ChunkReady>,
    mut render_state: ResMut<ChunkRenderState>,
) {
    for msg in reader.read() {
        let pos = msg.chunk.position();
        render_state.mark_dirty(pos);
        trace!(
            "Renderer: marked chunk {:?} dirty (ChunkReady received)",
            pos
        );
    }
}

/// Reads incoming [`ChunkPredicted`] messages and marks the corresponding
/// chunks dirty so the renderer remeshes them with the optimistic state.
///
/// Runs in `PreUpdate` so dirty flags are set before the `Update` rebuild
/// pass.
pub fn mark_dirty_on_chunk_predicted(
    mut reader: MessageReader<ChunkPredicted>,
    mut render_state: ResMut<ChunkRenderState>,
) {
    for msg in reader.read() {
        render_state.mark_dirty(msg.pos);
        trace!(
            "Renderer: marked chunk {:?} dirty (ChunkPredicted, change={:?})",
            msg.pos, msg.change,
        );
    }
}

/// Reads incoming [`ChunkChanged`] messages and marks the corresponding
/// chunks dirty so the renderer remeshes them with the authoritative state.
///
/// Runs in `PreUpdate` alongside [`mark_dirty_on_chunk_predicted`].
pub fn mark_dirty_on_chunk_changed(
    mut reader: MessageReader<ChunkChanged>,
    mut render_state: ResMut<ChunkRenderState>,
) {
    for msg in reader.read() {
        render_state.mark_dirty(msg.pos);
        trace!(
            "Renderer: marked chunk {:?} dirty (ChunkChanged, version={}, {} change(s))",
            msg.pos,
            msg.new_version,
            msg.changes.len(),
        );
    }
}

/// Iterates over every chunk tracked by [`ChunkRenderState`] and updates its
/// [`LodLevel`] based on the nearest physics body's current chunk position.
///
/// When a chunk's LOD level changes the entry is automatically marked dirty by
/// [`ChunkRenderState::update_lod`], so the mesh will be rebuilt this frame.
///
/// If no [`CharacterPosition`] entity exists the system is a no-op.
pub fn update_lod_levels(
    anchor_query: Query<&CharacterPosition>,
    mut render_state: ResMut<ChunkRenderState>,
    lod_config: Res<LodConfig>,
    chunk_cache: Res<ChunkCache>,
) {
    let Ok(anchor) = anchor_query.single() else {
        return;
    };

    let anchor_transform = Transform::from_translation(anchor.0);
    let player_chunk = chunk_pos_from_transform(&anchor_transform);

    // Collect the chunk positions we need to update.  We cannot iterate and
    // mutate `render_state` simultaneously, so we snapshot the positions first.
    let positions: Vec<ChunkPos> = chunk_cache.iter_positions().copied().collect();

    for pos in positions {
        let dist = chebyshev_distance(pos.x, pos.z, player_chunk.x, player_chunk.z);
        let new_lod = lod_config.select(dist);
        render_state.update_lod(pos, new_lod);
    }
}

/// For every dirty chunk, despawns the old mesh entity (on the main thread,
/// which is fast) and spawns an off-thread task on [`AsyncComputeTaskPool`]
/// that runs greedy meshing and builds the [`Mesh`].
///
/// The task handle is pushed into [`PendingMeshTasks`] for [`apply_mesh_tasks`]
/// to poll.  The dirty flag is cleared immediately so the chunk is not
/// re-submitted on the next frame while its task is still in flight.
///
/// # Why colors are pre-collected
///
/// [`BlockRegistry`] is not `Send`, so it cannot be moved into the async task.
/// Instead this system builds a `HashMap<BlockId, [f32; 4]>` of every block
/// color present in the registry before spawning the task.  The task receives
/// this `Send`-safe map and uses [`MeshBuilder::add_quad_with_color`] to avoid
/// any registry lookup off-thread.
///
/// [`AsyncComputeTaskPool`]: bevy::tasks::AsyncComputeTaskPool
/// [`apply_mesh_tasks`]: crate::systems::apply_mesh_tasks
pub fn spawn_mesh_tasks(
    mut render_state: ResMut<ChunkRenderState>,
    mut pending: ResMut<PendingMeshTasks>,
    chunk_cache: Res<ChunkCache>,
    registry: Res<BlockRegistry>,
) {
    // Collect dirty positions up-front to avoid borrow conflicts.
    let dirty: Vec<ChunkPos> = render_state.dirty_chunks().collect();

    if dirty.is_empty() {
        return;
    }

    // Pre-collect all block colors from the registry into a Send-safe map so
    // the async task does not need to reference BlockRegistry directly.
    let color_map: HashMap<BlockId, [f32; 4]> = registry
        .iter()
        .map(|def| {
            let c = def.color.to_linear();
            (def.id, [c.red, c.green, c.blue, c.alpha])
        })
        .collect();

    let task_pool = AsyncComputeTaskPool::get();

    for pos in dirty {
        // If a meshing task for this chunk is already in flight, skip
        // re-spawning. Otherwise multiple tasks for the same chunk could
        // complete out of order and a stale task would overwrite a fresh
        // one. Leave the dirty flag set so we retry next frame after the
        // current task completes.
        if pending.in_flight.contains(&pos) {
            continue;
        }

        // The OLD mesh entity is intentionally left in place — it is
        // despawned in apply_mesh_tasks only once the new mesh is ready.
        // Despawning here would leave a one-or-two-frame gap where the
        // chunk has no geometry on screen, which players see as flicker.

        // --- Look up chunk data ----------------------------------------------
        let Some(chunk) = chunk_cache.get(&pos) else {
            // Chunk data not yet available; leave dirty so we retry next frame.
            continue;
        };

        // Clone the chunk so the task owns its data.
        let chunk = chunk.clone();

        // Snapshot the LOD we are building at.
        let lod = render_state.current_lod(&pos);

        // Clone the color map so each task gets its own copy.
        let color_map = color_map.clone();

        // Pre-collect solidity flags so the task can answer is_solid checks
        // without needing the full BlockRegistry (which is not Send).
        // Every block in the registry that is solid gets an entry here.
        let solid_ids: std::collections::HashSet<dd40_core::block::BlockId> = registry
            .iter()
            .filter(|def| def.is_solid)
            .map(|def| def.id)
            .collect();

        // --- Spawn the meshing task off the main thread ----------------------
        let task = task_pool.spawn(async move {
            let origin_x = (pos.x * 16) as f32;
            let origin_z = (pos.z * 16) as f32;
            // Note: vertices are baked into world space by MeshBuilder using
            // origin_x/origin_z, so the spawned entity must use an identity
            // Transform — translating it again would double-offset every chunk.

            // build_chunk_quads needs a ChunkCache for cross-boundary face
            // culling.  We pass an empty cache here; faces at chunk boundaries
            // will be treated as visible (conservative, correct).  When the
            // neighbouring chunk later loads it will trigger its own rebuild
            // and correct any over-drawn faces.
            let neighbour_cache = dd40_core::chunk::cache::ChunkCache::default();

            // Reconstruct a minimal registry inside the task so that
            // build_chunk_quads can perform is_renderable / is_solid checks.
            // Every block ID in the color_map is a real renderable block;
            // every block ID in solid_ids is solid.
            let mut registry = dd40_core::block::BlockRegistry::new();
            for (&block_id, &color_arr) in &color_map {
                if block_id == dd40_core::block::BlockId::AIR {
                    continue;
                }
                let color = bevy::color::Color::LinearRgba(bevy::color::LinearRgba::new(
                    color_arr[0],
                    color_arr[1],
                    color_arr[2],
                    color_arr[3],
                ));
                let is_solid = solid_ids.contains(&block_id);
                registry.register_without_event(
                    dd40_core::block::BlockDefinition::new(block_id, "")
                        .with_solid(is_solid)
                        .with_renderable(true)
                        .with_color(color),
                );
            }

            let quads = build_chunk_quads(&chunk, lod, &registry, &neighbour_cache);

            let mut builder = MeshBuilder::new(origin_x, origin_z);
            for quad in &quads {
                if let Some(&color) = color_map.get(&quad.block_id) {
                    builder.add_quad_with_color(quad, color);
                }
            }

            MeshData {
                pos,
                lod,
                mesh: builder.build(),
            }
        });

        pending.tasks.push(task);
        pending.in_flight.insert(pos);

        // Clear dirty immediately — the task is now in-flight.
        render_state.clear_dirty(pos);
    }
}

/// Polls [`PendingMeshTasks`] for completed meshing tasks and, for each
/// finished task, uploads the [`Mesh`] to [`Assets<Mesh>`] and spawns (or
/// skips) a new mesh entity.
///
/// Tasks that are not yet complete are left in the queue for the next frame.
///
/// # All-air chunks
///
/// When a task returns `MeshData { mesh: None, .. }` the chunk produced no
/// visible geometry (all-air or fully occluded).  No entity is spawned and the
/// chunk's [`ChunkRenderState`] entry records `mesh_entity = None`.
pub fn apply_mesh_tasks(
    mut commands: Commands,
    mut pending: ResMut<PendingMeshTasks>,
    mut render_state: ResMut<ChunkRenderState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Drain completed tasks, keep pending ones.
    let drained: Vec<_> = pending.tasks.drain(..).collect();
    let mut still_pending: Vec<_> = Vec::with_capacity(drained.len());

    for mut task in drained {
        match block_on(future::poll_once(&mut task)) {
            None => {
                // Task not finished yet — keep it for next frame.
                still_pending.push(task);
            }
            Some(data) => {
                let pos = data.pos;
                pending.in_flight.remove(&pos);

                // Capture the previous mesh entity so we can despawn it
                // *after* the new one is in place. Despawning earlier
                // (e.g. in spawn_mesh_tasks) leaves a one-or-two-frame
                // gap where the chunk has no geometry on screen.
                let old_entity = render_state.mesh_entity(&pos);

                if let Some(mesh) = data.mesh {
                    let mesh_handle = meshes.add(mesh);

                    // Bevy 0.18 automatically enables vertex colors in the PBR
                    // shader when the mesh contains ATTRIBUTE_COLOR — no extra
                    // field needed on the material.
                    let material_handle = materials.add(StandardMaterial::default());

                    let entity = commands
                        .spawn((
                            Name::new(format!("ChunkMesh ({}, {})", pos.x, pos.z)),
                            ChunkMeshMarker { chunk_pos: pos },
                            Mesh3d(mesh_handle),
                            MeshMaterial3d(material_handle),
                            Transform::default(),
                            GlobalTransform::default(),
                        ))
                        .id();

                    render_state.set_mesh_entity(pos, Some(entity));
                } else {
                    // Chunk produced no geometry (all-air / fully occluded).
                    // Clear the stored entity so a future rebuild does not
                    // think a mesh is still associated with this chunk.
                    render_state.set_mesh_entity(pos, None);
                }

                if let Some(old) = old_entity {
                    commands.entity(old).despawn();
                }
            }
        }
    }

    pending.tasks = still_pending;
}

// ── Helper ────────────────────────────────────────────────────────────────────

/// Converts a world-space [`Transform`] to the [`ChunkPos`] it falls inside.
fn chunk_pos_from_transform(transform: &Transform) -> ChunkPos {
    use dd40_core::block::BlockPos;
    let bp = BlockPos::from(transform);
    bp.chunk_pos()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::tasks::TaskPool;
    use dd40_core::block::BlockRegistry;
    use dd40_core::chunk::Chunk;

    /// Regression: marking a chunk dirty while a previous mesh task for the
    /// same chunk is still in flight must not spawn a second task. Without
    /// this guard, two tasks for the same chunk could complete out of order
    /// and a stale task would overwrite a fresh one — the symptom users see
    /// as "block disappears from collision/targeting but the mesh is stale".
    #[test]
    fn spawn_skips_chunk_with_in_flight_task() {
        AsyncComputeTaskPool::get_or_init(TaskPool::default);

        let mut app = App::new();
        app.init_resource::<ChunkRenderState>();
        app.init_resource::<PendingMeshTasks>();
        app.init_resource::<BlockRegistry>();

        let mut cache = ChunkCache::default();
        let pos = ChunkPos::new(0, 0, 0);
        cache.insert(Chunk::new(pos));
        app.insert_resource(cache);

        // First dirty + spawn → one task in flight.
        app.world_mut()
            .resource_mut::<ChunkRenderState>()
            .mark_dirty(pos);
        app.add_systems(Update, spawn_mesh_tasks);
        app.update();

        let pending = app.world().resource::<PendingMeshTasks>();
        assert_eq!(pending.tasks.len(), 1, "first spawn should dispatch a task");
        assert!(
            pending.in_flight.contains(&pos),
            "in_flight should track the dispatched task"
        );

        // Mark dirty again before the first task can complete.
        app.world_mut()
            .resource_mut::<ChunkRenderState>()
            .mark_dirty(pos);
        app.update();

        let pending = app.world().resource::<PendingMeshTasks>();
        assert_eq!(
            pending.tasks.len(),
            1,
            "second spawn must not dispatch a duplicate task while one is in flight"
        );

        // Dirty bit must remain set so the chunk is retried after the
        // current task finishes.
        let render_state = app.world().resource::<ChunkRenderState>();
        let dirty: Vec<ChunkPos> = render_state.dirty_chunks().collect();
        assert_eq!(
            dirty,
            vec![pos],
            "dirty bit must remain set when spawn was skipped"
        );
    }

    /// Regression: dispatching a fresh mesh task for an already-meshed
    /// chunk must not despawn or clear its existing mesh entity. Only
    /// `apply_mesh_tasks` may despawn the old entity, after the new one
    /// is in place. Without this, players see a one-or-two-frame flicker
    /// where the chunk vanishes entirely while the async task runs.
    #[test]
    fn spawn_does_not_clear_existing_mesh_entity() {
        AsyncComputeTaskPool::get_or_init(TaskPool::default);

        let mut app = App::new();
        app.init_resource::<ChunkRenderState>();
        app.init_resource::<PendingMeshTasks>();
        app.init_resource::<BlockRegistry>();

        let mut cache = ChunkCache::default();
        let pos = ChunkPos::new(0, 0, 0);
        cache.insert(Chunk::new(pos));
        app.insert_resource(cache);

        let placeholder = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<ChunkRenderState>()
            .set_mesh_entity(pos, Some(placeholder));

        app.world_mut()
            .resource_mut::<ChunkRenderState>()
            .mark_dirty(pos);
        app.add_systems(Update, spawn_mesh_tasks);
        app.update();

        let render_state = app.world().resource::<ChunkRenderState>();
        assert_eq!(
            render_state.mesh_entity(&pos),
            Some(placeholder),
            "spawn must preserve the existing mesh entity to avoid flicker"
        );
    }
}
