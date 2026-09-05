// Gouraud-shaded terrain diamonds.

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) colour: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) colour: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = world_to_clip(in.pos);
    out.colour = in.colour;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return in.colour;
}
