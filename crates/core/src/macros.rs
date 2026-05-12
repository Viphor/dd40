/// Checks each listed plugin and adds it with [`Default::default()`] if not
/// already present in the app.
///
/// Call this at the top of every [`Plugin::build`] implementation, listing
/// every direct runtime dependency.  This is the only approved way to write
/// the auto-plugin dependency check — never write the
/// `if !app.is_plugin_added` block by hand.
///
/// # Example
///
/// ```ignore
/// impl Plugin for PhysicsPlugin {
///     fn build(&self, app: &mut App) {
///         ensure_plugins!(app, CorePlugin, PhysicsCorePlugin);
///         // add systems ...
///     }
/// }
/// ```
#[macro_export]
macro_rules! ensure_plugins {
    ($app:expr, $($plugin:ty),+ $(,)?) => {
        $(
            if !$app.is_plugin_added::<$plugin>() {
                $app.add_plugins(<$plugin>::default());
            }
        )+
    };
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    #[derive(Default)]
    struct FlagPlugin;

    #[derive(Resource, Default)]
    struct FlagResource;

    impl Plugin for FlagPlugin {
        fn build(&self, app: &mut App) {
            app.init_resource::<FlagResource>();
        }
    }

    #[derive(Default)]
    struct ConsumerPlugin;

    impl Plugin for ConsumerPlugin {
        fn build(&self, app: &mut App) {
            crate::ensure_plugins!(app, FlagPlugin);
        }
    }

    #[test]
    fn adds_missing_dependency() {
        let mut app = App::new();
        app.add_plugins(ConsumerPlugin);
        assert!(app.world().contains_resource::<FlagResource>());
    }

    #[test]
    fn skips_already_added_dependency() {
        // Bevy panics on duplicate unique plugins, so this verifies no double-add.
        let mut app = App::new();
        app.add_plugins(FlagPlugin);
        app.add_plugins(ConsumerPlugin);
        assert!(app.world().contains_resource::<FlagResource>());
    }
}
