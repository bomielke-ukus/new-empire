//! Sector graph and flow fields: the group-movement layer of `docs/04` §5.
//!
//! A path request used to be one A\* search per unit. Forty units sent to
//! one place ran forty searches over the same ground, and the per-tick node
//! budget then rationed them across ticks. Here the work is per
//! *destination* instead:
//!
//! 1. The map is cut into 16×16-tile **sectors**. Each sector knows its
//!    **portals** — the open runs along each shared edge — and the walking
//!    cost between its own portals. A search over portals is a few dozen
//!    nodes where a search over tiles is thousands, and it answers "which
//!    sectors does the route cross?" — the *corridor*.
//! 2. A **flow field** is an integration of walking cost from the
//!    destination outward over the corridor's tiles. Every unit heading for
//!    that destination reads the same field: at its tile, the neighbour
//!    with the lowest cost is the way to go. The field is built once per
//!    destination and extended when a unit needs it from a sector it does
//!    not cover yet.
//! 3. Steering along the field is the unit's own business (in
//!    `Simulation`): it looks a few tiles ahead along the field and walks
//!    straight to the furthest one it can see, so the result is a glide
//!    rather than a staircase.
//!
//! Everything here is derived from the [`NavGrid`] and is rebuilt on demand
//! when the tiles it was built from change. None of it is simulation state:
//! two machines with different caches make the same decisions, because a
//! field is a pure function of the grid and the destination.

use crate::nav::{self, NavGrid, Tile};
use crate::vec2::Vec2Fx;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

/// Side of a sector, in tiles.
pub const SECTOR: i32 = 16;

/// Orthogonal step cost, matching `nav`'s A\*.
const STRAIGHT: u32 = 10;
/// Diagonal step cost.
const DIAGONAL: u32 = 14;
const NEIGHBOURS: [(i32, i32, u32); 8] = [
    (1, 0, STRAIGHT),
    (-1, 0, STRAIGHT),
    (0, 1, STRAIGHT),
    (0, -1, STRAIGHT),
    (1, 1, DIAGONAL),
    (1, -1, DIAGONAL),
    (-1, 1, DIAGONAL),
    (-1, -1, DIAGONAL),
];

/// What a field leads to: a tile, or the ring around a footprint anchored
/// on a tile (a building or a resource node, which units cannot enter).
/// `(x, y, footprint)`, footprint 0 meaning the tile itself.
pub type FieldKey = (i32, i32, u8);

/// Tile mask bits for a field's box: passable at all, and passable inside a
/// covered sector (so the flood may enter it).
const PASSABLE: u8 = 1;
const COVERED: u8 = 2;

/// The step from a tile to a neighbour, if legal: passable, in bounds, and
/// not cutting a blocked corner.
fn step_ok(grid: &NavGrid, x: i32, y: i32, dx: i32, dy: i32) -> bool {
    let (nx, ny) = (x + dx, y + dy);
    grid.passable(nx, ny)
        && (dx == 0 || dy == 0 || (grid.passable(x + dx, y) && grid.passable(x, y + dy)))
}

/// A portal: one open run along a sector edge, as seen from one side.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Portal {
    /// The middle tile of the run, on this sector's side.
    tile: Tile,
    /// The tile across the edge, in the neighbouring sector.
    twin: Tile,
    /// Walking cost to the other portals of this sector, by portal index,
    /// where a walk exists inside the sector.
    inner: Vec<(usize, u32)>,
    /// Walking cost from this portal to every tile of the sector, indexed
    /// by position within the sector's range; `u32::MAX` where none.
    tiles: Vec<u32>,
}

/// The most portals one sector can have: a 16-tile edge holds at most eight
/// open runs, and a sector has four edges.
const MAX_PORTALS: usize = 32;

/// The sector graph, built lazily sector by sector and rebuilt when a
/// sector's tiles change.
#[derive(Default)]
pub struct Sectors {
    cols: i32,
    rows: i32,
    /// The grid generation each sector's portals were built at; `None`
    /// means never.
    built: Vec<Option<u32>>,
    portals: Vec<Vec<Portal>>,
    /// Scratch for intra-sector searches.
    dist: Vec<u32>,
    queue: Buckets,
    /// Scratch for the portal search, indexed by `sector * MAX_PORTALS +
    /// portal`, valid where `stamp` equals `epoch`.
    epoch: u32,
    stamp: Vec<u32>,
    best: Vec<u32>,
    parent: Vec<u32>,
}

impl Sectors {
    /// Sector columns for a grid.
    pub fn cols_for(width: i32) -> i32 {
        (width + SECTOR - 1) / SECTOR
    }

    fn fit(&mut self, grid: &NavGrid) {
        let (cols, rows) = (Self::cols_for(grid.width()), Self::cols_for(grid.height()));
        if (self.cols, self.rows) != (cols, rows) {
            self.cols = cols;
            self.rows = rows;
            let n = (cols * rows) as usize;
            self.built = vec![None; n];
            self.portals = vec![Vec::new(); n];
            self.stamp = vec![0; n * MAX_PORTALS];
            self.best = vec![u32::MAX; n * MAX_PORTALS];
            self.parent = vec![u32::MAX; n * MAX_PORTALS];
            self.epoch = 0;
        }
    }

    /// How many sectors the grid has.
    pub fn count(&self) -> usize {
        (self.cols * self.rows) as usize
    }

    /// The sector containing a tile.
    #[inline]
    pub fn of(&self, t: Tile) -> usize {
        ((t.1 / SECTOR) * self.cols + t.0 / SECTOR) as usize
    }

    /// Tile range of a sector: `(x0, y0, x1, y1)`, end exclusive.
    fn range(&self, grid: &NavGrid, s: usize) -> (i32, i32, i32, i32) {
        let (sx, sy) = (s as i32 % self.cols, s as i32 / self.cols);
        (
            sx * SECTOR,
            sy * SECTOR,
            ((sx + 1) * SECTOR).min(grid.width()),
            ((sy + 1) * SECTOR).min(grid.height()),
        )
    }

    /// Rebuilds a sector's portals if its tiles changed since they were built.
    fn ensure(&mut self, grid: &NavGrid, s: usize) {
        let gen = grid.sector_generation(s);
        if self.built[s] == Some(gen) {
            return;
        }
        let (x0, y0, x1, y1) = self.range(grid, s);
        let mut portals = Vec::new();
        // Runs along each of the four edges. An edge run is a maximal
        // stretch where the tile on this side and its twin across the edge
        // are both passable; the run's middle becomes the portal.
        let mut runs = |cells: Vec<(Tile, Tile)>| {
            let mut run: Vec<(Tile, Tile)> = Vec::new();
            let flush = |run: &mut Vec<(Tile, Tile)>, portals: &mut Vec<Portal>| {
                if !run.is_empty() {
                    let (tile, twin) = run[run.len() / 2];
                    portals.push(Portal {
                        tile,
                        twin,
                        inner: Vec::new(),
                        tiles: Vec::new(),
                    });
                    run.clear();
                }
            };
            for (a, b) in cells {
                if grid.passable(a.0, a.1) && grid.passable(b.0, b.1) {
                    run.push((a, b));
                } else {
                    flush(&mut run, &mut portals);
                }
            }
            flush(&mut run, &mut portals);
        };
        if x1 < grid.width() {
            runs((y0..y1).map(|y| ((x1 - 1, y), (x1, y))).collect());
        }
        if x0 > 0 {
            runs((y0..y1).map(|y| ((x0, y), (x0 - 1, y))).collect());
        }
        if y1 < grid.height() {
            runs((x0..x1).map(|x| ((x, y1 - 1), (x, y1))).collect());
        }
        if y0 > 0 {
            runs((x0..x1).map(|x| ((x, y0), (x, y0 - 1))).collect());
        }
        debug_assert!(portals.len() <= MAX_PORTALS);
        portals.truncate(MAX_PORTALS);
        // Costs from each portal to every tile of the sector, and so to the
        // other portals, walking inside the sector.
        let tiles: Vec<Tile> = portals.iter().map(|p| p.tile).collect();
        for i in 0..portals.len() {
            self.intra(grid, s, &[tiles[i]]);
            let mut inner = Vec::new();
            for (j, &t) in tiles.iter().enumerate() {
                if j != i {
                    let d = self.intra_dist(grid, s, t);
                    if d != u32::MAX {
                        inner.push((j, d));
                    }
                }
            }
            portals[i].inner = inner;
            portals[i].tiles = self.dist.clone();
        }
        self.portals[s] = portals;
        self.built[s] = Some(gen);
    }

    /// Dijkstra inside one sector from `sources`, into `self.dist`
    /// (indexed by position within the sector's range).
    fn intra(&mut self, grid: &NavGrid, s: usize, sources: &[Tile]) {
        let (x0, y0, x1, y1) = self.range(grid, s);
        let w = x1 - x0;
        let n = (w * (y1 - y0)) as usize;
        self.dist.clear();
        self.dist.resize(n, u32::MAX);
        self.queue.clear();
        let at = |t: Tile| ((t.1 - y0) * w + (t.0 - x0)) as usize;
        for &t in sources {
            if t.0 >= x0 && t.0 < x1 && t.1 >= y0 && t.1 < y1 && grid.passable(t.0, t.1) {
                self.dist[at(t)] = 0;
                self.queue.push(0, at(t) as u32);
            }
        }
        while let Some((d, i)) = self.queue.pop() {
            let i = i as usize;
            if d > self.dist[i] {
                continue;
            }
            let (x, y) = (x0 + i as i32 % w, y0 + i as i32 / w);
            for (dx, dy, cost) in NEIGHBOURS {
                let (nx, ny) = (x + dx, y + dy);
                if nx < x0 || nx >= x1 || ny < y0 || ny >= y1 || !step_ok(grid, x, y, dx, dy) {
                    continue;
                }
                let j = at((nx, ny));
                let nd = d + cost;
                if nd < self.dist[j] {
                    self.dist[j] = nd;
                    self.queue.push(nd, j as u32);
                }
            }
        }
    }

    /// Position of a tile within its sector's range.
    fn within(&self, grid: &NavGrid, s: usize, t: Tile) -> usize {
        let (x0, y0, x1, _) = self.range(grid, s);
        ((t.1 - y0) * (x1 - x0) + (t.0 - x0)) as usize
    }

    /// Distance found by the last [`Sectors::intra`] to a tile of that sector.
    fn intra_dist(&self, grid: &NavGrid, s: usize, t: Tile) -> u32 {
        self.dist[self.within(grid, s, t)]
    }

    /// Walking cost inside sector `s` from its portal `pi` to tile `t`.
    fn portal_dist(&self, grid: &NavGrid, s: usize, pi: usize, t: Tile) -> u32 {
        self.portals[s][pi].tiles[self.within(grid, s, t)]
    }

    fn portal_at(&self, s: usize, tile: Tile) -> Option<usize> {
        self.portals[s].iter().position(|p| p.tile == tile)
    }

    /// Starts a new portal search: every entry of the scratch tables reads
    /// as unvisited until written.
    fn new_epoch(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            self.stamp.fill(0);
            self.epoch = 1;
        }
    }

    #[inline]
    fn node(s: usize, pi: usize) -> usize {
        s * MAX_PORTALS + pi
    }

    #[inline]
    fn best_of(&self, node: usize) -> u32 {
        if self.stamp[node] == self.epoch {
            self.best[node]
        } else {
            u32::MAX
        }
    }

    #[inline]
    fn set_best(&mut self, node: usize, d: u32, parent: u32) {
        self.stamp[node] = self.epoch;
        self.best[node] = d;
        self.parent[node] = parent;
    }

    /// The sectors routes from each of `froms` to the nearest of `seeds`
    /// pass through, as one set. One search over portals, run outward from
    /// the destination until every start has been reached, then a trace
    /// back from each start. A start here is one connected group of units
    /// inside one sector: units walled off from each other inside a sector
    /// may leave by different portals. A start with no route contributes
    /// nothing; the result is empty if none has one. Always includes the
    /// seeds' sectors and every reachable start's sector.
    pub fn corridors(&mut self, grid: &NavGrid, froms: &[Tile], seeds: &[Tile]) -> Vec<usize> {
        self.fit(grid);
        if seeds.is_empty() {
            return Vec::new();
        }
        let goal_sectors: Vec<usize> = {
            let mut v: Vec<usize> = seeds.iter().map(|&t| self.of(t)).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        let mut starts: Vec<usize> = froms
            .iter()
            .filter(|t| grid.passable(t.0, t.1))
            .map(|&t| self.of(t))
            .collect();
        starts.sort_unstable();
        starts.dedup();
        if starts.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<usize> = Vec::new();
        // Costs out of the graph: from each goal-sector portal to a seed;
        // and the seeds' reach inside each goal sector, kept for the starts
        // that share a sector with them.
        self.new_epoch();
        let mut heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();
        let mut goal_reach: Vec<(usize, Vec<u32>)> = Vec::new();
        for &g in &goal_sectors {
            self.ensure(grid, g);
            self.intra(grid, g, seeds);
            for pi in 0..self.portals[g].len() {
                let d = self.intra_dist(grid, g, self.portals[g][pi].tile);
                let node = Self::node(g, pi);
                if d != u32::MAX && d < self.best_of(node) {
                    self.set_best(node, d, u32::MAX);
                    heap.push(Reverse((d, node as u32)));
                }
            }
            if starts.contains(&g) {
                goal_reach.push((g, self.dist.clone()));
            }
        }
        // Each start: its sector, whether it reaches the seeds inside that
        // sector already, and the walking cost from its units to each
        // portal of the sector (`u32::MAX` where none of them can). Units
        // of one sector that reach different sets of portals are walled
        // off from each other inside it, and make separate starts.
        struct Start {
            sector: usize,
            direct: bool,
            to_portal: Vec<u32>,
        }
        let mut entries: Vec<Start> = Vec::new();
        for &start in &starts {
            self.ensure(grid, start);
            let n_portals = self.portals[start].len();
            let inside = goal_reach.iter().find(|(g, _)| *g == start).map(|(_, d)| d);
            // (reachable-portal set, direct, best cost to each portal)
            let mut classes: Vec<(u32, bool, Vec<u32>)> = Vec::new();
            for &t in froms {
                if !grid.passable(t.0, t.1) || self.of(t) != start {
                    continue;
                }
                let mut set = 0u32;
                let mut costs = Vec::with_capacity(n_portals);
                for pi in 0..n_portals {
                    let d = self.portal_dist(grid, start, pi, t);
                    if d != u32::MAX {
                        set |= 1 << pi;
                    }
                    costs.push(d);
                }
                let direct = inside.is_some_and(|d| d[self.within(grid, start, t)] != u32::MAX);
                match classes
                    .iter_mut()
                    .find(|(k, dir, _)| *k == set && *dir == direct)
                {
                    Some((_, _, best)) => {
                        for (b, c) in best.iter_mut().zip(costs) {
                            *b = (*b).min(c);
                        }
                    }
                    None => classes.push((set, direct, costs)),
                }
            }
            for (_, direct, to_portal) in classes {
                entries.push(Start {
                    sector: start,
                    direct,
                    to_portal,
                });
            }
        }
        // Outward over portals until every start has a settled portal it
        // can walk to, or the graph is exhausted.
        let mut pending: Vec<usize> = (0..entries.len()).filter(|&k| !entries[k].direct).collect();
        while let Some(Reverse((d, node))) = heap.pop() {
            let node = node as usize;
            if d > self.best_of(node) {
                continue;
            }
            let (s, pi) = (node / MAX_PORTALS, node % MAX_PORTALS);
            pending.retain(|&k| !(entries[k].sector == s && entries[k].to_portal[pi] != u32::MAX));
            if pending.is_empty() {
                break;
            }
            let twin_tile = self.portals[s][pi].twin;
            // Across the edge, into the neighbour.
            let ns = self.of(twin_tile);
            self.ensure(grid, ns);
            if let Some(npi) = self.portal_at(ns, twin_tile) {
                let nd = d + STRAIGHT;
                let nn = Self::node(ns, npi);
                if nd < self.best_of(nn) {
                    self.set_best(nn, nd, node as u32);
                    heap.push(Reverse((nd, nn as u32)));
                }
            }
            // Along the inside of this sector.
            for k in 0..self.portals[s][pi].inner.len() {
                let (qi, cost) = self.portals[s][pi].inner[k];
                let nd = d + cost;
                let qn = Self::node(s, qi);
                if nd < self.best_of(qn) {
                    self.set_best(qn, nd, node as u32);
                    heap.push(Reverse((nd, qn as u32)));
                }
            }
        }
        // Trace each start back to the goal.
        for e in &entries {
            if e.direct {
                out.push(e.sector);
                continue;
            }
            let mut entry: Option<(u32, usize)> = None;
            for (pi, &d_in) in e.to_portal.iter().enumerate() {
                if d_in == u32::MAX {
                    continue;
                }
                let d = self.best_of(Self::node(e.sector, pi));
                if d == u32::MAX {
                    continue;
                }
                let total = d_in + d;
                if entry.is_none_or(|(t, _)| total < t) {
                    entry = Some((total, pi));
                }
            }
            let Some((_, pi)) = entry else {
                continue;
            };
            out.push(e.sector);
            let mut node = Self::node(e.sector, pi);
            loop {
                let p = self.parent[node];
                if p == u32::MAX {
                    break;
                }
                node = p as usize;
                out.push(node / MAX_PORTALS);
            }
        }
        if out.is_empty() {
            return out;
        }
        out.extend(goal_sectors);
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The sectors a route from `from` to any of `seeds` passes through, or
    /// `None` if the sector graph finds no route.
    pub fn corridor(&mut self, grid: &NavGrid, from: Tile, seeds: &[Tile]) -> Option<Vec<usize>> {
        let c = self.corridors(grid, &[from], seeds);
        (!c.is_empty()).then_some(c)
    }

    /// Every sector, for a field that must cover the whole map.
    pub fn all(&mut self, grid: &NavGrid) -> Vec<usize> {
        self.fit(grid);
        (0..(self.cols * self.rows) as usize).collect()
    }
}

/// A monotone priority queue for integer costs: one bucket per cost, popped
/// in order. Floods relax thousands of tiles a tick, and a binary heap's
/// log factor is most of what they cost.
#[derive(Default)]
pub struct Buckets {
    buckets: Vec<Vec<u32>>,
    cur: usize,
    live: usize,
}

impl Buckets {
    fn clear(&mut self) {
        for b in &mut self.buckets {
            b.clear();
        }
        self.cur = 0;
        self.live = 0;
    }

    fn push(&mut self, cost: u32, item: u32) {
        debug_assert!(cost != u32::MAX, "unreached tile queued");
        let c = cost as usize;
        if c >= self.buckets.len() {
            self.buckets.resize_with(c + 1, Vec::new);
        }
        self.buckets[c].push(item);
        self.live += 1;
        if c < self.cur {
            self.cur = c;
        }
    }

    fn pop(&mut self) -> Option<(u32, u32)> {
        if self.live == 0 {
            return None;
        }
        while self.cur < self.buckets.len() {
            if let Some(item) = self.buckets[self.cur].pop() {
                self.live -= 1;
                return Some((self.cur as u32, item));
            }
            self.cur += 1;
        }
        self.live = 0;
        None
    }
}

/// The sectors a field covers, as a dense table the flood's inner loop can
/// index, plus the sorted list of them for iteration.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
struct Cover {
    /// Per sector: the grid generation it was flooded at; `u32::MAX` when
    /// not covered.
    gen: Vec<u32>,
    /// The covered sectors, ascending.
    list: Vec<usize>,
}

impl Cover {
    fn new(sectors: usize) -> Self {
        Self {
            gen: vec![u32::MAX; sectors],
            list: Vec::new(),
        }
    }

    fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.list.len()
    }

    #[inline]
    fn contains(&self, s: usize) -> bool {
        self.gen.get(s).is_some_and(|&g| g != u32::MAX)
    }

    /// Covers `s` at generation `gen` (which must not be `u32::MAX`, the
    /// marker for uncovered); true if it was not covered before.
    fn insert(&mut self, s: usize, gen: u32) -> bool {
        debug_assert_ne!(gen, u32::MAX);
        if s >= self.gen.len() {
            return false;
        }
        let fresh = self.gen[s] == u32::MAX;
        self.gen[s] = gen;
        if fresh {
            let at = self.list.partition_point(|&x| x < s);
            self.list.insert(at, s);
        }
        fresh
    }

    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.list.iter().copied()
    }
}

/// A flow field: walking cost to a destination over the tiles of the
/// sectors it covers.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Field {
    /// What it leads to.
    pub key: FieldKey,
    /// The tiles at cost zero: the destination, or the ring around it.
    seeds: Vec<Tile>,
    /// The sectors flooded, and the grid generation each was flooded at.
    covered: Cover,
    /// Bounding box of the covered sectors, tiles, `(x0, y0, w, h)`.
    bbox: (i32, i32, i32, i32),
    /// Cost per tile of the box; `u32::MAX` outside the corridor or unreached.
    dist: Vec<u32>,
    /// `PASSABLE` / `COVERED` bits per tile of the box, so the flood's inner
    /// loop reads one byte instead of asking the grid and the cover.
    open: Vec<u8>,
    /// Costs up to this value are exact; the flood stopped there once every
    /// tile asked for was settled. `u32::MAX` once it has run to the end.
    bound: u32,
    /// Tiles the last flood visited, for the tick statistics.
    pub flooded: u32,
    /// The tick this field was last read, for eviction.
    pub last_used: u64,
}

impl Field {
    fn at(&self, t: Tile) -> Option<usize> {
        let (x0, y0, w, h) = self.bbox;
        let (x, y) = (t.0 - x0, t.1 - y0);
        (x >= 0 && y >= 0 && x < w && y < h).then(|| (y * w + x) as usize)
    }

    /// Cost at a tile; `u32::MAX` when the field does not reach it, or has
    /// not settled it yet.
    pub fn cost(&self, t: Tile) -> u32 {
        match self.at(t) {
            Some(i) if self.dist[i] <= self.bound => self.dist[i],
            _ => u32::MAX,
        }
    }

    /// True if `t`'s sector is covered by this field.
    pub fn covers(&self, sectors: &Sectors, t: Tile) -> bool {
        self.covered.contains(sectors.of(t))
    }

    /// True if any covered sector changed since the flood.
    pub fn stale(&self, grid: &NavGrid) -> bool {
        self.covered
            .iter()
            .any(|s| grid.sector_generation(s) != self.covered.gen[s])
    }

    /// The neighbour of `t` the field points to: the cheapest legal step
    /// that is strictly downhill, ties broken by neighbour order.
    pub fn next(&self, grid: &NavGrid, t: Tile) -> Option<Tile> {
        let here = self.cost(t);
        if here == 0 || here == u32::MAX {
            return None;
        }
        let mut best: Option<(u32, Tile)> = None;
        for (dx, dy, _) in NEIGHBOURS {
            if !step_ok(grid, t.0, t.1, dx, dy) {
                continue;
            }
            let n = (t.0 + dx, t.1 + dy);
            let c = self.cost(n);
            if c < here && best.is_none_or(|(b, _)| c < b) {
                best = Some((c, n));
            }
        }
        best.map(|(_, n)| n)
    }

    /// Floods cost outward over the covered sectors. From scratch when
    /// `fresh` (a new field, or one whose tiles changed); otherwise the
    /// costs already known are kept and the flood runs on from the edge of
    /// the old coverage into the sectors just added, which is what makes
    /// extending a field for one straggler cheap.
    fn flood(
        &mut self,
        grid: &NavGrid,
        sectors: &Sectors,
        heap: &mut Buckets,
        fresh: bool,
        added: &[usize],
        stop: &[Tile],
    ) {
        // Box the covered sectors.
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, 0, 0);
        for s in self.covered.iter() {
            let (sx0, sy0, sx1, sy1) = sectors.range(grid, s);
            x0 = x0.min(sx0);
            y0 = y0.min(sy0);
            x1 = x1.max(sx1);
            y1 = y1.max(sy1);
        }
        if x0 >= x1 || y0 >= y1 {
            self.bbox = (0, 0, 0, 0);
            self.dist.clear();
            self.open.clear();
            self.bound = u32::MAX;
            return;
        }
        let (w, h) = (x1 - x0, y1 - y0);
        let old_box = self.bbox;
        let old = if fresh {
            Vec::new()
        } else {
            std::mem::take(&mut self.dist)
        };
        self.bbox = (x0, y0, w, h);
        self.dist.clear();
        self.dist.resize((w * h) as usize, u32::MAX);
        self.open.clear();
        self.open.resize((w * h) as usize, 0);
        // The mask: `PASSABLE` over every covered sector and the one-tile
        // ring round it (corner checks look no further), `COVERED` inside.
        for s in self.covered.iter() {
            let (sx0, sy0, sx1, sy1) = sectors.range(grid, s);
            for y in (sy0 - 1).max(y0)..(sy1 + 1).min(y1) {
                for x in (sx0 - 1).max(x0)..(sx1 + 1).min(x1) {
                    let i = ((y - y0) * w + (x - x0)) as usize;
                    if self.open[i] == 0 && grid.passable(x, y) {
                        self.open[i] = PASSABLE;
                    }
                }
            }
            for y in sy0..sy1 {
                for x in sx0..sx1 {
                    let i = ((y - y0) * w + (x - x0)) as usize;
                    if self.open[i] & PASSABLE != 0 {
                        self.open[i] |= COVERED;
                    }
                }
            }
        }
        heap.clear();
        let covered = |t: Tile| self.covered.contains(sectors.of(t));
        if !old.is_empty() {
            // Carry the known costs into the new box, which contains the
            // old one: coverage only grows between fresh floods.
            let (ox0, oy0, ow, oh) = old_box;
            debug_assert!(ox0 >= x0 && oy0 >= y0 && ox0 + ow <= x1 && oy0 + oh <= y1);
            for oy in 0..oh {
                let src = &old[(oy * ow) as usize..((oy + 1) * ow) as usize];
                let at = ((oy0 + oy - y0) * w + (ox0 - x0)) as usize;
                self.dist[at..at + ow as usize].copy_from_slice(src);
            }
            // Restart the flood from every settled tile that borders a sector
            // just added, and from every settled tile the last flood stopped
            // at: those are the only places the frontier can grow.
            for &s in added {
                let (sx0, sy0, sx1, sy1) = sectors.range(grid, s);
                for y in sy0 - 1..=sy1 {
                    for x in sx0 - 1..=sx1 {
                        let edge = x < sx0 || x >= sx1 || y < sy0 || y >= sy1;
                        if !edge {
                            continue;
                        }
                        if let Some(i) = self.at((x, y)) {
                            let d = self.dist[i];
                            if d != u32::MAX && d <= self.bound {
                                heap.push(d, i as u32);
                            }
                        }
                    }
                }
            }
            if self.bound != u32::MAX {
                self.push_frontier(heap);
            }
        }
        for &t in &self.seeds {
            if grid.passable(t.0, t.1) && covered(t) {
                if let Some(i) = self.at(t) {
                    if self.dist[i] != 0 {
                        self.dist[i] = 0;
                        heap.push(0, i as u32);
                    }
                }
            }
        }
        for k in 0..self.covered.list.len() {
            let s = self.covered.list[k];
            self.covered.gen[s] = grid.sector_generation(s);
        }
        // What was exact before this run: nothing for a fresh flood; the old
        // bound for a stopped one; every finite cost for a completed one.
        let floor = if fresh {
            0
        } else if self.bound == u32::MAX {
            self.dist
                .iter()
                .copied()
                .filter(|&d| d != u32::MAX)
                .max()
                .unwrap_or(0)
        } else {
            self.bound
        };
        self.run(heap, stop, floor);
    }

    /// Every settled tile with an unsettled neighbour, pushed so a stopped
    /// flood can carry on from where it left off.
    fn push_frontier(&self, heap: &mut Buckets) {
        let (_, _, w, h) = self.bbox;
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                let d = self.dist[i];
                if d == u32::MAX || d > self.bound {
                    continue;
                }
                let open = NEIGHBOURS.iter().any(|&(dx, dy, _)| {
                    self.step(x, y, dx, dy)
                        .is_some_and(|j| self.dist[j] > self.bound)
                });
                if open {
                    heap.push(d, i as u32);
                }
            }
        }
    }

    /// The box index of the neighbour `(dx, dy)` of box tile `(x, y)` if the
    /// flood may step there: inside the box, in a covered sector, passable,
    /// and not cutting a blocked corner.
    #[inline]
    fn step(&self, x: i32, y: i32, dx: i32, dy: i32) -> Option<usize> {
        let (_, _, w, h) = self.bbox;
        let (nx, ny) = (x + dx, y + dy);
        if nx < 0 || ny < 0 || nx >= w || ny >= h {
            return None;
        }
        let j = (ny * w + nx) as usize;
        if self.open[j] & COVERED == 0 {
            return None;
        }
        if dx != 0 && dy != 0 {
            // The orthogonal neighbours share a row with one tile and a
            // column with the other, so both lie inside the box.
            let jx = (y * w + nx) as usize;
            let jy = (ny * w + x) as usize;
            if self.open[jx] & PASSABLE == 0 || self.open[jy] & PASSABLE == 0 {
                return None;
            }
        }
        Some(j)
    }

    /// Runs the flood from whatever is queued until every tile in `stop`
    /// is settled, or the queue is empty. Costs up to `floor` were exact
    /// before the run; the bound grows from there as tiles settle.
    fn run(&mut self, heap: &mut Buckets, stop: &[Tile], floor: u32) {
        let (_, _, w, _) = self.bbox;
        let mut waiting: Vec<usize> = stop
            .iter()
            .filter_map(|&t| self.at(t))
            .filter(|&i| self.dist[i] == u32::MAX || self.dist[i] > floor)
            .collect();
        waiting.sort_unstable();
        waiting.dedup();
        let mut remaining = waiting.len();
        self.bound = floor;
        let mut visited = 0u32;
        if remaining == 0 && floor != 0 {
            // Everything asked for is already exact; the frontier keeps
            // for whoever asks next.
            heap.clear();
            self.flooded = 0;
            return;
        }
        while let Some((d, i)) = heap.pop() {
            let i = i as usize;
            if d > self.dist[i] {
                continue;
            }
            visited += 1;
            self.bound = self.bound.max(d);
            if remaining > 0 && waiting.binary_search(&i).is_ok() {
                remaining -= 1;
                if remaining == 0 {
                    // Everyone who asked is settled; leave the rest for
                    // whoever asks next.
                    self.flooded = visited;
                    return;
                }
            }
            let (x, y) = (i as i32 % w, i as i32 / w);
            for (dx, dy, cost) in NEIGHBOURS {
                let Some(j) = self.step(x, y, dx, dy) else {
                    continue;
                };
                let nd = d + cost;
                if nd < self.dist[j] {
                    self.dist[j] = nd;
                    heap.push(nd, j as u32);
                }
            }
        }
        // Ran to the end: every finite cost is exact.
        self.bound = u32::MAX;
        self.flooded = visited;
    }
}

/// The tiles a field with `key` leads to.
pub fn seeds_for(grid: &NavGrid, key: FieldKey) -> Vec<Tile> {
    let (x, y, fp) = key;
    if fp == 0 {
        if grid.passable(x, y) {
            return vec![(x, y)];
        }
        return grid
            .nearest_passable(x, y, 10, None)
            .map(|t| vec![t])
            .unwrap_or_default();
    }
    // The ring of passable tiles around the footprint.
    let inside = nav::footprint_tiles(x, y, fp as i32);
    let (min_x, max_x) = (
        inside.iter().map(|t| t.0).min().unwrap_or(x),
        inside.iter().map(|t| t.0).max().unwrap_or(x),
    );
    let (min_y, max_y) = (
        inside.iter().map(|t| t.1).min().unwrap_or(y),
        inside.iter().map(|t| t.1).max().unwrap_or(y),
    );
    let mut out = Vec::new();
    for ty in min_y - 1..=max_y + 1 {
        for tx in min_x - 1..=max_x + 1 {
            let edge = tx == min_x - 1 || tx == max_x + 1 || ty == min_y - 1 || ty == max_y + 1;
            if edge && grid.passable(tx, ty) {
                out.push((tx, ty));
            }
        }
    }
    out
}

/// The fields in use, by destination. Not simulation state: a cache whose
/// contents are a pure function of the grid, so two machines with different
/// caches still steer identically.
#[derive(Default)]
pub struct Fields {
    by_key: BTreeMap<FieldKey, usize>,
    fields: Vec<Field>,
    heap: Buckets,
    /// Tiles flooded this tick, for statistics; reset by the caller.
    pub flooded: u32,
    /// Fields built or rebuilt this tick, likewise.
    pub built: u32,
    /// Corridor searches this tick, likewise.
    pub corridors: u32,
    /// Fields that fell back to the whole map this tick, likewise.
    pub full: u32,
}

/// Fields unread for this many ticks are dropped.
const KEEP_TICKS: u64 = 400;

impl Fields {
    /// The field for `key` reaching every tile in `froms`, building,
    /// refreshing or extending it as needed. Serving a whole group at once
    /// costs one corridor search rather than one per straggler. `None` if
    /// no tile in `froms` can reach the destination at all.
    pub fn reach_many(
        &mut self,
        grid: &NavGrid,
        sectors: &mut Sectors,
        key: FieldKey,
        froms: &[Tile],
        tick: u64,
    ) -> Option<&Field> {
        sectors.fit(grid);
        let idx = match self.by_key.get(&key) {
            Some(&i) => i,
            None => {
                let seeds = seeds_for(grid, key);
                if seeds.is_empty() {
                    return None;
                }
                self.fields.push(Field {
                    key,
                    seeds,
                    covered: Cover::new(sectors.count()),
                    bbox: (0, 0, 0, 0),
                    dist: Vec::new(),
                    open: Vec::new(),
                    bound: u32::MAX,
                    flooded: 0,
                    last_used: tick,
                });
                let i = self.fields.len() - 1;
                self.by_key.insert(key, i);
                i
            }
        };
        let field = &mut self.fields[idx];
        field.last_used = tick;
        let fresh = field.covered.is_empty() || field.stale(grid);
        let uncovered: Vec<Tile> = froms
            .iter()
            .copied()
            .filter(|&t| fresh || !field.covers(sectors, t))
            .collect();
        let mut added = Vec::new();
        if !uncovered.is_empty() {
            let seeds = field.seeds.clone();
            self.corridors += 1;
            let mut corridor = sectors.corridors(grid, &uncovered, &seeds);
            if corridor.is_empty() {
                // The portal graph found nothing, but the components may
                // know better (a route that threads a sector edge the
                // portals do not capture): cover everything.
                if uncovered.iter().any(|&t| grid.connected(t, seeds[0])) {
                    self.full += 1;
                    corridor = sectors.all(grid);
                } else if field.covered.is_empty() {
                    return None;
                }
            }
            for s in corridor {
                if field.covered.insert(s, grid.sector_generation(s)) {
                    added.push(s);
                }
            }
        }
        let unsettled = froms
            .iter()
            .any(|&t| field.covers(sectors, t) && field.cost(t) == u32::MAX);
        if fresh || !added.is_empty() {
            field.flood(grid, sectors, &mut self.heap, fresh, &added, froms);
            self.flooded += field.flooded;
            self.built += 1;
        } else if unsettled && field.bound != u32::MAX {
            // Covered, but the flood stopped short of these tiles last
            // time: carry it on from the frontier.
            self.heap.clear();
            field.push_frontier(&mut self.heap);
            let floor = field.bound;
            field.run(&mut self.heap, froms, floor);
            self.flooded += field.flooded;
            self.built += 1;
        }
        Some(&self.fields[idx])
    }

    /// [`Fields::reach_many`] for one tile.
    pub fn reach(
        &mut self,
        grid: &NavGrid,
        sectors: &mut Sectors,
        key: FieldKey,
        from: Tile,
        tick: u64,
    ) -> Option<&Field> {
        self.reach_many(grid, sectors, key, &[from], tick)
    }

    /// Drops fields nobody has read for a while.
    pub fn evict(&mut self, tick: u64) {
        if self.fields.iter().all(|f| tick <= f.last_used + KEEP_TICKS) {
            return;
        }
        self.fields.retain(|f| tick <= f.last_used + KEEP_TICKS);
        self.by_key.clear();
        for (i, f) in self.fields.iter().enumerate() {
            self.by_key.insert(f.key, i);
        }
    }

    /// How many fields are live.
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// True if no field is live.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

/// The point a unit at `pos` should walk toward: the furthest of the next
/// `lookahead` tiles along the field that it can see, else the first.
/// `None` when the field does not reach `pos`'s tile.
pub fn steer(field: &Field, grid: &NavGrid, pos: Vec2Fx, lookahead: usize) -> Option<Vec2Fx> {
    let start = nav::tile_of(pos);
    let mut chain = Vec::with_capacity(lookahead);
    let mut t = start;
    while chain.len() < lookahead {
        match field.next(grid, t) {
            Some(n) => {
                chain.push(n);
                t = n;
            }
            None => break,
        }
    }
    if chain.is_empty() {
        return None;
    }
    for &t in chain.iter().rev() {
        if grid.line_of_sight(pos, nav::centre(t)) {
            return Some(nav::centre(t));
        }
    }
    Some(nav::centre(chain[0]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::TileMap;

    fn grid(w: i32, h: i32) -> NavGrid {
        NavGrid::from_map(&TileMap::new(w as u16, h as u16))
    }

    #[test]
    fn a_wall_with_a_gap_makes_one_portal() {
        let mut g = grid(32, 16);
        for y in 0..16 {
            if y != 7 {
                g.block(15, y);
                g.block(16, y);
            }
        }
        g.refresh();
        let mut s = Sectors::default();
        s.fit(&g);
        s.ensure(&g, 0);
        assert_eq!(s.portals[0].len(), 1, "{:?}", s.portals[0]);
        assert_eq!(s.portals[0][0].tile, (15, 7));
        assert_eq!(s.portals[0][0].twin, (16, 7));
        let corridor = s
            .corridor(&g, (2, 2), &[(30, 12)])
            .expect("a route through the gap");
        assert_eq!(corridor, vec![0, 1]);
    }

    #[test]
    fn corridor_follows_the_route_not_the_straight_line() {
        // A wall across the middle of a 64-wide map, open only at the top.
        let mut g = grid(64, 48);
        for y in 4..48 {
            g.block(31, y);
        }
        g.refresh();
        let mut s = Sectors::default();
        let corridor = s
            .corridor(&g, (5, 40), &[(58, 40)])
            .expect("route round the top");
        assert!(corridor.contains(&1), "goes via the top row: {corridor:?}");
        assert!(corridor.len() >= 6, "{corridor:?}");
        // Sealed: no corridor.
        for y in 0..4 {
            g.block(31, y);
        }
        g.refresh();
        let mut s2 = Sectors::default();
        assert!(s2.corridor(&g, (5, 40), &[(58, 40)]).is_none());
    }

    #[test]
    fn a_field_flows_downhill_to_its_seed_and_extends_on_demand() {
        let mut g = grid(64, 32);
        for y in 0..28 {
            g.block(31, y);
        }
        g.refresh();
        let mut sectors = Sectors::default();
        let mut fields = Fields::default();
        let f = fields
            .reach(&g, &mut sectors, (60, 3, 0), (3, 3), 1)
            .expect("reachable");
        assert_eq!(f.cost((60, 3)), 0);
        assert!(f.cost((3, 3)) > 0 && f.cost((3, 3)) != u32::MAX);
        // Walk the field: strictly downhill, ending on the seed.
        let mut t = (3, 3);
        let mut steps = 0;
        while let Some(n) = f.next(&g, t) {
            assert!(f.cost(n) < f.cost(t));
            t = n;
            steps += 1;
            assert!(steps < 200);
        }
        assert_eq!(t, (60, 3));
        assert!(steps > 40, "must go round the wall: {steps}");
        let covered_before = f.covered.len();
        assert_eq!(fields.built, 1);
        // A unit in an uncovered sector extends the field rather than
        // getting a new one.
        let f2 = fields
            .reach(&g, &mut sectors, (60, 3, 0), (3, 30), 2)
            .expect("reachable");
        assert!(f2.covered.len() >= covered_before);
        assert_ne!(f2.cost((3, 30)), u32::MAX);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields.built, 2);
    }

    #[test]
    fn a_field_to_a_building_leads_to_its_ring_and_goes_stale_when_tiles_change() {
        let mut g = grid(32, 32);
        g.block_footprint(16, 16, 3);
        g.refresh();
        let seeds = seeds_for(&g, (16, 16, 3));
        assert_eq!(seeds.len(), 16, "a 3x3 footprint has a ring of 16");
        assert!(seeds.iter().all(|&t| g.passable(t.0, t.1)));
        let mut sectors = Sectors::default();
        let mut fields = Fields::default();
        let f = fields
            .reach(&g, &mut sectors, (16, 16, 3), (2, 2), 1)
            .expect("reachable")
            .clone();
        assert_eq!(f.cost((14, 16)), 0, "the ring is the goal");
        assert_eq!(f.cost((16, 16)), u32::MAX, "the building itself is not");
        assert!(!f.stale(&g));
        g.block(8, 8);
        g.refresh();
        assert!(f.stale(&g), "a changed tile in a covered sector");
        let before = fields.built;
        fields
            .reach(&g, &mut sectors, (16, 16, 3), (2, 2), 2)
            .unwrap();
        assert_eq!(fields.built, before + 1, "rebuilt once");
        fields
            .reach(&g, &mut sectors, (16, 16, 3), (2, 2), 3)
            .unwrap();
        assert_eq!(fields.built, before + 1, "and then cached");
    }

    #[test]
    fn steering_looks_ahead_along_the_field_and_stays_in_sight() {
        let mut g = grid(40, 20);
        for y in 0..16 {
            g.block(20, y);
        }
        g.refresh();
        let mut sectors = Sectors::default();
        let mut fields = Fields::default();
        let f = fields
            .reach(&g, &mut sectors, (36, 3, 0), (3, 3), 1)
            .unwrap()
            .clone();
        let pos = nav::centre((3, 3));
        let target = steer(&f, &g, pos, 12).expect("a steering point");
        assert!(g.line_of_sight(pos, target));
        assert!(
            target.distance(pos) > crate::fx::Fx::from_int(4),
            "looked ahead: {target:?}"
        );
        // Off the field: nothing to steer by.
        let nowhere = nav::centre((39, 19));
        let far_field = Field {
            key: (0, 0, 0),
            seeds: vec![(0, 0)],
            covered: Cover::default(),
            bbox: (0, 0, 0, 0),
            dist: Vec::new(),
            open: Vec::new(),
            bound: u32::MAX,
            flooded: 0,
            last_used: 0,
        };
        assert!(steer(&far_field, &g, nowhere, 4).is_none());
    }

    #[test]
    fn eviction_forgets_old_fields_and_keeps_keys_straight() {
        let g = grid(32, 32);
        let mut sectors = Sectors::default();
        let mut fields = Fields::default();
        fields
            .reach(&g, &mut sectors, (30, 30, 0), (1, 1), 1)
            .unwrap();
        fields
            .reach(&g, &mut sectors, (1, 30, 0), (1, 1), 300)
            .unwrap();
        fields
            .reach(&g, &mut sectors, (30, 1, 0), (1, 1), 300)
            .unwrap();
        assert_eq!(fields.len(), 3);
        fields.evict(1 + KEEP_TICKS + 1);
        assert_eq!(fields.len(), 2);
        let f = fields
            .reach(&g, &mut sectors, (30, 1, 0), (1, 1), 500)
            .unwrap();
        assert_eq!(f.key, (30, 1, 0), "the survivor is found under its own key");
        let before = fields.built;
        fields
            .reach(&g, &mut sectors, (1, 30, 0), (1, 1), 500)
            .unwrap();
        assert_eq!(fields.built, before, "still cached");
    }
}
