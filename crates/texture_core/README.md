# dd40_texture_core

Foundation vocabulary for the dd40 texture system: `BlockTextures`,
`TextureRef`, `RenderLayer`, `AtlasUv`, `BlockAtlas`,
`AnimationSpec`, and the `AtlasReady` `SystemSet`.

See the crate-level Rust docs (`cargo doc -p dd40_texture_core --open`)
for the authoritative API. In short:

- `BlockTextures` is a six-face texture assignment, attached to a
  `BlockDefinition` via `with_data(...)` just like `LootTable` is
  today.
- `TextureRef` points at a texture either by Minecraft-style name
  (`"namespace:block/name"`) or by a precomputed `(AtlasId, AtlasUv)`
  pair.
- `RenderLayer` (`Opaque` / `Cutout` / `Translucent`) is also a
  `BlockData`, so a block can override the layer auto-classified from
  its texture's alpha channel.
- `BlockAtlas` is the runtime lookup resource the renderer queries; it
  is populated by an atlas-owning Tier-1 plugin (e.g.
  `dd40_texture_pack`).
- `AtlasReady` is the system-set anchor for "atlas is populated;
  meshing may begin".
- `TextureCorePlugin` registers the two `BlockData` types and inserts
  the default empty `BlockAtlas`.

The crate has no runtime systems beyond the resource init in its
plugin. Loading PNGs, building the GPU atlas, classifying alpha, and
rendering textured chunks all live downstream.
