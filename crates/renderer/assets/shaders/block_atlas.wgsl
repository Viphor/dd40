// Block atlas material — fragment shader.
//
// Samples a 2D-array texture (the "block atlas") using a layer index
// supplied as a uniform.  All faces in the mesh that uses this material
// share the same atlas layer; the renderer groups quads into one mesh
// per (atlas layer, render layer, overlay layer) bucket before
// invoking this shader.
//
// When `params.has_overlay` is non-zero, a second sample is taken from
// `params.overlay_layer` using the secondary UV set (`in.uv_b`).  The
// overlay's RGB is multiplied by the per-vertex tint and alpha-composited
// on top of the base sample, matching Minecraft's grass-block-side
// overlay model.  The `params.tinted` flag (independent of the overlay)
// additionally multiplies the whole composited result by the tint —
// used for blocks like leaves whose entire texture is greyscale.

#import bevy_pbr::{
    forward_io::VertexOutput,
    pbr_fragment::pbr_input_from_vertex_output,
    pbr_functions,
    pbr_types,
}

// Bevy 0.18 reserves @group(2) for mesh data; custom material bindings
// live at the index exposed via the `MATERIAL_BIND_GROUP` shader-def
// (currently 3). Using the shader-def keeps us forward-compatible if
// upstream renumbers groups.
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var atlas: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var atlas_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> params: BlockAtlasParams;

struct BlockAtlasParams {
    // Array layer for the base texture.
    layer: u32,
    // Alpha cutoff for the cutout render layer; opaque/translucent
    // passes pass 0.0 so nothing is discarded.
    alpha_cutoff: f32,
    // Non-zero to multiply the per-vertex colour into the composited
    // result (used for leaves / water).
    tinted: u32,
    // Non-zero to enable the overlay sampling branch.
    has_overlay: u32,
    // Array layer for the overlay texture; ignored when has_overlay == 0.
    overlay_layer: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    // Atlas sub-rect for the base texture (within its array layer).
    // The vertex UV is in tile-space — one unit per block — so the
    // shader wraps with fract() and remaps into this rect.  This is
    // what makes greedy-merged quads tile per block.
    uv_min: vec2<f32>,
    uv_size: vec2<f32>,
    // Atlas sub-rect for the overlay texture.
    overlay_uv_min: vec2<f32>,
    overlay_uv_size: vec2<f32>,
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> @location(0) vec4<f32> {
    let tiled_uv = params.uv_min + fract(in.uv) * params.uv_size;
    let base = textureSample(atlas, atlas_sampler, tiled_uv, i32(params.layer));

    // Default: no overlay (mask alpha = 0 → mix returns base unchanged).
    var overlay_rgba = vec4<f32>(0.0, 0.0, 0.0, 0.0);
#ifdef VERTEX_UVS_B
    if (params.has_overlay != 0u) {
        let tiled_overlay_uv =
            params.overlay_uv_min + fract(in.uv_b) * params.overlay_uv_size;
        let o = textureSample(atlas, atlas_sampler, tiled_overlay_uv, i32(params.overlay_layer));
        // Multiply overlay RGB by the per-vertex tint so grass-style
        // greyscale overlays acquire their biome colour.  Preserve the
        // overlay's own alpha as the compositing mask.
        overlay_rgba = vec4<f32>(o.rgb * in.color.rgb, o.a);
    }
#endif

    // Alpha-composite overlay on top of base.  When overlay.a == 0 this
    // is a no-op and returns the base texel unmodified.
    let composited_rgb = mix(base.rgb, overlay_rgba.rgb, overlay_rgba.a);
    let composited = vec4<f32>(composited_rgb, base.a);

    // Optional whole-output tint (e.g. leaves whose full texture is
    // greyscale).  When `tinted == 0u`, multiply by white.
    let tint = select(vec4<f32>(1.0, 1.0, 1.0, 1.0), in.color, params.tinted != 0u);
    let rgba = composited * tint;

    if (rgba.a < params.alpha_cutoff) {
        discard;
    }

    // Hand the computed base colour off to Bevy's PBR pipeline so
    // directional / ambient light and shadows apply.  Blocks are matte
    // and non-metallic, so we keep the default `pbr_input_new` material
    // values (metallic = 0, perceptual_roughness = 0.5) and only
    // override the base colour.
    var pbr_input = pbr_input_from_vertex_output(in, is_front, false);
    pbr_input.material.base_color = rgba;
    // Keep the alpha channel intact on the material so the blend
    // pipeline gets the right transparency.
    pbr_input.material.perceptual_roughness = 1.0;
    pbr_input.material.metallic = 0.0;
    pbr_input.material.reflectance = vec3<f32>(0.0);

    let lit = pbr_functions::apply_pbr_lighting(pbr_input);
    return pbr_functions::main_pass_post_lighting_processing(pbr_input, lit);
}
