# Textures (work in progress)

dd40's texture system replaces the original colour-only block rendering
with full Minecraft-compatible **textured blocks**, loaded at runtime
from one or more **resource packs** that follow Minecraft's
`assets/<namespace>/textures/block/*.png` layout.

The system is **entirely opt-in**.  Without the `textures` Cargo
feature, every affected crate behaves exactly as before — the renderer
still emits flat-colour faces, vanilla blocks still ship colour-only,
and no texture-related dependencies are pulled in.

## Status

| Stage | Status | Notes |
|---|---|---|
| Tier-0 vocabulary (`dd40_texture_core`) | ✅ done | `BlockTextures`, `RenderLayer`, `BlockAtlas`, `AtlasUv`, `ResolvedTexture`, `AnimationSpec`. |
| Resource-pack loader (`dd40_texture_pack`) | ✅ done | Discovery, PNG decode, `.mcmeta`, uniform-grid layout, 2D-array atlas, installer. |
| Renderer `textures` feature scaffold | ✅ done | Feature gate + module boundary; renderer still emits colour-only meshes. |
| Vanilla palette `textures` | ✅ done | All renderable vanilla blocks attach `BlockTextures` with `minecraft:block/<name>` references. |
| Inventory-GUI / loose-item-render scaffolds | ✅ done | Optional features; future textured-icon / textured-cube hooks. |
| Custom `BlockAtlasMaterial` + WGSL shader | ⏳ pending | See `crates/renderer/src/textures.rs`. |
| Per-render-layer mesh split | ⏳ pending | Up to 3 meshes per chunk: Opaque / Cutout / Translucent. |
| Greedy-merge key includes texture | ⏳ pending | Today merges only by `block_id` + face direction. |
| Client wiring + placeholder pack | ⏳ pending | Will live at `assets/resourcepacks/default/`. |

## Crate layout

```text
dd40_core ── (unchanged)

dd40_texture_core  (Tier 0, depends on dd40_core)
  ↑
  ├── dd40_texture_pack    (Tier 1) — Minecraft-compatible loader
  ├── dd40_renderer        (Tier 1, optional)   — textured world meshes (WIP)
  ├── dd40_vanilla_palette (Tier 1, optional)   — attaches BlockTextures
  ├── dd40_inventory_gui   (Tier 1, optional)   — textured icons (scaffold)
  └── dd40_loose_item_render (Tier 1, optional) — textured cube (scaffold)
```

`dd40_server` never enables the `textures` feature on any crate; it
never compiles texture-related code.  All texture data is purely
client-side; the server is unaware textures exist.

## Resource-pack layout

`dd40_texture_pack` discovers every PNG under

```
<search_path>/assets/<namespace>/textures/block/<name>.png
```

where each search path is treated as one resource pack.  Multiple
search paths are honoured in order — **later packs override earlier
ones** for the same `<namespace>:block/<name>` key, matching
Minecraft's behaviour.  A standalone `.mcmeta` file next to a PNG
unlocks animation:

```json
{
  "animation": {
    "frametime": 4,
    "interpolate": false,
    "frames": [0, 1, 2, 1, { "index": 3, "time": 8 }]
  }
}
```

Frames are expanded into the precomputed `AnimationSpec.frame_indices`
list (e.g. `[0, 1, 2, 1, 3, 3, 3, 3, 3, 3, 3, 3]` for the example
above) so renderer-side animation is a single modulo lookup with no
parsing.

## Attaching textures to a block

Texture metadata lives on `BlockDefinition` via the same `with_data`
mechanism `LootTable` uses:

```rust
use dd40_core::block::registry::BlockDefinition;
use dd40_texture_core::{BlockTextures, TextureRef};

let def = BlockDefinition::new(MY_BLOCK_ID, "ruby_ore")
    .with_color(Color::srgb(0.9, 0.3, 0.3))  // tint + fallback
    .with_solid(true)
    .with_renderable(true)
    .with_data(BlockTextures::all(TextureRef::named("mymod:block/ruby_ore")));
```

For per-face textures (top, bottom, sides) use
`BlockTextures::top_bottom_sides(...)` or the full
`BlockTextures::default().with_face(Face::Top, ...).with_face(...)`
builder.

The texture name is just a string — it can refer to any `<namespace>:
block/<name>` PNG that the loaded pack supplies.  Modders are free to
ship their own resource packs alongside their crate.

## Why a custom material

Bevy's `StandardMaterial` cannot sample a 2D **texture array** — and
texture arrays are required for two reasons:

1. **Animation** — each animation frame becomes one array layer; the
   shader picks the active layer using a `time_ms` uniform and the
   precomputed `frame_indices` table.
2. **Per-face textures** — different faces of the same block can point
   into different array layers, removing the need for one material
   per block kind.

`BlockAtlasMaterial` (forthcoming in `crates/renderer/src/textures.rs`)
will be a small `#[derive(AsBindGroup)]` material with a hand-written
`block_atlas.wgsl` fragment shader.  Three material instances will be
created (one per `RenderLayer`) to feed Bevy's
`AlphaMode::Opaque` / `Mask(0.5)` / `Blend` pipelines.

## Modding considerations

| Want to… | Do this |
|---|---|
| Add a new block with a single texture | Attach `BlockTextures::all(TextureRef::named(...))` to your `BlockDefinition`. |
| Add a new block with per-face textures | Use `BlockTextures::top_bottom_sides(...)` or the per-face builder. |
| Ship a resource pack with your crate | Drop PNGs under `assets/<your_namespace>/textures/block/`. Add the path to `TexturePackConfig::search_paths`. |
| Override a vanilla texture | Ship an `assets/minecraft/textures/block/<name>.png` in a pack with a later position in `search_paths`. |
| Write a completely custom renderer | Read `BlockTextures` directly from `BlockDataRegistry`. Ignore `dd40_renderer`. |
| Build dd40 without textures at all | Don't enable the `textures` feature on any crate. The whole stack disappears. |

## See also

- [`crates/texture_core/`](../crates/texture_core/) — vocabulary types.
- [`crates/texture_pack/`](../crates/texture_pack/) — loader.
- [`crates/renderer/src/textures.rs`](../crates/renderer/src/textures.rs) — renderer integration (WIP).
