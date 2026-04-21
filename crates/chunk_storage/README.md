# dd40_chunk_storage

Disk-backed chunk persistence for dd40. Implements the chunk request/response
cycle by reading chunks from the local filesystem. When a chunk file is missing
it emits a `GenerateChunk` message so the world generator can produce it; once
generated the chunk is written back to disk.

Depends only on `dd40_core`. Replacing this crate with a different storage
backend (database, cloud, in-memory) requires only swapping the plugin in the
server or client.

## Module overview

```
src/
├── lib.rs              — DiskStoragePlugin wiring, channel newtypes, dispatch and collect systems
├── plugin.rs           — DiskStoragePlugin definition and build logic
├── provider.rs         — DiskChunkProvider: async file I/O using crossbeam channels
└── serialization/
    ├── mod.rs          — Versioned serialization entry point
    └── v1.rs           — Version-1 binary format (bincode-based)
```
