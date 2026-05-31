//! Re-exports of every stable public type in `dd40_texture_core`.
//!
//! ```no_run
//! use dd40_texture_core::prelude::*;
//! ```

pub use crate::animation::AnimationSpec;
pub use crate::atlas::{AtlasId, AtlasReady, AtlasUv, BlockAtlas, ResolvedTexture};
pub use crate::block_textures::{BlockTextures, Face};
pub use crate::plugin::TextureCorePlugin;
pub use crate::render_layer::RenderLayer;
pub use crate::texture_ref::TextureRef;
