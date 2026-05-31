// Block atlas material — fragment shader.
//
// Samples a 2D-array texture (the "block atlas") using a layer index
// supplied as a uniform.  All faces in the mesh that uses this material
// share the same atlas layer; the renderer groups quads into one mesh
// per (atlas layer, render layer) bucket before invoking this shader.
//
// The per-vertex `color` (carrying the per-block tint) is multiplied
// into the sampled texel.  This matches the colour-only fallback path
// and lets vanilla Minecraft tinting (e.g. grass) work the same way.

#import bevy_pbr::forward_io::VertexOutput

@group(2) @binding(0) var atlas: texture_2d_array<f32>;
@group(2) @binding(1) var atlas_sampler: sampler;
@group(2) @binding(2) var<uniform> params: BlockAtlasParams;

struct BlockAtlasParams {
    // Array layer this material samples from.
    layer: u32,
    // Alpha cutoff for the cutout render layer; opaque/translucent
    // passes pass 0.0 so nothing is discarded.
    alpha_cutoff: f32,
    _pad0: f32,
    _pad1: f32,
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let layer = i32(params.layer);
    let sampled = textureSample(atlas, atlas_sampler, in.uv, layer);
    let rgba = sampled * in.color;
    if (rgba.a < params.alpha_cutoff) {
        discard;
    }
    return rgba;
}
