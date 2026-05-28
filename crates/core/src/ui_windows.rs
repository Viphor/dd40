//! [`OpenUiWindows`] — cooperative registry of open UI windows.
//!
//! Multiple Bevy UI crates (inventory, pause menu, chat, …) need to
//! share two pieces of state with the player-input code:
//!
//! 1. **Cursor lock** — while any window with `releases_cursor = true`
//!    is open, the cursor must stay visible and unlocked so the player
//!    can click on widgets.
//! 2. **Keyboard capture** *(reserved)* — same idea for typing inside
//!    a window without firing world bindings.  Not yet wired in v1.
//!
//! Each UI crate registers a [`UiWindow`] when its window opens and
//! removes it when it closes.  The player-input cursor system reads
//! [`OpenUiWindows::cursor_should_release`] each frame instead of
//! making assumptions about which windows exist.
//!
//! # Identity
//!
//! Windows are keyed by [`UiWindowId`], a [`std::any::TypeId`]-derived
//! handle so different crates can register windows without
//! coordinating on a string namespace.  The recommended pattern is:
//!
//! ```ignore
//! struct InventoryWindow;
//! let id = UiWindowId::of::<InventoryWindow>();
//! open_windows.insert(id, UiWindow::cursor_released());
//! ```

use std::any::{Any, TypeId};

use bevy::platform::collections::HashMap;
use bevy::prelude::Resource;

/// Stable identifier for a UI window.
///
/// Built from a marker type's [`TypeId`].  Two crates that pick the
/// same marker type collide on purpose; two crates that pick
/// different marker types are guaranteed to be distinct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UiWindowId(TypeId);

impl UiWindowId {
    /// Builds an id from a marker type.  The marker type does not need
    /// to be instantiable — `struct InventoryWindow;` is enough.
    pub fn of<T: Any + 'static>() -> Self {
        Self(TypeId::of::<T>())
    }
}

/// Properties of an open UI window that other systems may need to
/// react to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UiWindow {
    /// When `true`, while this window is open the cursor stays
    /// visible and unlocked.
    pub releases_cursor: bool,
    /// When `true`, while this window is open keyboard bindings that
    /// would otherwise control the world should be suppressed.
    /// Reserved for future use; v1 does not consume this yet.
    pub captures_keyboard: bool,
}

impl UiWindow {
    /// Convenience constructor for a window that only releases the
    /// cursor and leaves keyboard handling alone.
    pub fn cursor_released() -> Self {
        Self {
            releases_cursor: true,
            captures_keyboard: false,
        }
    }
}

/// Global registry of currently-open UI windows.
///
/// Inserted as a [`Resource`] by
/// [`CorePlugin`][crate::plugin::CorePlugin].
#[derive(Resource, Default, Debug)]
pub struct OpenUiWindows {
    windows: HashMap<UiWindowId, UiWindow>,
}

impl OpenUiWindows {
    /// Registers `window` under `id`, replacing any existing entry.
    pub fn insert(&mut self, id: UiWindowId, window: UiWindow) {
        self.windows.insert(id, window);
    }

    /// Removes the window registered under `id`, returning it if it
    /// existed.
    pub fn remove(&mut self, id: UiWindowId) -> Option<UiWindow> {
        self.windows.remove(&id)
    }

    /// Returns `true` if a window is registered under `id`.
    pub fn contains(&self, id: UiWindowId) -> bool {
        self.windows.contains_key(&id)
    }

    /// Returns `true` when at least one open window has
    /// `releases_cursor = true`.  The player-input cursor system
    /// reads this each frame.
    pub fn cursor_should_release(&self) -> bool {
        self.windows.values().any(|w| w.releases_cursor)
    }

    /// Returns `true` when at least one open window has
    /// `captures_keyboard = true`.  Reserved for future use.
    pub fn keyboard_should_capture(&self) -> bool {
        self.windows.values().any(|w| w.captures_keyboard)
    }

    /// Returns `true` when no windows are registered.
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct WindowA;
    struct WindowB;

    #[test]
    fn distinct_marker_types_get_distinct_ids() {
        assert_ne!(UiWindowId::of::<WindowA>(), UiWindowId::of::<WindowB>());
    }

    #[test]
    fn same_marker_type_gets_same_id() {
        assert_eq!(UiWindowId::of::<WindowA>(), UiWindowId::of::<WindowA>());
    }

    #[test]
    fn cursor_should_release_reflects_any_releasing_window() {
        let mut w = OpenUiWindows::default();
        assert!(!w.cursor_should_release());
        w.insert(UiWindowId::of::<WindowA>(), UiWindow::cursor_released());
        assert!(w.cursor_should_release());
        w.remove(UiWindowId::of::<WindowA>());
        assert!(!w.cursor_should_release());
    }

    #[test]
    fn cursor_stays_released_while_any_releasing_window_open() {
        let mut w = OpenUiWindows::default();
        w.insert(UiWindowId::of::<WindowA>(), UiWindow::cursor_released());
        w.insert(UiWindowId::of::<WindowB>(), UiWindow::cursor_released());
        w.remove(UiWindowId::of::<WindowA>());
        assert!(
            w.cursor_should_release(),
            "Closing one window with another still open must not relock"
        );
    }
}
