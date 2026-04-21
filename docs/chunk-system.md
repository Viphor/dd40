# Chunk System

Chunk management is defined in `dd40_core::chunk` and `dd40_core::chunk::cache`.
The design is pipeline-based: messages flow between independent crates without
any direct dependencies between them.

---

## Types

### `ChunkPos`
```rust
pub struct ChunkPos { pub x: i32, pub z: i32 }
```
Position of a chunk in chunk coordinates. Implements conversions from
`BlockPos`, `Transform`, and `Vec3`.

### `Chunk`
```rust
pub struct Chunk { /* position + flat block array */ }
```
A 16 × 256 × 16 slab of `Block` data.

Key methods:
- `new(pos)` — create an empty (all-air) chunk
- `get(lx, ly, lz)` — read a block at chunk-local coordinates
- `set(lx, ly, lz, block)` — write a block at chunk-local coordinates
- `position()` — the chunk's `ChunkPos`

Index layout: `lx + lz * CHUNK_SIZE_X + ly * CHUNK_SIZE_X * CHUNK_SIZE_Z`

### `ChunkCache`
```rust
pub struct ChunkCache { /* ... */ }
```
Resource (registered by `CorePlugin`). Holds all currently loaded chunks in
memory. Updated automatically by `ChunkCachePlugin` when `ChunkReady` messages
arrive.

Key methods:
- `get(pos)` — get a chunk by position
- `get_block(pos)` — get a block at a global `BlockPos`
- `set_block(pos, block)` — write a block and mark the chunk dirty
- `chunk_count()` — number of loaded chunks

---

## Messages

### `RequestChunk`
```rust
pub struct RequestChunk { pub pos: ChunkPos }
```
Written by any system that needs a chunk. The storage crate listens for this
message, tries to load from disk, and either emits `ChunkReady` or
`GenerateChunk` depending on whether a saved file exists.

### `GenerateChunk`
```rust
pub struct GenerateChunk { pub pos: ChunkPos }
```
Written by the storage crate when no saved data exists. The world generation
crate listens for this and responds with `ChunkReady`.

### `ChunkReady`
```rust
pub struct ChunkReady { pub chunk: Chunk }
```
Written when a chunk is fully populated and ready to insert into `ChunkCache`.
`ChunkCachePlugin` inserts the chunk automatically. The renderer also listens
for this message to schedule mesh builds.

---

## System sets

### `WorldGenerationSet`
All world-generation systems run in this set (during `Startup` and/or
`PostUpdate`). Always ordered **after** `BlockRegistrySet` so blocks are
registered before generation uses them.

```rust
app.add_systems(PostUpdate, my_generation_system.in_set(WorldGenerationSet));
```

---

## Pipeline overview

```
[any system]
   │ write RequestChunk
   ▼
dd40_chunk_storage
   ├─ found on disk → write ChunkReady
   └─ not found     → write GenerateChunk
                          │
                          ▼
                    dd40_world (or custom generator)
                          │ write ChunkReady
                          ▼
                    ChunkCachePlugin (core)
                          │ inserts into ChunkCache
                          ▼
                    dd40_renderer
                          │ builds mesh
```

Example from `dd40_server` — requesting the initial 3×3 area around a player
spawn point:

```rust
for dz in -1..=1_i32 {
    for dx in -1..=1_i32 {
        writer.write(RequestChunk { pos: ChunkPos::new(cx + dx, cz + dz) });
    }
}
```
