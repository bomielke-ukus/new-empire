// Shared camera uniform and transforms. Prepended to every shader at load.

struct Camera {
    // focus.x, focus.y, zoom, the clock (seconds)
    c0: vec4<f32>,
    // viewport.w, viewport.h, unused, unused
    c1: vec4<f32>,
    // the atmosphere (1 on, 0 off), the map's width and height in tiles,
    // unused
    c2: vec4<f32>,
};

@group(0) @binding(0) var<uniform> cam: Camera;

// World-screen px -> clip space, through the camera.
fn world_to_clip(ws: vec2<f32>) -> vec4<f32> {
    let px = (ws - cam.c0.xy) * cam.c0.z + cam.c1.xy * 0.5;
    return screen_to_clip(px);
}

// Window px -> clip space.
fn screen_to_clip(px: vec2<f32>) -> vec4<f32> {
    let ndc = vec2<f32>(px.x / cam.c1.x * 2.0 - 1.0, 1.0 - px.y / cam.c1.y * 2.0);
    return vec4<f32>(ndc, 0.0, 1.0);
}

// The atmosphere (`docs/look/atmosphere.md`), the sums of
// `view::atmosphere` that need no texture: the fog's memory and the grade.

fn atmosphere_on() -> bool {
    return cam.c2.x > 0.5;
}

// `atmosphere::MEMORY_SATURATION`, `MEMORY_TINT`.
const MEMORY_SATURATION: f32 = 0.4;
const MEMORY_TINT: vec3<f32> = vec3<f32>(0.55, 0.59, 0.71);
// `atmosphere::GRADE_*`, `VIGNETTE`.
const GRADE_SATURATION: f32 = 0.14;
const GRADE_WARM: vec3<f32> = vec3<f32>(0.035, 0.01, -0.045);
const GRADE_COOL: vec3<f32> = vec3<f32>(-0.04, 0.0, 0.06);
const VIGNETTE: f32 = 0.06;

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

// `atmosphere::remembered`: faded toward its grey, dimmer and cool.
fn remembered(c: vec3<f32>) -> vec3<f32> {
    let l = luma(c);
    return mix(vec3<f32>(l), c, MEMORY_SATURATION) * MEMORY_TINT;
}

// The colour seen at fog light `light`, before the haze: in sight
// itself, seen once its memory.
fn seen_at(c: vec3<f32>, light: f32) -> vec3<f32> {
    if (light >= 1.0) {
        return c;
    }
    return mix(remembered(c), c, smoothstep(0.5, 1.0, light));
}

// `atmosphere::grade` at window pixel `px` (its centre).
fn grade(c: vec3<f32>, px: vec2<f32>) -> vec3<f32> {
    let l = luma(c);
    let mid = 1.0 - abs(2.0 * l - 1.0);
    let s = 1.0 + GRADE_SATURATION * mid;
    let hi = smoothstep(0.5, 0.95, l);
    let lo = 1.0 - smoothstep(0.04, 0.4, l);
    let d = (px / max(cam.c1.xy, vec2<f32>(1.0)) - 0.5) * 2.0;
    let r2 = dot(d, d) * 0.5;
    let v = 1.0 - VIGNETTE * smoothstep(0.3, 1.0, r2);
    let sat = vec3<f32>(l) + (c - vec3<f32>(l)) * s;
    let toned = sat * (vec3<f32>(1.0) + GRADE_WARM * hi + GRADE_COOL * lo);
    return clamp(toned * v, vec3<f32>(0.0), vec3<f32>(1.0));
}
