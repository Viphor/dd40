//! Character-facing GUI and gizmo rendering for dd40.
//!
//! # Overview
//!
//! `dd40_character_gui` owns every visual that is keyed off character
//! vocabulary types defined in `dd40_character_core` — the targeted-block
//! highlight, the mining break overlay, and (in the future) per-character
//! HUD elements such as a hotbar or health bar.
//!
//! Other vocabulary domains get their own `dd40_<vocab>_gui` companion
//! crate; the generic `dd40_gui` crate stays reserved for screen-level
//! HUD that has no character coupling (e.g. the crosshair).
//!
//! # Usage
//!
//! Add [`plugin::CharacterGuiPlugin`] to your client `App`:
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_character_gui::plugin::CharacterGuiPlugin;
//!
//! App::new()
//!     .add_plugins(CharacterGuiPlugin)
//!     .run();
//! ```

pub mod plugin;
