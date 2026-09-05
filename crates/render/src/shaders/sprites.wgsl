// Palette-indexed sprites, instanced. Index 0 is transparent; the owning
// player's palette row supplies the colour, so one atlas serves all players.

@group(1) @binding(0) var atlas: texture_2d<u32>;
@group(1) @binding(1) var palette: texture_2d<f32>;

struct Inst {
    // x, y, w, h in world-screen px
    @location(0) rect: vec4<f32>,
    // u, v, uw, vh in atlas px
    @location(1) uv: vec4<f32>,
    // palette row, flip, screen-space flag, unused
    @location(2) misc: vec4<u32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) row: u32,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Inst) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let c = corners[vi];
    var out: VsOut;
    let corner = inst.rect.xy + c * inst.rect.zw;
    if (inst.misc.z == 1u) {
        out.clip = screen_to_clip(corner);
    } else {
        out.clip = world_to_clip(corner);
    }
    var u = c.x;
    if (inst.misc.y == 1u) {
        u = 1.0 - u;
    }
    out.uv = inst.uv.xy + vec2<f32>(u, c.y) * inst.uv.zw;
    out.row = inst.misc.x;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Clamp inside the frame so a mirrored edge never samples a neighbour.
    let texel = vec2<i32>(in.uv);
    let idx = textureLoad(atlas, texel, 0).r;
    if (idx == 0u) {
        discard;
    }
    // Shadow (index 3) is the same translucent black on every row.
    var row = i32(in.row);
    if (idx == 3u) {
        row = 0;
    }
    return textureLoad(palette, vec2<i32>(i32(idx), row), 0);
}
