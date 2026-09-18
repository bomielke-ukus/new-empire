//! The top-down minimap image: one pixel per tile.

use sim::kinds;
use sim::{KindId, PlayerId, Simulation, Visibility};

use crate::fog;
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

/// The mark a thing leaves on the minimap: its owner's colour, or a
/// resource's.
fn mark(kind: KindId, owner: PlayerId) -> [u8; 4] {
    if owner != kinds::GAIA && (owner as usize) < PLAYER_COLOURS.len() {
        let c = PLAYER_COLOURS[owner as usize];
        return [c[0], c[1], c[2], 255];
    }
    let base = palette::base();
    match kind {
        kinds::TREE => base[palette::GREEN_DARK as usize],
        kinds::BERRY_BUSH => base[palette::GREEN_LIGHT as usize],
        kinds::GOLD_MINE => base[palette::GOLD as usize],
        kinds::STONE_MINE => base[palette::GREY_LIGHT as usize],
        _ => base[palette::HIDE as usize],
    }
}

impl Minimap {
    /// Renders terrain, scenery and units, all of them: the view with no
    /// fog.
    pub fn render(sim: &Simulation) -> Minimap {
        Minimap::render_for(sim, None)
    }

    /// Renders the map as `viewer` knows it (`GD-FOG-01`): ground never
    /// seen black, ground seen once dimmed with what was remembered on
    /// it, ground in sight live. `None` shows everything.
    pub fn render_for(sim: &Simulation, viewer: Option<u8>) -> Minimap {
        let map = sim.map();
        let fog = viewer.and_then(|p| sim.fog(p));
        let (w, h) = (map.width() as u32, map.height() as u32);
        let mut pixels = vec![[0u8; 4]; (w * h) as usize];
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let state = fog.map_or(Visibility::Visible, |f| f.state(x, y));
                let c = terrain_colour(map.terrain(x, y));
                let shade = (0.82 + 0.06 * map.elevation(x, y) as f32)
                    * fog::tile_light(state) as f32
                    / 255.0;
                let px = |v: f32| (v * shade * 255.0).min(255.0) as u8;
                pixels[(y as u32 * w + x as u32) as usize] = [px(c[0]), px(c[1]), px(c[2]), 255];
            }
        }
        let world = sim.world();
        let mut put = |x: i32, y: i32, c: [u8; 4], seen: Visibility| {
            if x >= 0
                && y >= 0
                && (x as u32) < w
                && (y as u32) < h
                && fog.is_none_or(|f| f.state(x, y) >= seen)
            {
                pixels[(y as u32 * w + x as u32) as usize] = c;
            }
        };
        let stamp = |put: &mut dyn FnMut(i32, i32, [u8; 4], Visibility),
                     x: i32,
                     y: i32,
                     fp: i32,
                     c: [u8; 4],
                     seen: Visibility| {
            if fp > 1 {
                let half = fp / 2;
                for dy in -half..fp - half {
                    for dx in -half..fp - half {
                        put(x + dx, y + dy, c, seen);
                    }
                }
            } else {
                put(x, y, c, seen);
            }
        };
        // What was seen once, where the ground was seen once.
        if let Some(f) = fog {
            for ((x, y), m) in f.memories() {
                let fp = kinds::info(m.kind).footprint as i32;
                let centre = sim::nav::building_centre(x, y, fp);
                stamp(
                    &mut put,
                    centre.x.floor(),
                    centre.y.floor(),
                    fp,
                    mark(m.kind, m.owner),
                    Visibility::Explored,
                );
            }
        }
        // Scenery first, units on top; only where in sight.
        for pass in 0..2 {
            for slot in world.slots() {
                let i = slot.index();
                let kind = world.kind[i];
                let info = kinds::info(kind);
                if (pass == 0) == info.mobile {
                    continue;
                }
                stamp(
                    &mut put,
                    world.pos[i].x.floor(),
                    world.pos[i].y.floor(),
                    info.footprint as i32,
                    mark(kind, world.owner[i]),
                    Visibility::Visible,
                );
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

    /// REQ: GD-FOG-01
    #[test]
    fn a_fogged_minimap_is_black_dimmed_or_live_and_marks_what_is_remembered() {
        let mut sim = Simulation::new(
            3,
            SimConfig {
                map: sim::MapSpec {
                    kind: sim::MapKind::Flat,
                    size: 64,
                    players: 2,
                },
                wander: false,
                ..SimConfig::default()
            },
        );
        let spawn = |sim: &mut Simulation, player: u8, kind, x: i32, y: i32| {
            sim.issue(sim::Command {
                player,
                kind: sim::CommandKind::Spawn {
                    kind,
                    pos: sim::nav::centre((x, y)),
                },
            });
        };
        spawn(&mut sim, 0, kinds::CLUBMAN, 10, 10);
        spawn(&mut sim, 1, kinds::CLUBMAN, 40, 40);
        spawn(&mut sim, 1, kinds::HOUSE, 12, 12);
        for _ in 0..3 {
            sim.step();
        }
        let all = Minimap::render(&sim);
        let mine = Minimap::render_for(&sim, Some(0));
        let at = |m: &Minimap, x: i32, y: i32| m.pixels[(y as u32 * m.width + x as u32) as usize];
        let red = PLAYER_COLOURS[1];
        assert_eq!(at(&all, 40, 40), [red[0], red[1], red[2], 255]);
        assert_eq!(at(&mine, 40, 40), [0, 0, 0, 255], "never seen: black");
        assert_eq!(at(&mine, 50, 20), [0, 0, 0, 255]);
        assert_eq!(at(&mine, 10, 10), at(&all, 10, 10), "in sight: live");
        assert_eq!(
            at(&mine, 12, 12),
            [red[0], red[1], red[2], 255],
            "their house, in sight"
        );

        let me = sim
            .world()
            .slots()
            .find(|s| sim.world().owner[s.index()] == 0)
            .map(|s| sim.world().id_at(s))
            .unwrap();
        sim.issue(sim::Command {
            player: 0,
            kind: sim::CommandKind::Move {
                ids: vec![me],
                target: sim::nav::centre((30, 10)),
            },
        });
        for _ in 0..300 {
            sim.step();
        }
        let later = Minimap::render_for(&sim, Some(0));
        let ground = at(&all, 8, 8);
        let dim = at(&later, 8, 8);
        assert!(
            dim[0] < ground[0] && dim[1] < ground[1] && dim != [0, 0, 0, 255],
            "seen once: dimmed ground, {dim:?} vs {ground:?}"
        );
        assert_eq!(
            at(&later, 12, 12),
            [red[0], red[1], red[2], 255],
            "the house is remembered where it stood"
        );
        assert_eq!(
            at(&later, 30, 10),
            at(&all, 30, 10),
            "where I stand now: live"
        );
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
