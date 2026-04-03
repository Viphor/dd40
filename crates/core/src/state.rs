use bevy::state::state::States;

#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppState {
    /// Initial loading state where assets are loaded.
    Loading,
    /// Main menu state where the player can start the game or change settings.
    Menu,
    /// Main gameplay state where the world is active.
    Playing,
}

impl Default for AppState {
    fn default() -> Self {
        AppState::Playing
    }
}

#[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameState {
    /// State for the main game loop where the world is active.
    Running,
    /// State for when the game is paused.
    Paused,
}

impl Default for GameState {
    fn default() -> Self {
        GameState::Running
    }
}
