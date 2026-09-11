//! A software rasteriser that draws exactly what the GPU renderer draws:
//! Gouraud terrain triangles, then palette-indexed sprites in depth order.
//! It exists so a frame can be checked as a PNG without a GPU, and so the
//! renderer has a reference to match.

use crate::camera::Camera;
use crate::palette::{self, SHADOW};
use crate::scene::SpriteInstance;
use crate::sprites::Atlas;
use crate::terrain::ChunkMesh;

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

/// Draws terrain chunks through the camera.
pub fn draw_terrain(img: &mut Image, cam: &Camera, chunks: &[ChunkMesh]) {
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
            triangle(img, p, v.map(|v| v.colour));
        }
    }
}

/// Gouraud-shaded triangle with a top-left fill rule.
fn triangle(img: &mut Image, p: [(f32, f32); 3], c: [[u8; 4]; 3]) {
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
    if min_x >= max_x || min_y >= max_y {
        return;
    }
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
                let ch = |i: usize| {
                    (c[0][i] as f32 * w0 + c[1][i] as f32 * w1 + c[2][i] as f32 * w2).round() as u8
                };
                img.put(x, y, [ch(0), ch(1), ch(2), 255]);
            }
        }
    }
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
                let idx = atlas.index_at(s.u as u32 + sx, s.v as u32 + sy);
                if idx == palette::TRANSPARENT {
                    continue;
                }
                let c = if idx == SHADOW {
                    palette_tex[SHADOW as usize]
                } else {
                    palette_tex[row + idx as usize]
                };
                img.blend(x0 + dx, y0 + dy, c);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;
    use crate::terrain;
    use sim::{SimConfig, Simulation};

    #[test]
    fn triangle_fills_inside_only() {
        let mut img = Image::new(10, 10, [0, 0, 0, 255]);
        triangle(
            &mut img,
            [(1.0, 1.0), (9.0, 1.0), (1.0, 9.0)],
            [[255, 0, 0, 255]; 3],
        );
        assert_eq!(img.get(2, 2), [255, 0, 0, 255]);
        assert_eq!(img.get(8, 8), [0, 0, 0, 255]);
        assert_eq!(img.get(0, 0), [0, 0, 0, 255]);
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
        draw_terrain(&mut img, &cam, &chunks);
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
}
