//! The [`RngPlugin`] entry point.

use bevy::prelude::*;
use rand::{RngCore, SeedableRng, rngs::StdRng};

use crate::resource::GameRng;

/// Factory closure that produces a fresh boxed [`RngCore`].
///
/// Stored on [`RngPlugin`] so the same factory can build the RNG once
/// `Plugin::build` runs.  `dyn Fn` rather than `FnOnce` so the plugin
/// type stays `Send + Sync + 'static` and composable with Bevy's plugin
/// machinery.
pub type RngFactory = dyn Fn() -> Box<dyn RngCore + Send + Sync> + Send + Sync;

/// Inserts a shared [`GameRng`] resource.
///
/// The default factory produces a [`StdRng`] seeded from operating-system
/// entropy.  Tests, replay tooling, or any binary that wants determinism
/// should construct the plugin via [`RngPlugin::with_factory`] and pass a
/// closure that returns a seeded RNG.
///
/// # Examples
///
/// Default (OS entropy):
///
/// ```no_run
/// use bevy::prelude::*;
/// use dd40_rng::RngPlugin;
///
/// App::new().add_plugins(RngPlugin::default());
/// ```
///
/// Seeded:
///
/// ```no_run
/// use bevy::prelude::*;
/// use dd40_rng::RngPlugin;
/// use rand::{SeedableRng, rngs::StdRng};
///
/// App::new().add_plugins(RngPlugin::with_factory(|| {
///     Box::new(StdRng::seed_from_u64(0xDEAD_BEEF))
/// }));
/// ```
#[derive(Default)]
pub struct RngPlugin {
    factory: Option<Box<RngFactory>>,
}

impl RngPlugin {
    /// Constructs a plugin that uses the default RNG (`StdRng` from
    /// OS entropy).  Equivalent to [`RngPlugin::default`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Constructs a plugin that builds its [`GameRng`] from the supplied
    /// factory closure.
    ///
    /// The closure runs exactly once, inside [`Plugin::build`], when the
    /// app is being constructed.
    pub fn with_factory<F>(factory: F) -> Self
    where
        F: Fn() -> Box<dyn RngCore + Send + Sync> + Send + Sync + 'static,
    {
        Self {
            factory: Some(Box::new(factory)),
        }
    }
}

impl Plugin for RngPlugin {
    fn build(&self, app: &mut App) {
        let rng: Box<dyn RngCore + Send + Sync> = match &self.factory {
            Some(factory) => factory(),
            None => Box::new(StdRng::from_os_rng()),
        };
        app.insert_resource(GameRng::from_boxed(rng));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn default_plugin_inserts_resource() {
        let mut app = App::new();
        app.add_plugins(RngPlugin::default());
        assert!(app.world().get_resource::<GameRng>().is_some());
    }

    #[test]
    fn custom_factory_is_used() {
        let mut app = App::new();
        app.add_plugins(RngPlugin::with_factory(|| {
            Box::new(StdRng::seed_from_u64(123))
        }));
        let mut rng = app.world_mut().resource_mut::<GameRng>();
        let first: u64 = rng.random();

        let mut reference = StdRng::seed_from_u64(123);
        let expected: u64 = reference.random();

        assert_eq!(first, expected);
    }
}
