use bevy::{
    app::{App, Plugin, PostUpdate},
    ecs::resource::Resource,
};
use dd40_core::plugin::CorePlugin;

use crate::generators::{WorldGenerator, generate_chunks};

pub struct WorldPlugin<G: WorldGenerator + Resource + Clone> {
    generator: G,
}

impl<G: WorldGenerator + Resource + Clone> WorldPlugin<G> {
    pub fn new(generator: G) -> Self {
        Self { generator }
    }
}

impl<G: WorldGenerator + Resource + Clone> Plugin for WorldPlugin<G> {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<CorePlugin>() {
            panic!("WorldPlugin requires CorePlugin to be added first");
        }

        app.insert_resource(self.generator.clone())
            .add_systems(PostUpdate, generate_chunks::<G>);
    }
}
