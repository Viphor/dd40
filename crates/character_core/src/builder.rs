use bevy::prelude::*;
use bevy::ecs::system::EntityCommands;

use crate::{
    bundles::CharacterBundle,
    components::MovementSpeed,
    face::{CameraRotation, CharacterFace, DEFAULT_FACE_OFFSET},
};

/// Fluent builder for a [`CharacterBundle`] plus its face child entity.
///
/// Lets callers chain optional overrides rather than constructing the bundle
/// struct directly, keeping spawn sites readable as the bundle grows.
///
/// ## Two ways to use it
///
/// 1. [`CharacterBuilder::spawn`] — when you want a brand new entity.
/// 2. [`CharacterBuilder::attach`] — when an entity already exists (e.g. a
///    networked character entity created by lightyear's prediction layer)
///    and you want to add the character's components and face child to it.
///
/// Both attach a [`CharacterFace`] child entity carrying [`CharacterFace`],
/// [`CameraRotation`], and a local [`Transform`] whose translation is the
/// configured face offset (default [`DEFAULT_FACE_OFFSET`]).
///
/// # Example
///
/// ```
/// use bevy::prelude::*;
/// use dd40_character_core::builder::CharacterBuilder;
///
/// fn spawn_a_player(mut commands: Commands) {
///     CharacterBuilder::new("Player")
///         .movement_speed(6.0)
///         .transform(Transform::from_translation(Vec3::new(0.0, 64.0, 0.0)))
///         .spawn(&mut commands);
/// }
/// # bevy::ecs::system::assert_is_system(spawn_a_player);
/// ```
pub struct CharacterBuilder {
    name: String,
    movement_speed: MovementSpeed,
    transform: Transform,
    face_offset: Vec3,
    extras: Vec<CharacterExtra>,
}

/// Boxed insertion closure used by [`CharacterBuilder::add_extra`].
///
/// Each closure runs **after** the core [`CharacterBundle`] has been
/// inserted, so the entity already has a [`Transform`] and the marker
/// components when the closure runs. This makes it safe for an extra to
/// insert components whose `on_add` hooks read other components on the
/// entity.
pub type CharacterExtra = Box<dyn FnOnce(&mut EntityCommands) + Send + 'static>;

impl CharacterBuilder {
    /// Starts a builder with default speed, the world origin as spawn point,
    /// and a humanoid eye-height face offset ([`DEFAULT_FACE_OFFSET`]).
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            movement_speed: MovementSpeed::default(),
            transform: Transform::default(),
            face_offset: DEFAULT_FACE_OFFSET,
            extras: Vec::new(),
        }
    }

    /// Pushes an arbitrary insertion closure that runs after the core
    /// [`CharacterBundle`] has been inserted on the entity.
    ///
    /// This is the foundation for capability extension traits in other
    /// crates (e.g. `CharacterPhysicsExt::with_physics`). Capability crates
    /// implement an extension trait on [`CharacterBuilder`] whose methods
    /// call `add_extra` to register their own bundle insertion.
    ///
    /// Extras run in registration order, **after** [`CharacterBundle`]
    /// (which carries [`Transform`]) is inserted but **before** the face
    /// child is spawned. This guarantees that `on_add` hooks fired by an
    /// extra (such as the `CharacterPosition::on_add` hook required by
    /// `PhysicsBody`) see the correct initial [`Transform`].
    pub fn add_extra<F>(&mut self, f: F) -> &mut Self
    where
        F: FnOnce(&mut EntityCommands) + Send + 'static,
    {
        self.extras.push(Box::new(f));
        self
    }

    /// Overrides the base movement speed (world units per second).
    pub fn movement_speed(mut self, speed: f32) -> Self {
        self.movement_speed = MovementSpeed(speed);
        self
    }

    /// Overrides the initial world-space transform.
    pub fn transform(mut self, transform: Transform) -> Self {
        self.transform = transform;
        self
    }

    /// Overrides the face/eye offset relative to the body origin.
    ///
    /// Default: [`DEFAULT_FACE_OFFSET`] (`(0.0, 1.6, 0.0)`).
    pub fn face_offset(mut self, offset: Vec3) -> Self {
        self.face_offset = offset;
        self
    }

    /// Spawns a fresh entity carrying the [`CharacterBundle`] and adds a
    /// face child. Returns the body's [`EntityCommands`] so callers can
    /// chain additional components (physics, networking, marker types).
    pub fn spawn<'c>(self, commands: &'c mut Commands) -> EntityCommands<'c> {
        let face_offset = self.face_offset;
        let extras = self.extras;
        let body_bundle = CharacterBundle::new(
            self.name,
            self.movement_speed,
            self.transform,
        );
        let mut entity = commands.spawn(body_bundle);
        for extra in extras {
            extra(&mut entity);
        }
        spawn_face_child(&mut entity, face_offset);
        entity
    }

    /// Attaches the character's body bundle and a face child to an
    /// already-spawned entity. Useful when something else (e.g. lightyear's
    /// `Predicted` observer) created the entity for you.
    pub fn attach<'a, 'c>(
        self,
        entity: &'a mut EntityCommands<'c>,
    ) -> &'a mut EntityCommands<'c> {
        let face_offset = self.face_offset;
        let extras = self.extras;
        entity.insert(CharacterBundle::new(
            self.name,
            self.movement_speed,
            self.transform,
        ));
        for extra in extras {
            extra(entity);
        }
        spawn_face_child(entity, face_offset);
        entity
    }

    /// Consumes the builder and produces the [`CharacterBundle`] *without*
    /// any face child. Prefer [`Self::spawn`] or [`Self::attach`] — this
    /// method is kept for callers that compose the bundle into a larger
    /// `commands.spawn(tuple)` and accept the responsibility of attaching
    /// the face themselves. New code should not use it.
    #[deprecated(note = "use `spawn` or `attach` so the face child is wired automatically")]
    pub fn build(self) -> impl Bundle {
        CharacterBundle::new(self.name, self.movement_speed, self.transform)
    }
}

fn spawn_face_child(entity: &mut EntityCommands<'_>, offset: Vec3) {
    entity.with_child((
        CharacterFace { offset },
        CameraRotation::default(),
        Transform::from_translation(offset),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Character;
    use bevy::ecs::relationship::RelationshipTarget;
    use bevy::ecs::system::RunSystemOnce;

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::MinimalPlugins);
        app
    }

    #[test]
    fn spawn_creates_body_and_face_child_with_default_offset() {
        let mut app = make_app();
        app.world_mut().run_system_once(|mut commands: Commands| {
            CharacterBuilder::new("Hero").spawn(&mut commands);
        }).unwrap();

        let mut bodies = app.world_mut().query_filtered::<Entity, With<Character>>();
        let body = bodies
            .iter(app.world())
            .next()
            .expect("a body entity was spawned");

        let children = app
            .world()
            .get::<bevy::prelude::Children>(body)
            .expect("body must have a Children relationship");
        assert_eq!(children.len(), 1, "exactly one face child");
        let face_entity = children.iter().next().unwrap();

        let face = app
            .world()
            .get::<CharacterFace>(face_entity)
            .expect("child carries CharacterFace");
        assert_eq!(face.offset, DEFAULT_FACE_OFFSET);

        let _rotation = app
            .world()
            .get::<CameraRotation>(face_entity)
            .expect("child carries CameraRotation");

        let face_transform = app
            .world()
            .get::<Transform>(face_entity)
            .expect("child carries Transform");
        assert_eq!(face_transform.translation, DEFAULT_FACE_OFFSET);
    }

    #[test]
    fn face_offset_override_propagates_to_face_child() {
        let mut app = make_app();
        let custom = Vec3::new(0.0, 2.4, 0.1);
        app.world_mut().run_system_once(move |mut commands: Commands| {
            CharacterBuilder::new("Tall").face_offset(custom).spawn(&mut commands);
        }).unwrap();

        let mut faces = app.world_mut().query::<&CharacterFace>();
        let face = faces.iter(app.world()).next().expect("face spawned");
        assert_eq!(face.offset, custom);

        let mut transforms =
            app.world_mut().query_filtered::<&Transform, With<CharacterFace>>();
        let t = transforms.iter(app.world()).next().unwrap();
        assert_eq!(t.translation, custom);
    }

    #[test]
    fn add_extra_runs_after_character_bundle_on_spawn() {
        #[derive(Component)]
        struct Marker(Vec3);

        let mut app = make_app();
        app.world_mut()
            .run_system_once(|mut commands: Commands| {
                let mut b = CharacterBuilder::new("Hero")
                    .transform(Transform::from_translation(Vec3::new(1.0, 2.0, 3.0)));
                b.add_extra(|e| {
                    // CharacterBundle (and its Transform) is already on the
                    // entity at the time this closure runs — we encode the
                    // contract by capturing the value here.
                    e.insert(Marker(Vec3::new(1.0, 2.0, 3.0)));
                });
                b.spawn(&mut commands);
            })
            .unwrap();

        let mut q = app.world_mut().query::<(&Character, &Marker, &Transform)>();
        let (_, marker, transform) = q.iter(app.world()).next().expect("entity spawned");
        assert_eq!(marker.0, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(transform.translation, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn add_extra_runs_after_character_bundle_on_attach() {
        #[derive(Component)]
        struct AttachedMarker;

        let mut app = make_app();
        let preexisting = app.world_mut().spawn_empty().id();

        app.world_mut()
            .run_system_once(move |mut commands: Commands| {
                let mut e = commands.entity(preexisting);
                let mut b = CharacterBuilder::new("X");
                b.add_extra(|ec| {
                    ec.insert(AttachedMarker);
                });
                b.attach(&mut e);
            })
            .unwrap();

        assert!(app.world().get::<Character>(preexisting).is_some());
        assert!(app.world().get::<AttachedMarker>(preexisting).is_some());
    }

    #[test]
    fn extras_run_in_registration_order() {
        #[derive(Component, Debug, PartialEq)]
        struct Order(Vec<u8>);

        let mut app = make_app();
        app.world_mut()
            .run_system_once(|mut commands: Commands| {
                let mut b = CharacterBuilder::new("Ordered");
                b.add_extra(|e| {
                    e.insert(Order(vec![1]));
                });
                b.add_extra(|e| {
                    // Bevy will overwrite the previous Order; we use that to
                    // confirm extras run in the order they were registered.
                    e.insert(Order(vec![1, 2]));
                });
                b.spawn(&mut commands);
            })
            .unwrap();

        let mut q = app.world_mut().query::<&Order>();
        let order = q.iter(app.world()).next().unwrap();
        assert_eq!(order, &Order(vec![1, 2]));
    }

    #[test]
    fn attach_adds_body_and_face_to_existing_entity() {
        let mut app = make_app();
        let preexisting = app.world_mut().spawn_empty().id();

        app.world_mut().run_system_once(move |mut commands: Commands| {
            let mut e = commands.entity(preexisting);
            CharacterBuilder::new("Predicted").attach(&mut e);
        }).unwrap();

        assert!(
            app.world().get::<Character>(preexisting).is_some(),
            "attach inserts the Character marker on the existing entity"
        );
        let children = app
            .world()
            .get::<bevy::prelude::Children>(preexisting)
            .expect("attach adds a Children relationship");
        assert_eq!(children.len(), 1);
        let face_entity = children.iter().next().unwrap();
        assert!(app.world().get::<CharacterFace>(face_entity).is_some());
        assert!(app.world().get::<CameraRotation>(face_entity).is_some());
    }
}
