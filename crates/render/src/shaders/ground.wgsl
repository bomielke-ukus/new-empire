// The ground painted from its textures (`docs/look/ground.md`), the same
// sums in the same order as `view::ground::Ground::colour`, which the
// software rasteriser runs. The diamonds are the terrain's
// (`view::terrain::build_all_textured`): each vertex carries its slope's
// shade in its colour and its tile corner, which interpolated across the
// diamond is the pixel's world position in tiles, elevation or not. The
// fog's light is read per corner in the vertex shader, as the terrain
// shader reads it. The clock is the camera's fourth number, seconds.

@group(1) @binding(0) var fog: texture_2d<f32>;
// The ground type of every tile (`view::ground::TypeMap`).
@group(2) @binding(0) var types: texture_2d<u32>;
// Smooth tiling noise, 256 x 256 over 8 x 8 tiles.
@group(2) @binding(1) var noise: texture_2d<f32>;
// One layer per ground type, 512 x 512 over 4 x 4 tiles, with its mips.
@group(2) @binding(2) var ground: texture_2d_array<f32>;
// Linear, mipmapped, repeating.
@group(2) @binding(3) var samp: sampler;

// `view::ground`'s constants.
const TEX_PER_TILE: f32 = 128.0;
const TEX_SIZE: f32 = 512.0;
const NOISE_PER_TILE: f32 = 32.0;
const NOISE_SIZE: f32 = 256.0;
const WOBBLE: f32 = 0.35;
const LAST_LEVEL: f32 = 9.0;
const DRIFT_A: vec2<f32> = vec2<f32>(5.0, 2.0);
const DRIFT_B: vec2<f32> = vec2<f32>(-3.0, 4.0);
const BREATH: f32 = 4.0;
const BREATH_DEPTH: f32 = 0.04;
const FOAM: vec3<f32> = vec3<f32>(0.97, 0.99, 1.0);
const TAU: f32 = 6.2831855;
// `view::terrain::SHADE_ONE`: a shade byte of 128 is 1.
const SHADE_ONE: f32 = 128.0;

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) colour: vec4<f32>,
    @location(2) corner: vec2<u32>,
    @location(3) detail: vec2<u32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    // World position, tiles.
    @location(0) world: vec2<f32>,
    // The slope's shade times the fog's light.
    @location(1) light: f32,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = world_to_clip(in.pos);
    let last = textureDimensions(fog) - vec2<u32>(1u, 1u);
    let light = textureLoad(fog, vec2<i32>(min(in.corner, last)), 0).r;
    out.light = in.colour.r * (255.0 / SHADE_ONE) * light;
    out.world = vec2<f32>(in.corner);
    return out;
}

// The noise at texel position `t`, bilinear and wrapping.
fn noise_at(t: vec2<f32>) -> f32 {
    return textureSampleLevel(noise, samp, t / NOISE_SIZE, 0.0).r;
}

fn is_water(t: u32) -> bool {
    return t == 4u || t == 5u;
}

fn type_at(i: vec2<i32>) -> u32 {
    let last = vec2<i32>(textureDimensions(types)) - vec2<i32>(1, 1);
    return textureLoad(types, clamp(i, vec2<i32>(0, 0), last), 0).r;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let w = in.world;
    // `view::ground::lod`: from how the world moves a pixel across and a
    // pixel down, constant over the triangle. Before anything returns, so
    // the derivatives are taken in uniform control flow.
    let dx = length(dpdx(w)) * TEX_PER_TILE;
    let dy = length(dpdy(w)) * TEX_PER_TILE;
    let lod = clamp(log2(max(max(dx, dy) * 0.70710677, 1e-6)), 0.0, LAST_LEVEL);
    if (in.light <= 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let time = cam.c0.w;

    // The four tile centres around the point the noise pushes this one
    // to, weighted bilinearly, each type once.
    let a = noise_at(w * NOISE_PER_TILE);
    let b = noise_at(vec2<f32>(w.y * NOISE_PER_TILE + 101.0, w.x * NOISE_PER_TILE + 57.0));
    let p = w + (vec2<f32>(a, b) - 0.5) * (2.0 * WOBBLE) - 0.5;
    let p0 = floor(p);
    let f = p - p0;
    let i0 = vec2<i32>(p0);
    var t = array<u32, 4>(
        type_at(i0),
        type_at(i0 + vec2<i32>(1, 0)),
        type_at(i0 + vec2<i32>(0, 1)),
        type_at(i0 + vec2<i32>(1, 1)),
    );
    var wt = array<f32, 4>(
        (1.0 - f.x) * (1.0 - f.y),
        f.x * (1.0 - f.y),
        (1.0 - f.x) * f.y,
        f.x * f.y,
    );
    for (var i = 1; i < 4; i++) {
        for (var j = 0; j < i; j++) {
            if (wt[j] > 0.0 && t[j] == t[i]) {
                wt[j] += wt[i];
                wt[i] = 0.0;
                break;
            }
        }
    }

    // Each type's texture at the pixel's own position; the water twice,
    // drifting, averaged.
    let uv = w * (TEX_PER_TILE / TEX_SIZE);
    var col = vec3<f32>(0.0);
    var water = 0.0;
    for (var i = 0; i < 4; i++) {
        if (wt[i] <= 0.0) {
            continue;
        }
        let layer = i32(t[i]);
        var c: vec3<f32>;
        if (is_water(t[i])) {
            let s = time / TEX_SIZE;
            let p1 = textureSampleLevel(ground, samp, uv + DRIFT_A * s, layer, lod).rgb;
            let p2 = textureSampleLevel(ground, samp, uv + DRIFT_B * s, layer, lod).rgb;
            c = (p1 + p2) * 0.5;
            water += wt[i];
        } else {
            c = textureSampleLevel(ground, samp, uv, layer, lod).rgb;
        }
        col += c * wt[i];
    }

    // The foam along the shore, broken by the finer noise, breathing.
    let shift = BREATH_DEPTH * sin(time * TAU / BREATH);
    let band = clamp((water - (0.35 - shift)) / 0.05, 0.0, 1.0)
        * clamp(((0.6 - shift) - water) / 0.05, 0.0, 1.0);
    if (band > 0.0) {
        let breakup = noise_at(w * (NOISE_PER_TILE * 3.1) + vec2<f32>(17.0, 5.0));
        let foam = band * clamp((breakup - 0.3) / 0.25, 0.0, 1.0) * 0.85;
        col += (FOAM - col) * foam;
    }
    return vec4<f32>(col * in.light, 1.0);
}
