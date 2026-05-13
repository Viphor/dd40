use bevy::state::state::States;

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    /// Initial loading state where assets are loaded.
    #[default]
    Loading,
    /// Main menu state where the player can start the game or change settings.
    Menu,
    /// Main gameplay state where the world is active.
    Playing,
}

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    /// State for the main game loop where the world is active.
    #[default]
    Running,
    /// State for when the game is paused.
    Paused,
}
