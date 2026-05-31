//! Textured-block rendering scaffold.
//!
//! This module is **only compiled** when the `textures` Cargo feature
//! is enabled.  It will hold:
//!
//! - The `BlockAtlasMaterial` custom Bevy [`Material`](bevy::pbr::Material)
//!   that samples the [`BlockAtlas`](dd40_texture_core::BlockAtlas)
//!   2D-array texture (added in a follow-up commit).
//! - The per-render-layer mesh split logic (added in a follow-up
//!   commit).
//! - The greedy-mesh merge key extended to include texture / face /
//!   tint / layer (added in a follow-up commit).
//!
//! This initial scaffold establishes the feature gate and the
//! module boundary so the colour-only renderer keeps building
//! unchanged with `default-features` and the textured path can be
//! filled in incrementally without breaking either configuration.
//!
//! # Why a feature flag
//!
//! Per the dd40 design principles, texturing is **opt-in**: a
//! downstream user who wants a 2D / text-mode / custom-renderer
//! variant of dd40 must be able to compile the renderer without
//! ever pulling in the texture system.  Toggling the `textures`
//! feature is the single switch that does that.

use bevy::prelude::*;

/// Marker placeholder so the module compiles when the textures
/// feature is on but the implementation modules are not yet
/// populated.  Will be removed once the real systems land.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct TexturedRendererScaffold;

/// Returns `true` once the texture pipeline modules are populated.
/// Today: always `false`.  This is a deliberate signal for the
/// upcoming WGSL / mesh-split commits to flip when they land.
pub const fn pipeline_ready() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_resource_compiles_and_defaults() {
        let _r = TexturedRendererScaffold;
    }

    #[test]
    fn pipeline_is_not_ready_yet() {
        assert!(!pipeline_ready());
    }
}
