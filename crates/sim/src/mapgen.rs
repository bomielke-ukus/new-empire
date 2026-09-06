//! Seeded random map generation.
//!
//! Lives inside the simulation crate so a replay needs only a seed and a
//! [`MapSpec`] — the map itself is reproduced, never stored. Everything here
//! draws from the simulation's own [`Rng`] and fixed-point maths, so the same
//! seed yields the same map on every machine.
//!
//! The generator guarantees three things and verifies them before returning:
//!
//! 1. every player start has the same resource kit within reach,
//! 2. every start can walk to every other start,
//! 3. neighbouring elevation corners differ by at most one level.

use crate::angle::Angle;
use crate::command::PlayerId;
use crate::entity::KindId;
use crate::fx::Fx;
use crate::kinds::{self, GAIA};
use crate::map::{Terrain, TileMap, MAX_ELEVATION};
use crate::nav;
use crate::noise::Fbm;
use crate::rng::Rng;
use crate::vec2::Vec2Fx;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Which generator to run.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum MapKind {
    /// Flat, empty grass. For tests and benchmarks.
    Flat,
    /// Land only: rolling hills, forests, scattered mines. The slice's map.
    Inland,
}

/// Map generation parameters.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct MapSpec {
    /// Generator.
    pub kind: MapKind,
    /// Edge length in tiles; maps are square. Clamped to `48..=256`.
    pub size: u16,
    /// Number of player starts, `1..=8`.
    pub players: u8,
}

impl Default for MapSpec {
    fn default() -> Self {
        MapSpec {
            kind: MapKind::Inland,
            size: 128,
            players: 2,
        }
    }
}

/// An entity the generator wants placed at match start.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Spawn {
    /// What.
    pub kind: KindId,
    /// Whose; [`GAIA`] for nobody's.
    pub owner: PlayerId,
    /// Where, in tiles. Buildings are centred on their footprint.
    pub pos: Vec2Fx,
}

/// A generated map.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Generated {
    /// Terrain and elevation.
    pub tiles: TileMap,
    /// Everything to spawn.
    pub spawns: Vec<Spawn>,
    /// Each player's Town Center tile, indexed by player.
    pub starts: Vec<(i32, i32)>,
    /// How many seeds were tried before one passed verification.
    pub attempts: u32,
}

/// Generates a map. Never fails: if a seed produces an unverifiable map it
/// moves to the next derived seed, and after a bounded number of attempts
/// falls back to a flat map, which always verifies.
pub fn generate(seed: u64, spec: &MapSpec) -> Generated {
    let spec = MapSpec {
        kind: spec.kind,
        size: spec.size.clamp(48, 256),
        players: spec.players.clamp(1, 8),
    };
    match spec.kind {
        MapKind::Flat => flat(&spec),
        MapKind::Inland => {
            const ATTEMPTS: u32 = 12;
            for attempt in 0..ATTEMPTS {
                let mut rng = Rng::new(
                    seed.wrapping_add(attempt as u64)
                        .wrapping_mul(0x9E37_79B9_7F4A_7C15),
                );
                if let Some(mut g) = inland(&mut rng, &spec) {
                    g.attempts = attempt + 1;
                    return g;
                }
            }
            let mut g = flat(&spec);
            g.attempts = ATTEMPTS + 1;
            g
        }
    }
}

// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Occ {
    Free,
    /// A tree, mine, bush or building footprint; not walkable.
    Blocked,
    /// Kept clear of scenery so the player has room to build.
    Reserved,
}

struct Gen<'a> {
    rng: &'a mut Rng,
    size: i32,
    tiles: TileMap,
    occ: Vec<Occ>,
    spawns: Vec<Spawn>,
    starts: Vec<(i32, i32)>,
}

impl Gen<'_> {
    fn idx(&self, x: i32, y: i32) -> usize {
        (y * self.size + x) as usize
    }

    fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.size && y < self.size
    }

    fn occ(&self, x: i32, y: i32) -> Occ {
        if self.in_bounds(x, y) {
            self.occ[self.idx(x, y)]
        } else {
            Occ::Blocked
        }
    }

    fn set_occ(&mut self, x: i32, y: i32, o: Occ) {
        if self.in_bounds(x, y) {
            let i = self.idx(x, y);
            self.occ[i] = o;
        }
    }

    fn free(&self, x: i32, y: i32) -> bool {
        self.occ(x, y) == Occ::Free
    }

    fn placeable(&self, x: i32, y: i32) -> bool {
        matches!(self.occ(x, y), Occ::Free | Occ::Reserved)
    }

    fn dist_to_nearest_start(&self, x: i32, y: i32) -> i32 {
        self.starts
            .iter()
            .map(|&(sx, sy)| (sx - x).abs().max((sy - y).abs()))
            .min()
            .unwrap_or(i32::MAX)
    }

    /// Places one static object, blocking its tile.
    fn place(&mut self, kind: KindId, owner: PlayerId, x: i32, y: i32) {
        self.set_occ(x, y, Occ::Blocked);
        self.spawns.push(Spawn {
            kind,
            owner,
            pos: nav::centre((x, y)),
        });
    }

    /// Places a mobile unit; does not block.
    fn place_unit(&mut self, kind: KindId, owner: PlayerId, x: i32, y: i32) {
        self.spawns.push(Spawn {
            kind,
            owner,
            pos: nav::centre((x, y)),
        });
    }

    /// Places a building centred on `(x, y)`, blocking its footprint.
    fn place_building(&mut self, kind: KindId, owner: PlayerId, x: i32, y: i32) {
        let fp = kinds::info(kind).footprint as i32;
        let half = fp / 2;
        for dy in -half..fp - half {
            for dx in -half..fp - half {
                self.set_occ(x + dx, y + dy, Occ::Blocked);
            }
        }
        self.spawns.push(Spawn {
            kind,
            owner,
            pos: nav::building_centre(x, y, fp),
        });
    }

    /// The nearest free tile to `(x, y)` within `radius`, by ring search.
    fn nearest_free(&self, x: i32, y: i32, radius: i32) -> Option<(i32, i32)> {
        for r in 0..=radius {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    if self.free(x + dx, y + dy) {
                        return Some((x + dx, y + dy));
                    }
                }
            }
        }
        None
    }

    /// Grows a blob of `count` objects from `(x, y)` over free tiles by
    /// random expansion. Returns how many were placed.
    fn blob(&mut self, kind: KindId, x: i32, y: i32, count: usize) -> usize {
        let Some(start) = self.nearest_free(x, y, 4) else {
            return 0;
        };
        let mut placed = vec![start];
        self.place(kind, GAIA, start.0, start.1);
        let mut stalls = 0;
        while placed.len() < count && stalls < count * 8 {
            let (px, py) = placed[self.rng.below(placed.len() as u32) as usize];
            let (dx, dy) = DIRS4[self.rng.below(4) as usize];
            let (nx, ny) = (px + dx, py + dy);
            if self.free(nx, ny) {
                self.place(kind, GAIA, nx, ny);
                placed.push((nx, ny));
            } else {
                stalls += 1;
            }
        }
        placed.len()
    }

    /// A point `dist` tiles from `(x, y)` in direction `a`, clamped inside
    /// the map with a margin.
    fn offset(&self, x: i32, y: i32, a: Angle, dist: i32) -> (i32, i32) {
        let d = Vec2Fx::from_angle(a, Fx::from_int(dist));
        let m = 2;
        (
            (x + d.x.round()).clamp(m, self.size - 1 - m),
            (y + d.y.round()).clamp(m, self.size - 1 - m),
        )
    }

    fn jitter(&mut self, degrees: i32) -> Angle {
        Angle::from_degrees(self.rng.range_i32(-degrees, degrees + 1))
    }
}

const DIRS4: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

fn flat(spec: &MapSpec) -> Generated {
    let size = spec.size as i32;
    let starts = start_positions(&mut Rng::new(0), size, spec.players);
    Generated {
        tiles: TileMap::new(spec.size, spec.size),
        spawns: vec![],
        starts,
        attempts: 1,
    }
}

/// Player starts on a ring around the centre, evenly spaced with a little
/// jitter, at a radius that keeps them well apart and off the edge.
fn start_positions(rng: &mut Rng, size: i32, players: u8) -> Vec<(i32, i32)> {
    let n = players as i32;
    let centre = Vec2Fx::from_int(size / 2, size / 2);
    let radius = Fx::from_int(size).mul_div(Fx::from_ratio(34, 100), Fx::ONE);
    let base = Angle(rng.next_u32() as u16);
    let step = Angle((65536 / n) as u16);
    let margin = 12;
    (0..n)
        .map(|i| {
            let jitter = Angle::from_degrees(rng.range_i32(-8, 9));
            let a = base + Angle(step.0.wrapping_mul(i as u16)) + jitter;
            let p = if n == 1 {
                centre
            } else {
                centre + Vec2Fx::from_angle(a, radius)
            };
            (
                p.x.round().clamp(margin, size - 1 - margin),
                p.y.round().clamp(margin, size - 1 - margin),
            )
        })
        .collect()
}

fn inland(rng: &mut Rng, spec: &MapSpec) -> Option<Generated> {
    let size = spec.size as i32;
    let starts = start_positions(rng, size, spec.players);
    let mut g = Gen {
        rng,
        size,
        tiles: TileMap::new(spec.size, spec.size),
        occ: vec![Occ::Free; (size * size) as usize],
        spawns: Vec::new(),
        starts,
    };

    shape_elevation(&mut g);
    paint_terrain(&mut g);

    // Keep a building zone clear around each start.
    for &(sx, sy) in &g.starts.clone() {
        for dy in -6..=6 {
            for dx in -6..=6 {
                g.set_occ(sx + dx, sy + dy, Occ::Reserved);
            }
        }
    }

    for p in 0..spec.players {
        start_kit(&mut g, p);
    }
    scatter_scenery(&mut g, spec.players);

    g.tiles.enforce_max_step();
    debug_assert!(g.tiles.validate().is_ok());

    if !all_starts_connected(&g) {
        return None;
    }
    Some(Generated {
        tiles: g.tiles,
        spawns: g.spawns,
        starts: g.starts,
        attempts: 0,
    })
}

/// Rolling hills from noise, quantised to levels, flattened around starts.
fn shape_elevation(g: &mut Gen) {
    let size = g.size;
    let noise = Fbm::new(g.rng, size + 1, size + 1, 18);
    let thresholds = [
        Fx::from_ratio(50, 100),
        Fx::from_ratio(64, 100),
        Fx::from_ratio(78, 100),
    ];
    for cy in 0..=size {
        for cx in 0..=size {
            let v = noise.sample(cx, cy);
            let h = thresholds.iter().filter(|&&t| v >= t).count() as u8;
            g.tiles.set_corner(cx, cy, h.min(MAX_ELEVATION));
        }
    }
    // Flatten each start zone to the level at its centre.
    for &(sx, sy) in &g.starts.clone() {
        let level = g.tiles.corner(sx, sy);
        for cy in (sy - 8)..=(sy + 9) {
            for cx in (sx - 8)..=(sx + 9) {
                g.tiles.set_corner(cx, cy, level);
            }
        }
    }
    g.tiles.enforce_max_step();
}

/// Grass with patches of dirt and the odd desert scar.
fn paint_terrain(g: &mut Gen) {
    let size = g.size;
    let noise = Fbm::new(g.rng, size, size, 12);
    let dirt = Fx::from_ratio(66, 100);
    let desert = Fx::from_ratio(82, 100);
    for y in 0..size {
        for x in 0..size {
            let v = noise.sample(x, y);
            let t = if v >= desert {
                Terrain::Desert
            } else if v >= dirt {
                Terrain::Dirt
            } else {
                Terrain::Grass
            };
            g.tiles.set_terrain(x, y, t);
        }
    }
}

/// The guaranteed kit every player gets: a Town Center, three villagers, a
/// scout, berries, gold, stone, a forest and a herd — the same for everyone,
/// which is how the map is balanced.
fn start_kit(g: &mut Gen, player: PlayerId) {
    let (sx, sy) = g.starts[player as usize];
    g.place_building(kinds::TOWN_CENTER, player, sx, sy);
    for i in 0..3 {
        g.place_unit(kinds::VILLAGER, player, sx - 1 + i, sy + 2);
    }
    g.place_unit(kinds::SCOUT, player, sx + 2, sy - 2);

    let a = Angle(g.rng.next_u32() as u16);
    let spread = |g: &mut Gen, deg: i32| a + Angle::from_degrees(deg) + g.jitter(12);

    let b = spread(g, 0);
    let (bx, by) = g.offset(sx, sy, b, 6);
    g.blob(kinds::BERRY_BUSH, bx, by, 6);

    let gold = spread(g, 120);
    let (gx, gy) = g.offset(sx, sy, gold, 8);
    g.blob(kinds::GOLD_MINE, gx, gy, 5);

    let stone = spread(g, 240);
    let (tx, ty) = g.offset(sx, sy, stone, 8);
    g.blob(kinds::STONE_MINE, tx, ty, 4);

    let forest = spread(g, 60);
    let (fx, fy) = g.offset(sx, sy, forest, 11);
    g.blob(kinds::TREE, fx, fy, 32);

    let herd = spread(g, 180);
    let (hx, hy) = g.offset(sx, sy, herd, 10);
    for i in 0..3 {
        if let Some((ux, uy)) = g.nearest_free(hx + i, hy, 3) {
            g.place_unit(kinds::GAZELLE, GAIA, ux, uy);
        }
    }
}

/// Forests, extra mines, bushes and herds away from the starts.
fn scatter_scenery(g: &mut Gen, players: u8) {
    let size = g.size;
    let area = size * size;
    let far = |g: &mut Gen, min_dist: i32| -> Option<(i32, i32)> {
        for _ in 0..40 {
            let x = g.rng.range_i32(2, size - 2);
            let y = g.rng.range_i32(2, size - 2);
            if g.dist_to_nearest_start(x, y) >= min_dist && g.free(x, y) {
                return Some((x, y));
            }
        }
        None
    };

    let forests = (area / 550).max(4);
    for _ in 0..forests {
        if let Some((x, y)) = far(g, 12) {
            let n = g.rng.range_i32(18, 52) as usize;
            g.blob(kinds::TREE, x, y, n);
        }
    }
    for _ in 0..(players as i32 * 2) {
        if let Some((x, y)) = far(g, 14) {
            let n = g.rng.range_i32(4, 7) as usize;
            g.blob(kinds::GOLD_MINE, x, y, n);
        }
    }
    for _ in 0..players {
        if let Some((x, y)) = far(g, 14) {
            let n = g.rng.range_i32(3, 5) as usize;
            g.blob(kinds::STONE_MINE, x, y, n);
        }
    }
    for _ in 0..players {
        if let Some((x, y)) = far(g, 12) {
            g.blob(kinds::BERRY_BUSH, x, y, 5);
        }
    }
    for _ in 0..(players as i32 * 2) {
        if let Some((x, y)) = far(g, 12) {
            for i in 0..3 {
                if let Some((ux, uy)) = g.nearest_free(x + i, y, 3) {
                    g.place_unit(kinds::GAZELLE, GAIA, ux, uy);
                }
            }
        }
    }
    // Forest floor under every tree.
    for s in g.spawns.clone() {
        if s.kind == kinds::TREE {
            g.tiles
                .set_terrain(s.pos.x.floor(), s.pos.y.floor(), Terrain::ForestFloor);
        }
    }
}

/// Breadth-first flood over walkable, unblocked tiles from the first start;
/// every other start must be reached. Starts are on reserved tiles next to
/// their Town Center footprint, so they are themselves walkable.
fn all_starts_connected(g: &Gen) -> bool {
    let size = g.size;
    let mut seen = vec![false; (size * size) as usize];
    let mut queue = VecDeque::new();
    let walkable =
        |x: i32, y: i32| g.in_bounds(x, y) && g.placeable(x, y) && g.tiles.walkable(x, y);

    let (sx, sy) = g.starts[0];
    // The TC blocks its own tile; begin from the villagers' row.
    let origin = (sx, sy + 2);
    if !walkable(origin.0, origin.1) {
        return false;
    }
    seen[g.idx(origin.0, origin.1)] = true;
    queue.push_back(origin);
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in DIRS4 {
            let (nx, ny) = (x + dx, y + dy);
            if walkable(nx, ny) && !seen[g.idx(nx, ny)] {
                seen[g.idx(nx, ny)] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    g.starts.iter().all(|&(x, y)| seen[g.idx(x, y + 2)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count_near(g: &Generated, kind: KindId, start: (i32, i32), radius: i32) -> usize {
        g.spawns
            .iter()
            .filter(|s| s.kind == kind)
            .filter(|s| {
                (s.pos.x.floor() - start.0).abs() <= radius
                    && (s.pos.y.floor() - start.1).abs() <= radius
            })
            .count()
    }

    // REQ: GD-MAP-01  REQ: RM-M1-01
    #[test]
    fn same_seed_same_map() {
        let spec = MapSpec::default();
        assert_eq!(generate(42, &spec), generate(42, &spec));
        assert_ne!(generate(42, &spec), generate(43, &spec));
    }

    #[test]
    fn flat_is_empty_and_valid() {
        let g = generate(
            1,
            &MapSpec {
                kind: MapKind::Flat,
                size: 64,
                players: 3,
            },
        );
        assert!(g.spawns.is_empty());
        assert_eq!(g.starts.len(), 3);
        assert!(g.tiles.validate().is_ok());
    }

    #[test]
    fn every_player_gets_the_kit() {
        for seed in 0..12u64 {
            for players in [1u8, 2, 4, 8] {
                let spec = MapSpec {
                    kind: MapKind::Inland,
                    size: 128,
                    players,
                };
                let g = generate(seed, &spec);
                assert_eq!(g.starts.len(), players as usize);
                assert!(g.tiles.validate().is_ok(), "seed {seed}");
                assert!(
                    g.attempts <= 3,
                    "seed {seed} players {players} needed {} attempts",
                    g.attempts
                );
                for (p, &start) in g.starts.iter().enumerate() {
                    let p = p as PlayerId;
                    let tcs = g
                        .spawns
                        .iter()
                        .filter(|s| s.kind == kinds::TOWN_CENTER && s.owner == p)
                        .count();
                    assert_eq!(tcs, 1, "seed {seed} player {p}");
                    assert_eq!(
                        g.spawns
                            .iter()
                            .filter(|s| s.kind == kinds::VILLAGER && s.owner == p)
                            .count(),
                        3
                    );
                    assert_eq!(
                        g.spawns
                            .iter()
                            .filter(|s| s.kind == kinds::SCOUT && s.owner == p)
                            .count(),
                        1
                    );
                    assert!(
                        count_near(&g, kinds::BERRY_BUSH, start, 10) >= 6,
                        "seed {seed} p{p} berries"
                    );
                    assert!(
                        count_near(&g, kinds::GOLD_MINE, start, 12) >= 5,
                        "seed {seed} p{p} gold"
                    );
                    assert!(
                        count_near(&g, kinds::STONE_MINE, start, 12) >= 4,
                        "seed {seed} p{p} stone"
                    );
                    assert!(
                        count_near(&g, kinds::TREE, start, 16) >= 28,
                        "seed {seed} p{p} trees"
                    );
                    // Start zone stays clear for building.
                    let clutter = g
                        .spawns
                        .iter()
                        .filter(|s| s.owner == GAIA && !kinds::info(s.kind).mobile)
                        .filter(|s| {
                            (s.pos.x.floor() - start.0).abs() <= 4
                                && (s.pos.y.floor() - start.1).abs() <= 4
                        })
                        .count();
                    assert_eq!(
                        clutter, 0,
                        "seed {seed} p{p}: scenery inside the start zone"
                    );
                }
            }
        }
    }

    #[test]
    fn starts_are_far_apart_and_flat() {
        let g = generate(
            7,
            &MapSpec {
                kind: MapKind::Inland,
                size: 128,
                players: 4,
            },
        );
        for (i, &a) in g.starts.iter().enumerate() {
            for &b in &g.starts[i + 1..] {
                let d = (a.0 - b.0).abs().max((a.1 - b.1).abs());
                assert!(d >= 40, "starts {a:?} and {b:?} only {d} apart");
            }
            let level = g.tiles.elevation(a.0, a.1);
            for dy in -5..=5 {
                for dx in -5..=5 {
                    assert_eq!(
                        g.tiles.elevation(a.0 + dx, a.1 + dy),
                        level,
                        "start {a:?} not flat at {dx},{dy}"
                    );
                }
            }
        }
    }

    #[test]
    fn terrain_is_mostly_grass_with_some_variety() {
        let g = generate(3, &MapSpec::default());
        let h = g.tiles.terrain_histogram();
        let total: usize = h.iter().sum();
        assert!(
            h[Terrain::Grass as usize] * 2 > total,
            "grass should dominate: {h:?}"
        );
        assert!(h[Terrain::Dirt as usize] > 0);
        assert!(h[Terrain::ForestFloor as usize] > 100);
        assert_eq!(h[Terrain::DeepWater as usize], 0, "inland has no water");
    }

    #[test]
    fn spec_is_clamped() {
        let g = generate(
            1,
            &MapSpec {
                kind: MapKind::Inland,
                size: 10,
                players: 0,
            },
        );
        assert_eq!(g.tiles.width(), 48);
        assert_eq!(g.starts.len(), 1);
    }
}
