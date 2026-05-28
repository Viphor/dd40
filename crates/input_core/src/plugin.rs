//! Root plugin for `dd40_input_core`.
//!
//! [`InputCorePlugin`] is the single entry point: it installs
//! [`bevy_enhanced_input::EnhancedInputPlugin`] exactly once so that every
//! downstream crate can register actions, contexts, and bindings without
//! having to coordinate plugin ordering with peers.

use bevy::prelude::*;
use bevy_enhanced_input::EnhancedInputPlugin;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;

/// Installs the `bevy_enhanced_input` runtime and the dd40 input vocabulary.
///
/// ## What this plugin sets up
///
/// - Idempotently adds [`EnhancedInputPlugin`] (Bevy panics on duplicate
///   unique plugins, so the [`App::is_plugin_added`] guard here is
///   load-bearing).
/// - Auto-adds [`CorePlugin`] via [`ensure_plugins!`].
///
/// Action types themselves live in [`crate::actions`] and are not registered
/// here — registration happens in `dd40_player_input` (bindings + the
/// translator from action state → `CharacterInput`).
///
/// # Example
///
/// ```no_run
/// use bevy::prelude::*;
/// use dd40_input_core::InputCorePlugin;
///
/// App::new()
///     .add_plugins(InputCorePlugin)
///     .run();
/// ```
#[derive(Default)]
pub struct InputCorePlugin;

impl Plugin for InputCorePlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin);
        if !app.is_plugin_added::<EnhancedInputPlugin>() {
            app.add_plugins(EnhancedInputPlugin);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A consumer plugin that uses [`ensure_plugins!`] to install
    /// [`InputCorePlugin`]. Mirrors how every real downstream crate adds
    /// the dependency.
    #[derive(Default)]
    struct ConsumerPlugin;

    impl Plugin for ConsumerPlugin {
        fn build(&self, app: &mut App) {
            ensure_plugins!(app, InputCorePlugin);
        }
    }

    /// Adding [`InputCorePlugin`] explicitly and then having a downstream
    /// plugin request it via [`ensure_plugins!`] must not panic with a
    /// duplicate-plugin error. This is the realistic add path; Bevy itself
    /// rejects a direct second [`add_plugins`] call.
    #[test]
    fn ensure_plugins_is_idempotent() {
        let mut app = App::new();
        app.add_plugins(InputCorePlugin);
        app.add_plugins(ConsumerPlugin);
        assert!(app.is_plugin_added::<InputCorePlugin>());
        assert!(app.is_plugin_added::<EnhancedInputPlugin>());
    }

    /// [`InputCorePlugin`] must install [`EnhancedInputPlugin`] when added
    /// to a fresh [`App`].
    #[test]
    fn installs_enhanced_input_plugin() {
        let mut app = App::new();
        app.add_plugins(InputCorePlugin);
        assert!(app.is_plugin_added::<EnhancedInputPlugin>());
    }

    /// If [`EnhancedInputPlugin`] is already present (e.g. installed
    /// directly by the binary), [`InputCorePlugin`] must skip its own add
    /// rather than panic with a duplicate-plugin error.
    #[test]
    fn skips_already_installed_enhanced_input_plugin() {
        let mut app = App::new();
        app.add_plugins(EnhancedInputPlugin);
        app.add_plugins(InputCorePlugin);
        assert!(app.is_plugin_added::<EnhancedInputPlugin>());
        assert!(app.is_plugin_added::<InputCorePlugin>());
    }

    /// [`InputCorePlugin`] must auto-add [`CorePlugin`] via
    /// [`ensure_plugins!`].
    #[test]
    fn auto_adds_core_plugin() {
        let mut app = App::new();
        app.add_plugins(InputCorePlugin);
        assert!(app.is_plugin_added::<CorePlugin>());
    }
}
