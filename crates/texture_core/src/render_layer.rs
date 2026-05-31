//! [`RenderLayer`] — opaque / cutout / translucent classification.
//!
//! A block's render layer governs which pass the renderer puts it in:
//!
//! - [`RenderLayer::Opaque`]: standard depth-tested, no alpha.
//! - [`RenderLayer::Cutout`]: alpha-tested (e.g. leaves, ladders).
//! - [`RenderLayer::Translucent`]: alpha-blended (e.g. stained glass,
//!   water).
//!
//! The atlas loader infers a default from the texture's alpha channel,
//! but a block can override it by attaching [`RenderLayer`] as
//! [`BlockData`] on its [`BlockDefinition`] — useful for cases like
//! solid-coloured glass blocks that should still render translucent.
//!
//! [`BlockData`]: dd40_core::block::BlockData
//! [`BlockDefinition`]: dd40_core::block::BlockDefinition

use std::any::Any;

use dd40_core::block::BlockData;
use serde::{Deserialize, Serialize};

/// How a block's faces should be composited with the rest of the
/// scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RenderLayer {
    /// Fully opaque.  Default for textures whose alpha channel is
    /// constant 255 (or absent).
    Opaque,
    /// Alpha-tested with a fixed cutoff.  Default for textures that
    /// use only 0 and 255 alpha values.
    Cutout,
    /// Alpha-blended.  Default for textures with partial alpha values.
    Translucent,
}

impl Default for RenderLayer {
    fn default() -> Self {
        Self::Opaque
    }
}

impl BlockData for RenderLayer {
    fn type_key(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
    fn clone_box(&self) -> Box<dyn BlockData> {
        Box::new(*self)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_opaque() {
        assert_eq!(RenderLayer::default(), RenderLayer::Opaque);
    }

    #[test]
    fn block_data_clone_box_round_trips() {
        let v = RenderLayer::Translucent;
        let boxed = BlockData::clone_box(&v);
        let back = boxed.as_any().downcast_ref::<RenderLayer>().copied();
        assert_eq!(back, Some(RenderLayer::Translucent));
    }
}
