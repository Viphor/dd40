# dd40_texture_pack

Tier-1 Minecraft-compatible texture-pack loader for dd40. Turns one or
more directories of Minecraft-style resource-pack assets into a
populated `BlockAtlas` resource.

See the crate-level Rust docs (`cargo doc -p dd40_texture_pack --open`)
for the authoritative API. In short:

- `TexturePackConfig` holds an ordered list of pack-root directories
  (later overrides earlier) and per-key `RenderLayer` overrides.
- `discover` walks the configured search paths and finds every
  `assets/<ns>/textures/block/**/*.png` (with companion `.mcmeta`).
  It returns a `Vec<DiscoveredTexture>` keyed by
  `"<ns>:block/<name>"`.
- `TexturePackPlugin` is the single entry point. It auto-adds
  `CorePlugin` and `TextureCorePlugin` via `ensure_plugins!` and
  inserts a default `TexturePackConfig` if the binary did not.
- PNG decoding, `.mcmeta` parsing, alpha classification, atlas
  building, and `BlockAtlas` installation are added in follow-up
  commits — this initial cut only covers configuration and
  filesystem discovery.

This crate is **one possible** atlas-owning plugin; downstream users
who want a different format (hand-built atlas, streaming, etc.) can
write their own plugin that populates `BlockAtlas` and skip this
crate entirely.
