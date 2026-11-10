// Sprites, instanced. An indexed page's index 0 is transparent and the
// owning player's palette row supplies the colour, so one atlas serves all
// players. A colour page (bit 7 of the page) is full colour with straight
// alpha, the owner's colour laid on through its mask
// (`docs/look/colour-sets.md`), as `view::raster::tint` does it.

@group(1) @binding(0) var atlas: texture_2d_array<u32>;
@group(1) @binding(1) var palette: texture_2d<f32>;
@group(1) @binding(2) var colour: texture_2d_array<f32>;
@group(1) @binding(3) var mask: texture_2d_array<f32>;

// The page bit that marks a colour page (`view::sprites::COLOUR_PAGE`).
const COLOUR_PAGE: u32 = 0x80u;
// The first of the eight player-ramp indices, dark to light.
const RAMP: i32 = 240;

struct Inst {
    // x, y, w, h in world-screen px
    @location(0) rect: vec4<f32>,
    // u, v, uw, vh in atlas px
    @location(1) uv: vec4<f32>,
    // palette row, flip (bit 0) and atlas page (above it), screen-space
    // flag, light out of 255
    @location(2) misc: vec4<u32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) row: u32,
    @location(2) @interpolate(flat) light: u32,
    @location(3) @interpolate(flat) page: u32,
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
    if ((inst.misc.y & 1u) == 1u) {
        u = 1.0 - u;
    }
    out.uv = inst.uv.xy + vec2<f32>(u, c.y) * inst.uv.zw;
    out.row = inst.misc.x;
    out.light = inst.misc.w;
    out.page = inst.misc.y >> 1u;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Clamp inside the frame so a mirrored edge never samples a neighbour.
    let texel = vec2<i32>(in.uv);
    let light = f32(in.light) / 255.0;
    if ((in.page & COLOUR_PAGE) != 0u) {
        let layer = i32(in.page & (COLOUR_PAGE - 1u));
        let c = textureLoad(colour, texel, layer, 0);
        if (c.a == 0.0) {
            discard;
        }
        // Where the mask is set, the pixel's grey picks a step of the
        // owner's ramp by its luma, lerped toward the next.
        let m = textureLoad(mask, texel, layer, 0).r;
        var rgb = c.rgb;
        if (m > 0.0) {
            let t = clamp(dot(c.rgb, vec3<f32>(0.299, 0.587, 0.114)), 0.0, 1.0) * 7.0;
            let i = min(i32(floor(t)), 7);
            let row = i32(in.row);
            let lo = textureLoad(palette, vec2<i32>(RAMP + i, row), 0).rgb;
            let hi = textureLoad(palette, vec2<i32>(RAMP + min(i + 1, 7), row), 0).rgb;
            rgb = mix(rgb, mix(lo, hi, t - f32(i)), m);
        }
        return vec4<f32>(rgb * light, c.a);
    }
    let idx = textureLoad(atlas, texel, i32(in.page), 0).r;
    if (idx == 0u) {
        discard;
    }
    // Shadow (index 239, the palette's "shadow" special) is the same
    // translucent black on every row; its alpha lives in the palette texture.
    var row = i32(in.row);
    if (idx == 239u) {
        row = 0;
    }
    // Drawn from memory: dimmed to the explored light (`GD-FOG-01`).
    let c = textureLoad(palette, vec2<i32>(i32(idx), row), 0);
    return vec4<f32>(c.rgb * light, c.a);
}
