//! Convenience re-exports for downstream crates.
//!
//! ```no_run
//! use dd40_input_core::prelude::*;
//! ```

pub use crate::actions::{
    Attack, FreeCamDown, FreeCamUp, HotbarSelect, Interact, Jump, Look, Move, Pause, Place,
    Sprint, ToggleFreeCam, ToggleInventory,
};
pub use crate::contexts::OnFoot;
pub use crate::plugin::InputCorePlugin;
