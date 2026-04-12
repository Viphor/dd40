//! Root plugin for the dd40 GUI crate.
//!
//! [`GuiPlugin`] is the single registration point for all in-game HUD elements
//! provided by this crate.  Add it to your [`App`] once and every GUI
//! subsystem (crosshair, etc.) will be set up automatically.
//!
//! # Example
//!
//! ```no_run
//! use bevy::prelude::*;
//! use dd40_gui::plugin::GuiPlugin;
//!
//! App::new()
//!     .add_plugins(GuiPlugin)
//!     .run();
//! ```

use bevy::prelude::*;

use crate::crosshair::spawn_crosshair;

/// Plugin that registers all dd40 GUI elements.
///
/// Currently includes:
/// - Crosshair (centre of screen) via [`spawn_crosshair`]
pub struct GuiPlugin;

impl Plugin for GuiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_crosshair);
    }
}
