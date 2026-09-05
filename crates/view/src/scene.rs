//! Turns simulation state into a sorted list of sprite instances.

use sim::kinds;
use sim::{Simulation, Vec2Fx};

use crate::fx_to_f32;
use crate::iso;
use crate::palette;
use crate::sprites::Atlas;

/// One sprite to draw, in world-screen space at 1×.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SpriteInstance {
    /// Top-left in world-screen px.
    pub x: f32,
    /// Top-left in world-screen px.
    pub y: f32,
    /// Size in px at 1×.
    pub w: f32,
    /// Size in px at 1×.
    pub h: f32,
    /// Atlas rectangle, px.
    pub u: u16,
    /// Atlas rectangle, px.
    pub v: u16,
    /// Atlas rectangle, px.
    pub uw: u16,
    /// Atlas rectangle, px.
    pub vh: u16,
    /// Palette row (0 neutral, 1..=8 players).
    pub row: u8,
    /// Mirror horizontally.
    pub flip: bool,
    /// Draw order; larger draws later (closer to the viewer).
    pub depth: f32,
    /// Entity slot, for stable ordering and picking; `u32::MAX` for overlays.
    pub slot: u32,
    /// Positioned in window pixels rather than world-screen space.
    pub screen: bool,
}

/// A building the player is about to place.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Ghost {
    /// What.
    pub kind: sim::entity::KindId,
    /// Anchor tile.
    pub x: i32,
    /// Anchor tile.
    pub y: i32,
    /// Whether placement would succeed.
    pub ok: bool,
    /// Player colour row.
    pub row: u8,
}

/// A frame's worth of sprites, sorted back to front.
#[derive(Clone, Default, Debug)]
pub struct Scene {
    /// World sprites in draw order.
    pub sprites: Vec<SpriteInstance>,
    /// Screen-space sprites (the HUD), drawn after the world in order.
    pub ui: Vec<SpriteInstance>,
}

impl Scene {
    /// Builds the scene. `prev` holds last tick's positions by slot for
    /// interpolation at `alpha`; pass `None` to draw current positions.
    pub fn build(sim: &Simulation, atlas: &Atlas, prev: Option<&[Vec2Fx]>, alpha: f32) -> Scene {
        Scene::build_with(sim, atlas, prev, alpha, &[], None)
    }

    /// Builds the scene with selection rings under `selected` slots and an
    /// optional placement ghost.
    pub fn build_with(
        sim: &Simulation,
        atlas: &Atlas,
        prev: Option<&[Vec2Fx]>,
        alpha: f32,
        selected: &[u32],
        ghost: Option<Ghost>,
    ) -> Scene {
        let world = sim.world();
        let map = sim.map();
        let mut sprites = Vec::with_capacity(world.len() + selected.len() + 2);
        for slot in world.slots() {
            let i = slot.index();
            let kind = world.kind[i];
            let info = kinds::info(kind);
            // A site shows pegs until half built, then the building itself.
            let half_built =
                world.construction[i].is_some_and(|done| done * 2 < info.build_ticks().max(1));
            let looked_up = if half_built {
                atlas.site(info.footprint).map(|f| (f, false))
            } else {
                atlas.frame(kind, world.facing[i])
            };
            let Some((frame, flip)) = looked_up else {
                continue;
            };
            let cur = world.pos[i];
            let (px, py) = match prev {
                Some(p) if i < p.len() && kinds::info(kind).mobile && close(p[i], cur) => {
                    let (ax, ay) = (fx_to_f32(p[i].x), fx_to_f32(p[i].y));
                    let (bx, by) = (fx_to_f32(cur.x), fx_to_f32(cur.y));
                    (ax + (bx - ax) * alpha, ay + (by - ay) * alpha)
                }
                _ => (fx_to_f32(cur.x), fx_to_f32(cur.y)),
            };
            let h = iso::ground_height(map, px, py);
            let (gx, gy) = iso::project(px, py, h);
            let anchor_x = if flip {
                frame.w as f32 - frame.anchor_x as f32
            } else {
                frame.anchor_x as f32
            };
            let footprint = kinds::info(kind).footprint.max(1) as f32;
            let depth = px + py + (footprint - 1.0) * 0.5;
            let row = palette::row_for_owner(world.owner[i]);
            if selected.contains(&(i as u32)) {
                if let Some(r) = atlas.ring(kinds::info(kind).footprint) {
                    sprites.push(overlay(r, gx, gy, row, depth - 0.01, i as u32));
                }
            }
            sprites.push(SpriteInstance {
                x: (gx - anchor_x).round(),
                y: (gy - frame.anchor_y as f32).round(),
                w: frame.w as f32,
                h: frame.h as f32,
                u: frame.x,
                v: frame.y,
                uw: frame.w,
                vh: frame.h,
                row,
                flip,
                depth,
                slot: i as u32,
                screen: false,
            });
        }
        if let Some(g) = ghost {
            let fp = kinds::info(g.kind).footprint.max(1);
            let centre = sim::nav::building_centre(g.x, g.y, fp as i32);
            let (cx, cy) = (fx_to_f32(centre.x), fx_to_f32(centre.y));
            let h = iso::ground_height(map, cx, cy);
            let (gx, gy) = iso::project(cx, cy, h);
            let depth = cx + cy + (fp as f32 - 1.0) * 0.5;
            if let Some((frame, _)) = atlas.frame(g.kind, 0) {
                sprites.push(overlay(frame, gx, gy, g.row, depth + 0.5, u32::MAX));
            }
            // The valid/blocked hatch draws over the ghost so it always shows.
            if let Some(f) = atlas.footprint(fp, g.ok) {
                sprites.push(overlay(f, gx, gy, 0, depth + 0.51, u32::MAX));
            }
        }
        sprites.sort_by(|a, b| a.depth.total_cmp(&b.depth).then(a.slot.cmp(&b.slot)));
        Scene {
            sprites,
            ui: Vec::new(),
        }
    }
}

/// A world-space overlay sprite anchored on a ground point.
fn overlay(
    frame: &crate::sprites::Frame,
    gx: f32,
    gy: f32,
    row: u8,
    depth: f32,
    slot: u32,
) -> SpriteInstance {
    SpriteInstance {
        x: (gx - frame.anchor_x as f32).round(),
        y: (gy - frame.anchor_y as f32).round(),
        w: frame.w as f32,
        h: frame.h as f32,
        u: frame.x,
        v: frame.y,
        uw: frame.w,
        vh: frame.h,
        row,
        flip: false,
        depth,
        slot,
        screen: false,
    }
}

/// Interpolation guard: only blend positions that are plausibly one tick
/// apart, so a respawned or teleported unit does not smear across the map.
fn close(a: Vec2Fx, b: Vec2Fx) -> bool {
    a.manhattan(b) < sim::Fx::from_int(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::{MapKind, MapSpec, SimConfig};

    #[test]
    fn scene_matches_world_and_is_depth_sorted() {
        let sim = Simulation::new(5, SimConfig::default());
        let atlas = Atlas::placeholder();
        let scene = Scene::build(&sim, &atlas, None, 0.0);
        assert_eq!(scene.sprites.len(), sim.world().len());
        for w in scene.sprites.windows(2) {
            assert!(w[0].depth <= w[1].depth);
        }
        let tc = scene
            .sprites
            .iter()
            .find(|s| sim.world().kind[s.slot as usize] == kinds::TOWN_CENTER)
            .unwrap();
        assert_eq!((tc.uw, tc.vh), (192, 144));
        assert_eq!(tc.row, 1, "player 0 draws with palette row 1");
        let tree = scene
            .sprites
            .iter()
            .find(|s| sim.world().kind[s.slot as usize] == kinds::TREE)
            .unwrap();
        assert_eq!(tree.row, 0, "gaia draws neutral");
    }

    #[test]
    fn selection_rings_and_ghosts_are_added_in_order() {
        let sim = Simulation::new(5, SimConfig::default());
        let atlas = Atlas::placeholder();
        let first = sim.world().slots().next().unwrap().index() as u32;
        let (sx, sy) = sim.starts()[0];
        let ghost = Ghost {
            kind: kinds::HOUSE,
            x: sx + 4,
            y: sy,
            ok: sim.can_place(0, kinds::HOUSE, sx + 4, sy).is_ok(),
            row: 1,
        };
        let scene = Scene::build_with(&sim, &atlas, None, 0.0, &[first], Some(ghost));
        assert_eq!(scene.sprites.len(), sim.world().len() + 3);
        let fp = kinds::info(sim.world().kind[first as usize]).footprint;
        let ring = *atlas.ring(fp).unwrap();
        let is_ring = |s: &SpriteInstance| s.slot == first && s.u == ring.x && s.v == ring.y;
        let ring_pos = scene.sprites.iter().position(is_ring).unwrap();
        let unit_pos = scene
            .sprites
            .iter()
            .position(|s| s.slot == first && !is_ring(s))
            .unwrap();
        assert!(ring_pos < unit_pos, "ring draws under its unit");
        assert_eq!(
            scene.sprites.iter().filter(|s| s.slot == u32::MAX).count(),
            2,
            "footprint and ghost building"
        );
    }

    #[test]
    fn interpolation_blends_only_mobile_and_nearby() {
        let mut sim = Simulation::new(
            1,
            SimConfig {
                map: MapSpec {
                    kind: MapKind::Flat,
                    size: 64,
                    players: 1,
                },
                wander: false,
                ..SimConfig::default()
            },
        );
        sim.issue(sim::Command {
            player: 0,
            kind: sim::CommandKind::Spawn {
                kind: kinds::VILLAGER,
                pos: Vec2Fx::from_int(10, 10),
            },
        });
        for _ in 0..3 {
            sim.step();
        }
        let atlas = Atlas::placeholder();
        let prev = vec![Vec2Fx::from_int(9, 10)];
        let half = Scene::build(&sim, &atlas, Some(&prev), 0.5);
        let none = Scene::build(&sim, &atlas, None, 0.5);
        let (gx_half, _) = (half.sprites[0].x, half.sprites[0].y);
        let (gx_none, _) = (none.sprites[0].x, none.sprites[0].y);
        assert!(
            gx_half < gx_none,
            "halfway between x=9 and x=10 is further left"
        );
        let far = vec![Vec2Fx::from_int(50, 50)];
        let guarded = Scene::build(&sim, &atlas, Some(&far), 0.5);
        assert_eq!(
            guarded.sprites[0].x, gx_none,
            "teleports are not interpolated"
        );
    }
}
