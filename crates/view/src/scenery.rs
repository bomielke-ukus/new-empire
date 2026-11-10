//! Scenery (`docs/look/ground.md`): flowers, tufts of grass, rocks, bushes
//! and reeds scattered over the ground. Pictures only: nothing here is in
//! the simulation, nothing is selectable (every sprite's slot is
//! `u32::MAX`), and the scatter is a pure function of the map's seed and
//! each tile's coordinates and ground, so the same map shows the same
//! meadow every frame and every run.
//!
//! What grows where: on grass, tufts and flowers each in drifts a few
//! tiles across, now and then a rock or a bush, about one thing to every
//! three or four tiles so units stay easy to read; reeds where grass or
//! sand meets shallow water; small rocks and dry tufts on dirt and desert;
//! a little on the beach, the forest floor and the snow; nothing in the
//! water. Nothing is drawn where a building, a site, rubble, a tree, a
//! mine or a bush stands, as the world is now (or as the viewer remembers
//! it, out of sight), nor on ground never seen; ground seen once is dimmed
//! as the terrain is.

use sim::entity::KindId;
use sim::{kinds, Simulation, Terrain, TileMap};

use crate::fog;
use crate::iso;
use crate::scene::{overlay, Scene, SpriteInstance};
use crate::sprites::{decor_id, Atlas};

/// Index of the flowers in [`crate::sprites::DECOR_KINDS`].
const FLOWERS: usize = 0;
/// Index of the tufts.
const TUFT: usize = 1;
/// Index of the rocks.
const ROCK: usize = 2;
/// Index of the bushes.
const BUSH: usize = 3;
/// Index of the reeds.
const REEDS: usize = 4;
/// Drawn this far behind its ground point in depth: scenery is low, so a
/// unit standing just behind a flower is drawn over it.
const DEPTH_BIAS: f32 = 0.25;
/// Tiles across a drift of flowers.
const DRIFT_TILES: f32 = 6.0;

/// One thing scattered on the ground.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Item {
    /// The tile it stands on.
    tile: (i32, i32),
    /// Its sprite, in full light.
    sprite: SpriteInstance,
}

/// A map's scenery, scattered once and drawn every frame around what
/// stands on the map then.
#[derive(Clone, Debug, Default)]
pub struct Scenery {
    /// Everything scattered, in depth order.
    items: Vec<Item>,
    width: i32,
    height: i32,
    /// Per tile, the [`Scenery::stamp`] of the last frame something stood
    /// on it: one pass over the world a frame, no clearing.
    cover: Vec<u32>,
    stamp: u32,
}

/// What variants of each kind the atlas holds, by
/// [`crate::sprites::DECOR_KINDS`] index.
fn variants(atlas: &Atlas) -> [Vec<KindId>; 5] {
    std::array::from_fn(|k| {
        (0..100)
            .map(|n| decor_id(k, n))
            .filter(|&id| atlas.frame(id, 0).is_some())
            .collect()
    })
}

/// A well-mixed 64-bit hash of the seed, a tile and a salt.
fn hash(seed: u64, x: i32, y: i32, salt: u64) -> u64 {
    let mut h = seed
        ^ (x as u32 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u32 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ salt.wrapping_mul(0x1656_67B1_9E37_79F9);
    // splitmix64's finaliser.
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^ (h >> 31)
}

/// A hash as a number in `[0, 1)`.
fn unit(h: u64) -> f32 {
    (h >> 40) as f32 / (1u64 << 24) as f32
}

fn smoothstep(lo: f32, hi: f32, v: f32) -> f32 {
    let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much a thing grows around tile `(x, y)`, 0 to 1: smooth value
/// noise over cells [`DRIFT_TILES`] across, so it comes in drifts; `salt`
/// gives each thing its own.
fn drift(seed: u64, x: i32, y: i32, salt: u64) -> f32 {
    let (fx, fy) = (
        (x as f32 + 0.5) / DRIFT_TILES,
        (y as f32 + 0.5) / DRIFT_TILES,
    );
    let (cx, cy) = (fx.floor(), fy.floor());
    let (tx, ty) = (smoothstep(0.0, 1.0, fx - cx), smoothstep(0.0, 1.0, fy - cy));
    let (cx, cy) = (cx as i32, cy as i32);
    let v = |dx: i32, dy: i32| unit(hash(seed, cx + dx, cy + dy, salt));
    let top = v(0, 0) + (v(1, 0) - v(0, 0)) * tx;
    let bottom = v(0, 1) + (v(1, 1) - v(0, 1)) * tx;
    top + (bottom - top) * ty
}

/// What grows on tile `(x, y)`, if anything: a
/// [`crate::sprites::DECOR_KINDS`] index and a
/// number to pick its variant by.
fn choose(map: &TileMap, seed: u64, x: i32, y: i32) -> Option<(usize, u64)> {
    let t = map.terrain(x, y);
    let r = unit(hash(seed, x, y, 0));
    let pick = hash(seed, x, y, 3);
    let shore = matches!(t, Terrain::Grass | Terrain::Sand)
        && (-1..=1).any(|dy| {
            (-1..=1).any(|dx| {
                map.in_bounds(x + dx, y + dy)
                    && map.terrain(x + dx, y + dy) == Terrain::ShallowWater
            })
        });
    if shore && r < 0.3 {
        return Some((REEDS, pick));
    }
    match t {
        Terrain::Grass => {
            let flowers = 0.5 * smoothstep(0.55, 0.85, drift(seed, x, y, 7));
            if r < flowers {
                // A drift is one or two colours, not confetti: the variant
                // comes from the drift's cell, now and then its neighbour's.
                let cell = (
                    (x as f32 / DRIFT_TILES).floor() as i32,
                    (y as f32 / DRIFT_TILES).floor() as i32,
                );
                let other = pick.is_multiple_of(4);
                return Some((FLOWERS, hash(seed, cell.0, cell.1, 11 + other as u64)));
            }
            // Tufts in patches of their own, a few between.
            let tufts = 0.04 + 0.14 * smoothstep(0.4, 0.8, drift(seed, x, y, 13));
            let r = r - flowers;
            if r < tufts {
                Some((TUFT, pick))
            } else if r < tufts + 0.02 {
                Some((ROCK, pick))
            } else if r < tufts + 0.04 {
                Some((BUSH, pick))
            } else {
                None
            }
        }
        Terrain::Dirt | Terrain::Desert => {
            if r < 0.05 {
                Some((ROCK, pick))
            } else if r < 0.09 {
                Some((TUFT, pick))
            } else {
                None
            }
        }
        Terrain::Sand => (r < 0.025).then_some((ROCK, pick)),
        Terrain::ForestFloor => {
            if r < 0.04 {
                Some((TUFT, pick))
            } else if r < 0.06 {
                Some((BUSH, pick))
            } else {
                None
            }
        }
        Terrain::Snow => (r < 0.015).then_some((ROCK, pick)),
        Terrain::ShallowWater | Terrain::DeepWater => None,
    }
}

/// Which of a kind's variants grows on ground of type `t`: the dry tuft
/// and the small rocks off the grass, any of them on it.
fn variant(kind: usize, t: Terrain, have: &[KindId], pick: u64) -> Option<KindId> {
    let lush = matches!(t, Terrain::Grass | Terrain::ForestFloor);
    let fits: Vec<KindId> = have
        .iter()
        .copied()
        .filter(|&id| {
            let n = id - decor_id(kind, 0);
            match kind {
                TUFT if !lush => n == 2,
                TUFT => n != 2,
                ROCK if !lush => n < 2,
                _ => true,
            }
        })
        .collect();
    let pool = if fits.is_empty() { have } else { &fits };
    (!pool.is_empty()).then(|| pool[(pick % pool.len() as u64) as usize])
}

impl Scenery {
    /// Scatters scenery over `map` from `seed`, with whatever scenery sets
    /// the atlas holds: none, with the new art off, and then nothing is
    /// ever drawn.
    pub fn new(map: &TileMap, seed: u64, atlas: &Atlas) -> Scenery {
        let have = variants(atlas);
        let (width, height) = (map.width().max(0), map.height().max(0));
        let mut items = Vec::new();
        if have.iter().any(|v| !v.is_empty()) {
            for y in 0..height {
                for x in 0..width {
                    let Some((kind, pick)) = choose(map, seed, x, y) else {
                        continue;
                    };
                    let Some(id) = variant(kind, map.terrain(x, y), &have[kind], pick) else {
                        continue;
                    };
                    let Some((frame, _)) = atlas.frame(id, 0) else {
                        continue;
                    };
                    // Somewhere in the middle of the tile, on the ground
                    // however it slopes.
                    let wx = x as f32 + 0.2 + 0.6 * unit(hash(seed, x, y, 1));
                    let wy = y as f32 + 0.2 + 0.6 * unit(hash(seed, x, y, 2));
                    let (gx, gy) = iso::project(wx, wy, iso::ground_height(map, wx, wy));
                    let sprite = overlay(frame, gx, gy, 0, wx + wy - DEPTH_BIAS, u32::MAX);
                    items.push(Item {
                        tile: (x, y),
                        sprite,
                    });
                }
            }
        }
        items.sort_by(|a, b| a.sprite.depth.total_cmp(&b.sprite.depth));
        Scenery {
            items,
            width,
            height,
            cover: vec![0; (width * height) as usize],
            stamp: 0,
        }
    }

    /// How many things are scattered.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether nothing is.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Marks a tile as stood on this frame.
    fn cover(&mut self, x: i32, y: i32) {
        if x >= 0 && y >= 0 && x < self.width && y < self.height {
            self.cover[(y * self.width + x) as usize] = self.stamp;
        }
    }

    /// The sprites to draw this frame, in depth order: what is scattered
    /// over the world-screen rectangle `view` (everything, with `None`),
    /// less what stands under a building, a site, rubble or a node, in the
    /// light of `viewer`'s fog (`None`: all of it, lit).
    pub fn sprites(
        &mut self,
        sim: &Simulation,
        viewer: Option<u8>,
        view: Option<(f32, f32, f32, f32)>,
    ) -> Vec<SpriteInstance> {
        if self.items.is_empty() {
            return Vec::new();
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.cover.fill(0);
            self.stamp = 1;
        }
        let fog = viewer.and_then(|p| sim.fog(p));
        let world = sim.world();
        // What stands: the world as it is where the viewer sees it (and
        // the viewer's own everywhere), as remembered where not.
        for slot in world.slots() {
            let i = slot.index();
            let info = kinds::info(world.kind[i]);
            if info.mobile || world.inside[i].is_some() {
                continue;
            }
            let fp = info.footprint as i32;
            let (ax, ay) = sim::nav::anchor_tile(world.pos[i], fp);
            let own = viewer == Some(world.owner[i]);
            for (x, y) in sim::nav::footprint_tiles(ax, ay, fp) {
                if own || fog.is_none_or(|f| f.visible(x, y)) {
                    self.cover(x, y);
                }
            }
        }
        if let Some(f) = fog {
            for ((x, y), m) in f.memories() {
                let fp = kinds::info(m.kind).footprint as i32;
                for (tx, ty) in sim::nav::footprint_tiles(x, y, fp) {
                    self.cover(tx, ty);
                }
            }
        }
        let mut out = Vec::new();
        for item in &self.items {
            let s = &item.sprite;
            if let Some((l, t, r, b)) = view {
                if s.x + s.w < l || s.x > r || s.y + s.h < t || s.y > b {
                    continue;
                }
            }
            let (x, y) = item.tile;
            if self.cover[(y * self.width + x) as usize] == self.stamp {
                continue;
            }
            let light = fog.map_or(fog::VISIBLE, |f| fog::tile_light(f.state(x, y)));
            if light == 0 {
                continue;
            }
            let mut s = *s;
            s.light = light;
            out.push(s);
        }
        out
    }

    /// Adds this frame's scenery ([`Scenery::sprites`]) to a scene's world
    /// sprites, keeping them in depth order.
    pub fn decorate(
        &mut self,
        scene: &mut Scene,
        sim: &Simulation,
        viewer: Option<u8>,
        view: Option<(f32, f32, f32, f32)>,
    ) {
        let more = self.sprites(sim, viewer, view);
        if more.is_empty() {
            return;
        }
        scene.sprites.extend(more);
        // Two sorted runs: the sort merges them.
        scene
            .sprites
            .sort_by(|a, b| a.depth.total_cmp(&b.depth).then(a.slot.cmp(&b.slot)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::{MapKind, MapSpec, SimConfig};

    /// The atlas with the committed scenery sets in it.
    fn atlas() -> Atlas {
        let look = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/look");
        let sheets: Vec<_> = crate::sheets::find_sets(&look)
            .into_iter()
            .filter(|p| p.to_string_lossy().contains("decor_"))
            .map(|p| crate::sheets::Sheet::load(&p).unwrap())
            .collect();
        assert!(sheets.len() >= 10, "the scenery sets load");
        Atlas::with_sheets(&sheets)
    }

    fn sim() -> Simulation {
        Simulation::new(
            5,
            SimConfig {
                map: MapSpec {
                    kind: MapKind::Coastal,
                    size: 96,
                    players: 2,
                },
                ..SimConfig::default()
            },
        )
    }

    #[test]
    fn the_scenery_sets_are_filed_under_their_ids() {
        assert_eq!(
            crate::sprites::decor_id_for_set("decor_flowers_3"),
            Some(decor_id(FLOWERS, 3))
        );
        assert_eq!(
            crate::sprites::decor_id_for_set("decor_reeds_1"),
            Some(decor_id(REEDS, 1))
        );
        assert_eq!(crate::sprites::decor_id_for_set("decor_cow_1"), None);
        assert_eq!(crate::sprites::decor_id_for_set("house"), None);
        let atlas = atlas();
        let have = variants(&atlas);
        assert_eq!(have[FLOWERS].len(), 5);
        assert_eq!(have[REEDS].len(), 2);
        let (f, _) = atlas.frame(decor_id(ROCK, 2), 0).unwrap();
        assert_eq!(
            f.page & crate::sprites::COLOUR_PAGE,
            crate::sprites::COLOUR_PAGE
        );
    }

    /// The same map scatters the same scenery, every time; another seed,
    /// other scenery; and with no scenery sets, none at all.
    #[test]
    fn the_scatter_is_deterministic() {
        let (sim, atlas) = (sim(), atlas());
        let a = Scenery::new(sim.map(), sim.seed(), &atlas);
        let b = Scenery::new(sim.map(), sim.seed(), &atlas);
        assert!(!a.is_empty());
        assert_eq!(a.items, b.items);
        let c = Scenery::new(sim.map(), sim.seed() + 1, &atlas);
        assert_ne!(a.items, c.items);
        let none = Scenery::new(sim.map(), sim.seed(), &Atlas::placeholder());
        assert!(none.is_empty());
        let mut a = a;
        let one = a.sprites(&sim, None, None);
        let two = a.sprites(&sim, None, None);
        assert_eq!(one, two, "the same picture every frame");
        assert!(one.iter().all(|s| s.slot == u32::MAX && !s.screen));
        assert!(one.windows(2).all(|w| w[0].depth <= w[1].depth));
    }

    /// About one thing to every three or four tiles of grass, the flowers
    /// gathered in drifts rather than spread evenly, and nothing in the
    /// water.
    #[test]
    fn the_meadow_is_calm_and_the_flowers_drift() {
        let (sim, atlas) = (sim(), atlas());
        let map = sim.map();
        let s = Scenery::new(map, sim.seed(), &atlas);
        let grass = (0..map.height())
            .flat_map(|y| (0..map.width()).map(move |x| (x, y)))
            .filter(|&(x, y)| map.terrain(x, y) == Terrain::Grass)
            .count();
        let on = |t: Terrain| {
            s.items
                .iter()
                .filter(|i| map.terrain(i.tile.0, i.tile.1) == t)
                .count()
        };
        let per = grass as f32 / on(Terrain::Grass) as f32;
        assert!(
            (2.8..=4.8).contains(&per),
            "one thing to every {per:.1} grass tiles"
        );
        assert_eq!(on(Terrain::DeepWater) + on(Terrain::ShallowWater), 0);
        // Flowers in drifts: many a block of meadow six tiles square has
        // none, where flowers spread evenly at the same density would
        // leave almost none bare.
        let flowers: std::collections::HashSet<(i32, i32)> = s
            .items
            .iter()
            .filter(|i| (decor_id(FLOWERS, 0)..decor_id(FLOWERS, 99)).contains(&kind_of(i, &atlas)))
            .map(|i| i.tile)
            .collect();
        let (mut blocks, mut bare) = (0, 0);
        for by in 0..map.height() / 6 {
            for bx in 0..map.width() / 6 {
                let tiles: Vec<(i32, i32)> =
                    (0..36).map(|k| (bx * 6 + k % 6, by * 6 + k / 6)).collect();
                if tiles
                    .iter()
                    .any(|&(x, y)| map.terrain(x, y) != Terrain::Grass)
                {
                    continue;
                }
                blocks += 1;
                if !tiles.iter().any(|t| flowers.contains(t)) {
                    bare += 1;
                }
            }
        }
        let density = flowers.len() as f32 / grass as f32;
        let even = (1.0 - density).powi(36);
        let share = bare as f32 / blocks as f32;
        assert!(blocks > 20, "{blocks} blocks of meadow");
        assert!(
            share > even * 3.0,
            "{share:.2} of meadow blocks without flowers, {even:.3} if even"
        );
        // Reeds only at the water's edge.
        let reeds = s
            .items
            .iter()
            .filter(|i| (decor_id(REEDS, 0)..decor_id(REEDS, 99)).contains(&kind_of(i, &atlas)))
            .count();
        assert!(reeds > 0, "a coast has reeds");
    }

    /// Which kind an item draws, by the atlas rectangle of its sprite.
    fn kind_of(item: &Item, atlas: &Atlas) -> KindId {
        atlas
            .frames()
            .iter()
            .find(|f| f.page == item.sprite.page && f.x == item.sprite.u && f.y == item.sprite.v)
            .map_or(0, |f| f.kind)
    }

    /// Nothing under what stands, and what stands changes it: a building
    /// put down hides the scenery under it.
    #[test]
    fn nothing_grows_under_what_stands() {
        let (mut sim, atlas) = (sim(), atlas());
        let mut s = Scenery::new(sim.map(), sim.seed(), &atlas);
        let world = sim.world();
        let mut stood = std::collections::HashSet::new();
        for slot in world.slots() {
            let i = slot.index();
            let info = kinds::info(world.kind[i]);
            if !info.mobile {
                let fp = info.footprint as i32;
                let (ax, ay) = sim::nav::anchor_tile(world.pos[i], fp);
                stood.extend(sim::nav::footprint_tiles(ax, ay, fp));
            }
        }
        let drawn = s.sprites(&sim, None, None);
        let tiles: Vec<(i32, i32)> = s
            .items
            .iter()
            .filter(|i| drawn.contains(&i.sprite))
            .map(|i| i.tile)
            .collect();
        assert!(!tiles.is_empty());
        assert!(
            tiles.iter().all(|t| !stood.contains(t)),
            "scenery under a tree or a building"
        );
        assert!(
            s.items.iter().any(|i| stood.contains(&i.tile)),
            "some was scattered where things stand, and left out"
        );
        // A house put down on a tile with scenery on it hides it.
        let target = *tiles
            .iter()
            .find(|&&(x, y)| sim.map().terrain(x, y) == Terrain::Grass)
            .expect("scenery on grass");
        let fp = kinds::info(kinds::HOUSE).footprint as i32;
        sim.issue(sim::Command {
            player: 0,
            kind: sim::CommandKind::Spawn {
                kind: kinds::HOUSE,
                pos: sim::nav::building_centre(target.0, target.1, fp),
            },
        });
        for _ in 0..3 {
            sim.step();
        }
        let under = sim::nav::footprint_tiles(target.0, target.1, fp);
        let after = s.sprites(&sim, None, None);
        assert!(after.len() < drawn.len());
        assert!(!s
            .items
            .iter()
            .filter(|i| under.contains(&i.tile))
            .any(|i| after.contains(&i.sprite)));
    }

    /// Never on ground never seen; dimmed on ground seen once.
    #[test]
    fn scenery_respects_the_fog() {
        let (sim, atlas) = (sim(), atlas());
        let mut s = Scenery::new(sim.map(), sim.seed(), &atlas);
        let all = s.sprites(&sim, None, None).len();
        let seen = s.sprites(&sim, Some(0), None);
        let f = sim.fog(0).unwrap();
        assert!(seen.len() < all / 4, "{} of {all} at the start", seen.len());
        for item in &s.items {
            if let Some(d) = seen
                .iter()
                .find(|d| d.x == item.sprite.x && d.y == item.sprite.y)
            {
                let (x, y) = item.tile;
                assert!(f.explored(x, y));
                assert_eq!(d.light, fog::tile_light(f.state(x, y)));
            }
        }
        // Culled to the view.
        let some = s.sprites(&sim, None, Some((0.0, 0.0, 200.0, 200.0)));
        assert!(some.len() < all);
    }

    /// One frame's work on a large map, for the record.
    #[test]
    fn a_frame_of_scenery_is_cheap() {
        let sim = Simulation::new(
            9,
            SimConfig {
                map: MapSpec {
                    kind: MapKind::Inland,
                    size: 256,
                    players: 4,
                },
                ..SimConfig::default()
            },
        );
        let atlas = atlas();
        let t = std::time::Instant::now();
        let mut s = Scenery::new(sim.map(), sim.seed(), &atlas);
        let built = t.elapsed();
        let t = std::time::Instant::now();
        let n = 20;
        for _ in 0..n {
            std::hint::black_box(s.sprites(&sim, Some(0), Some((-1500.0, 2000.0, 1500.0, 3000.0))));
        }
        let frame = t.elapsed() / n;
        eprintln!(
            "scenery on 256x256: {} items scattered in {built:?}, {frame:?} a frame",
            s.len()
        );
    }
}
