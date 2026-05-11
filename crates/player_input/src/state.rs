use bevy::prelude::*;

/// Controls how the locally-controlled player's camera and input are handled.
///
/// Toggle between modes at runtime with the **F1** key.
#[derive(States, Debug, Default, Clone, PartialEq, Eq, Hash, Reflect)]
pub enum PlayerMode {
    /// Camera is attached to the physics-driven player entity.
    /// Keyboard input feeds into [`CharacterController`] so movement is subject
    /// to gravity, block collisions, and the physics pipeline.
    ///
    /// [`CharacterController`]: dd40_character_core::controller::CharacterController
    #[default]
    Controller,
    /// Camera detaches from the player entity and flies freely.  Position is
    /// updated directly as a function of time — no physics, no collisions.
    /// The player entity remains where it was and continues to be simulated.
    FreeCam,
}
