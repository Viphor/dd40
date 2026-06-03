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
#[cfg(not(feature = "textures"))]
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

/// Spawns a child visual, using a texture tile extracted from the
/// [`BlockAtlas`] when one is available.  Falls back to colour when
/// the atlas is not yet ready or the block has no texture data.
///
/// Uses a single `ResMut<Assets<Image>>`: atlas pixels are extracted
/// into an owned buffer (borrow dropped) before adding the new tile
/// image, avoiding Bevy B0002.
#[cfg(feature = "textures")]
fn attach_visuals(
    mut commands: Commands,
    assets: Res<LooseItemAssets>,
    item_registry: Res<ItemRegistry>,
    block_registry: Res<BlockRegistry>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    atlas: Res<dd40_texture_core::BlockAtlas>,
    new: Query<(Entity, &LooseItem), (Without<VisualAttached>, With<Transform>)>,
) {
    for (parent, loose) in &new {
        let material = try_build_textured_material(
            loose,
            &item_registry,
            &block_registry,
            &atlas,
            &mut images,
        )
        .unwrap_or_else(|| {
            let color = resolve_color(loose, &item_registry, &block_registry);
            StandardMaterial {
                base_color: color,
                perceptual_roughness: 0.6,
                ..default()
            }
        });
        let material_handle = materials.add(material);
        let phase = phase_for(parent);
        commands.entity(parent).insert(VisualAttached);
        commands.spawn((
            ChildOf(parent),
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(material_handle),
            Transform::default(),
            LooseItemVisual { phase },
        ));
    }
}

/// Tries to build a [`StandardMaterial`] with the block's top-face
/// texture as the base colour texture.
///
/// Returns `None` when the atlas isn't ready, the item isn't a
/// placeable block, or the block has no `BlockTextures`.
///
/// A single `&mut Assets<Image>` is used for both reading (atlas
/// pixels are cloned into an owned buffer before the borrow is
/// released) and writing (adding the new tile image).
#[cfg(feature = "textures")]
fn try_build_textured_material(
    loose: &LooseItem,
    item_registry: &ItemRegistry,
    block_registry: &BlockRegistry,
    atlas: &dd40_texture_core::BlockAtlas,
    images: &mut Assets<Image>,
) -> Option<StandardMaterial> {
    use dd40_texture_core::{BlockTextures, Face};
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

    if !atlas.is_ready() {
        return None;
    }

    let item_def = item_registry.get(loose.stack.item)?;
    let block_id = item_def.placeable?;
    let block_def = block_registry.get(block_id)?;
    let textures = block_def.data::<BlockTextures>()?;

    // Pick the top face; fall back through a priority list.
    let tex_ref = textures.get(Face::Top)
        .or_else(|| textures.get(Face::South))
        .or_else(|| textures.get(Face::North))?;

    let resolved = atlas.resolve(tex_ref)?;
    let atlas_handle = atlas.texture(dd40_texture_core::AtlasId(0))?;

    // Extract pixels into an owned buffer while holding a shared borrow of
    // the atlas image, then drop the borrow before calling images.add().
    let (pixels, tile_w, tile_h) = {
        let atlas_image = images.get(&atlas_handle)?;
        let pixels = resolved.uv.extract_tile_pixels(atlas_image)?;
        let tile_w = ((resolved.uv.max.x - resolved.uv.min.x) * atlas_image.width() as f32)
            .round() as u32;
        let tile_h = ((resolved.uv.max.y - resolved.uv.min.y) * atlas_image.height() as f32)
            .round() as u32;
        (pixels, tile_w, tile_h)
    };

    if tile_w == 0 || tile_h == 0 {
        return None;
    }

    let tile_image = bevy::image::Image::new(
        Extent3d {
            width: tile_w,
            height: tile_h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let handle = images.add(tile_image);

    Some(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(handle),
        perceptual_roughness: 0.6,
        ..default()
    })
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

