//! Generic builder-extras protocol shared by every dd40 entity builder.
//!
//! [`AddExtra`] is a tiny abstraction implemented by any builder type that
//! stores deferred component-insertion closures. It exists so that
//! capability crates (physics, networking, items, …) can provide
//! **extension traits** with a *blanket impl* on `T: AddExtra`, without
//! depending on the specific builder crate.
//!
//! For example, `dd40_physics_core` defines
//!
//! ```ignore
//! pub trait CharacterPhysicsExt {
//!     fn with_physics(self) -> Self;
//! }
//!
//! impl<T: AddExtra> CharacterPhysicsExt for T {
//!     fn with_physics(mut self) -> Self {
//!         self.add_extra(|e| { e.insert(/* physics bundle */); });
//!         self
//!     }
//! }
//! ```
//!
//! and the builder type — `dd40_character_core::CharacterBuilder` — only
//! needs to `impl AddExtra` for itself. Neither crate depends on the other.

use bevy::ecs::system::EntityCommands;

/// Builders that accept deferred per-entity insertion closures.
///
/// An "extra" is a `FnOnce(&mut EntityCommands)` closure that the builder
/// runs after its core bundle has been inserted on the spawned entity.
/// Capability crates push extras via this trait so a single
/// `.with_xxx()` call inserts whatever components they own.
///
/// # Contract
///
/// Implementors must:
///
/// 1. Run every registered extra after the entity exists and **after** the
///    builder's own core bundle has been inserted (so the entity already
///    has [`Transform`][bevy::prelude::Transform] and the builder's marker
///    components when the closure runs).
/// 2. Run extras in registration order.
///
/// The `Send + 'static` bounds match `bevy::ecs::system::Commands` requirements
/// and let extras be stored in a `Box<dyn FnOnce + Send + 'static>`.
pub trait AddExtra {
    /// Registers an insertion closure to run on the spawned entity.
    ///
    /// Returns `&mut Self` so chains of `.add_extra(...).add_extra(...)`
    /// are possible without intermediate `let` bindings.
    fn add_extra<F>(&mut self, f: F) -> &mut Self
    where
        F: FnOnce(&mut EntityCommands) + Send + 'static;
}
