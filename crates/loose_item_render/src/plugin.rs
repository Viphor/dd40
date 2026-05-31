//! Client-only plugin that gives loose items their spinning,
//! bobbing visual.

use bevy::prelude::*;
use dd40_core::block::registry::BlockRegistry;
use dd40_core::ensure_plugins;
use dd40_core::plugin::CorePlugin;
use dd40_item_core::plugin::ItemCorePlugin;
use dd40_item_core::registry::ItemRegistry;
use dd40_loose_item_core::{LooseItem, plugin::LooseItemCorePlugin};

const SPIN_RATE: f32 = std::f32::consts::TAU * 0.15;
const BOB_AMPLITUDE: f32 = 0.05;
const BOB_FREQUENCY: f32 = 0.4;
const BOB_BASE_HEIGHT: f32 = 0.35;
const CUBE_SIZE: f32 = 0.25;
const FALLBACK_COLOR: Color = Color::srgb(0.8, 0.8, 0.8);

/// Marker for the child entity that carries the spinning + bobbing
/// mesh.  Carries a per-entity phase so neighbouring items don't bob
/// in lock-step.
#[derive(Component, Debug, Clone, Copy)]
pub struct LooseItemVisual {
    /// Phase offset (radians) applied to the bob sine wave.
    pub phase: f32,
}

/// Marker on the parent loose-item entity, indicating that a visual
/// child has already been spawned.  Stops [`attach_visuals`] from
/// spawning duplicate children.
#[derive(Component, Debug, Clone, Copy)]
struct VisualAttached;

#[derive(Resource)]
struct LooseItemAssets {
    mesh: Handle<Mesh>,
}

fn setup_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let mesh = meshes.add(Cuboid::new(CUBE_SIZE, CUBE_SIZE, CUBE_SIZE));
    commands.insert_resource(LooseItemAssets { mesh });
}

/// Spawns a child visual for every newly-seen [`LooseItem`] that
/// already has a [`Transform`] (i.e. is the interpolated network copy
/// — the confirmed copy is invisible and not in the transform stack).
fn attach_visuals(
    mut commands: Commands,
    assets: Res<LooseItemAssets>,
    item_registry: Res<ItemRegistry>,
    block_registry: Res<BlockRegistry>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    new: Query<(Entity, &LooseItem), (Without<VisualAttached>, With<Transform>)>,
) {
    for (parent, loose) in &new {
        let color = resolve_color(loose, &item_registry, &block_registry);
        let material = materials.add(StandardMaterial {
            base_color: color,
            perceptual_roughness: 0.6,
            ..default()
        });
        let phase = phase_for(parent);
        commands.entity(parent).insert(VisualAttached);
        commands.spawn((
            ChildOf(parent),
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(material),
            Transform::default(),
            LooseItemVisual { phase },
        ));
    }
}

fn resolve_color(
    loose: &LooseItem,
    item_registry: &ItemRegistry,
    block_registry: &BlockRegistry,
) -> Color {
    let Some(item_def) = item_registry.get(loose.stack.item) else {
        return FALLBACK_COLOR;
    };
    let Some(block_id) = item_def.placeable else {
        return FALLBACK_COLOR;
    };
    block_registry
        .get(block_id)
        .map(|def| def.color)
        .unwrap_or(FALLBACK_COLOR)
}

/// Spins each visual child around Y and offsets its local Y by a
/// gentle sine wave.  The parent Transform — driven by the network
/// bridge — is left untouched.
fn animate_visuals(time: Res<Time>, mut q: Query<(&LooseItemVisual, &mut Transform)>) {
    let elapsed = time.elapsed_secs();
    let spin_delta = SPIN_RATE * time.delta_secs();
    for (visual, mut transform) in &mut q {
        transform.rotate_y(spin_delta);
        let bob = ((elapsed + visual.phase) * BOB_FREQUENCY * std::f32::consts::TAU).sin();
        transform.translation.y = BOB_BASE_HEIGHT + bob * BOB_AMPLITUDE;
    }
}

fn phase_for(entity: Entity) -> f32 {
    (entity.index_u32() as f32 * 0.7) % std::f32::consts::TAU
}

/// Client-only plugin that adds loose-item visuals + animation.
#[derive(Default)]
pub struct LooseItemRenderPlugin;

impl Plugin for LooseItemRenderPlugin {
    fn build(&self, app: &mut App) {
        ensure_plugins!(app, CorePlugin, ItemCorePlugin, LooseItemCorePlugin);
        #[cfg(feature = "textures")]
        ensure_plugins!(app, dd40_texture_core::TextureCorePlugin);

        app.add_systems(Startup, setup_assets)
            .add_systems(Update, (attach_visuals, animate_visuals));
    }
}
