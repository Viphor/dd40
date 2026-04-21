# dd40_renderer

Greedy-mesh chunk renderer for dd40. Listens for `ChunkReady` messages, builds
optimised 3-D meshes off the main thread using face culling and greedy meshing,
manages level-of-detail (LOD), and uploads the finished meshes into Bevy's
asset system.

Depends only on `dd40_core`. The renderer is entirely driven by the chunk
message pipeline; swapping it for a different renderer requires only changing
which plugin the client adds.

**Note:** `dd40_renderer` currently also depends on `dd40_player` in order to
read the player's position for LOD distance calculations. This is an
architectural inconsistency — see `INCONSISTENCIES.md`.

## Module overview

```
src/
├── lib.rs           — RendererPlugin, crate-level docs and quick-start
├── systems.rs       — Bevy systems: chunk dirty tracking, task spawning, task application
├── chunk_mesh.rs    — Per-chunk meshing orchestrator; drives face culling + greedy meshing
├── face_culling.rs  — Determines which of a block's 6 faces are visible
├── greedy_mesh.rs   — Merges adjacent same-type visible faces into maximal quads
├── mesh_builder.rs  — Converts merged quads into a Bevy Mesh (positions, normals, UVs, colors)
├── mesh_task.rs     — MeshData output type and PendingMeshTasks queue
├── lod.rs           — LodLevel enum, LodConfig resource, Chebyshev-distance selection
└── render_state.rs  — Per-chunk RenderState: mesh entity handles and dirty flags
```
