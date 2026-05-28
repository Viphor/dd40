//! `bevy_enhanced_input` action state → [`CharacterInput`] translation.
//!
//! [`apply_actions_to_character_input`] is the single source of truth for
//! converting one tick's worth of action state into a per-tick movement
//! intent. It runs on **both** the server-authoritative character entity
//! and the client predicted entity, so its output must be deterministic and
//! identical on both sides — any divergence will cause constant rollback
//! corrections on the controlling client.
//!
//! ## What this module is responsible for
//!
//! - Movement, jump, sprint, attack, place, and interact intents derived
//!   from networked actions in the [`OnFoot`] context.
//!
//! ## What this module is **not** responsible for
//!
//! - Persistent camera orientation (pitch / yaw). The local client's
//!   `mouse_look` system integrates raw `Look` deltas into
//!   [`CharacterInput::yaw`] and [`CharacterInput::pitch`] directly each
//!   render frame; those values are then shipped to the server via the
//!   `PlayerInput` wire struct.
//! - Raw mouse-look deltas (`Look`). See `mouse_look`.
//! - Bindings. The keyboard / mouse → action mapping lives in
//!   `crate::bindings`; this translator is binding-agnostic.
//! - Inserting [`OnFoot`] on the player entity. That happens in
//!   `crate::bindings::spawn_local_player_input_tree`.

use bevy::prelude::*;
use bevy_enhanced_input::prelude::{Action, ActionOf};
use dd40_character_core::controller::CharacterInput;
use dd40_input_core::actions::{Attack, Interact, Jump, Move, Place, Sprint};
use dd40_input_core::contexts::OnFoot;

/// Writes the per-tick [`CharacterInput`] for every entity that owns an
/// [`OnFoot`] context, reading the BEI action state of the actions related
/// to that context.
///
/// Runs in [`FixedPreUpdate`] **after** `EnhancedInputSystems::Apply`, so
/// the action values have already been refreshed from local bindings
/// (client) or replicated input messages (server) by the time we read them.
///
/// ## Determinism
///
/// This system depends only on the value of the queried actions and the
/// previous [`CharacterInput`] state — no time, randomness, or external
/// resources. The output is therefore identical on server and client given
/// the same input state, which is required for prediction / rollback to
/// converge.
pub fn apply_actions_to_character_input(
    moves: Query<(&Action<Move>, &ActionOf<OnFoot>)>,
    jumps: Query<(&Action<Jump>, &ActionOf<OnFoot>)>,
    sprints: Query<(&Action<Sprint>, &ActionOf<OnFoot>)>,
    attacks: Query<(&Action<Attack>, &ActionOf<OnFoot>)>,
    places: Query<(&Action<Place>, &ActionOf<OnFoot>)>,
    interacts: Query<(&Action<Interact>, &ActionOf<OnFoot>)>,
    mut characters: Query<(Entity, &mut CharacterInput), With<OnFoot>>,
) {
    for (entity, mut input) in &mut characters {
        let movement_local = action_value_for(entity, &moves).unwrap_or(Vec2::ZERO);
        let yaw_rot = Quat::from_rotation_y(input.yaw);
        let forward = yaw_rot * Vec3::NEG_Z;
        let right = yaw_rot * Vec3::X;
        let direction = right * movement_local.x + forward * movement_local.y;
        input.movement = direction.normalize_or_zero();

        input.jump = action_value_for(entity, &jumps).unwrap_or(false);
        input.sprint = action_value_for(entity, &sprints).unwrap_or(false);
        input.attack = action_value_for(entity, &attacks).unwrap_or(false);
        input.place = action_value_for(entity, &places).unwrap_or(false);
        input.interact = action_value_for(entity, &interacts).unwrap_or(false);
    }
}

/// Finds the action that belongs to `owner` via [`ActionOf`] and returns
/// the dereferenced action value, or [`None`] if no action of that type is
/// related to the owner.
fn action_value_for<A>(
    owner: Entity,
    query: &Query<(&Action<A>, &ActionOf<OnFoot>)>,
) -> Option<A::Output>
where
    A: bevy_enhanced_input::prelude::InputAction,
    A::Output: Copy,
{
    query
        .iter()
        .find_map(|(action, of)| (**of == owner).then(|| **action))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_enhanced_input::context::InputContextAppExt;
    use bevy_enhanced_input::prelude::*;
    use dd40_input_core::plugin::InputCorePlugin;

    fn new_app() -> App {
        let mut app = App::new();
        app.add_plugins(InputCorePlugin);
        app.add_input_context::<OnFoot>();
        app.finish();
        app.add_systems(FixedPreUpdate, apply_actions_to_character_input);
        app
    }

    /// Spawn a player entity with `OnFoot` and the six networked actions,
    /// each with its default action value.
    fn spawn_player(app: &mut App) -> Entity {
        let player = app
            .world_mut()
            .spawn((OnFoot, CharacterInput::default()))
            .id();
        app.world_mut()
            .spawn((Action::<Move>::new(), ActionOf::<OnFoot>::new(player)));
        app.world_mut()
            .spawn((Action::<Jump>::new(), ActionOf::<OnFoot>::new(player)));
        app.world_mut()
            .spawn((Action::<Sprint>::new(), ActionOf::<OnFoot>::new(player)));
        app.world_mut()
            .spawn((Action::<Attack>::new(), ActionOf::<OnFoot>::new(player)));
        app.world_mut()
            .spawn((Action::<Place>::new(), ActionOf::<OnFoot>::new(player)));
        app.world_mut()
            .spawn((Action::<Interact>::new(), ActionOf::<OnFoot>::new(player)));
        player
    }

    /// Overwrites the typed action value (the same field BEI's apply pass
    /// would normally populate from bindings or replicated input).
    fn set_action<A: InputAction>(app: &mut App, owner: Entity, value: A::Output) {
        let world = app.world_mut();
        let mut query = world.query::<(&mut Action<A>, &ActionOf<OnFoot>)>();
        for (mut action, of) in query.iter_mut(world) {
            if **of == owner {
                **action = value;
            }
        }
    }

    #[test]
    fn movement_is_remapped_to_world_space_using_yaw() {
        let mut app = new_app();
        let player = spawn_player(&mut app);
        // yaw=0 → forward is -Z, right is +X. WASD: x=strafe, y=forward.
        set_action::<Move>(&mut app, player, Vec2::new(1.0, 2.0));
        app.world_mut().run_schedule(FixedPreUpdate);

        let ci = app.world().entity(player).get::<CharacterInput>().unwrap();
        let expected = Vec3::new(1.0, 0.0, -2.0).normalize();
        assert!(
            (ci.movement - expected).length() < 1e-5,
            "movement = {:?}, expected = {:?}",
            ci.movement,
            expected
        );
    }

    #[test]
    fn movement_rotates_with_yaw() {
        let mut app = new_app();
        let player = spawn_player(&mut app);
        // 90° left turn: forward axis rotates from -Z to +X (right-handed
        // about Y). Set yaw directly on CharacterInput — this is what
        // mouse_look does on the client.
        {
            let mut entity = app.world_mut().entity_mut(player);
            let mut ci = entity.get_mut::<CharacterInput>().unwrap();
            ci.yaw = std::f32::consts::FRAC_PI_2;
        }
        // Pure forward intent.
        set_action::<Move>(&mut app, player, Vec2::new(0.0, 1.0));
        app.world_mut().run_schedule(FixedPreUpdate);

        let ci = app.world().entity(player).get::<CharacterInput>().unwrap();
        let expected = Vec3::new(-1.0, 0.0, 0.0);
        assert!(
            (ci.movement - expected).length() < 1e-5,
            "movement = {:?}, expected = {:?}",
            ci.movement,
            expected
        );
    }

    #[test]
    fn propagates_action_triple() {
        let mut app = new_app();
        let player = spawn_player(&mut app);
        set_action::<Attack>(&mut app, player, true);
        set_action::<Place>(&mut app, player, true);
        set_action::<Interact>(&mut app, player, true);
        app.world_mut().run_schedule(FixedPreUpdate);

        let ci = app.world().entity(player).get::<CharacterInput>().unwrap();
        assert!(ci.attack);
        assert!(ci.place);
        assert!(ci.interact);
    }

    #[test]
    fn clears_action_triple_when_input_is_false() {
        let mut app = new_app();
        let player = spawn_player(&mut app);
        {
            let mut entity = app.world_mut().entity_mut(player);
            let mut ci = entity.get_mut::<CharacterInput>().unwrap();
            ci.attack = true;
            ci.place = true;
            ci.interact = true;
        }
        app.world_mut().run_schedule(FixedPreUpdate);

        let ci = app.world().entity(player).get::<CharacterInput>().unwrap();
        assert!(!ci.attack, "stale attack must be cleared");
        assert!(!ci.place, "stale place must be cleared");
        assert!(!ci.interact, "stale interact must be cleared");
    }

    #[test]
    fn propagates_jump_and_sprint() {
        let mut app = new_app();
        let player = spawn_player(&mut app);
        set_action::<Jump>(&mut app, player, true);
        set_action::<Sprint>(&mut app, player, true);
        app.world_mut().run_schedule(FixedPreUpdate);

        let ci = app.world().entity(player).get::<CharacterInput>().unwrap();
        assert!(ci.jump);
        assert!(ci.sprint);
    }

    #[test]
    fn no_onfoot_context_is_a_noop() {
        let mut app = new_app();
        let entity = app
            .world_mut()
            .spawn(CharacterInput {
                attack: true,
                ..Default::default()
            })
            .id();
        app.world_mut().run_schedule(FixedPreUpdate);

        let ci = app.world().entity(entity).get::<CharacterInput>().unwrap();
        assert!(ci.attack, "non-OnFoot entity must not be touched");
    }
}
