//! A software rasteriser that draws exactly what the GPU renderer draws:
//! Gouraud terrain triangles (or with the new art, the ground's textures:
//! `docs/look/ground.md`), then sprites in depth order: palette-indexed,
//! or full colour with the owner's colour laid on through a mask
//! (`docs/look/colour-sets.md`).
//! It exists so a frame can be checked as a PNG without a GPU, and so the
//! renderer has a reference to match.

use crate::camera::Camera;
use crate::detail::Detail;
use crate::fog::FogLights;
use crate::ground::{self, Ground};
use crate::minimap::{Minimap, MinimapRect};
use crate::palette::{self, SHADOW};
use crate::scene::SpriteInstance;
use crate::sprites::{Atlas, COLOUR_PAGE};
use crate::terrain::{ChunkMesh, SHADE_ONE};

/// An RGBA8 image.
#[derive(Clone, PartialEq, Debug)]
pub struct Image {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// Pixels, row-major.
    pub pixels: Vec<[u8; 4]>,
}

impl Image {
    /// A filled image.
    pub fn new(width: u32, height: u32, fill: [u8; 4]) -> Image {
        Image {
            width,
            height,
            pixels: vec![fill; (width * height) as usize],
        }
    }

    /// Pixel at `(x, y)`, or transparent black outside.
    pub fn get(&self, x: i32, y: i32) -> [u8; 4] {
        if x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height {
            self.pixels[(y as u32 * self.width + x as u32) as usize]
        } else {
            [0; 4]
        }
    }

    fn put(&mut self, x: i32, y: i32, c: [u8; 4]) {
        if x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height {
            self.pixels[(y as u32 * self.width + x as u32) as usize] = c;
        }
    }

    /// Alpha-blends `c` over the pixel.
    fn blend(&mut self, x: i32, y: i32, c: [u8; 4]) {
        if c[3] == 255 {
            self.put(x, y, c);
            return;
        }
        if c[3] == 0 {
            return;
        }
        let d = self.get(x, y);
        let a = c[3] as u32;
        let mix = |s: u8, d: u8| ((s as u32 * a + d as u32 * (255 - a)) / 255) as u8;
        self.put(
            x,
            y,
            [mix(c[0], d[0]), mix(c[1], d[1]), mix(c[2], d[2]), 255],
        );
    }

    /// Flat RGBA bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.pixels.iter().flatten().copied().collect()
    }
}

/// Draws terrain chunks through the camera, each vertex in the fog light
/// of its corner (`None` draws everything lit) and each pixel in the grain
/// of its tile's type, exactly as the terrain shader does.
pub fn draw_terrain(
    img: &mut Image,
    cam: &Camera,
    chunks: &[ChunkMesh],
    fog: Option<&FogLights>,
    detail: &Detail,
) {
    let visible = cam.visible_rect();
    for chunk in chunks.iter().filter(|c| c.overlaps(visible)) {
        // `as_chunks::<3>().0` rather than `chunks_exact(3)`: same triangles,
        // same dropped remainder, but the chunk size is in the type, so the
        // indexing below cannot go out of bounds. Requires Rust 1.88, which is
        // the workspace MSRV.
        for tri in chunk.indices.as_chunks::<3>().0 {
            let v = [
                chunk.vertices[tri[0] as usize],
                chunk.vertices[tri[1] as usize],
                chunk.vertices[tri[2] as usize],
            ];
            let p = v.map(|v| cam.to_window(v.pos[0], v.pos[1]));
            let c = v.map(|v| match fog {
                Some(f) => lit(v.colour, f.at(v.corner[0] as i32, v.corner[1] as i32)),
                None => v.colour,
            });
            let uv = v.map(|v| (v.detail[0] as f32, v.detail[1] as f32));
            triangle(img, p, c, uv, detail);
        }
    }
}

/// Draws terrain chunks built by [`crate::terrain::build_all_textured`]
/// painted with the ground's textures, as the GPU's ground shader does:
/// each pixel's world position interpolated from its tile's corners, its
/// colour from [`Ground::colour`] at `time` seconds, the textures read at
/// the triangle's [`ground::lod`] with bilinear filtering between their
/// levels, and scaled by the slope's shade and the fog's light, both
/// interpolated from the vertices.
pub fn draw_ground(
    img: &mut Image,
    cam: &Camera,
    chunks: &[ChunkMesh],
    fog: Option<&FogLights>,
    ground: Ground,
    time: f32,
) {
    let visible = cam.visible_rect();
    for chunk in chunks.iter().filter(|c| c.overlaps(visible)) {
        for tri in chunk.indices.as_chunks::<3>().0 {
            let v = [
                chunk.vertices[tri[0] as usize],
                chunk.vertices[tri[1] as usize],
                chunk.vertices[tri[2] as usize],
            ];
            let p = v.map(|v| cam.to_window(v.pos[0], v.pos[1]));
            let light = v.map(|v| {
                let fog = fog.map_or(255, |f| f.at(v.corner[0] as i32, v.corner[1] as i32));
                v.colour[0] as f32 / SHADE_ONE * fog as f32 / 255.0
            });
            let world = v.map(|v| (v.corner[0] as f32, v.corner[1] as f32));
            ground_triangle(img, p, light, world, |x, y, lod| {
                ground.colour(x, y, time, lod)
            });
        }
    }
}

/// A triangle of textured ground with the same fill rule as [`triangle`]:
/// `colour` gives the ground at an interpolated world position and the
/// triangle's mip level, scaled by the interpolated light.
fn ground_triangle(
    img: &mut Image,
    p: [(f32, f32); 3],
    light: [f32; 3],
    world: [(f32, f32); 3],
    colour: impl Fn(f32, f32, f32) -> [f32; 3],
) {
    let Some((min_x, max_x, min_y, max_y)) = bounds(img, p) else {
        return;
    };
    let edge = |a: (f32, f32), b: (f32, f32), x: f32, y: f32| {
        (b.0 - a.0) * (y - a.1) - (b.1 - a.1) * (x - a.0)
    };
    let area = edge(p[0], p[1], p[2].0, p[2].1);
    if area.abs() < 1e-6 {
        return;
    }
    // How the world position moves a pixel right and a pixel down: each
    // barycentric weight is linear in the pixel, so this is constant.
    let dw = [
        ((p[1].1 - p[2].1) / area, (p[2].0 - p[1].0) / area),
        ((p[2].1 - p[0].1) / area, (p[0].0 - p[2].0) / area),
        ((p[0].1 - p[1].1) / area, (p[1].0 - p[0].0) / area),
    ];
    let d = |axis: usize| {
        let at = |k: usize| if axis == 0 { dw[k].0 } else { dw[k].1 };
        (
            world[0].0 * at(0) + world[1].0 * at(1) + world[2].0 * at(2),
            world[0].1 * at(0) + world[1].1 * at(1) + world[2].1 * at(2),
        )
    };
    let lod = ground::lod(d(0), d(1));
    for y in min_y..max_y {
        for x in min_x..max_x {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let w0 = edge(p[1], p[2], fx, fy) / area;
            let w1 = edge(p[2], p[0], fx, fy) / area;
            let w2 = edge(p[0], p[1], fx, fy) / area;
            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                let l = light[0] * w0 + light[1] * w1 + light[2] * w2;
                if l <= 0.0 {
                    img.put(x, y, [0, 0, 0, 255]);
                    continue;
                }
                let wx = world[0].0 * w0 + world[1].0 * w1 + world[2].0 * w2;
                let wy = world[0].1 * w0 + world[1].1 * w1 + world[2].1 * w2;
                let c = colour(wx, wy, lod);
                let ch = |k: usize| (c[k] * l * 255.0).round().clamp(0.0, 255.0) as u8;
                img.put(x, y, [ch(0), ch(1), ch(2), 255]);
            }
        }
    }
}

/// The window pixels a triangle's box covers, clipped to the image:
/// `(min_x, max_x, min_y, max_y)`, or `None` if nothing.
fn bounds(img: &Image, p: [(f32, f32); 3]) -> Option<(i32, i32, i32, i32)> {
    let min_x = p
        .iter()
        .map(|q| q.0)
        .fold(f32::MAX, f32::min)
        .floor()
        .max(0.0) as i32;
    let max_x = p
        .iter()
        .map(|q| q.0)
        .fold(f32::MIN, f32::max)
        .ceil()
        .min(img.width as f32) as i32;
    let min_y = p
        .iter()
        .map(|q| q.1)
        .fold(f32::MAX, f32::min)
        .floor()
        .max(0.0) as i32;
    let max_y = p
        .iter()
        .map(|q| q.1)
        .fold(f32::MIN, f32::max)
        .ceil()
        .min(img.height as f32) as i32;
    (min_x < max_x && min_y < max_y).then_some((min_x, max_x, min_y, max_y))
}

/// `c` scaled by `light / 255`, the alpha untouched.
fn lit(c: [u8; 4], light: u8) -> [u8; 4] {
    if light == 255 {
        return c;
    }
    let l = u32::from(light);
    let scale = |v: u8| ((u32::from(v) * l + 127) / 255) as u8;
    [scale(c[0]), scale(c[1]), scale(c[2]), c[3]]
}

/// Gouraud-shaded triangle with a top-left fill rule, each pixel scaled by
/// the grain texel its interpolated `uv` falls in (nearest, as the shader
/// reads it).
fn triangle(
    img: &mut Image,
    p: [(f32, f32); 3],
    c: [[u8; 4]; 3],
    uv: [(f32, f32); 3],
    detail: &Detail,
) {
    let Some((min_x, max_x, min_y, max_y)) = bounds(img, p) else {
        return;
    };
    let edge = |a: (f32, f32), b: (f32, f32), x: f32, y: f32| {
        (b.0 - a.0) * (y - a.1) - (b.1 - a.1) * (x - a.0)
    };
    let area = edge(p[0], p[1], p[2].0, p[2].1);
    if area.abs() < 1e-6 {
        return;
    }
    for y in min_y..max_y {
        for x in min_x..max_x {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let w0 = edge(p[1], p[2], fx, fy) / area;
            let w1 = edge(p[2], p[0], fx, fy) / area;
            let w2 = edge(p[0], p[1], fx, fy) / area;
            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                let u = uv[0].0 * w0 + uv[1].0 * w1 + uv[2].0 * w2;
                let v = uv[0].1 * w0 + uv[1].1 * w1 + uv[2].1 * w2;
                let grain = detail.texel(u.floor() as i32, v.floor() as i32) as f32
                    / crate::detail::NEUTRAL as f32;
                let ch = |i: usize| {
                    let base = c[0][i] as f32 * w0 + c[1][i] as f32 * w1 + c[2][i] as f32 * w2;
                    (base * grain).round().min(255.0) as u8
                };
                img.put(x, y, [ch(0), ch(1), ch(2), 255]);
            }
        }
    }
}

/// A colour frame's pixel with the owner's colour laid on, in sRGB bytes
/// as the palette stores them (`docs/look/colour-sets.md`): the pixel's
/// luma picks a step along `ramp` (the row's eight player-ramp entries,
/// dark to light), lerped to the next, and `mask` blends from the pixel's
/// own colour to that. Alpha is untouched. The sprite shader does the same
/// sums in floats.
pub fn tint(c: [u8; 4], mask: u8, ramp: &[[u8; 4]]) -> [u8; 4] {
    if mask == 0 {
        return c;
    }
    let l = (0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32) / 255.0;
    let t = l.clamp(0.0, 1.0) * 7.0;
    let i = (t.floor() as usize).min(7);
    let (lo, hi, f) = (ramp[i], ramp[(i + 1).min(7)], t - i as f32);
    let m = mask as f32 / 255.0;
    let ch = |k: usize| {
        let tint = lo[k] as f32 + (hi[k] as f32 - lo[k] as f32) * f;
        (c[k] as f32 + (tint - c[k] as f32) * m)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    [ch(0), ch(1), ch(2), c[3]]
}

/// Draws sprites through the camera, nearest-neighbour scaled.
pub fn draw_sprites(
    img: &mut Image,
    cam: &Camera,
    atlas: &Atlas,
    palette_tex: &[[u8; 4]],
    sprites: &[SpriteInstance],
) {
    let visible = cam.visible_rect();
    for s in sprites {
        let (x0, y0, zoom) = if s.screen {
            (s.x, s.y, 1.0)
        } else {
            if s.x + s.w < visible.0 || s.x > visible.2 || s.y + s.h < visible.1 || s.y > visible.3
            {
                continue;
            }
            let (x, y) = cam.to_window(s.x, s.y);
            (x, y, cam.zoom())
        };
        let dw = (s.w * zoom).round() as i32;
        let dh = (s.h * zoom).round() as i32;
        let (x0, y0) = (x0.round() as i32, y0.round() as i32);
        let row = s.row as usize * 256;
        // Source scale: a stretched fill (w != uw) maps proportionally.
        let sxs = s.uw as f32 / (s.w * zoom).max(1.0);
        let sys = s.vh as f32 / (s.h * zoom).max(1.0);
        for dy in 0..dh {
            let sy = (dy as f32 * sys) as u32;
            if sy >= s.vh as u32 {
                continue;
            }
            for dx in 0..dw {
                let mut sx = (dx as f32 * sxs) as u32;
                if sx >= s.uw as u32 {
                    continue;
                }
                if s.flip {
                    sx = s.uw as u32 - 1 - sx;
                }
                let (ax, ay) = (s.u as u32 + sx, s.v as u32 + sy);
                if s.page & COLOUR_PAGE != 0 {
                    // Full colour: tinted where the mask says, lit, and
                    // laid over what is behind by its own alpha.
                    let c = atlas.rgba_at(s.page, ax, ay);
                    if c[3] == 0 {
                        continue;
                    }
                    let ramp = &palette_tex[row + palette::PLAYER_RAMP.start as usize..][..8];
                    let c = tint(c, atlas.mask_at(s.page, ax, ay), ramp);
                    img.blend(x0 + dx, y0 + dy, lit(c, s.light));
                    continue;
                }
                let idx = atlas.index_at(s.page, ax, ay);
                if idx == palette::TRANSPARENT {
                    continue;
                }
                let c = if idx == SHADOW {
                    palette_tex[SHADOW as usize]
                } else {
                    palette_tex[row + idx as usize]
                };
                img.blend(x0 + dx, y0 + dy, lit(c, s.light));
            }
        }
    }
}

/// Draws the minimap in its diamond, nearest-neighbour, as the UI pass
/// does: the map's `(0, 0)` corner at the top point.
pub fn draw_minimap(img: &mut Image, m: &Minimap, rect: MinimapRect) {
    if m.width == 0 || m.height == 0 {
        return;
    }
    let (hw, hh) = (rect.w * 0.5, rect.h * 0.5);
    let x0 = (rect.cx - hw).floor().max(0.0) as i32;
    let x1 = (rect.cx + hw).ceil().min(img.width as f32) as i32;
    let y0 = (rect.cy - hh).floor().max(0.0) as i32;
    let y1 = (rect.cy + hh).ceil().min(img.height as f32) as i32;
    for y in y0..y1 {
        for x in x0..x1 {
            let Some((u, v)) = rect.to_uv(x as f32 + 0.5, y as f32 + 0.5) else {
                continue;
            };
            let tx = ((u * m.width as f32) as u32).min(m.width - 1);
            let ty = ((v * m.height as f32) as u32).min(m.height - 1);
            img.put(x, y, m.pixels[(ty * m.width + tx) as usize]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;
    use crate::terrain;
    use sim::{SimConfig, Simulation};

    /// The spec's tint, worked in doubles straight from
    /// `docs/look/colour-sets.md`, for checking [`tint`] against.
    fn spec_tint(c: [u8; 4], mask: u8, ramp: &[[u8; 4]]) -> [u8; 4] {
        let l = (0.299 * c[0] as f64 + 0.587 * c[1] as f64 + 0.114 * c[2] as f64) / 255.0;
        let t = l.clamp(0.0, 1.0) * 7.0;
        let i = t.floor() as usize;
        let lerp = |a: f64, b: f64, f: f64| a + (b - a) * f;
        let ch = |k: usize| {
            let tint = lerp(ramp[i][k] as f64, ramp[(i + 1).min(7)][k] as f64, t.fract());
            lerp(c[k] as f64, tint, mask as f64 / 255.0).round() as u8
        };
        [ch(0), ch(1), ch(2), c[3]]
    }

    /// A colour frame draws per `docs/look/colour-sets.md`: the masked
    /// cloth takes each owner's ramp by its grey, the unmasked red is
    /// itself for every owner, the shadow lays over the ground by its
    /// alpha, the fog's light dims it, and a mirrored draw reads the
    /// frame from its other edge.
    #[test]
    fn a_colour_frame_takes_its_owners_colour_and_blends_by_alpha() {
        use crate::sheets::fixture;
        assert_eq!(palette::PLAYER_RAMP.start, 240, "the spec's ramp");
        let atlas = Atlas::with_sheets(&[fixture::house("raster")]);
        let (f, _) = atlas.frame(sim::kinds::HOUSE, 0).unwrap();
        assert_eq!(f.page & COLOUR_PAGE, COLOUR_PAGE);
        let pal = palette::texture();
        let cam = Camera::new(16, 16, (16.0, 8.0));
        let ground = [90, 110, 70, 255];
        let draw = |row: u8, light: u8, flip: bool| {
            let mut img = Image::new(16, 8, ground);
            let s = SpriteInstance {
                x: 1.0,
                y: 1.0,
                // A pixel of the frame to a pixel of the window.
                w: f.w as f32,
                h: f.h as f32,
                page: f.page,
                u: f.x,
                v: f.y,
                uw: f.w,
                vh: f.h,
                row,
                flip,
                depth: 0.0,
                slot: u32::MAX,
                screen: true,
                light,
            };
            draw_sprites(&mut img, &cam, &atlas, &pal, &[s]);
            img
        };
        let ramp = |row: usize| &pal[row * 256 + 240..row * 256 + 248];
        let (one, two) = (draw(1, 255, false), draw(2, 255, false));
        for (row, img) in [(1, &one), (2, &two)] {
            assert_eq!(img.get(1, 1), spec_tint(fixture::CLOTH, 255, ramp(row)));
            assert_eq!(img.get(4, 1), spec_tint(fixture::HALF, 128, ramp(row)));
            assert_eq!(img.get(2, 1), fixture::RED);
            assert_eq!(img.get(1, 2), fixture::GREEN);
            assert_eq!(img.get(0, 0), ground, "outside the frame");
        }
        assert_ne!(one.get(1, 1), two.get(1, 1), "two owners, two colours");
        // Lit at 128, a quarter of the way up a step: exactly the spec's
        // lerp, for the cloth's luma of 128 / 255.
        assert_eq!(
            tint(fixture::CLOTH, 255, ramp(1)),
            spec_tint(fixture::CLOTH, 255, ramp(1))
        );
        // Source over: half-covered black over the ground.
        let half = |g: u8| ((g as u32 * 127) / 255) as u8;
        assert_eq!(one.get(3, 1), [half(90), half(110), half(70), 255]);
        // Drawn from memory: dimmed as an indexed frame is.
        let dim = draw(1, 128, false);
        assert_eq!(dim.get(2, 1), lit(fixture::RED, 128));
        assert_eq!(dim.get(1, 2), lit(fixture::GREEN, 128));
        // Mirrored: the frame's right edge on the left.
        let flipped = draw(1, 255, true);
        assert_eq!(flipped.get(1, 1), one.get(4, 1));
        assert_eq!(flipped.get(3, 1), one.get(2, 1));
        // Scaled to half (the frame's 1x size): every other pixel.
        let mut small = Image::new(16, 8, ground);
        let s = SpriteInstance {
            x: 1.0,
            y: 1.0,
            w: f.draw_w(),
            h: f.draw_h(),
            page: f.page,
            u: f.x,
            v: f.y,
            uw: f.w,
            vh: f.h,
            row: 1,
            flip: false,
            depth: 0.0,
            slot: u32::MAX,
            screen: true,
            light: 255,
        };
        draw_sprites(&mut small, &cam, &atlas, &pal, &[s]);
        assert_eq!(small.get(1, 1), one.get(1, 1));
        assert_eq!(small.get(2, 1), one.get(3, 1));
        assert_eq!(small.get(3, 1), ground);
    }

    #[test]
    fn triangle_fills_inside_only() {
        let mut img = Image::new(10, 10, [0, 0, 0, 255]);
        triangle(
            &mut img,
            [(1.0, 1.0), (9.0, 1.0), (1.0, 9.0)],
            [[255, 0, 0, 255]; 3],
            [(0.0, 0.0); 3],
            &Detail::flat(),
        );
        assert_eq!(img.get(2, 2), [255, 0, 0, 255]);
        assert_eq!(img.get(8, 8), [0, 0, 0, 255]);
        assert_eq!(img.get(0, 0), [0, 0, 0, 255]);
    }

    /// The grain multiplies the ground: a sheet of half-neutral texels
    /// draws it at half the brightness.
    #[test]
    fn the_grain_scales_the_ground() {
        let sim = Simulation::new(3, SimConfig::default());
        let chunks = terrain::build_all(sim.map());
        let mut cam = Camera::new(sim.map().width(), sim.map().height(), (64.0, 48.0));
        let (sx, sy) = sim.starts()[0];
        cam.look_at_tile(sx as f32 + 0.5, sy as f32 + 0.5);
        let mut plain = Image::new(64, 48, [0, 0, 0, 255]);
        draw_terrain(&mut plain, &cam, &chunks, None, &Detail::flat());
        let half = Detail {
            width: 128,
            height: 64 * 8,
            texels: vec![64; 128 * 64 * 8],
        };
        let mut grained = Image::new(64, 48, [0, 0, 0, 255]);
        draw_terrain(&mut grained, &cam, &chunks, None, &half);
        let (a, b) = (plain.get(32, 24), grained.get(32, 24));
        for i in 0..3 {
            assert!((a[i] as i32 / 2 - b[i] as i32).abs() <= 1, "{a:?} {b:?}");
        }
    }

    /// The textured ground (`docs/look/ground.md`): a pixel is the ground's
    /// colour at the world point under it, scaled by the fog's light;
    /// raising the ground lifts the picture without sliding the texture
    /// over it; and a frame is drawn the same every time.
    #[test]
    fn the_ground_is_painted_from_its_textures() {
        use crate::ground::{Ground, TypeMap};
        let art = crate::ground::tests::committed_art();
        let mut map = sim::TileMap::new(12, 12);
        for y in 3..7 {
            for x in 5..9 {
                map.set_terrain(x, y, sim::Terrain::Sand);
            }
        }
        let types = TypeMap::from_map(&map);
        let ground = Ground {
            art: &art,
            types: &types,
        };
        let mut cam = Camera::new(12, 12, (96.0, 64.0));
        cam.set_zoom_index(4); // 2x
        cam.look_at_tile(6.0, 5.0);
        let draw = |map: &sim::TileMap, fog: Option<&FogLights>| {
            let mut img = Image::new(96, 64, [0, 0, 0, 255]);
            let chunks = terrain::build_all_textured(map);
            draw_ground(&mut img, &cam, &chunks, fog, ground, 0.0);
            img
        };
        let flat = draw(&map, None);
        assert_eq!(flat, draw(&map, None), "the same picture every time");
        // The pixel at the middle of the window, against the reference.
        let (wsx, wsy) = cam.from_window(48.5, 32.5);
        let (wx, wy) = crate::iso::unproject(wsx, wsy);
        let (ax, ay) = crate::iso::unproject(1.0 / cam.zoom(), 0.0);
        let (bx, by) = crate::iso::unproject(0.0, 1.0 / cam.zoom());
        let want = ground.colour(wx, wy, 0.0, crate::ground::lod((ax, ay), (bx, by)));
        let got = flat.get(48, 32);
        for k in 0..3 {
            assert!(
                (got[k] as f32 - want[k] * 255.0).abs() <= 1.0,
                "{got:?} vs {want:?}"
            );
        }
        // Seen once: half the light. Never seen: black.
        let mut fog = sim::Fog::new(12, 12);
        for y in 0..12 {
            for x in 0..12 {
                fog.see(x, y);
            }
        }
        fog.clear_visible();
        let dim = draw(&map, Some(&FogLights::from_fog(&fog)));
        let d = dim.get(48, 32);
        for k in 0..3 {
            assert!(
                (d[k] as i32 - (got[k] as i32 + 1) / 2).abs() <= 1,
                "{d:?} {got:?}"
            );
        }
        let dark = draw(&map, Some(&FogLights::from_fog(&sim::Fog::new(12, 12))));
        assert_eq!(dark.get(48, 32), [0, 0, 0, 255]);
        // The whole map a level up: the same ground, 16 px higher at 1x.
        let mut high = map.clone();
        for y in 0..=12 {
            for x in 0..=12 {
                high.set_corner(x, y, 1);
            }
        }
        let lifted = draw(&high, None);
        let lift = (crate::iso::ELEV_PX * cam.zoom()) as i32;
        for (x, y) in [(48, 40), (20, 50), (70, 45)] {
            assert_eq!(lifted.get(x, y - lift), flat.get(x, y), "at {x},{y}");
        }
    }

    #[test]
    fn a_frame_renders_terrain_and_sprites() {
        let sim = Simulation::new(3, SimConfig::default());
        let atlas = Atlas::placeholder();
        let chunks = terrain::build_all(sim.map());
        let scene = Scene::build(&sim, &atlas, None, 0.0);
        let mut cam = Camera::new(sim.map().width(), sim.map().height(), (640.0, 480.0));
        let (sx, sy) = sim.starts()[0];
        cam.look_at_tile(sx as f32 + 0.5, sy as f32 + 0.5);
        let mut img = Image::new(640, 480, [0, 0, 0, 255]);
        draw_terrain(&mut img, &cam, &chunks, None, &Detail::flat());
        let black = img.pixels.iter().filter(|p| **p == [0, 0, 0, 255]).count();
        assert!(
            black < 640 * 480 / 20,
            "terrain should cover the frame: {black} black px"
        );
        let before = img.clone();
        draw_sprites(&mut img, &cam, &atlas, &palette::texture(), &scene.sprites);
        assert_ne!(img, before, "sprites should change pixels");
        // The Town Center's roof is player 0's blue, at the centre of the frame.
        let blue = palette::PLAYER_COLOURS[0];
        let near_centre = (280..360)
            .flat_map(|x| (160..260).map(move |y| (x, y)))
            .filter(|&(x, y)| img.get(x, y) == [blue[0], blue[1], blue[2], 255])
            .count();
        assert!(
            near_centre > 100,
            "expected the TC roof near the centre, found {near_centre} blue px"
        );
    }

    /// The three states on the ground: black where never seen, dimmed
    /// where seen once, live in sight; and a sprite's light scales it.
    ///
    /// REQ: GD-FOG-01
    #[test]
    fn fog_darkens_the_ground_and_the_minimap_draws_in_its_diamond() {
        let sim = Simulation::new(3, SimConfig::default());
        let chunks = terrain::build_all(sim.map());
        let mut cam = Camera::new(sim.map().width(), sim.map().height(), (640.0, 480.0));
        let (sx, sy) = sim.starts()[0];
        cam.look_at_tile(sx as f32 + 0.5, sy as f32 + 0.5);
        let mut lit_img = Image::new(640, 480, [0, 0, 0, 255]);
        draw_terrain(&mut lit_img, &cam, &chunks, None, &Detail::flat());
        let mut fogged = Image::new(640, 480, [0, 0, 0, 255]);
        let lights = FogLights::from_fog(sim.fog(0).unwrap());
        draw_terrain(&mut fogged, &cam, &chunks, Some(&lights), &Detail::flat());
        // At the start the Town Center sees its surroundings: the centre
        // pixel is drawn as it would be with no fog.
        assert_eq!(fogged.get(320, 240), lit_img.get(320, 240));
        // Nothing has seen the far corner of the window: black.
        let corner = fogged.get(2, 2);
        assert_eq!(corner, [0, 0, 0, 255], "{corner:?}");
        assert_ne!(lit_img.get(2, 2), [0, 0, 0, 255]);
        // Seen once: a corner far from any unit but inside the map.
        let mut fog = sim.fog(0).unwrap().clone();
        for y in 0..sim.map().height() {
            for x in 0..sim.map().width() {
                fog.see(x, y);
            }
        }
        fog.clear_visible();
        let mut explored = Image::new(640, 480, [0, 0, 0, 255]);
        draw_terrain(
            &mut explored,
            &cam,
            &chunks,
            Some(&FogLights::from_fog(&fog)),
            &Detail::flat(),
        );
        let (a, b) = (explored.get(320, 240), lit_img.get(320, 240));
        assert!(
            a[0] < b[0] && a[1] < b[1] && a[0] > 0,
            "seen once is dimmed, not black: {a:?} vs {b:?}"
        );
        assert_eq!(lit([200, 100, 50, 255], 128), [100, 50, 25, 255]);
        assert_eq!(lit([200, 100, 50, 7], 255), [200, 100, 50, 7]);

        let m = Minimap::render(&sim);
        let rect = MinimapRect::bottom_right((640.0, 480.0), 128.0, 8.0);
        let before = fogged.clone();
        draw_minimap(&mut fogged, &m, rect);
        assert_ne!(fogged, before);
        let centre = fogged.get(rect.cx as i32, rect.cy as i32);
        let (mx, my) = (m.width / 2, m.height / 2);
        assert!(
            [(mx - 1, my - 1), (mx, my - 1), (mx - 1, my), (mx, my)]
                .iter()
                .any(|&(x, y)| m.pixels[(y * m.width + x) as usize] == centre),
            "the diamond's centre is the map's centre: {centre:?}"
        );
        assert_eq!(
            fogged.get(
                (rect.cx - rect.w * 0.5) as i32 + 1,
                (rect.cy - rect.h * 0.5) as i32 + 1
            ),
            before.get(
                (rect.cx - rect.w * 0.5) as i32 + 1,
                (rect.cy - rect.h * 0.5) as i32 + 1
            ),
            "outside the diamond nothing is drawn"
        );
    }
}
