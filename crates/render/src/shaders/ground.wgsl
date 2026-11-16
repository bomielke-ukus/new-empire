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
const FOAM_MAX: f32 = 0.6;
const FOAM_BLUR: f32 = 0.45;
const FOAM_SWAY: f32 = 0.3;
const SWAY_SCALE: f32 = 0.27;
const TINT_SCALE_A: f32 = 0.45;
const TINT_SCALE_B: f32 = 0.13;
const TINT_DEPTH: f32 = 0.07;
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

// `view::ground::blend_at(..).water()`: the bilinear weight of the water
// among the four tile centres around `p` (already pushed).
fn water_at(p: vec2<f32>) -> f32 {
    let q = p - 0.5;
    let q0 = floor(q);
    let f = q - q0;
    let i0 = vec2<i32>(q0);
    var w = 0.0;
    if (is_water(type_at(i0))) { w += (1.0 - f.x) * (1.0 - f.y); }
    if (is_water(type_at(i0 + vec2<i32>(1, 0)))) { w += f.x * (1.0 - f.y); }
    if (is_water(type_at(i0 + vec2<i32>(0, 1)))) { w += (1.0 - f.x) * f.y; }
    if (is_water(type_at(i0 + vec2<i32>(1, 1)))) { w += f.x * f.y; }
    return w;
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
    let wob = (vec2<f32>(a, b) - 0.5) * (2.0 * WOBBLE);
    let p = w + wob - 0.5;
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
    // `Ground::tint`: the land's broad tint, over tens of tiles.
    let m = (noise_at(w * (NOISE_PER_TILE * TINT_SCALE_A) + vec2<f32>(7.0, 211.0)) * 0.6
        + noise_at(w * (NOISE_PER_TILE * TINT_SCALE_B) + vec2<f32>(151.0, 37.0)) * 0.4 - 0.5) * 2.0;
    let l = 1.0 + m * TINT_DEPTH;
    let tint = vec3<f32>(l + m * 0.02, l, l - m * 0.03);
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
            c = textureSampleLevel(ground, samp, uv, layer, lod).rgb * tint;
        }
        col += c * wt[i];
    }

    // `Ground::foam`: on the water side of the shore, following the water
    // smoothed around a point pushed by the wobble and a broader sway,
    // broken by two scales of the noise, breathing.
    let wet = smoothstep(0.2, 0.55, water);
    if (wet > 0.0) {
        let ss = NOISE_PER_TILE * SWAY_SCALE;
        let sx = noise_at(vec2<f32>(w.x * ss + 41.0, w.y * ss + 13.0)) - 0.5;
        let sy = noise_at(vec2<f32>(w.y * ss + 77.0, w.x * ss + 29.0)) - 0.5;
        let fp = w + wob + vec2<f32>(sx, sy) * (2.0 * FOAM_SWAY);
        let d = FOAM_BLUR;
        let calm = (water_at(fp + vec2<f32>(-d, -d)) + water_at(fp + vec2<f32>(d, -d))
            + water_at(fp + vec2<f32>(-d, d)) + water_at(fp + vec2<f32>(d, d))) * 0.25;
        let shift = BREATH_DEPTH * sin(time * TAU / BREATH);
        let band = smoothstep(0.44 - shift, 0.53 - shift, calm)
            * (1.0 - smoothstep(0.66 - shift, 0.9 - shift, calm));
        if (band > 0.0) {
            let fine = noise_at(w * (NOISE_PER_TILE * 3.1) + vec2<f32>(17.0, 5.0));
            let broad = noise_at(vec2<f32>(w.y * (NOISE_PER_TILE * 0.9) + 93.0, w.x * (NOISE_PER_TILE * 0.9) + 61.0));
            let breakup = smoothstep(0.42, 0.66, fine * 0.6 + broad * 0.4);
            col += (FOAM - col) * (band * wet * breakup * FOAM_MAX);
        }
    }
    return vec4<f32>(col * in.light, 1.0);
}
