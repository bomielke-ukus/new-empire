// Gouraud-shaded terrain diamonds, each vertex in the fog light of its
// tile corner (`view::fog`): a texture with one texel per corner, sampled
// in the vertex shader so the light shades across the tile exactly as the
// software rasteriser shades it.

@group(1) @binding(0) var fog: texture_2d<f32>;

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) colour: vec4<f32>,
    @location(2) corner: vec2<u32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) colour: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = world_to_clip(in.pos);
    // Clamped, so a map larger than the texture (none uploaded yet: a
    // single lit texel) still reads a defined light.
    let last = textureDimensions(fog) - vec2<u32>(1u, 1u);
    let light = textureLoad(fog, vec2<i32>(min(in.corner, last)), 0).r;
    out.colour = vec4<f32>(in.colour.rgb * light, in.colour.a);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return in.colour;
}
