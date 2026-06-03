//! Client-only plugin that gives loose items their spinning, bobbing visual.

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

/// Marker for the entity that carries the spinning + bobbing transform.
#[derive(Component, Debug, Clone, Copy)]
pub struct LooseItemVisual {
    /// Phase offset (radians) applied to the bob sine wave.
    pub phase: f32,
}

/// Marker on the parent loose-item entity to prevent duplicate children.
#[derive(Component, Debug, Clone, Copy)]
struct VisualAttached;

/// Shared mesh handles created at startup.
#[derive(Resource)]
struct LooseItemAssets {
    /// Cube mesh with per-face-group UVs (top column 0, sides column 1, bottom column 2).
    mesh: Handle<Mesh>,
}

fn setup_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let mesh = meshes.add(cube_mesh_per_face_uvs(CUBE_SIZE));
    commands.insert_resource(LooseItemAssets { mesh });
}

/// Spawns a child visual for every newly-seen [`LooseItem`] with a [`Transform`].
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

/// Spawns a child visual using textures from the [`BlockAtlas`] when available.
///
/// The cube mesh has per-face-group UV columns so a single packed image
/// (top tile | side tile | bottom tile) covers all 6 faces correctly.
/// Falls back to a flat colour cube when the atlas is not ready or the block
/// has no [`dd40_texture_core::BlockTextures`].
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
        let mat_handle = materials.add(material);
        let phase = phase_for(parent);
        commands.entity(parent).insert(VisualAttached);
        commands.spawn((
            ChildOf(parent),
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(mat_handle),
            Transform::default(),
            LooseItemVisual { phase },
        ));
    }
}

/// Builds a [`StandardMaterial`] with a packed top/side/bottom texture.
///
/// Uses a single `ResMut<Assets<Image>>`: atlas pixels are extracted into an
/// owned buffer (borrow dropped) before `images.add()`, avoiding B0002.
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

    if !atlas.is_ready() { return None; }

    let item_def = item_registry.get(loose.stack.item)?;
    let block_id = item_def.placeable?;
    let block_def = block_registry.get(block_id)?;
    let textures = block_def.data::<BlockTextures>()?;

    let block_rgb = {
        let s = block_def.color.to_srgba();
        let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;
        [b(s.red), b(s.green), b(s.blue)]
    };

    let atlas_handle = atlas.texture(dd40_texture_core::AtlasId(0))?;

    // Extract pixels into owned buffers while atlas image is briefly borrowed.
    struct FaceData { pixels: Vec<u8>, w: u32, h: u32, tinted: bool }

    let extract_face = |face: Face| -> Option<FaceData> {
        let tex_ref = textures.get(face)?;
        let resolved = atlas.resolve(tex_ref)?;
        let atlas_image = images.get(&atlas_handle)?;
        let pixels = resolved.uv.extract_tile_pixels(atlas_image)?;
        let w = ((resolved.uv.max.x - resolved.uv.min.x) * atlas_image.width() as f32).round() as u32;
        let h = ((resolved.uv.max.y - resolved.uv.min.y) * atlas_image.height() as f32).round() as u32;
        if w == 0 || h == 0 { return None; }
        Some(FaceData { pixels, w, h, tinted: textures.tinted_for(face) })
    };

    // Representative side: South → North → East → West fallback chain.
    let top = extract_face(Face::Top);
    let side = extract_face(Face::South)
        .or_else(|| extract_face(Face::North))
        .or_else(|| extract_face(Face::East))
        .or_else(|| extract_face(Face::West))
        .or_else(|| extract_face(Face::Top));
    let bottom = extract_face(Face::Bottom)
        .or_else(|| extract_face(Face::Top));

    if top.is_none() && side.is_none() && bottom.is_none() {
        return None;
    }

    // Apply per-face tinting directly into pixel buffers.
    let tint_pixels = |mut fd: FaceData| {
        if fd.tinted {
            for chunk in fd.pixels.chunks_mut(4) {
                chunk[0] = (chunk[0] as u32 * block_rgb[0] as u32 / 255) as u8;
                chunk[1] = (chunk[1] as u32 * block_rgb[1] as u32 / 255) as u8;
                chunk[2] = (chunk[2] as u32 * block_rgb[2] as u32 / 255) as u8;
            }
        }
        fd
    };

    let top = top.map(&tint_pixels);
    let side = side.map(&tint_pixels);
    let bottom = bottom.map(&tint_pixels);

    // Get uniform tile dimensions from the first available tile.
    let (tile_w, tile_h) = top.as_ref().or(side.as_ref()).or(bottom.as_ref())
        .map(|f| (f.w, f.h))?;

    // Pack into a (3 × tile_w) × tile_h image: [top | side | bottom].
    let packed_w = tile_w * 3;
    let mut packed = vec![128u8; (packed_w * tile_h * 4) as usize];

    let copy_col = |dst: &mut Vec<u8>, src_pix: &[u8], col_offset: u32, tw: u32, ph: u32, pw: u32| {
        for row in 0..ph as usize {
            for col in 0..tw as usize {
                let src_i = (row * tw as usize + col) * 4;
                let dst_i = (row * pw as usize + col_offset as usize + col) * 4;
                if src_i + 4 <= src_pix.len() && dst_i + 4 <= dst.len() {
                    dst[dst_i..dst_i + 4].copy_from_slice(&src_pix[src_i..src_i + 4]);
                }
            }
        }
    };

    if let Some(ref f) = top    { copy_col(&mut packed, &f.pixels, 0,        tile_w, tile_h, packed_w); }
    if let Some(ref f) = side   { copy_col(&mut packed, &f.pixels, tile_w,   tile_w, tile_h, packed_w); }
    if let Some(ref f) = bottom { copy_col(&mut packed, &f.pixels, tile_w*2, tile_w, tile_h, packed_w); }

    let packed_image = bevy::image::Image::new(
        Extent3d { width: packed_w, height: tile_h, depth_or_array_layers: 1 },
        TextureDimension::D2,
        packed,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let handle = images.add(packed_image);

    Some(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(handle),
        perceptual_roughness: 0.6,
        ..default()
    })
}

/// Builds a `Cuboid`-like [`Mesh`] where UV `u` is split into three equal
/// columns:
/// - `[0, 1/3]`   → top face
/// - `[1/3, 2/3]` → four side faces
/// - `[2/3, 1]`   → bottom face
///
/// Pass an image with those three columns packed side-by-side to get
/// correct per-face-group textures with a single material.
///
/// Vertex ordering follows [`dd40_renderer`]'s `mesh_builder.rs` conventions
/// so normals and UV orientation are consistent with the chunk renderer.
fn cube_mesh_per_face_uvs(size: f32) -> Mesh {
    use bevy::prelude::Mesh;
    use bevy::mesh::{Indices, PrimitiveTopology};
    use bevy::asset::RenderAssetUsages;

    let h = size / 2.0;

    // Column U-ranges in the packed (top | side | bottom) image.
    let top_u0: f32 = 0.0;
    let top_u1: f32 = 1.0 / 3.0;
    let side_u0: f32 = 1.0 / 3.0;
    let side_u1: f32 = 2.0 / 3.0;
    let bot_u0: f32 = 2.0 / 3.0;
    let bot_u1: f32 = 1.0;

    // Each face has 4 vertices.  Vertex ordering matches mesh_builder.rs so
    // that the triangle fan [0,1,2, 0,2,3] gives outward-facing normals.
    //
    // UV convention (matches textured_mesh.rs `uv_pattern_for`):
    // - Side faces and top: vertices are ordered bottom-first / front-first,
    //   so V is *flipped* — v=1 at the bottom/front vertex, v=0 at top/back.
    //   This maps the texture's row-0 (visual top) to the visual top of the face.
    // - Bottom face: un-flipped [0,0],[1,0],[1,1],[0,1].

    // Top (+Y) — front-left, front-right, back-right, back-left (from PosY in mesh_builder)
    let top: [[f32; 3]; 4] = [[-h, h, h], [h, h, h], [h, h, -h], [-h, h, -h]];
    // Bottom (-Y) — back-left, back-right, front-right, front-left (NegY)
    let bot: [[f32; 3]; 4] = [[-h,-h,-h], [h,-h,-h], [h,-h, h], [-h,-h, h]];
    // South (+Z) — bottom-left, bottom-right, top-right, top-left (PosZ)
    let south: [[f32; 3]; 4] = [[-h,-h, h], [h,-h, h], [h, h, h], [-h, h, h]];
    // North (-Z) — bottom-right, bottom-left, top-left, top-right (NegZ)
    let north: [[f32; 3]; 4] = [[h,-h,-h], [-h,-h,-h], [-h, h,-h], [h, h,-h]];
    // East (+X) — bottom(+Z), bottom(-Z), top(-Z), top(+Z)  (PosX, U axis = Z)
    let east: [[f32; 3]; 4] = [[h,-h, h], [h,-h,-h], [h, h,-h], [h, h, h]];
    // West (-X) — bottom(-Z), bottom(+Z), top(+Z), top(-Z)  (NegX, U axis = Z)
    let west: [[f32; 3]; 4] = [[-h,-h,-h], [-h,-h, h], [-h, h, h], [-h, h,-h]];

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(24);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(24);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(24);

    // Top face and all side faces: V-flipped so texture row-0 = visual top.
    for (verts, normal, u0, u1) in [
        (top,   [ 0.0f32,  1.0, 0.0], top_u0,  top_u1),
        (south, [ 0.0f32,  0.0, 1.0], side_u0, side_u1),
        (north, [ 0.0f32,  0.0,-1.0], side_u0, side_u1),
        (east,  [ 1.0f32,  0.0, 0.0], side_u0, side_u1),
        (west,  [-1.0f32,  0.0, 0.0], side_u0, side_u1),
    ] {
        positions.extend(verts);
        normals.extend([[normal[0], normal[1], normal[2]]; 4]);
        uvs.extend([[u0,1.0],[u1,1.0],[u1,0.0],[u0,0.0]]);
    }

    // Bottom face: un-flipped (matches chunk renderer NegY pattern).
    positions.extend(bot);
    normals.extend([[0.0f32, -1.0, 0.0]; 4]);
    uvs.extend([[bot_u0,0.0],[bot_u1,0.0],[bot_u1,1.0],[bot_u0,1.0]]);

    let indices: Vec<u32> = (0..6u32)
        .flat_map(|f| { let b = f * 4; [b, b+1, b+2, b, b+2, b+3] })
        .collect();

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn resolve_color(
    loose: &LooseItem,
    item_registry: &ItemRegistry,
    block_registry: &BlockRegistry,
) -> Color {
    item_registry.get(loose.stack.item)
        .and_then(|d| d.placeable)
        .and_then(|id| block_registry.get(id))
        .map(|def| def.color)
        .unwrap_or(FALLBACK_COLOR)
}

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

