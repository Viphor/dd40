//! Builder extension trait for adding a physics body to any builder.
//!
//! [`CharacterPhysicsExt`] is a blanket-implemented extension trait on
//! [`AddExtra`][dd40_core::builder_extra::AddExtra]: any builder type that
//! supports the extras protocol gains [`CharacterPhysicsExt::with_physics`]
//! and [`CharacterPhysicsExt::with_physics_config`] for free, without that
//! builder's crate having to know about `dd40_physics_core`.
//!
//! # Example
//!
//! ```ignore
//! use dd40_character_core::builder::CharacterBuilder;
//! use dd40_physics_core::prelude::CharacterPhysicsExt;
//!
//! CharacterBuilder::new("Hero")
//!     .with_physics()         // ← provided by this trait
//!     .with_controller()
//!     .spawn(&mut commands);
//! ```

use bevy::prelude::*;
use dd40_core::builder_extra::AddExtra;

use crate::components::{Aabb, CharacterCollider, PhysicsBody};

/// Caller-tunable parameters for [`CharacterPhysicsExt::with_physics_config`].
///
/// All fields have sane defaults via [`CharacterPhysicsConfig::default`]
/// matching the canonical player shape, so callers that just want a
/// player-sized body should prefer the parameterless
/// [`CharacterPhysicsExt::with_physics`].
///
/// # Example
///
/// ```ignore
/// use dd40_physics_core::components::Aabb;
/// use dd40_physics_core::prelude::CharacterPhysicsConfig;
///
/// let cfg = CharacterPhysicsConfig {
///     collider: Aabb::new(0.4, 1.2, 0.4), // taller, slightly wider
/// };
/// ```
#[derive(Debug, Clone, Copy)]
pub struct CharacterPhysicsConfig {
    /// Axis-aligned bounding box used for collision queries.
    ///
    /// Defaults to [`Aabb::player`] (0.3 × 0.9 × 0.3 half-extents).
    pub collider: Aabb,
}

impl Default for CharacterPhysicsConfig {
    fn default() -> Self {
        Self {
            collider: Aabb::player(),
        }
    }
}

/// Adds a physics body bundle to any builder that implements
/// [`AddExtra`][dd40_core::builder_extra::AddExtra].
///
/// Inserts:
///
/// - [`PhysicsBody`] — marks the entity as participating in physics.
///   Auto-requires `Velocity`, `GravityScale`, `Grounded`, `Impulse`,
///   `CharacterPosition`. The `CharacterPosition::on_add` hook reads the
///   entity's `Transform` at insert time, which is why builders should
///   guarantee `Transform` is present **before** running extras (see the
///   [`AddExtra`][dd40_core::builder_extra::AddExtra] contract).
/// - [`CharacterCollider`] — marker enabling per-frame block collision.
/// - [`Aabb`] — the collision shape (player-sized by default; configurable
///   via [`Self::with_physics_config`]).
///
/// Does **not** add character-controller intent components like
/// `CharacterInput` / `CharacterController` / `JumpImpulse` — those live
/// in `dd40_character_core` and are added via
/// `CharacterBuilder::with_controller()`. Splitting the two lets non-
/// character physics bodies (loose blocks, falling debris) reuse the same
/// `with_physics` method without dragging in character intent state.
pub trait CharacterPhysicsExt: Sized {
    /// Adds a player-sized physics body
    /// ([`CharacterPhysicsConfig::default`]).
    fn with_physics(self) -> Self {
        self.with_physics_config(CharacterPhysicsConfig::default())
    }

    /// Adds a physics body using caller-provided configuration.
    fn with_physics_config(self, config: CharacterPhysicsConfig) -> Self;
}

impl<T> CharacterPhysicsExt for T
where
    T: AddExtra,
{
    fn with_physics_config(mut self, config: CharacterPhysicsConfig) -> Self {
        let collider = config.collider;
        self.add_extra(move |e| {
            e.insert((PhysicsBody, CharacterCollider, collider));
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::{Commands, EntityCommands, RunSystemOnce};

    /// Tiny test-only builder so the physics_core tests don't depend on
    /// `dd40_character_core`. It only carries the bare minimum the
    /// [`CharacterPhysicsExt`] blanket impl requires: an extras vector
    /// and a `Transform` insert before extras run (so
    /// `CharacterPosition::on_add` sees the right value).
    struct TestBuilder {
        transform: Transform,
        extras: Vec<Box<dyn FnOnce(&mut EntityCommands) + Send + 'static>>,
    }

    impl TestBuilder {
        fn new(transform: Transform) -> Self {
            Self {
                transform,
                extras: Vec::new(),
            }
        }

        fn spawn<'c>(self, commands: &'c mut Commands) -> EntityCommands<'c> {
            let mut e = commands.spawn(self.transform);
            for extra in self.extras {
                extra(&mut e);
            }
            e
        }
    }

    impl AddExtra for TestBuilder {
        fn add_extra<F>(&mut self, f: F) -> &mut Self
        where
            F: FnOnce(&mut EntityCommands) + Send + 'static,
        {
            self.extras.push(Box::new(f));
            self
        }
    }

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins);
        crate::plugin::PhysicsCorePlugin.build(&mut app);
        app
    }

    #[test]
    fn with_physics_inserts_physics_body_collider_and_default_aabb() {
        let mut app = make_app();
        app.world_mut()
            .run_system_once(|mut commands: Commands| {
                TestBuilder::new(Transform::from_translation(Vec3::new(1.0, 2.0, 3.0)))
                    .with_physics()
                    .spawn(&mut commands);
            })
            .unwrap();

        let mut q = app
            .world_mut()
            .query::<(&PhysicsBody, &CharacterCollider, &Aabb, &Transform)>();
        let (_, _, aabb, transform) = q.iter(app.world()).next().expect("entity spawned");
        assert_eq!(aabb.half_x, 0.3);
        assert_eq!(aabb.half_y, 0.9);
        assert_eq!(aabb.half_z, 0.3);
        assert_eq!(transform.translation, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn with_physics_config_uses_caller_supplied_aabb() {
        let mut app = make_app();
        app.world_mut()
            .run_system_once(|mut commands: Commands| {
                let cfg = CharacterPhysicsConfig {
                    collider: Aabb::new(0.4, 1.2, 0.5),
                };
                TestBuilder::new(Transform::default())
                    .with_physics_config(cfg)
                    .spawn(&mut commands);
            })
            .unwrap();

        let mut q = app.world_mut().query::<&Aabb>();
        let aabb = q.iter(app.world()).next().expect("entity spawned");
        assert_eq!(aabb.half_x, 0.4);
        assert_eq!(aabb.half_y, 1.2);
        assert_eq!(aabb.half_z, 0.5);
    }

    #[test]
    fn character_position_picks_up_transform_inserted_before_extras() {
        use crate::components::CharacterPosition;

        let mut app = make_app();
        app.world_mut()
            .run_system_once(|mut commands: Commands| {
                TestBuilder::new(Transform::from_translation(Vec3::new(10.0, 20.0, 30.0)))
                    .with_physics()
                    .spawn(&mut commands);
            })
            .unwrap();

        let mut q = app.world_mut().query::<&CharacterPosition>();
        let pos = q.iter(app.world()).next().expect("entity spawned");
        assert_eq!(
            pos.0,
            Vec3::new(10.0, 20.0, 30.0),
            "CharacterPosition::on_add must see the Transform inserted before extras"
        );
    }
}
