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
const FOAM_CORE: vec3<f32> = vec3<f32>(0.90, 0.96, 0.99);
const FOAM_EDGE: vec3<f32> = vec3<f32>(1.0, 1.0, 1.0);
const FOAM_MAX: f32 = 0.85;
const LACE_SCALE: f32 = 7.0;
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
    // The fog's light alone, and the slope's shade alone: the atmosphere
    // tones by the one and scales by the other.
    @location(2) fog: f32,
    @location(3) shade: f32,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = world_to_clip(in.pos);
    let last = textureDimensions(fog) - vec2<u32>(1u, 1u);
    let light = textureLoad(fog, vec2<i32>(min(in.corner, last)), 0).r;
    out.light = in.colour.r * (255.0 / SHADE_ONE) * light;
    out.fog = light;
    out.shade = in.colour.r * (255.0 / SHADE_ONE);
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

// `view::ground::Ground::colour_deep`: the ground's colour at world `w`,
// its textures read at `lod`, and how much of the blend is deep water.
fn ground_colour(w: vec2<f32>, lod: f32) -> vec4<f32> {
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
    var deep = 0.0;
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
            if (t[i] == 5u) {
                deep += wt[i];
            }
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
        var body = smoothstep(0.44 - shift, 0.52 - shift, calm)
            * (1.0 - smoothstep(0.70 - shift, 1.00 - shift, calm));
        var lead = smoothstep(0.40 - shift, 0.46 - shift, calm)
            * (1.0 - smoothstep(0.52 - shift, 0.62 - shift, calm));
        if (body > 0.0 || lead > 0.0) {
            let fine = noise_at(w * (NOISE_PER_TILE * 3.1) + vec2<f32>(17.0, 5.0));
            let broad = noise_at(vec2<f32>(w.y * (NOISE_PER_TILE * 0.9) + 93.0, w.x * (NOISE_PER_TILE * 0.9) + 61.0));
            let lace = noise_at(vec2<f32>(w.y * (NOISE_PER_TILE * LACE_SCALE) + 131.0, w.x * (NOISE_PER_TILE * LACE_SCALE) + 23.0));
            let breakup = smoothstep(0.22, 0.46, fine * 0.6 + broad * 0.4);
            body = body * breakup * (0.7 + 0.3 * smoothstep(0.25, 0.6, lace));
            lead = lead * smoothstep(0.12, 0.36, lace * 0.7 + broad * 0.3);
            let alpha = max(body * FOAM_MAX, lead * 0.95) * wet;
            let edge = min(lead * 1.2, 1.0) * wet;
            col += (mix(FOAM_CORE, FOAM_EDGE, edge) - col) * alpha;
        }
    }
    return vec4<f32>(col, deep);
}

// The atmosphere's sums that read the noise (`view::atmosphere`).
const HAZE_DARK: vec3<f32> = vec3<f32>(0.095, 0.105, 0.15);
const HAZE_LIGHT: vec3<f32> = vec3<f32>(0.145, 0.14, 0.165);
const HAZE_SCALE_A: f32 = 0.4;
const HAZE_SCALE_B: f32 = 1.3;
const FEATHER: f32 = 0.4;
const FEATHER_SCALE_A: f32 = 2.5;
const FEATHER_SCALE_B: f32 = 7.0;
const KNOWN_RAMP: f32 = 0.5;
const CLOUD_SCALE_A: f32 = 0.25;
const CLOUD_SCALE_B: f32 = 0.2;
const CLOUD_DRIFT: f32 = 0.5;
const CLOUD_DEPTH: f32 = 0.15;
const CLOUD_LO: f32 = 0.54;
const CLOUD_HI: f32 = 0.68;
const GLINT_SCALE_A: f32 = 13.7;
const GLINT_SCALE_B: f32 = 9.3;
const GLINT_DRIFT: vec2<f32> = vec2<f32>(2.0, -1.5);
const GLINT_MAX: f32 = 0.55;
const EDGE_FADE: f32 = 1.6;

// `atmosphere::haze`.
fn haze(w: vec2<f32>) -> vec3<f32> {
    let a = HAZE_SCALE_A * NOISE_PER_TILE;
    let b = HAZE_SCALE_B * NOISE_PER_TILE;
    let n = noise_at(vec2<f32>(w.x * a + 59.0, w.y * a + 173.0)) * 0.7
        + noise_at(vec2<f32>(w.y * b + 211.0, w.x * b + 7.0)) * 0.3;
    return mix(HAZE_DARK, HAZE_LIGHT, smoothstep(0.3, 0.7, n));
}

// `atmosphere::feather`.
fn feather(w: vec2<f32>) -> f32 {
    let a = FEATHER_SCALE_A * NOISE_PER_TILE;
    let b = FEATHER_SCALE_B * NOISE_PER_TILE;
    let n = noise_at(vec2<f32>(w.x * a + 31.0, w.y * a + 97.0)) * 0.65
        + noise_at(vec2<f32>(w.y * b + 143.0, w.x * b + 61.0)) * 0.35;
    return n * FEATHER;
}

// `atmosphere::fog_tone`: the haze where the light is zero, the memory
// where it is half, the colour where it is full, the haze's edge
// feathered onto known ground only.
fn fog_tone(c: vec3<f32>, light: f32, w: vec2<f32>) -> vec3<f32> {
    if (light <= 0.0) {
        return haze(w);
    }
    let seen = seen_at(c, light);
    if (light >= KNOWN_RAMP + FEATHER) {
        return seen;
    }
    let known = smoothstep(0.0, KNOWN_RAMP, light - feather(w));
    return mix(haze(w), seen, known);
}

// `atmosphere::cloud`, as it falls at light `light` (`cloud_in`).
fn cloud(w: vec2<f32>, light: f32) -> f32 {
    let a = CLOUD_SCALE_A * NOISE_PER_TILE;
    let b = CLOUD_SCALE_B * NOISE_PER_TILE;
    let drift = CLOUD_DRIFT * cam.c0.w;
    let n = noise_at(vec2<f32>(w.x * a + drift + 13.0, w.y * a + 71.0)) * 0.55
        + noise_at(vec2<f32>(w.y * b + 5.0, w.x * b + drift + 191.0)) * 0.45;
    let c = 1.0 - CLOUD_DEPTH * smoothstep(CLOUD_LO, CLOUD_HI, n);
    return 1.0 - (1.0 - c) * smoothstep(0.5, 1.0, light);
}

// `atmosphere::glint`.
fn glint(w: vec2<f32>, deep: f32) -> f32 {
    if (deep <= 0.0) {
        return 0.0;
    }
    let a = GLINT_SCALE_A * NOISE_PER_TILE;
    let b = GLINT_SCALE_B * NOISE_PER_TILE;
    let time = cam.c0.w;
    let f = noise_at(vec2<f32>(w.x * a + GLINT_DRIFT.x * time + 3.0, w.y * a + GLINT_DRIFT.y * time + 89.0));
    let g = noise_at(vec2<f32>(w.y * b - GLINT_DRIFT.y * time + 151.0, w.x * b + GLINT_DRIFT.x * time + 47.0));
    let sparse = noise_at(vec2<f32>(w.y * (NOISE_PER_TILE * 1.1) + 17.0, w.x * (NOISE_PER_TILE * 1.1) + 233.0));
    return smoothstep(0.8, 0.9, f) * smoothstep(0.78, 0.88, g) * smoothstep(0.5, 0.75, sparse)
        * deep * GLINT_MAX;
}

// The ground at world `w` (its colour and deep water from
// `ground_colour`), in the atmosphere: glints, the slope's `shade`, the
// cloud, the fog's tone at `light` and the grade at window pixel `px`.
fn atmosphere(g: vec4<f32>, w: vec2<f32>, shade: f32, light: f32, px: vec2<f32>) -> vec3<f32> {
    var c = g.rgb;
    let gl = glint(w, g.a);
    c = (c + (vec3<f32>(1.0) - c) * gl) * shade * cloud(w, light);
    return grade(fog_tone(c, light, w), px);
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
    if (atmosphere_on()) {
        if (in.fog <= 0.0) {
            return vec4<f32>(grade(haze(w), in.clip.xy), 1.0);
        }
        return vec4<f32>(atmosphere(ground_colour(w, lod), w, in.shade, in.fog, in.clip.xy), 1.0);
    }
    if (in.light <= 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    return vec4<f32>(ground_colour(w, lod).rgb * in.light, 1.0);
}

// The backdrop (`view::raster::draw_backdrop`), drawn first under the
// ground in the atmosphere: a triangle over the whole window. On the map
// the haze; past its edge the ground going on as the map ends, in the
// fog's light at the nearest point of the edge, fading into the haze.

struct BackOut {
    @builtin(position) clip: vec4<f32>,
};

@vertex
fn vs_backdrop(@builtin(vertex_index) vi: u32) -> BackOut {
    var corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0),
    );
    var out: BackOut;
    out.clip = vec4<f32>(corners[vi], 0.0, 1.0);
    return out;
}

// The fog's light at a point on the map, its corners' bilinear.
fn fog_bilinear(p: vec2<f32>) -> f32 {
    let last = vec2<i32>(textureDimensions(fog)) - vec2<i32>(1, 1);
    let p0 = floor(p);
    let f = p - p0;
    let i = vec2<i32>(p0);
    let a = textureLoad(fog, clamp(i, vec2<i32>(0), last), 0).r;
    let b = textureLoad(fog, clamp(i + vec2<i32>(1, 0), vec2<i32>(0), last), 0).r;
    let c = textureLoad(fog, clamp(i + vec2<i32>(0, 1), vec2<i32>(0), last), 0).r;
    let d = textureLoad(fog, clamp(i + vec2<i32>(1, 1), vec2<i32>(0), last), 0).r;
    let top = a + (b - a) * f.x;
    let bottom = c + (d - c) * f.x;
    return top + (bottom - top) * f.y;
}

@fragment
fn fs_backdrop(in: BackOut) -> @location(0) vec4<f32> {
    let px = in.clip.xy;
    // Window px -> world-screen px -> the ground plane, in tiles
    // (`view::Camera::from_window`, `view::iso::unproject`).
    let ws = (px - cam.c1.xy * 0.5) / cam.c0.z + cam.c0.xy;
    let a = ws.x / 32.0;
    let b = ws.y / 16.0;
    let w = vec2<f32>((a + b) * 0.5, (b - a) * 0.5);
    let dx = length(dpdx(w)) * TEX_PER_TILE;
    let dy = length(dpdy(w)) * TEX_PER_TILE;
    let lod = clamp(log2(max(max(dx, dy) * 0.70710677, 1e-6)), 0.0, LAST_LEVEL);
    let size = cam.c2.yz;
    if (w.x >= 0.0 && w.y >= 0.0 && w.x <= size.x && w.y <= size.y) {
        return vec4<f32>(grade(haze(w), px), 1.0);
    }
    let edge = clamp(w, vec2<f32>(0.0), size);
    // `atmosphere::edge_fade`.
    let o = max(max(-w, w - size), vec2<f32>(0.0));
    let dist = length(o);
    let s = NOISE_PER_TILE * 1.2;
    let n = noise_at(vec2<f32>(w.x * s + 113.0, w.y * s + 29.0));
    let fade = 1.0 - smoothstep(0.0, EDGE_FADE, dist * (0.6 + 0.8 * n));
    let light = fog_bilinear(edge) * fade;
    if (light <= 0.0) {
        return vec4<f32>(grade(haze(w), px), 1.0);
    }
    return vec4<f32>(atmosphere(ground_colour(w, lod), w, 1.0, light, px), 1.0);
}
