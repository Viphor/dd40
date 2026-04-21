# dd40_world

World generation crate for dd40. Provides a generic `WorldPlugin<G>` that
drives any `WorldGenerator` implementation, listening for `GenerateChunk`
messages and emitting `ChunkReady` messages in response. Ships with
`FlatWorldGenerator` as the built-in default.

Depends only on `dd40_core`. Swap the generator by passing a different type to
`WorldPlugin::new(...)` — or write an entirely separate crate that speaks the
same `GenerateChunk` / `ChunkReady` message contract and skip this crate
altogether.

## Module overview

```
src/
├── lib.rs             — re-exports WorldPlugin and generators module
├── plugin.rs          — WorldPlugin<G: WorldGenerator + Resource + Clone>
└── generators/
    ├── mod.rs         — WorldGenerator trait definition
    └── flat.rs        — FlatWorldGenerator (produces a flat stone/grass world)
```

## Implementing a custom generator

Implement `WorldGenerator` from `dd40_core::world` (re-exported via
`dd40_core::prelude`), wrap it in `WorldPlugin::new(MyGenerator)`, and add
that plugin to your app. No changes to this crate are necessary.
