// Shared camera uniform and transforms. Prepended to every shader at load.

struct Camera {
    // focus.x, focus.y, zoom, unused
    c0: vec4<f32>,
    // viewport.w, viewport.h, unused, unused
    c1: vec4<f32>,
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
