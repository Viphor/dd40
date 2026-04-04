use bevy::prelude::*;
use std::collections::HashMap;

/// System set for systems that register loading items during startup.
///
/// Add your loading item registrations to this set so they are guaranteed
/// to run before the loading completion check begins.
///
/// # Examples
///
/// ```
/// use bevy::prelude::*;
/// use dd40_core::loading::{LoadingSet, LoadingTracker};
///
/// fn register_my_loading_item(mut tracker: ResMut<LoadingTracker>) {
///     tracker.add("my_system:ready", "Waiting for my system to be ready");
/// }
///
/// pub struct MyPlugin;
///
/// impl Plugin for MyPlugin {
///     fn build(&self, app: &mut App) {
///         app.add_systems(Startup, register_my_loading_item.in_set(LoadingSet));
///     }
/// }
/// ```
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LoadingSet;

/// A single pending loading item, combining its unique key with a
/// human-readable description of what is being waited on.
///
/// Returned by [`LoadingTracker::iter`] so that loading screens and debug
/// overlays can display meaningful status text without needing to parse the
/// key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadingItem {
    /// The unique key that identifies this loading item.
    /// Use namespaced, snake_case keys to avoid collisions between crates,
    /// e.g. `"network:server_connection"`.
    pub key: String,
    /// A short, human-readable description of what is being waited on,
    /// suitable for display in a loading screen, e.g.
    /// `"Connecting to server…"`.
    pub description: String,
}

/// Tracks named items that are currently pending during the loading state.
///
/// Systems that require async initialisation (e.g. connecting to a server,
/// loading assets) should:
///
/// 1. Call [`LoadingTracker::add`] with a unique key and a human-readable
///    description during startup.
/// 2. Call [`LoadingTracker::remove`] once the work is complete.
///
/// When the tracker becomes empty the [`LoadingPlugin`] will automatically
/// transition [`AppState`](crate::state::AppState) from `Loading` to `Playing`.
///
/// The description stored alongside each key is intended for use by loading
/// screens or debug overlays — any system can read all pending items via
/// [`LoadingTracker::iter`] and display them to the player.
///
/// # Key Conventions
///
/// Use namespaced, snake_case keys to avoid collisions between crates:
///
/// ```text
/// "network:server_connection"
/// "assets:terrain_textures"
/// "world:initial_chunks"
/// ```
///
/// # Examples
///
/// ```
/// use dd40_core::loading::LoadingTracker;
///
/// let mut tracker = LoadingTracker::default();
/// assert!(tracker.is_empty());
///
/// tracker.add("network:server_connection", "Connecting to server…");
/// assert!(!tracker.is_empty());
/// assert!(tracker.contains("network:server_connection"));
/// assert_eq!(
///     tracker.description("network:server_connection"),
///     Some("Connecting to server…"),
/// );
///
/// tracker.remove("network:server_connection");
/// assert!(tracker.is_empty());
/// ```
#[derive(Resource, Default, Debug)]
pub struct LoadingTracker {
    /// Maps each pending key to its human-readable description.
    pending: HashMap<String, String>,
}

impl LoadingTracker {
    /// Registers a new pending loading item with an associated description.
    ///
    /// The `description` should be a short, human-readable string suitable for
    /// display in a loading screen, e.g. `"Connecting to server…"`.
    ///
    /// If the key is already present this is a no-op and returns `false`. A
    /// loading item can only be registered once — call [`remove`](Self::remove)
    /// first if you need to update the description.
    ///
    /// Returns `true` if the item was newly inserted.
    ///
    /// # Arguments
    ///
    /// * `key` – A unique, namespaced identifier for the thing being waited on
    ///   (e.g. `"network:server_connection"`).
    /// * `description` – A short, human-readable label shown on loading screens
    ///   (e.g. `"Connecting to server…"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use dd40_core::loading::LoadingTracker;
    ///
    /// let mut tracker = LoadingTracker::default();
    /// assert!(tracker.add("world:chunks", "Generating initial chunks…"));
    /// // Duplicate registration is a no-op.
    /// assert!(!tracker.add("world:chunks", "Generating initial chunks…"));
    /// ```
    pub fn add(&mut self, key: impl Into<String>, description: impl Into<String>) -> bool {
        let key = key.into();
        if self.pending.contains_key(&key) {
            return false;
        }
        let description = description.into();
        debug!("LoadingTracker: waiting on \"{}\" — {}", key, description);
        self.pending.insert(key, description);
        true
    }

    /// Marks a pending loading item as complete and removes it from the
    /// tracker.
    ///
    /// Returns `true` if the key was present and has been removed. Returns
    /// `false` if the key was not found — this is not an error; the item may
    /// never have been registered on this instance.
    ///
    /// # Examples
    ///
    /// ```
    /// use dd40_core::loading::LoadingTracker;
    ///
    /// let mut tracker = LoadingTracker::default();
    /// tracker.add("network:server_connection", "Connecting to server…");
    /// assert!(tracker.remove("network:server_connection"));
    /// // Removing a key that is no longer present is safe.
    /// assert!(!tracker.remove("network:server_connection"));
    /// ```
    pub fn remove(&mut self, key: &str) -> bool {
        let removed = self.pending.remove(key).is_some();
        if removed {
            debug!("LoadingTracker: completed \"{}\"", key);
        }
        removed
    }

    /// Returns `true` when there are no pending loading items.
    ///
    /// The [`LoadingPlugin`] watches this and transitions the app to `Playing`
    /// once it becomes `true`.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Returns `true` if the given key is currently registered as pending.
    pub fn contains(&self, key: &str) -> bool {
        self.pending.contains_key(key)
    }

    /// Returns the number of items currently pending.
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Returns the human-readable description for the given key, or `None` if
    /// the key is not currently pending.
    ///
    /// # Examples
    ///
    /// ```
    /// use dd40_core::loading::LoadingTracker;
    ///
    /// let mut tracker = LoadingTracker::default();
    /// tracker.add("assets:textures", "Loading terrain textures…");
    /// assert_eq!(tracker.description("assets:textures"), Some("Loading terrain textures…"));
    /// assert_eq!(tracker.description("does:not:exist"), None);
    /// ```
    pub fn description(&self, key: &str) -> Option<&str> {
        self.pending.get(key).map(String::as_str)
    }

    /// Returns an iterator over all pending [`LoadingItem`]s in arbitrary
    /// order.
    ///
    /// Each item exposes both the unique `key` and the human-readable
    /// `description`, making this the primary API for loading screens and
    /// debug overlays.
    ///
    /// # Examples
    ///
    /// ```
    /// use dd40_core::loading::LoadingTracker;
    ///
    /// let mut tracker = LoadingTracker::default();
    /// tracker.add("a:one", "Loading one…");
    /// tracker.add("b:two", "Loading two…");
    ///
    /// let mut items: Vec<_> = tracker.iter().collect();
    /// items.sort_by_key(|i| i.key.clone());
    ///
    /// assert_eq!(items[0].key, "a:one");
    /// assert_eq!(items[0].description, "Loading one…");
    /// assert_eq!(items[1].key, "b:two");
    /// assert_eq!(items[1].description, "Loading two…");
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = LoadingItem> + '_ {
        self.pending.iter().map(|(key, description)| LoadingItem {
            key: key.clone(),
            description: description.clone(),
        })
    }

    /// Returns an iterator over just the pending keys in arbitrary order.
    ///
    /// Prefer [`iter`](Self::iter) when you also need the description. Use this
    /// when you only need to check or display the keys themselves.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.pending.keys().map(String::as_str)
    }
}

/// Bevy plugin that manages the automatic `Loading → Playing` state transition.
///
/// This plugin:
///
/// - Inserts the [`LoadingTracker`] resource.
/// - Defines the [`LoadingSet`] system set (runs during `Startup`).
/// - Runs a per-frame system (only while in [`AppState::Loading`]) that
///   transitions to [`AppState::Playing`] as soon as [`LoadingTracker`] is
///   empty.
///
/// # Usage
///
/// Add this plugin *before* any plugin that registers loading items so the
/// resource is available during their startup systems.
///
/// ```rust,no_run
/// use bevy::prelude::*;
/// use dd40_core::loading::LoadingPlugin;
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(LoadingPlugin)
///     .run();
/// ```
pub struct LoadingPlugin;

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadingTracker>()
            .configure_sets(Startup, LoadingSet)
            .add_systems(
                Update,
                check_loading_complete.run_if(in_state(crate::state::AppState::Loading)),
            );
    }
}

/// Transitions from [`AppState::Loading`] to [`AppState::Playing`] once the
/// [`LoadingTracker`] has no remaining pending items.
///
/// This system runs every frame while in the `Loading` state. Transitioning
/// only happens after at least one frame, so any startup systems that add
/// loading items (in [`LoadingSet`]) are guaranteed to have run first.
fn check_loading_complete(
    tracker: Res<LoadingTracker>,
    mut next_state: ResMut<NextState<crate::state::AppState>>,
) {
    if tracker.is_empty() {
        info!("LoadingTracker: all items resolved — transitioning to Playing");
        next_state.set(crate::state::AppState::Playing);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use bevy::state::app::StatesPlugin;

    // ── LoadingTracker unit tests ────────────────────────────────────────────

    #[test]
    fn tracker_starts_empty() {
        let tracker = LoadingTracker::default();
        assert!(tracker.is_empty());
        assert_eq!(tracker.len(), 0);
    }

    #[test]
    fn add_and_remove_item() {
        let mut tracker = LoadingTracker::default();
        let inserted = tracker.add("test:item", "Testing item");
        assert!(inserted);
        assert!(!tracker.is_empty());
        assert!(tracker.contains("test:item"));
        assert_eq!(tracker.len(), 1);

        let removed = tracker.remove("test:item");
        assert!(removed);
        assert!(tracker.is_empty());
    }

    #[test]
    fn description_is_stored_and_retrievable() {
        let mut tracker = LoadingTracker::default();
        tracker.add("network:server_connection", "Connecting to server…");

        assert_eq!(
            tracker.description("network:server_connection"),
            Some("Connecting to server…"),
        );
        assert_eq!(tracker.description("does:not:exist"), None);
    }

    #[test]
    fn description_is_gone_after_remove() {
        let mut tracker = LoadingTracker::default();
        tracker.add("assets:textures", "Loading terrain textures…");
        tracker.remove("assets:textures");

        assert_eq!(tracker.description("assets:textures"), None);
    }

    #[test]
    fn add_duplicate_is_noop() {
        let mut tracker = LoadingTracker::default();
        tracker.add("test:item", "First description");
        let second = tracker.add("test:item", "Second description");

        assert!(!second, "adding a duplicate should return false");
        assert_eq!(tracker.len(), 1);
        // Original description must be preserved.
        assert_eq!(tracker.description("test:item"), Some("First description"));
    }

    #[test]
    fn remove_nonexistent_returns_false() {
        let mut tracker = LoadingTracker::default();
        let removed = tracker.remove("does:not:exist");
        assert!(!removed);
    }

    #[test]
    fn multiple_items_must_all_be_removed() {
        let mut tracker = LoadingTracker::default();
        tracker.add("a:one", "Loading one");
        tracker.add("b:two", "Loading two");
        tracker.add("c:three", "Loading three");

        assert_eq!(tracker.len(), 3);
        assert!(!tracker.is_empty());

        tracker.remove("a:one");
        assert!(!tracker.is_empty());

        tracker.remove("b:two");
        assert!(!tracker.is_empty());

        tracker.remove("c:three");
        assert!(tracker.is_empty());
    }

    #[test]
    fn iter_returns_all_items_with_descriptions() {
        let mut tracker = LoadingTracker::default();
        tracker.add("x:alpha", "Alpha loading…");
        tracker.add("x:beta", "Beta loading…");

        let mut items: Vec<LoadingItem> = tracker.iter().collect();
        items.sort_by_key(|i| i.key.clone());

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].key, "x:alpha");
        assert_eq!(items[0].description, "Alpha loading…");
        assert_eq!(items[1].key, "x:beta");
        assert_eq!(items[1].description, "Beta loading…");
    }

    #[test]
    fn keys_returns_all_keys() {
        let mut tracker = LoadingTracker::default();
        tracker.add("x:alpha", "Alpha");
        tracker.add("x:beta", "Beta");

        let mut keys: Vec<&str> = tracker.keys().collect();
        keys.sort();
        assert_eq!(keys, vec!["x:alpha", "x:beta"]);
    }

    // ── Integration tests: state transition ─────────────────────────────────

    /// Builds a minimal app with `LoadingPlugin` and asserts that an empty
    /// tracker causes a transition to `Playing` after one update.
    #[test]
    fn empty_tracker_transitions_to_playing() {
        let mut app = App::new();
        app.add_plugins(StatesPlugin);
        app.init_state::<AppState>();
        // Start in Loading
        app.insert_resource(NextState::Pending(AppState::Loading));
        app.add_plugins(LoadingPlugin);

        // First update applies the pending state change to Loading
        app.update();
        // Second update runs check_loading_complete while actually in Loading
        app.update();
        // Third update applies the transition to Playing
        app.update();

        let state = app.world().resource::<State<AppState>>();
        assert_eq!(*state.get(), AppState::Playing);
    }

    /// When a loading item is registered, the transition must NOT happen until
    /// that item is removed.
    #[test]
    fn non_empty_tracker_blocks_transition() {
        let mut app = App::new();
        app.add_plugins(StatesPlugin);
        app.init_state::<AppState>();
        app.insert_resource(NextState::Pending(AppState::Loading));
        app.add_plugins(LoadingPlugin);

        // Manually add a pending item
        app.world_mut()
            .resource_mut::<LoadingTracker>()
            .add("test:blocker", "Blocking the loading screen");

        // Run several updates — should stay in Loading
        for _ in 0..5 {
            app.update();
        }

        let state = app.world().resource::<State<AppState>>();
        assert_eq!(
            *state.get(),
            AppState::Loading,
            "state should still be Loading while tracker is non-empty"
        );
    }

    /// Removing the last item mid-game should eventually trigger the transition.
    #[test]
    fn removing_last_item_triggers_transition() {
        let mut app = App::new();
        app.add_plugins(StatesPlugin);
        app.init_state::<AppState>();
        app.insert_resource(NextState::Pending(AppState::Loading));
        app.add_plugins(LoadingPlugin);

        app.world_mut()
            .resource_mut::<LoadingTracker>()
            .add("test:network", "Connecting to server…");

        // Several updates while blocked
        for _ in 0..3 {
            app.update();
        }

        // Remove the blocker
        app.world_mut()
            .resource_mut::<LoadingTracker>()
            .remove("test:network");

        // Allow the system and state flush to run
        app.update();
        app.update();

        let state = app.world().resource::<State<AppState>>();
        assert_eq!(*state.get(), AppState::Playing);
    }
}
