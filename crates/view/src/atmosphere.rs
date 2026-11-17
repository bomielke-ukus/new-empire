//! The atmosphere (`docs/look/atmosphere.md`): with the new art on, the
//! touches that make the picture one sunlit place rather than sprites on a
//! texture. Never-seen ground is a dark ink-blue haze with a feathered,
//! wobbling edge; ground seen once and out of sight is a cool, faded
//! memory of itself; past the map's edge the ground fades into the haze;
//! soft cloud shadows drift over ground and sprites alike; a gentle grade
//! warms the highlights, cools the shadows and darkens the corners a
//! touch; and glints sparkle on deep water.
//!
//! This module is the reference: both renderers do these sums in this
//! order, the software rasteriser by calling these functions, the GPU in
//! `crates/render/src/shaders/` (`common.wgsl`, `ground.wgsl`,
//! `sprites.wgsl`). Everything is a function of world position, the
//! window position (the grade's vignette) and the renderers' clock, which
//! wraps at [`crate::ground::PERIOD`] with every drift a whole number of
//! cycles in it: no randomness, no wall clock.
//!
//! Nothing here can show what the fog hides: the haze takes over wherever
//! the fog's light is zero, and the feather only ever pushes the haze
//! further onto ground the player knows, never back off ground they do
//! not (see [`fog_tone`]).

use crate::ground::{noise_at, smoothstep, NOISE_PER_TILE};

/// The haze's darker colour, sRGB 0 to 1: ink blue.
pub const HAZE_DARK: [f32; 3] = [0.095, 0.105, 0.15];
/// The haze's lighter colour: a dusky parchment grey.
pub const HAZE_LIGHT: [f32; 3] = [0.145, 0.14, 0.165];
/// The noise scales the haze is mottled at, multiples of
/// [`NOISE_PER_TILE`]: about 20 and 6 tiles across.
pub const HAZE_SCALES: [f32; 2] = [0.4, 1.3];
/// The feather: how far, in fog light, the noise pushes the haze onto
/// known ground, at most.
pub const FEATHER: f32 = 0.4;
/// The feather's noise scales: about 3 tiles and 1 across.
pub const FEATHER_SCALES: [f32; 2] = [2.5, 7.0];
/// The light over which known ground rises out of the haze: from 0 to this.
pub const KNOWN_RAMP: f32 = 0.5;
/// How much colour a memory keeps: 0 is grey, 1 all of it.
pub const MEMORY_SATURATION: f32 = 0.4;
/// A memory's light per channel: dimmer, and cool.
pub const MEMORY_TINT: [f32; 3] = [0.55, 0.59, 0.71];
/// The cloud shadows' noise scales: about 32 and 40 tiles across, so the
/// two together repeat only every 160.
pub const CLOUD_SCALES: [f32; 2] = [0.25, 0.2];
/// How fast the clouds' noise drifts, texels a second: half a texel, so
/// each comes back to where it started after [`crate::ground::PERIOD`].
/// Toward the world's -x, up and to the left on screen, 0.06 and 0.08
/// tiles a second.
pub const CLOUD_DRIFT: f32 = 0.5;
/// How much a cloud's shadow darkens, at its thickest.
pub const CLOUD_DEPTH: f32 = 0.15;
/// The share of the sky in cloud: the noise over which the shadow rises.
pub const CLOUD_EDGES: [f32; 2] = [0.54, 0.68];
/// The grade's extra saturation at mid-grey (none at black or white).
pub const GRADE_SATURATION: f32 = 0.14;
/// The highlights' warmth: a factor per channel at full.
pub const GRADE_WARM: [f32; 3] = [0.035, 0.01, -0.045];
/// The shadows' coolness: a factor per channel at full.
pub const GRADE_COOL: [f32; 3] = [-0.04, 0.0, 0.06];
/// How much the corners are darkened.
pub const VIGNETTE: f32 = 0.06;
/// How far past the map's edge the ground fades out, tiles.
pub const EDGE_FADE: f32 = 1.6;
/// The glints' two fine noise scales: the noise 0.6 and 0.85 tiles across.
pub const GLINT_SCALES: [f32; 2] = [13.7, 9.3];
/// The glints' drift, texels a second; whole halves, so they wrap.
pub const GLINT_DRIFT: [f32; 2] = [2.0, -1.5];
/// How bright a glint is at most.
pub const GLINT_MAX: f32 = 0.55;

/// Rec. 601 luma, as the colour sets' tint reads it.
#[inline]
pub fn luma(c: [f32; 3]) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

#[inline]
fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// The haze over never-seen ground at world `(x, y)` (tiles): ink blue,
/// mottled lighter over tens of tiles.
pub fn haze(noise: &[u8], x: f32, y: f32) -> [f32; 3] {
    let a = HAZE_SCALES[0] * NOISE_PER_TILE;
    let b = HAZE_SCALES[1] * NOISE_PER_TILE;
    let n = noise_at(noise, x * a + 59.0, y * a + 173.0) * 0.7
        + noise_at(noise, y * b + 211.0, x * b + 7.0) * 0.3;
    mix3(HAZE_DARK, HAZE_LIGHT, smoothstep(0.3, 0.7, n))
}

/// How far the haze reaches onto known ground at world `(x, y)`, in fog
/// light: 0 to [`FEATHER`], never less than 0.
pub fn feather(noise: &[u8], x: f32, y: f32) -> f32 {
    let a = FEATHER_SCALES[0] * NOISE_PER_TILE;
    let b = FEATHER_SCALES[1] * NOISE_PER_TILE;
    let n = noise_at(noise, x * a + 31.0, y * a + 97.0) * 0.65
        + noise_at(noise, y * b + 143.0, x * b + 61.0) * 0.35;
    n * FEATHER
}

/// What a colour looks like from memory: faded toward its grey, dimmer
/// and cool.
#[inline]
pub fn remembered(c: [f32; 3]) -> [f32; 3] {
    let l = luma(c);
    let d = mix3([l; 3], c, MEMORY_SATURATION);
    [
        d[0] * MEMORY_TINT[0],
        d[1] * MEMORY_TINT[1],
        d[2] * MEMORY_TINT[2],
    ]
}

/// A colour `c` (already in the cloud's shadow) seen through the fog at
/// light `light` (0 to 1, as `crate::fog` gives it, interpolated):
///
/// - in sight (1): itself;
/// - seen once (0.5): [`remembered`];
/// - never seen (0): the haze.
///
/// Between, it rises out of the haze over light 0 to [`KNOWN_RAMP`],
/// with the feather (0 to [`FEATHER`]) taken off the light first, so the
/// haze's edge wobbles onto known ground. Where the light is 0 this is the
/// haze exactly, whatever the feather: nothing of the ground shows. The
/// haze and the feather are only read where they can matter (`haze` and
/// `feather` give them), as the GPU reads them.
pub fn fog_tone(
    c: [f32; 3],
    light: f32,
    feather: impl FnOnce() -> f32,
    haze: impl FnOnce() -> [f32; 3],
) -> [f32; 3] {
    if light <= 0.0 {
        return haze();
    }
    let seen = if light >= 1.0 {
        c
    } else {
        mix3(remembered(c), c, smoothstep(0.5, 1.0, light))
    };
    if light >= KNOWN_RAMP + FEATHER {
        return seen;
    }
    let known = smoothstep(0.0, KNOWN_RAMP, light - feather().max(0.0));
    mix3(haze(), seen, known)
}

/// The cloud shadows at world `(x, y)` at `time` seconds: a factor from
/// `1 - CLOUD_DEPTH` (under the thickest cloud) to 1 (open sky). Two
/// reads of the noise, broad, drifting the same way at a little different
/// speeds so the clouds change shape as they go.
pub fn cloud(noise: &[u8], x: f32, y: f32, time: f32) -> f32 {
    let a = CLOUD_SCALES[0] * NOISE_PER_TILE;
    let b = CLOUD_SCALES[1] * NOISE_PER_TILE;
    let drift = CLOUD_DRIFT * time;
    let n = noise_at(noise, x * a + drift + 13.0, y * a + 71.0) * 0.55
        + noise_at(noise, y * b + 5.0, x * b + drift + 191.0) * 0.45;
    1.0 - CLOUD_DEPTH * smoothstep(CLOUD_EDGES[0], CLOUD_EDGES[1], n)
}

/// The cloud as it falls on what stands at the light `light`: all of it
/// in sight, none on a memory, as the fog has it.
#[inline]
pub fn cloud_in(cloud: f32, light: f32) -> f32 {
    1.0 - (1.0 - cloud) * smoothstep(0.5, 1.0, light)
}

/// The glints on deep water at world `(x, y)` at `time`: how much of a
/// glint is laid over, 0 to [`GLINT_MAX`], where `deep` of the blend is
/// deep water. Small and sparse: two fine reads of the noise at scales
/// that do not divide each other, drifting different ways with the
/// water, must both be high, and a broad one too, so the glints neither
/// line up in a lattice nor fill the sea.
pub fn glint(noise: &[u8], x: f32, y: f32, time: f32, deep: f32) -> f32 {
    if deep <= 0.0 {
        return 0.0;
    }
    let a = GLINT_SCALES[0] * NOISE_PER_TILE;
    let b = GLINT_SCALES[1] * NOISE_PER_TILE;
    let f = noise_at(
        noise,
        x * a + GLINT_DRIFT[0] * time + 3.0,
        y * a + GLINT_DRIFT[1] * time + 89.0,
    );
    let g = noise_at(
        noise,
        y * b - GLINT_DRIFT[1] * time + 151.0,
        x * b + GLINT_DRIFT[0] * time + 47.0,
    );
    let sparse = noise_at(
        noise,
        y * (NOISE_PER_TILE * 1.1) + 17.0,
        x * (NOISE_PER_TILE * 1.1) + 233.0,
    );
    smoothstep(0.8, 0.9, f)
        * smoothstep(0.78, 0.88, g)
        * smoothstep(0.5, 0.75, sparse)
        * deep
        * GLINT_MAX
}

/// The grade, on a colour of the world at window pixel `(px, py)` (its
/// centre) of a `viewport`-sized window: a little more colour in the
/// mid-tones, warm highlights, cool shadows (by factors, so black stays
/// black and white is not pushed past white), and the corners darkened by
/// [`VIGNETTE`] at most. Never on the UI.
pub fn grade(c: [f32; 3], px: f32, py: f32, viewport: (f32, f32)) -> [f32; 3] {
    let l = luma(c);
    let mid = 1.0 - (2.0 * l - 1.0).abs();
    let s = 1.0 + GRADE_SATURATION * mid;
    let hi = smoothstep(0.5, 0.95, l);
    let lo = 1.0 - smoothstep(0.04, 0.4, l);
    let dx = (px / viewport.0.max(1.0) - 0.5) * 2.0;
    let dy = (py / viewport.1.max(1.0) - 0.5) * 2.0;
    let r2 = (dx * dx + dy * dy) * 0.5;
    let v = 1.0 - VIGNETTE * smoothstep(0.3, 1.0, r2);
    let mut out = [0.0; 3];
    for k in 0..3 {
        let sat = l + (c[k] - l) * s;
        let toned = sat * (1.0 + GRADE_WARM[k] * hi + GRADE_COOL[k] * lo);
        out[k] = (toned * v).clamp(0.0, 1.0);
    }
    out
}

/// How much of the ground shows at world `(x, y)` past the edge of a
/// `w` x `h` map: 1 on the map, fading to 0 over [`EDGE_FADE`] tiles out,
/// the distance stretched by the noise so the fade's edge is ragged.
pub fn edge_fade(noise: &[u8], x: f32, y: f32, w: f32, h: f32) -> f32 {
    let ox = (-x).max(x - w).max(0.0);
    let oy = (-y).max(y - h).max(0.0);
    let d = (ox * ox + oy * oy).sqrt();
    if d <= 0.0 {
        return 1.0;
    }
    let s = NOISE_PER_TILE * 1.2;
    let n = noise_at(noise, x * s + 113.0, y * s + 29.0);
    1.0 - smoothstep(0.0, EDGE_FADE, d * (0.6 + 0.8 * n))
}

/// The point of a `w` x `h` map nearest world `(x, y)`.
#[inline]
pub fn clamp_to_map(x: f32, y: f32, w: f32, h: f32) -> (f32, f32) {
    (x.clamp(0.0, w), y.clamp(0.0, h))
}

/// The world point (tiles, on the ground plane) a sprite's light is read
/// at: the middle of its box's bottom edge, `(x, y, w, h)` in
/// world-screen px, where it meets the ground.
#[inline]
pub fn sprite_ground(x: f32, y: f32, w: f32, h: f32) -> (f32, f32) {
    crate::iso::unproject(x + w * 0.5, y + h)
}

/// The cloud on a world sprite at `(x, y, w, h)` lit `light` (out of
/// 255), at `time`: a byte, 255 for none. Both renderers take it from
/// here, so a sprite is in the same shadow on both.
pub fn sprite_cloud(noise: &[u8], rect: (f32, f32, f32, f32), light: u8, time: f32) -> u8 {
    let (gx, gy) = sprite_ground(rect.0, rect.1, rect.2, rect.3);
    let c = cloud_in(cloud(noise, gx, gy, time), f32::from(light) / 255.0);
    (c * 255.0).round().clamp(0.0, 255.0) as u8
}

/// A world sprite's colour (sRGB 0 to 1, already tinted) as the
/// atmosphere draws it at window pixel `(px, py)`: in its cloud
/// (`shade`, a byte from [`sprite_cloud`]), toned by its light as the fog
/// tones the ground (a memory is faded and cool, not merely dim), and
/// graded.
pub fn sprite_colour(
    c: [f32; 3],
    light: u8,
    shade: u8,
    px: f32,
    py: f32,
    viewport: (f32, f32),
) -> [f32; 3] {
    let k = f32::from(shade) / 255.0;
    let lit = [c[0] * k, c[1] * k, c[2] * k];
    let toned = fog_tone(lit, f32::from(light) / 255.0, || 0.0, || [0.0; 3]);
    grade(toned, px, py, viewport)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ground::tests::committed_art;

    /// Where the fog's light is zero, the haze and nothing else, whatever
    /// the colour under it and however the feather falls; where it is
    /// full, the colour itself.
    #[test]
    fn the_unseen_is_haze_and_the_seen_is_itself() {
        let haze = [0.1, 0.1, 0.15];
        for c in [[0.0; 3], [1.0; 3], [0.9, 0.2, 0.1]] {
            for f in [0.0, 0.1, FEATHER] {
                assert_eq!(fog_tone(c, 0.0, || f, || haze), haze);
                assert_eq!(fog_tone(c, 1.0, || f, || haze), c);
            }
        }
        // A memory: dimmer, greyer and cooler, not black.
        let grass = [0.35, 0.55, 0.2];
        let m = fog_tone(grass, 128.0 / 255.0, || 0.0, || haze);
        assert!(m[1] < grass[1] * 0.7 && m[1] > 0.15, "{m:?}");
        assert!(m[2] / m[1] > grass[2] / grass[1], "cooler: {m:?}");
        assert!((m[1] - m[0]) < (grass[1] - grass[0]) * 0.6, "greyer: {m:?}");
    }

    #[test]
    fn the_haze_and_the_feather_are_bounded_and_deterministic() {
        let art = committed_art();
        let (mut lo, mut hi) = (1.0f32, 0.0f32);
        for i in 0..4000 {
            let (x, y) = (i as f32 * 0.173 - 40.0, i as f32 * 0.091 - 20.0);
            let f = feather(&art.noise, x, y);
            assert!((0.0..=FEATHER).contains(&f), "{f}");
            assert_eq!(f, feather(&art.noise, x, y));
            lo = lo.min(f);
            hi = hi.max(f);
            let h = haze(&art.noise, x, y);
            for k in 0..3 {
                assert!(h[k] >= HAZE_DARK[k] - 1e-6 && h[k] <= HAZE_LIGHT[k].max(HAZE_DARK[k]));
            }
        }
        assert!(
            hi - lo > FEATHER * 0.4,
            "the feather should wobble: {lo}..{hi}"
        );
    }

    /// The cloud shadows: within their depth, a function of place and time
    /// alone, moving with time, and the same when the clock wraps.
    #[test]
    fn cloud_shadows_are_bounded_deterministic_and_drift() {
        let art = committed_art();
        let n = &art.noise;
        let (mut lo, mut hi) = (1.0f32, 0.0f32);
        let mut moved = 0;
        for i in 0..5000 {
            let (x, y) = (i as f32 * 0.37 % 160.0, i as f32 * 0.23 % 140.0);
            let c = cloud(n, x, y, 0.0);
            assert!((1.0 - CLOUD_DEPTH - 1e-6..=1.0).contains(&c), "{c}");
            assert_eq!(c, cloud(n, x, y, 0.0));
            let later = cloud(n, x, y, 20.0);
            moved += usize::from((later - c).abs() > 1e-3);
            let wrapped = cloud(n, x, y, crate::ground::PERIOD);
            assert!((wrapped - c).abs() < 1e-3, "{c} {wrapped}");
            lo = lo.min(c);
            hi = hi.max(c);
        }
        assert!(hi == 1.0 && lo < 1.0 - CLOUD_DEPTH * 0.9, "{lo}..{hi}");
        assert!(moved > 500, "the clouds drift: {moved}");
        // On a sprite in sight, the same as on the ground at its foot; on
        // a memory, none.
        let rect = (100.0, 300.0, 40.0, 60.0);
        let (gx, gy) = sprite_ground(rect.0, rect.1, rect.2, rect.3);
        let want = (cloud(n, gx, gy, 3.0) * 255.0).round() as u8;
        assert_eq!(sprite_cloud(n, rect, 255, 3.0), want);
        assert_eq!(sprite_cloud(n, rect, 128, 3.0), 255);
    }

    /// The grade is gentle: black stays black, white stays white in the
    /// middle of the window, mid-tones gain a little colour, and the
    /// corners darken by the vignette at most.
    #[test]
    fn the_grade_is_gentle() {
        let vp = (1280.0, 720.0);
        assert_eq!(grade([0.0; 3], 640.0, 360.0, vp), [0.0; 3]);
        let w = grade([1.0; 3], 640.0, 360.0, vp);
        assert!(w.iter().all(|&v| v >= 0.94), "{w:?}");
        let mid = [0.55, 0.45, 0.3];
        let g = grade(mid, 640.0, 360.0, vp);
        assert!(g[0] - g[2] > mid[0] - mid[2], "more colour: {g:?}");
        for c in [[0.2, 0.3, 0.4], mid, [0.8, 0.8, 0.75]] {
            let centre = grade(c, 640.0, 360.0, vp);
            let corner = grade(c, 0.5, 0.5, vp);
            for k in 0..3 {
                let r = corner[k] / centre[k];
                assert!((1.0 - VIGNETTE - 0.01..=1.0).contains(&r), "{r}");
            }
        }
    }

    #[test]
    fn the_ground_fades_past_the_edge() {
        let art = committed_art();
        let n = &art.noise;
        assert_eq!(edge_fade(n, 3.0, 4.0, 10.0, 10.0), 1.0);
        assert_eq!(edge_fade(n, 0.0, 10.0, 10.0, 10.0), 1.0);
        assert!(edge_fade(n, -0.3, 5.0, 10.0, 10.0) > 0.4);
        assert_eq!(edge_fade(n, -5.0, 5.0, 10.0, 10.0), 0.0);
        assert_eq!(edge_fade(n, 5.0, 15.0, 10.0, 10.0), 0.0);
    }

    #[test]
    fn glints_are_sparse_and_only_on_deep_water() {
        let art = committed_art();
        let n = &art.noise;
        let mut lit = 0;
        for i in 0..10_000 {
            let (x, y) = (i as f32 * 0.0137 % 20.0, i as f32 * 0.0291 % 17.0);
            assert_eq!(glint(n, x, y, 1.0, 0.0), 0.0);
            let g = glint(n, x, y, 1.0, 1.0);
            assert!((0.0..=GLINT_MAX).contains(&g));
            lit += usize::from(g > 0.05);
        }
        assert!((10..1500).contains(&lit), "{lit} of 10000 glint");
    }
}
