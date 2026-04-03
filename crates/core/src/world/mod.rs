use bevy::ecs::schedule::SystemSet;

/// System set for world generation systems.
/// All world generation should run in this set, after BlockRegistrySet.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorldGenerationSet;
