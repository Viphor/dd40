# dd40_renderer

Greedy-mesh chunk renderer for dd40. Listens for `ChunkReady` messages, builds
optimised 3-D meshes off the main thread using face culling and greedy meshing,
manages level-of-detail (LOD), and uploads the finished meshes into Bevy's
asset system.

LOD distance is anchored to `CharacterPosition` from `dd40_physics_core`.
Swapping this renderer requires only changing which plugin the client adds.

## Module overview

```
src/
├── lib.rs           — RendererPlugin, crate-level docs and quick-start
├── systems.rs       — chunk dirty tracking, task spawning, task application
├── chunk_mesh.rs    — per-chunk meshing orchestrator; drives face culling + greedy meshing
├── face_culling.rs  — determines which of a block's 6 faces are visible
├── greedy_mesh.rs   — merges adjacent same-type visible faces into maximal quads
├── mesh_builder.rs  — converts merged quads into a Bevy Mesh (positions, normals, UVs, colors)
├── mesh_task.rs     — MeshData output type and PendingMeshTasks queue
├── lod.rs           — LodLevel enum, LodConfig resource, Chebyshev-distance selection
└── render_state.rs  — per-chunk RenderState: mesh entity handles and dirty flags
```

## Dependencies (dd40)

`dd40_core`, `dd40_physics_core`
