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
    /// Hills everywhere and high ground to fight over; the mines are
    /// rich and the forests thin.
    Highland,
    /// Desert round a lake in the middle, its palms the best of the wood.
    Oasis,
    /// Land along a sea that runs down one side of the map.
    Coastal,
    /// Land in the middle of a sea.
    Continental,
    /// Two shores of a river, crossed at three fords.
    Narrows,
    /// Each start on an island of its own in a sea: nobody walks to
    /// anybody (`GD-NAVAL-03`).
    Islands,
}

impl MapKind {
    /// The maps a match can be played on, in the setup screen's order.
    pub const PLAYABLE: [MapKind; 7] = [
        MapKind::Inland,
        MapKind::Highland,
        MapKind::Oasis,
        MapKind::Coastal,
        MapKind::Continental,
        MapKind::Narrows,
        MapKind::Islands,
    ];

    /// Display name.
    pub const fn name(self) -> &'static str {
        match self {
            MapKind::Flat => "Flat",
            MapKind::Inland => "Inland",
            MapKind::Highland => "Highland",
            MapKind::Oasis => "Oasis",
            MapKind::Coastal => "Coastal",
            MapKind::Continental => "Continental",
            MapKind::Narrows => "Narrows",
            MapKind::Islands => "Islands",
        }
    }
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
        kind => {
            const ATTEMPTS: u32 = 12;
            for attempt in 0..ATTEMPTS {
                let mut rng = Rng::new(
                    seed.wrapping_add(attempt as u64)
                        .wrapping_mul(0x9E37_79B9_7F4A_7C15),
                );
                let made = if kind == MapKind::Inland {
                    inland(&mut rng, &spec)
                } else {
                    varied(&mut rng, &spec)
                };
                if let Some(mut g) = made {
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
    // Last, so everything else on the map is as it was before relics.
    place_relics(&mut g);
    Some(Generated {
        tiles: g.tiles,
        spawns: g.spawns,
        starts: g.starts,
        attempts: 0,
    })
}

/// Starts on a ring round `centre`, `ring` hundredths of the map's size
/// out, as [`start_positions`] places them round the middle, each bearing
/// jittered by up to `jitter_degrees` either way; with the bearing of the
/// first.
fn start_ring(
    rng: &mut Rng,
    size: i32,
    players: u8,
    centre: (i32, i32),
    ring: i32,
    jitter_degrees: i32,
) -> (Vec<(i32, i32)>, Angle) {
    let n = players as i32;
    let c = Vec2Fx::from_int(centre.0, centre.1);
    let radius = Fx::from_int(size).mul_div(Fx::from_ratio(ring, 100), Fx::ONE);
    let base = Angle(rng.next_u32() as u16);
    let step = Angle((65536 / n) as u16);
    let margin = 12;
    let starts = (0..n)
        .map(|i| {
            let jitter = Angle::from_degrees(rng.range_i32(-jitter_degrees, jitter_degrees + 1));
            let a = base + Angle(step.0.wrapping_mul(i as u16)) + jitter;
            let p = if n == 1 {
                c
            } else {
                c + Vec2Fx::from_angle(a, radius)
            };
            (
                p.x.round().clamp(margin, size - 1 - margin),
                p.y.round().clamp(margin, size - 1 - margin),
            )
        })
        .collect();
    (starts, base)
}

/// No water this close to a start, in tiles: its kit is all on dry land.
const DRY_AROUND_STARTS: i32 = 14;

/// On Islands, the channel between two starts' islands runs no nearer
/// either start than this: the start's own kit lies within it.
const CHANNEL_CLEAR: i32 = 12;

/// The map types after Inland (`docs/02` §9): Inland's making, with the
/// ground, the water and the scenery changed. Water is in the way until
/// there are ships: nothing crosses it and nothing stands in it.
fn varied(rng: &mut Rng, spec: &MapSpec) -> Option<Generated> {
    let size = spec.size as i32;
    let kind = spec.kind;
    // Coastal's sea runs down one side, chosen by the seed; the starts'
    // ring moves away from it and shrinks to fit the land.
    let side = rng.below(4) as usize;
    let shore = size * 22 / 100;
    let (centre, ring) = match kind {
        MapKind::Coastal => {
            let (dx, dy) = [(1, 0), (-1, 0), (0, 1), (0, -1)][side];
            let push = shore * 2 / 3;
            ((size / 2 + dx * push, size / 2 + dy * push), 25)
        }
        MapKind::Continental => ((size / 2, size / 2), 24),
        // Islands spreads the starts wide and evenly, so a channel of sea
        // runs between every two.
        MapKind::Islands => ((size / 2, size / 2), 40),
        _ => ((size / 2, size / 2), 34),
    };
    let jitter = if kind == MapKind::Islands { 2 } else { 8 };
    let (starts, base) = start_ring(rng, size, spec.players, centre, ring, jitter);
    let mut g = Gen {
        rng,
        size,
        tiles: TileMap::new(spec.size, spec.size),
        occ: vec![Occ::Free; (size * size) as usize],
        spawns: Vec::new(),
        starts,
    };

    let hills = if kind == MapKind::Highland {
        [36, 48, 60]
    } else {
        [50, 64, 78]
    };
    shape_elevation_at(&mut g, hills);
    paint_terrain_as(&mut g, kind == MapKind::Oasis);
    pour(&mut g, kind, side, shore, base);

    // Keep a building zone clear around each start.
    for &(sx, sy) in &g.starts.clone() {
        for dy in -6..=6 {
            for dx in -6..=6 {
                if g.occ(sx + dx, sy + dy) == Occ::Free {
                    g.set_occ(sx + dx, sy + dy, Occ::Reserved);
                }
            }
        }
    }
    for p in 0..spec.players {
        start_kit(&mut g, p);
    }
    if kind == MapKind::Oasis {
        // The palms: six groves round the lake's shore.
        let (cx, cy) = (size / 2, size / 2);
        let r = size * 9 / 100 + 4;
        for k in 0..6 {
            let a = Angle::from_degrees(k * 60) + g.jitter(15);
            let (x, y) = g.offset(cx, cy, a, r);
            let n = g.rng.range_i32(10, 16) as usize;
            g.blob(kinds::TREE, x, y, n);
        }
    }
    let (forests, mines) = match kind {
        MapKind::Highland => (70, 160),
        MapKind::Oasis => (35, 100),
        _ => (100, 100),
    };
    scatter_scenery_as(&mut g, spec.players, forests, mines);

    g.tiles.enforce_max_step();
    debug_assert!(g.tiles.validate().is_ok());

    // Islands keeps every start apart by design; everywhere else the
    // starts must be walkable to one another.
    let islands = kind == MapKind::Islands;
    if !islands && !all_starts_connected(&g) {
        return None;
    }
    if islands && (!every_start_stands(&g) || any_start_walks_to_another(&g)) {
        return None;
    }
    place_relics_on(&mut g, true, islands);
    place_fish(&mut g);
    Some(Generated {
        tiles: g.tiles,
        spawns: g.spawns,
        starts: g.starts,
        attempts: 0,
    })
}

/// The water of a map type: how deep each tile lies, from noise-bent
/// lines. Three tiles in or more is deep, one or two shallow, and the two
/// tiles along its edge a beach of sand. Never within
/// [`DRY_AROUND_STARTS`] of a start. Water lies at the lowest level and is
/// blocked to everything.
fn pour(g: &mut Gen, kind: MapKind, side: usize, shore: i32, base: Angle) {
    let size = g.size;
    let wiggle = Fbm::new(g.rng, size, size, 10);
    let c = size / 2;
    // The river runs through the middle half-way between the first two
    // starts' bearings, so the starts fall either side of it.
    let n = g.starts.len().max(1) as i32;
    let dir = Vec2Fx::from_angle(base + Angle((65536 / (2 * n)) as u16), Fx::ONE);
    let fords = [-size * 30 / 100, 0, size * 30 / 100];
    // An island to each start, as wide as half the gap to the nearest
    // other start allows, and never narrower than its dry ground.
    let gap = g
        .starts
        .iter()
        .enumerate()
        .flat_map(|(a, &p)| g.starts.iter().skip(a + 1).map(move |&q| (p, q)))
        .map(|((ax, ay), (bx, by))| Vec2Fx::from_int(ax - bx, ay - by).length().floor())
        .min()
        .unwrap_or(size);
    let island = (gap / 2 - 3).clamp(DRY_AROUND_STARTS + 2, size * 22 / 100);
    for y in 0..size {
        for x in 0..size {
            // -3 to 3, smoothly over the map.
            let jig = ((i64::from(wiggle.sample(x, y).raw()) * 7) >> 16) as i32 - 3;
            let mut channel = false;
            let (dx, dy) = (x - c, y - c);
            let depth = match kind {
                MapKind::Coastal => {
                    let into = [x, size - 1 - x, y, size - 1 - y][side];
                    shore + jig * 2 - into
                }
                // A round land, the sea at the edges and deepest in the
                // corners.
                MapKind::Continental => {
                    let d = Vec2Fx::from_int(dx, dy).length().floor();
                    d - size * 40 / 100 + jig * 2
                }
                MapKind::Oasis => {
                    let d = Vec2Fx::from_int(dx, dy).length().floor();
                    size * 9 / 100 + jig / 2 - d
                }
                MapKind::Narrows => {
                    let along = (i64::from(dx) * i64::from(dir.x.raw())
                        + i64::from(dy) * i64::from(dir.y.raw()))
                        >> 16;
                    let across = ((i64::from(dx) * i64::from(dir.y.raw())
                        - i64::from(dy) * i64::from(dir.x.raw()))
                        >> 16)
                        .abs();
                    if fords.iter().any(|&f| (along - i64::from(f)).abs() <= 3) {
                        // A ford stays open: no forest grows across it.
                        if across <= 8 && g.occ(x, y) == Occ::Free {
                            g.set_occ(x, y, Occ::Reserved);
                        }
                        -2
                    } else {
                        4 + jig / 2 - across as i32
                    }
                }
                MapKind::Islands => {
                    let mut near: Vec<i32> = g
                        .starts
                        .iter()
                        .map(|&(sx, sy)| Vec2Fx::from_int(x - sx, y - sy).length().floor())
                        .collect();
                    near.sort_unstable();
                    let (d1, d2) = (near[0], near.get(1).copied().unwrap_or(size));
                    // A channel of sea halfway between neighbours, so no
                    // two islands touch however close their starts.
                    if d2 - d1 < 3 && d1 >= CHANNEL_CLEAR {
                        channel = true;
                        3
                    } else {
                        d1 - island + jig * 2
                    }
                }
                _ => -9,
            };
            let dry = if channel {
                CHANNEL_CLEAR
            } else {
                DRY_AROUND_STARTS
            };
            if depth < -1 || g.dist_to_nearest_start(x, y) < dry {
                continue;
            }
            if depth >= 1 {
                let t = if depth >= 3 {
                    Terrain::DeepWater
                } else {
                    Terrain::ShallowWater
                };
                g.tiles.set_terrain(x, y, t);
                g.set_occ(x, y, Occ::Blocked);
                for (cx, cy) in [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)] {
                    g.tiles.set_corner(cx, cy, 0);
                }
            } else {
                g.tiles.set_terrain(x, y, Terrain::Sand);
            }
        }
    }
    g.tiles.enforce_max_step();
}

/// Rolling hills from noise, quantised to levels, flattened around starts.
fn shape_elevation(g: &mut Gen) {
    shape_elevation_at(g, [50, 64, 78]);
}

/// [`shape_elevation`] with the noise levels, in hundredths, at which the
/// ground steps up: lower makes hillier.
fn shape_elevation_at(g: &mut Gen, levels: [i32; 3]) {
    let size = g.size;
    let noise = Fbm::new(g.rng, size + 1, size + 1, 18);
    let thresholds = levels.map(|l| Fx::from_ratio(l, 100));
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
    paint_terrain_as(g, false);
}

/// [`paint_terrain`]; `arid`, desert with patches of dirt and the odd
/// green.
fn paint_terrain_as(g: &mut Gen, arid: bool) {
    let size = g.size;
    let noise = Fbm::new(g.rng, size, size, 12);
    let dirt = Fx::from_ratio(66, 100);
    let desert = Fx::from_ratio(82, 100);
    for y in 0..size {
        for x in 0..size {
            let v = noise.sample(x, y);
            let t = match (arid, v >= desert, v >= dirt) {
                (false, true, _) => Terrain::Desert,
                (false, false, true) => Terrain::Dirt,
                (false, false, false) => Terrain::Grass,
                (true, true, _) => Terrain::Grass,
                (true, false, true) => Terrain::Dirt,
                (true, false, false) => Terrain::Desert,
            };
            g.tiles.set_terrain(x, y, t);
        }
    }
}

/// The guaranteed kit every player gets: a Town Center, three villagers, a
/// scout, berries, gold, stone, a forest and a herd — the same for everyone,
/// which is how the map is balanced. The herd is four gazelles seven tiles
/// out, close enough to hunt from the first minute (`GD-ECON-06`).
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
    let (hx, hy) = g.offset(sx, sy, herd, 7);
    // A tight group, searched for widely, so every start gets all four.
    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        if let Some((ux, uy)) = g.nearest_free(hx + dx, hy + dy, 5) {
            g.place_unit(kinds::GAZELLE, GAIA, ux, uy);
        }
    }
}

/// Forests, extra mines, bushes and herds away from the starts.
fn scatter_scenery(g: &mut Gen, players: u8) {
    scatter_scenery_as(g, players, 100, 100);
}

/// [`scatter_scenery`] with the forests and the mines away from the starts
/// scaled, in percent.
fn scatter_scenery_as(g: &mut Gen, players: u8, forests_pct: i32, mines_pct: i32) {
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

    let forests = ((area / 550).max(4) * forests_pct / 100).max(1);
    for _ in 0..forests {
        if let Some((x, y)) = far(g, 12) {
            let n = g.rng.range_i32(18, 52) as usize;
            g.blob(kinds::TREE, x, y, n);
        }
    }
    for _ in 0..(players as i32 * 2 * mines_pct / 100) {
        if let Some((x, y)) = far(g, 14) {
            let n = g.rng.range_i32(4, 7) as usize;
            g.blob(kinds::GOLD_MINE, x, y, n);
        }
    }
    for _ in 0..(players as i32 * mines_pct / 100) {
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

/// The relics (`GD-WIN-03`): [`crate::relics::RELICS_PER_MAP`] of them in
/// the open ground between the starts, apart from one another, each on a
/// tile the first start's people can walk to and with open ground all
/// round it, so none walls anything in. Fewer if the map has no room.
fn place_relics(g: &mut Gen) {
    place_relics_within(g, false);
}

/// [`place_relics`]; with `relax`, a map with too little open ground for
/// all of them at the full distances tries again closer to the starts and
/// to each other, down to half.
fn place_relics_within(g: &mut Gen, relax: bool) {
    place_relics_on(g, relax, false);
}

/// Relics where the first start can walk, or with `any_start` where any
/// start can: on Islands every island has its share of the open ground.
fn place_relics_on(g: &mut Gen, relax: bool, any_start: bool) {
    let size = g.size;
    let reach = if any_start {
        flood_from(g, &g.starts.clone())
    } else {
        flood(g)
    };
    let open = |g: &Gen, x: i32, y: i32| {
        g.free(x, y) && g.tiles.walkable(x, y) && reach.get(g.idx(x, y)) == Some(&true)
    };
    let mut placed: Vec<(i32, i32)> = Vec::new();
    let passes: &[i32] = if relax { &[4, 3, 2] } else { &[4] };
    for &quarters in passes {
        let far = ((size / 6).max(14) * quarters / 4).max(12);
        let apart = ((size / 8).max(10) * quarters / 4).max(8);
        let wanted = crate::relics::RELICS_PER_MAP - placed.len();
        for _ in 0..wanted {
            let mut put = false;
            for _ in 0..200 {
                let x = g.rng.range_i32(3, size - 3);
                let y = g.rng.range_i32(3, size - 3);
                let clear = (-1..=1).all(|dy| (-1..=1).all(|dx| open(g, x + dx, y + dy)));
                if clear
                    && g.dist_to_nearest_start(x, y) >= far
                    && placed
                        .iter()
                        .all(|&(px, py)| (px - x).abs().max((py - y).abs()) >= apart)
                {
                    g.place(kinds::RELIC, GAIA, x, y);
                    placed.push((x, y));
                    put = true;
                    break;
                }
            }
            // No room at these distances: the next pass tries closer.
            if !put && relax {
                break;
            }
        }
    }
}

/// Fish in the water (`docs/02` §3.2): one for each seventy tiles of
/// water, up to six a player, each in open water a tile or more from the
/// shore and within six of it, where a Dock's boats can reach them, and
/// four tiles or more from the next. Placed last, so the rest of the map
/// is what it was before there were fish.
fn place_fish(g: &mut Gen) {
    let size = g.size;
    let wet = |g: &Gen, x: i32, y: i32| g.in_bounds(x, y) && g.tiles.terrain(x, y).is_water();
    let water = (0..size)
        .flat_map(|y| (0..size).map(move |x| (x, y)))
        .filter(|&(x, y)| wet(g, x, y))
        .count() as i32;
    let wanted = (water / 70).min(6 * g.starts.len() as i32);
    let mut placed: Vec<(i32, i32)> = Vec::new();
    for _ in 0..wanted * 40 {
        if placed.len() as i32 >= wanted {
            break;
        }
        let x = g.rng.range_i32(1, size - 1);
        let y = g.rng.range_i32(1, size - 1);
        let open = (-1..=1).all(|dy| (-1..=1).all(|dx| wet(g, x + dx, y + dy)));
        let shore = (-6..=6).any(|dy| (-6..=6).any(|dx: i32| !wet(g, x + dx, y + dy)));
        if open
            && shore
            && placed
                .iter()
                .all(|&(px, py)| (px - x).abs().max((py - y).abs()) >= 4)
        {
            g.place(kinds::FISH, GAIA, x, y);
            placed.push((x, y));
        }
    }
}

/// Breadth-first flood over walkable, unblocked tiles from the first start;
/// every other start must be reached. Starts are on reserved tiles next to
/// their Town Center footprint, so they are themselves walkable.
fn all_starts_connected(g: &Gen) -> bool {
    let seen = flood(g);
    !seen.is_empty() && g.starts.iter().all(|&(x, y)| seen[g.idx(x, y + 2)])
}

/// Every tile reachable on foot from the first start's villagers' row;
/// empty if that row is itself blocked.
fn flood(g: &Gen) -> Vec<bool> {
    flood_from(g, &g.starts[..1])
}

/// Whether any start can walk to another: on Islands, a failed map.
fn any_start_walks_to_another(g: &Gen) -> bool {
    g.starts.iter().enumerate().any(|(a, &start)| {
        let reach = flood_from(g, &[start]);
        g.starts
            .iter()
            .enumerate()
            .any(|(b, &(x, y))| a != b && reach.get(g.idx(x, y + 2)) == Some(&true))
    })
}

/// Every start's villagers' row stands on open land.
fn every_start_stands(g: &Gen) -> bool {
    g.starts.iter().all(|&(sx, sy)| {
        g.in_bounds(sx, sy + 2) && g.placeable(sx, sy + 2) && g.tiles.walkable(sx, sy + 2)
    })
}

/// Every tile reachable on foot from any of `starts`' villagers' rows;
/// empty if the first row is itself blocked.
fn flood_from(g: &Gen, starts: &[(i32, i32)]) -> Vec<bool> {
    let size = g.size;
    let mut seen = vec![false; (size * size) as usize];
    let mut queue = VecDeque::new();
    let walkable =
        |x: i32, y: i32| g.in_bounds(x, y) && g.placeable(x, y) && g.tiles.walkable(x, y);

    for (k, &(sx, sy)) in starts.iter().enumerate() {
        // The TC blocks its own tile; begin from the villagers' row.
        let origin = (sx, sy + 2);
        if !walkable(origin.0, origin.1) {
            if k == 0 {
                return Vec::new();
            }
            continue;
        }
        if !seen[g.idx(origin.0, origin.1)] {
            seen[g.idx(origin.0, origin.1)] = true;
            queue.push_back(origin);
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in DIRS4 {
            let (nx, ny) = (x + dx, y + dy);
            if walkable(nx, ny) && !seen[g.idx(nx, ny)] {
                seen[g.idx(nx, ny)] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    seen
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

    /// Every map type gives every start the same kit on dry, reachable
    /// ground: water is never by a start, every start reaches every other
    /// on foot, and the relics are out.
    ///
    /// REQ: GD-MAP-01
    #[test]
    fn every_map_type_gives_every_start_its_kit_on_dry_land() {
        for kind in MapKind::PLAYABLE {
            for seed in 0..6u64 {
                for (players, size) in [(2u8, 96u16), (4, 128), (8, 168)] {
                    let g = generate(
                        seed,
                        &MapSpec {
                            kind,
                            size,
                            players,
                        },
                    );
                    let at = format!("{kind:?} seed {seed} players {players}");
                    assert!(g.attempts <= 6, "{at}: {} attempts", g.attempts);
                    assert!(g.tiles.validate().is_ok(), "{at}");
                    assert_eq!(g.starts.len(), players as usize, "{at}");
                    for (p, &start) in g.starts.iter().enumerate() {
                        let owned = |k: KindId| {
                            g.spawns
                                .iter()
                                .filter(|s| s.kind == k && s.owner == p as PlayerId)
                                .count()
                        };
                        assert_eq!(owned(kinds::TOWN_CENTER), 1, "{at}");
                        assert_eq!(owned(kinds::VILLAGER), 3, "{at}");
                        assert!(count_near(&g, kinds::BERRY_BUSH, start, 12) >= 6, "{at}");
                        assert!(count_near(&g, kinds::GOLD_MINE, start, 12) >= 5, "{at}");
                        assert!(count_near(&g, kinds::STONE_MINE, start, 12) >= 4, "{at}");
                        assert!(count_near(&g, kinds::TREE, start, 16) >= 20, "{at} trees");
                        // Islands' channels come nearer, past the kit.
                        let dry = if kind == MapKind::Islands {
                            CHANNEL_CLEAR
                        } else {
                            DRY_AROUND_STARTS
                        };
                        for dy in -dry + 1..dry {
                            for dx in -dry + 1..dry {
                                let (x, y) = (start.0 + dx, start.1 + dy);
                                if g.tiles.in_bounds(x, y) {
                                    assert!(
                                        !g.tiles.terrain(x, y).is_water(),
                                        "{at}: water at {dx},{dy} from start {p}"
                                    );
                                }
                            }
                        }
                    }
                    let relics = g.spawns.iter().filter(|s| s.kind == kinds::RELIC).count();
                    assert!(relics >= 3, "{at}: {relics} relics");
                    let water = g.tiles.terrain_histogram()[Terrain::DeepWater as usize]
                        + g.tiles.terrain_histogram()[Terrain::ShallowWater as usize];
                    let wet = matches!(
                        kind,
                        MapKind::Oasis
                            | MapKind::Coastal
                            | MapKind::Continental
                            | MapKind::Narrows
                            | MapKind::Islands
                    );
                    assert_eq!(water > 0, wet, "{at}: {water} water tiles");
                }
            }
        }
    }

    /// On Islands nobody walks to anybody: every start is on land of its
    /// own, whatever the number of players, and the sea between is one.
    ///
    /// REQ: GD-NAVAL-03
    #[test]
    fn every_island_is_its_own() {
        for seed in 0..8u64 {
            for (players, size) in [(2u8, 96u16), (3, 96), (4, 128), (6, 128), (8, 96), (8, 168)] {
                let spec = MapSpec {
                    kind: MapKind::Islands,
                    size,
                    players,
                };
                let g = generate(seed, &spec);
                let at = format!("seed {seed} players {players} size {size}");
                assert!(g.attempts <= 6, "{at}: {} attempts", g.attempts);
                let mut rng = Rng::new(0);
                let gen = Gen {
                    rng: &mut rng,
                    size: size as i32,
                    tiles: g.tiles.clone(),
                    occ: vec![Occ::Free; size as usize * size as usize],
                    spawns: Vec::new(),
                    starts: g.starts.clone(),
                };
                for (a, &start) in g.starts.iter().enumerate() {
                    let reach = flood_from(&gen, &[start]);
                    assert!(!reach.is_empty(), "{at}: start {a} on land");
                    for (b, &(x, y)) in g.starts.iter().enumerate() {
                        if a != b {
                            assert!(!reach[gen.idx(x, y + 2)], "{at}: {a} walks to {b}");
                        }
                    }
                }
                // Every island's shore is on the one sea round the map's
                // edge (a pond inland is no way off it).
                let water = crate::nav::NavGrid::water_from_map(&g.tiles);
                // The sea is the largest body of water.
                let mut bodies: std::collections::BTreeMap<u16, (usize, (i32, i32))> =
                    std::collections::BTreeMap::new();
                for y in 0..size as i32 {
                    for x in 0..size as i32 {
                        let c = water.component(x, y);
                        if c != 0 {
                            bodies.entry(c).or_insert((0, (x, y))).0 += 1;
                        }
                    }
                }
                let sea = bodies.values().max().expect("water").1;
                for (a, &start) in g.starts.iter().enumerate() {
                    let reach = flood_from(&gen, &[start]);
                    let shore = (0..size as i32)
                        .flat_map(|y| (0..size as i32).map(move |x| (x, y)))
                        .filter(|&(x, y)| reach[gen.idx(x, y)])
                        .any(|(x, y)| {
                            crate::nav::ORTHO_STEPS
                                .iter()
                                .any(|&(dx, dy)| water.connected(sea, (x + dx, y + dy)))
                        });
                    assert!(shore, "{at}: island {a} on the sea");
                }
            }
        }
    }

    /// The river of the Narrows has starts on both its shores: from some
    /// two starts the nearest water lies in opposite directions.
    #[test]
    fn the_narrows_river_has_starts_on_both_shores() {
        for seed in 0..8u64 {
            for players in [2u8, 4, 6] {
                let g = generate(
                    seed,
                    &MapSpec {
                        kind: MapKind::Narrows,
                        size: 128,
                        players,
                    },
                );
                let toward_water = |(sx, sy): (i32, i32)| {
                    let mut best = (i32::MAX, (0, 0));
                    for y in 0..128 {
                        for x in 0..128 {
                            if g.tiles.terrain(x, y).is_water() {
                                let d = (x - sx) * (x - sx) + (y - sy) * (y - sy);
                                if d < best.0 {
                                    best = (d, (x - sx, y - sy));
                                }
                            }
                        }
                    }
                    best.1
                };
                let v: Vec<(i32, i32)> = g.starts.iter().map(|&s| toward_water(s)).collect();
                let split = v
                    .iter()
                    .enumerate()
                    .any(|(i, a)| v[i + 1..].iter().any(|b| a.0 * b.0 + a.1 * b.1 < 0));
                assert!(split, "seed {seed} players {players}: all on one shore");
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
