//! Convenience re-exports for downstream crates.
//!
//! ```no_run
//! use dd40_input_core::prelude::*;
//! ```

pub use crate::actions::{
    Attack, FreeCamDown, FreeCamUp, Interact, Jump, Look, Move, Pause, Place, Sprint,
    ToggleFreeCam,
};
pub use crate::plugin::InputCorePlugin;
