//! The top-down minimap image: one pixel per tile.

use sim::kinds;
use sim::Simulation;

use crate::palette::{self, PLAYER_COLOURS};
use crate::terrain::terrain_colour;

/// An RGBA image, one pixel per tile.
#[derive(Clone, PartialEq, Debug)]
pub struct Minimap {
    /// Width in tiles.
    pub width: u32,
    /// Height in tiles.
    pub height: u32,
    /// RGBA pixels, row-major.
    pub pixels: Vec<[u8; 4]>,
}

impl Minimap {
    /// Renders terrain, scenery and units.
    pub fn render(sim: &Simulation) -> Minimap {
        let map = sim.map();
        let (w, h) = (map.width() as u32, map.height() as u32);
        let mut pixels = vec![[0u8; 4]; (w * h) as usize];
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let c = terrain_colour(map.terrain(x, y));
                let shade = 0.82 + 0.06 * map.elevation(x, y) as f32;
                let px = |v: f32| (v * shade * 255.0).min(255.0) as u8;
                pixels[(y as u32 * w + x as u32) as usize] = [px(c[0]), px(c[1]), px(c[2]), 255];
            }
        }
        let world = sim.world();
        let base = palette::base();
        let mut put = |x: i32, y: i32, c: [u8; 4]| {
            if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                pixels[(y as u32 * w + x as u32) as usize] = c;
            }
        };
        // Scenery first, units on top.
        for pass in 0..2 {
            for slot in world.slots() {
                let i = slot.index();
                let kind = world.kind[i];
                let info = kinds::info(kind);
                if (pass == 0) == info.mobile {
                    continue;
                }
                let x = world.pos[i].x.floor();
                let y = world.pos[i].y.floor();
                let owner = world.owner[i];
                let colour = if owner != kinds::GAIA && (owner as usize) < PLAYER_COLOURS.len() {
                    let c = PLAYER_COLOURS[owner as usize];
                    [c[0], c[1], c[2], 255]
                } else {
                    match kind {
                        kinds::TREE => base[palette::GREEN_DARK as usize],
                        kinds::BERRY_BUSH => base[palette::GREEN_LIGHT as usize],
                        kinds::GOLD_MINE => base[palette::GOLD as usize],
                        kinds::STONE_MINE => base[palette::GREY_LIGHT as usize],
                        _ => base[palette::HIDE as usize],
                    }
                };
                let fp = info.footprint as i32;
                if fp > 1 {
                    let half = fp / 2;
                    for dy in -half..fp - half {
                        for dx in -half..fp - half {
                            put(x + dx, y + dy, colour);
                        }
                    }
                } else {
                    put(x, y, colour);
                }
            }
        }
        Minimap {
            width: w,
            height: h,
            pixels,
        }
    }
}

/// Where the minimap diamond sits on screen, in window pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MinimapRect {
    /// Centre.
    pub cx: f32,
    /// Centre.
    pub cy: f32,
    /// Full diamond width.
    pub w: f32,
    /// Full diamond height.
    pub h: f32,
}

impl MinimapRect {
    /// A diamond of the given width anchored to the bottom-right of a window,
    /// with `margin` px of padding.
    pub fn bottom_right(viewport: (f32, f32), width: f32, margin: f32) -> MinimapRect {
        let h = width * 0.5;
        MinimapRect {
            cx: viewport.0 - margin - width * 0.5,
            cy: viewport.1 - margin - h * 0.5,
            w: width,
            h,
        }
    }

    /// The four corners `[top, right, bottom, left]` with their texture
    /// coordinates: the map's `(0,0)` corner is the top point.
    pub fn corners(&self) -> [((f32, f32), (f32, f32)); 4] {
        let (hw, hh) = (self.w * 0.5, self.h * 0.5);
        [
            ((self.cx, self.cy - hh), (0.0, 0.0)),
            ((self.cx + hw, self.cy), (1.0, 0.0)),
            ((self.cx, self.cy + hh), (1.0, 1.0)),
            ((self.cx - hw, self.cy), (0.0, 1.0)),
        ]
    }

    /// Window pixel → map fraction `(u, v)` in `[0, 1]`, or `None` outside
    /// the diamond.
    pub fn to_uv(&self, px: f32, py: f32) -> Option<(f32, f32)> {
        let lx = (px - self.cx) / (self.w * 0.5);
        let ly = (py - self.cy) / (self.h * 0.5);
        let u = (lx + ly + 1.0) * 0.5;
        let v = (ly - lx + 1.0) * 0.5;
        ((0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v)).then_some((u, v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::SimConfig;

    #[test]
    fn diamond_mapping_round_trips() {
        let r = MinimapRect::bottom_right((1280.0, 720.0), 256.0, 16.0);
        assert_eq!(r.to_uv(r.cx, r.cy), Some((0.5, 0.5)));
        for (p, uv) in r.corners() {
            let got = r.to_uv(p.0, p.1).unwrap();
            assert!(
                (got.0 - uv.0).abs() < 1e-4 && (got.1 - uv.1).abs() < 1e-4,
                "{p:?} -> {got:?} != {uv:?}"
            );
        }
        assert_eq!(
            r.to_uv(r.cx - r.w * 0.5, r.cy - r.h * 0.5),
            None,
            "outside the diamond"
        );
        assert_eq!(r.to_uv(0.0, 0.0), None);
    }

    #[test]
    fn minimap_shows_starts_in_player_colours() {
        let sim = Simulation::new(9, SimConfig::default());
        let m = Minimap::render(&sim);
        assert_eq!((m.width, m.height), (128, 128));
        for (p, &(sx, sy)) in sim.starts().iter().enumerate() {
            let c = PLAYER_COLOURS[p];
            assert_eq!(
                m.pixels[(sy as u32 * m.width + sx as u32) as usize],
                [c[0], c[1], c[2], 255]
            );
        }
        let greens = m
            .pixels
            .iter()
            .filter(|p| **p == palette::base()[palette::GREEN_DARK as usize])
            .count();
        assert!(greens > 100, "trees should show: {greens}");
    }
}
