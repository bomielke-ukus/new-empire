//! Navigation: which tiles are passable, whether two tiles are connected,
//! and how to get from one to the other.
//!
//! Three tools, cheapest first:
//!
//! 1. **Connected components** over passable tiles, relabelled lazily when a
//!    static blocker appears or disappears. "Can I get there?" is one
//!    comparison, and an unreachable goal is redirected to the nearest tile
//!    that *is* reachable before any search runs.
//! 2. **Line of sight** on the tile grid. Most trips a villager makes are
//!    short and straight; if nothing is in the way, that is the path.
//! 3. **A\*** on tiles, 8-connected without corner cutting, octile heuristic,
//!    node-budgeted, followed by string-pulling so the result walks
//!    diagonals instead of staircases.
//!
//! The original game spent a third of its frame here and still got it
//! wrong. Budgets keep the cost bounded; components keep failures cheap.

use crate::fx::Fx;
use crate::hash::{HashState, StateHasher};
use crate::map::TileMap;
use crate::vec2::Vec2Fx;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

/// A tile coordinate.
pub type Tile = (i32, i32);

/// Passability and connectivity for a map.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct NavGrid {
    width: i32,
    height: i32,
    /// Number of things blocking each tile; passable when zero.
    blockers: Vec<u16>,
    /// Component label per tile; 0 for impassable. Valid when `!dirty`.
    components: Vec<u16>,
    dirty: bool,
}

const ORTHO: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

impl NavGrid {
    /// A grid where water is blocked and everything else is open.
    pub fn from_map(map: &TileMap) -> NavGrid {
        let (w, h) = (map.width(), map.height());
        let mut blockers = vec![0u16; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                if !map.terrain(x, y).walkable() {
                    blockers[(y * w + x) as usize] = 1;
                }
            }
        }
        let mut g = NavGrid {
            width: w,
            height: h,
            blockers,
            components: vec![0; (w * h) as usize],
            dirty: true,
        };
        g.relabel();
        g
    }

    /// Width in tiles.
    pub fn width(&self) -> i32 {
        self.width
    }

    /// Height in tiles.
    pub fn height(&self) -> i32 {
        self.height
    }

    fn idx(&self, x: i32, y: i32) -> usize {
        (y * self.width + x) as usize
    }

    /// True if the tile exists.
    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width && y < self.height
    }

    /// True if a unit may stand on the tile.
    pub fn passable(&self, x: i32, y: i32) -> bool {
        self.in_bounds(x, y) && self.blockers[self.idx(x, y)] == 0
    }

    /// Adds a blocker to a tile.
    pub fn block(&mut self, x: i32, y: i32) {
        if self.in_bounds(x, y) {
            let i = self.idx(x, y);
            self.blockers[i] = self.blockers[i].saturating_add(1);
            self.dirty = true;
        }
    }

    /// Removes a blocker from a tile.
    pub fn unblock(&mut self, x: i32, y: i32) {
        if self.in_bounds(x, y) {
            let i = self.idx(x, y);
            self.blockers[i] = self.blockers[i].saturating_sub(1);
            self.dirty = true;
        }
    }

    /// Blocks a square footprint centred like the generator centres buildings.
    pub fn block_footprint(&mut self, x: i32, y: i32, footprint: i32) {
        for (tx, ty) in footprint_tiles(x, y, footprint) {
            self.block(tx, ty);
        }
    }

    /// Unblocks a square footprint.
    pub fn unblock_footprint(&mut self, x: i32, y: i32, footprint: i32) {
        for (tx, ty) in footprint_tiles(x, y, footprint) {
            self.unblock(tx, ty);
        }
    }

    /// True if every tile of a footprint is passable and in bounds.
    pub fn footprint_clear(&self, x: i32, y: i32, footprint: i32) -> bool {
        footprint_tiles(x, y, footprint)
            .into_iter()
            .all(|(tx, ty)| self.passable(tx, ty))
    }

    /// Recomputes component labels if anything changed since the last time.
    pub fn refresh(&mut self) {
        if self.dirty {
            self.relabel();
        }
    }

    fn relabel(&mut self) {
        let n = self.blockers.len();
        self.components.clear();
        self.components.resize(n, 0);
        let mut next = 1u16;
        let mut queue = VecDeque::new();
        for start in 0..n {
            if self.blockers[start] != 0 || self.components[start] != 0 {
                continue;
            }
            let label = next;
            next = next.wrapping_add(1).max(1);
            self.components[start] = label;
            queue.push_back(start);
            while let Some(i) = queue.pop_front() {
                let (x, y) = ((i as i32) % self.width, (i as i32) / self.width);
                for (dx, dy) in ORTHO {
                    let (nx, ny) = (x + dx, y + dy);
                    if self.passable(nx, ny) {
                        let j = self.idx(nx, ny);
                        if self.components[j] == 0 {
                            self.components[j] = label;
                            queue.push_back(j);
                        }
                    }
                }
            }
        }
        self.dirty = false;
    }

    /// Component label of a tile; 0 if impassable. Requires a fresh grid.
    pub fn component(&self, x: i32, y: i32) -> u16 {
        debug_assert!(!self.dirty, "NavGrid::refresh before querying components");
        if self.in_bounds(x, y) {
            self.components[self.idx(x, y)]
        } else {
            0
        }
    }

    /// True if a walk exists between two tiles. Requires a fresh grid.
    pub fn connected(&self, a: Tile, b: Tile) -> bool {
        let ca = self.component(a.0, a.1);
        ca != 0 && ca == self.component(b.0, b.1)
    }

    /// The nearest passable tile to `(x, y)` — in the same component as
    /// `from` if given — searching outward up to `radius`. Ties break toward
    /// the lowest ring, then scan order, so the answer is deterministic.
    pub fn nearest_passable(
        &self,
        x: i32,
        y: i32,
        radius: i32,
        from: Option<Tile>,
    ) -> Option<Tile> {
        let want = from.map(|f| self.component(f.0, f.1));
        let ok = |tx: i32, ty: i32| {
            self.passable(tx, ty) && want.is_none_or(|c| c == self.component(tx, ty))
        };
        for r in 0..=radius {
            let mut best: Option<(i64, Tile)> = None;
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    let (tx, ty) = (x + dx, y + dy);
                    if ok(tx, ty) {
                        let d = (dx as i64) * (dx as i64) + (dy as i64) * (dy as i64);
                        if best.is_none_or(|(bd, _)| d < bd) {
                            best = Some((d, (tx, ty)));
                        }
                    }
                }
            }
            if let Some((_, t)) = best {
                return Some(t);
            }
        }
        None
    }

    /// Up to `n` distinct passable tiles nearest `(x, y)` (same component as
    /// `from` if given), nearest first. Used to spread a group over its goal.
    pub fn spread(&self, x: i32, y: i32, n: usize, from: Option<Tile>) -> Vec<Tile> {
        let want = from.map(|f| self.component(f.0, f.1));
        let mut out = Vec::with_capacity(n);
        let mut r: i32 = 0;
        while out.len() < n && r < 24 {
            let mut ring = Vec::new();
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    let (tx, ty) = (x + dx, y + dy);
                    if self.passable(tx, ty) && want.is_none_or(|c| c == self.component(tx, ty)) {
                        ring.push((
                            (dx as i64) * (dx as i64) + (dy as i64) * (dy as i64),
                            (tx, ty),
                        ));
                    }
                }
            }
            ring.sort();
            out.extend(ring.into_iter().map(|(_, t)| t).take(n - out.len()));
            r += 1;
        }
        out
    }

    /// True if a straight walk from `a` to `b` crosses only passable tiles.
    /// Supercover: every tile the segment touches is checked, and diagonal
    /// steps between two blocked orthogonal neighbours are refused.
    pub fn line_of_sight(&self, a: Vec2Fx, b: Vec2Fx) -> bool {
        let (mut x, mut y) = (a.x.floor(), a.y.floor());
        let (x1, y1) = (b.x.floor(), b.y.floor());
        if !self.passable(x, y) || !self.passable(x1, y1) {
            return false;
        }
        // Amanatides–Woo grid traversal in fixed point.
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let step_x = if dx.is_positive() { 1 } else { -1 };
        let step_y = if dy.is_positive() { 1 } else { -1 };
        let next_boundary = |p: Fx, step: i32| {
            if step > 0 {
                Fx::from_int(p.floor() + 1)
            } else {
                Fx::from_int(p.floor())
            }
        };
        let mut t_max_x = if dx.is_zero() {
            Fx::MAX
        } else {
            ((next_boundary(a.x, step_x) - a.x) / dx).abs()
        };
        let mut t_max_y = if dy.is_zero() {
            Fx::MAX
        } else {
            ((next_boundary(a.y, step_y) - a.y) / dy).abs()
        };
        let t_delta_x = if dx.is_zero() {
            Fx::MAX
        } else {
            (Fx::ONE / dx).abs()
        };
        let t_delta_y = if dy.is_zero() {
            Fx::MAX
        } else {
            (Fx::ONE / dy).abs()
        };
        let mut guard = 0;
        while (x, y) != (x1, y1) {
            guard += 1;
            if guard > 1024 {
                return false;
            }
            if t_max_x < t_max_y {
                t_max_x += t_delta_x;
                x += step_x;
            } else if t_max_y < t_max_x {
                t_max_y += t_delta_y;
                y += step_y;
            } else {
                // Exactly through a corner: both orthogonal neighbours must be open.
                if !self.passable(x + step_x, y) || !self.passable(x, y + step_y) {
                    return false;
                }
                t_max_x += t_delta_x;
                t_max_y += t_delta_y;
                x += step_x;
                y += step_y;
            }
            if !self.passable(x, y) {
                return false;
            }
        }
        true
    }

    /// A\* from tile `from` to tile `to`. Returns the tile sequence *after*
    /// `from` ending at `to`, or `None` if no path exists or the node budget
    /// ran out. `scratch` is reused between calls to avoid allocation.
    pub fn astar(
        &self,
        from: Tile,
        to: Tile,
        budget: usize,
        scratch: &mut Scratch,
    ) -> Option<Vec<Tile>> {
        if !self.passable(from.0, from.1) || !self.passable(to.0, to.1) {
            return None;
        }
        if from == to {
            return Some(vec![]);
        }
        let n = self.blockers.len();
        scratch.reset(n);
        let start = self.idx(from.0, from.1);
        let goal = self.idx(to.0, to.1);
        scratch.g[start] = 0;
        scratch
            .open
            .push(Reverse((heuristic(from, to), 0, start as u32)));
        let mut expanded = 0usize;
        let mut order = 0u32;
        scratch.expanded = 0;
        while let Some(Reverse((_, _, current))) = scratch.open.pop() {
            let current = current as usize;
            if scratch.closed[current] {
                continue;
            }
            scratch.closed[current] = true;
            if current == goal {
                return Some(self.reconstruct(scratch, start, goal));
            }
            expanded += 1;
            scratch.expanded = expanded;
            if expanded > budget {
                return None;
            }
            let (x, y) = ((current as i32) % self.width, (current as i32) / self.width);
            let g = scratch.g[current];
            for (dx, dy, cost) in NEIGHBOURS {
                let (nx, ny) = (x + dx, y + dy);
                if !self.passable(nx, ny) {
                    continue;
                }
                if dx != 0 && dy != 0 && (!self.passable(x + dx, y) || !self.passable(x, y + dy)) {
                    continue;
                }
                let ni = self.idx(nx, ny);
                if scratch.closed[ni] {
                    continue;
                }
                let ng = g + cost;
                if ng < scratch.g[ni] {
                    scratch.g[ni] = ng;
                    scratch.parent[ni] = current as u32;
                    order += 1;
                    scratch
                        .open
                        .push(Reverse((ng + heuristic((nx, ny), to), order, ni as u32)));
                }
            }
        }
        None
    }

    fn reconstruct(&self, scratch: &Scratch, start: usize, goal: usize) -> Vec<Tile> {
        let mut out = Vec::new();
        let mut i = goal;
        while i != start {
            out.push(((i as i32) % self.width, (i as i32) / self.width));
            i = scratch.parent[i] as usize;
        }
        out.reverse();
        out
    }

    /// Removes waypoints that a straight walk can skip. The result keeps the
    /// final tile and never introduces a segment without line of sight.
    pub fn smooth(&self, from: Vec2Fx, tiles: &[Tile]) -> Vec<Vec2Fx> {
        let mut out = Vec::new();
        let mut anchor = from;
        let mut i = 0;
        while i < tiles.len() {
            // Furthest tile visible from the anchor.
            let mut j = i;
            while j + 1 < tiles.len() && self.line_of_sight(anchor, centre(tiles[j + 1])) {
                j += 1;
            }
            anchor = centre(tiles[j]);
            out.push(anchor);
            i = j + 1;
        }
        out
    }

    /// Full path query: LOS shortcut, else A\* plus smoothing. `budget` is
    /// the A\* node limit. Returns waypoints ending exactly at `goal`, or
    /// `None` if the goal tile is unreachable or the budget ran out.
    pub fn find_path(
        &self,
        from: Vec2Fx,
        goal: Vec2Fx,
        budget: usize,
        scratch: &mut Scratch,
    ) -> Option<Vec<Vec2Fx>> {
        let a = (from.x.floor(), from.y.floor());
        let b = (goal.x.floor(), goal.y.floor());
        if !self.passable(b.0, b.1) {
            return None;
        }
        if self.line_of_sight(from, goal) {
            return Some(vec![goal]);
        }
        let tiles = self.astar(a, b, budget, scratch)?;
        let mut way = self.smooth(from, &tiles);
        // Finish on the exact goal point rather than the tile centre.
        if let Some(last) = way.last_mut() {
            *last = goal;
        }
        Some(way)
    }
}

impl HashState for NavGrid {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_i32(self.width);
        h.write_i32(self.height);
        for b in &self.blockers {
            h.write_u16(*b);
        }
    }
}

/// Tiles covered by a square footprint centred the way buildings are.
pub fn footprint_tiles(x: i32, y: i32, footprint: i32) -> Vec<Tile> {
    let fp = footprint.max(1);
    let half = fp / 2;
    let mut out = Vec::with_capacity((fp * fp) as usize);
    for dy in -half..fp - half {
        for dx in -half..fp - half {
            out.push((x + dx, y + dy));
        }
    }
    out
}

/// World position of a building whose footprint is anchored on tile
/// `(x, y)` the way [`footprint_tiles`] lays it out: the geometric centre.
pub fn building_centre(x: i32, y: i32, footprint: i32) -> Vec2Fx {
    let fp = footprint.max(1);
    let half = fp / 2;
    let off = Fx::from_ratio(fp, 2);
    Vec2Fx::new(Fx::from_int(x - half) + off, Fx::from_int(y - half) + off)
}

/// Inverse of [`building_centre`]: the anchor tile of a building at `pos`.
pub fn anchor_tile(pos: Vec2Fx, footprint: i32) -> Tile {
    if footprint.max(1) % 2 == 1 {
        (pos.x.floor(), pos.y.floor())
    } else {
        (pos.x.round(), pos.y.round())
    }
}

/// Centre point of a tile.
pub fn centre(t: Tile) -> Vec2Fx {
    Vec2Fx::new(Fx::from_int(t.0) + Fx::HALF, Fx::from_int(t.1) + Fx::HALF)
}

/// Tile containing a point.
pub fn tile_of(p: Vec2Fx) -> Tile {
    (p.x.floor(), p.y.floor())
}

const NEIGHBOURS: [(i32, i32, u32); 8] = [
    (1, 0, 10),
    (-1, 0, 10),
    (0, 1, 10),
    (0, -1, 10),
    (1, 1, 14),
    (1, -1, 14),
    (-1, 1, 14),
    (-1, -1, 14),
];

/// Octile distance in the same units as the step costs.
fn heuristic(a: Tile, b: Tile) -> u32 {
    let dx = (a.0 - b.0).unsigned_abs();
    let dy = (a.1 - b.1).unsigned_abs();
    let (lo, hi) = (dx.min(dy), dx.max(dy));
    14 * lo + 10 * (hi - lo)
}

/// Reusable A\* buffers. Not part of the simulation state.
#[derive(Default)]
pub struct Scratch {
    g: Vec<u32>,
    parent: Vec<u32>,
    closed: Vec<bool>,
    open: BinaryHeap<Reverse<(u32, u32, u32)>>,
    /// Nodes expanded by the last search.
    pub expanded: usize,
}

impl Scratch {
    fn reset(&mut self, n: usize) {
        self.g.clear();
        self.g.resize(n, u32::MAX);
        self.parent.clear();
        self.parent.resize(n, u32::MAX);
        self.closed.clear();
        self.closed.resize(n, false);
        self.open.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Terrain;

    fn grid(w: i32, h: i32) -> NavGrid {
        NavGrid::from_map(&TileMap::new(w as u16, h as u16))
    }

    fn wall(g: &mut NavGrid, x: i32, y0: i32, y1: i32) {
        for y in y0..=y1 {
            g.block(x, y);
        }
    }

    #[test]
    fn water_blocks_and_components_split() {
        let mut map = TileMap::new(6, 3);
        for y in 0..3 {
            map.set_terrain(3, y, Terrain::DeepWater);
        }
        let g = NavGrid::from_map(&map);
        assert!(!g.passable(3, 1));
        assert!(g.passable(2, 1));
        assert!(!g.connected((0, 0), (5, 0)));
        assert!(g.connected((0, 0), (2, 2)));
        assert_eq!(g.component(3, 1), 0);
    }

    #[test]
    fn blockers_count_and_refresh() {
        let mut g = grid(4, 4);
        g.block(1, 1);
        g.block(1, 1);
        g.unblock(1, 1);
        assert!(
            !g.passable(1, 1),
            "two blockers, one removed: still blocked"
        );
        g.unblock(1, 1);
        assert!(g.passable(1, 1));
        g.block_footprint(2, 2, 2);
        assert!(!g.passable(1, 1) && !g.passable(2, 2) && !g.passable(1, 2) && !g.passable(2, 1));
        assert!(g.passable(3, 3));
        assert!(!g.footprint_clear(2, 2, 2));
        assert!(g.footprint_clear(3, 3, 1));
        assert!(!g.footprint_clear(0, 0, 3), "off-map is not clear");
        g.unblock_footprint(2, 2, 2);
        assert!(g.footprint_clear(2, 2, 2));
    }

    #[test]
    fn nearest_passable_respects_component() {
        let mut g = grid(7, 3);
        wall(&mut g, 3, 0, 2);
        g.refresh();
        assert_eq!(g.nearest_passable(3, 1, 3, None), Some((2, 1)));
        assert_eq!(g.nearest_passable(3, 1, 3, Some((6, 1))), Some((4, 1)));
        assert_eq!(g.nearest_passable(3, 1, 0, None), None);
        let s = g.spread(1, 1, 4, None);
        assert_eq!(s[0], (1, 1));
        assert_eq!(s.len(), 4);
        assert!(s.iter().all(|&(x, _)| x < 3));
    }

    #[test]
    fn line_of_sight() {
        let mut g = grid(10, 10);
        assert!(g.line_of_sight(centre((0, 0)), centre((9, 9))));
        assert!(g.line_of_sight(centre((0, 5)), centre((9, 5))));
        wall(&mut g, 5, 0, 9);
        assert!(!g.line_of_sight(centre((0, 5)), centre((9, 5))));
        assert!(!g.line_of_sight(centre((0, 0)), centre((9, 9))));
        assert!(g.line_of_sight(centre((0, 0)), centre((4, 9))));
        // A diagonal squeezed between two blocked tiles is refused.
        let mut h = grid(4, 4);
        h.block(1, 0);
        h.block(0, 1);
        assert!(!h.line_of_sight(centre((0, 0)), centre((1, 1))));
        assert!(h.line_of_sight(centre((0, 0)), centre((0, 0))));
    }

    #[test]
    fn astar_finds_way_around_a_wall_and_respects_corners() {
        let mut g = grid(10, 10);
        wall(&mut g, 5, 0, 8);
        g.refresh();
        let mut s = Scratch::default();
        let path = g.astar((0, 5), (9, 5), 10_000, &mut s).expect("path");
        assert_eq!(*path.last().unwrap(), (9, 5));
        assert!(path.iter().all(|&(x, y)| g.passable(x, y)));
        assert!(
            path.iter().any(|&(_, y)| y == 9),
            "must go round the bottom"
        );
        // Every step is a king move.
        let mut prev = (0, 5);
        for &t in &path {
            assert!((t.0 - prev.0).abs() <= 1 && (t.1 - prev.1).abs() <= 1);
            if t.0 != prev.0 && t.1 != prev.1 {
                assert!(
                    g.passable(t.0, prev.1) && g.passable(prev.0, t.1),
                    "corner cut at {t:?}"
                );
            }
            prev = t;
        }
        // Sealed off: no path, and the budget cannot rescue it.
        wall(&mut g, 5, 9, 9);
        g.refresh();
        assert!(g.astar((0, 5), (9, 5), 10_000, &mut s).is_none());
        assert!(!g.connected((0, 5), (9, 5)));
        // Tiny budget fails rather than hanging.
        let open = grid(50, 50);
        assert!(open.astar((0, 0), (49, 49), 5, &mut s).is_none());
    }

    #[test]
    fn smoothing_collapses_open_ground() {
        let g = grid(20, 20);
        let mut s = Scratch::default();
        let tiles = g.astar((0, 0), (19, 7), 10_000, &mut s).unwrap();
        assert!(tiles.len() >= 19);
        let way = g.smooth(centre((0, 0)), &tiles);
        assert_eq!(way.len(), 1, "open ground needs one waypoint: {way:?}");
        assert_eq!(way[0], centre((19, 7)));
        // With a wall, smoothing keeps a bend but nothing more.
        let mut w = grid(20, 20);
        wall(&mut w, 10, 0, 15);
        w.refresh();
        let tiles = w.astar((2, 5), (18, 5), 10_000, &mut s).unwrap();
        let way = w.smooth(centre((2, 5)), &tiles);
        assert!(way.len() <= 3, "{way:?}");
        let mut prev = centre((2, 5));
        for &p in &way {
            assert!(w.line_of_sight(prev, p));
            prev = p;
        }
    }

    #[test]
    fn find_path_end_to_end() {
        let mut g = grid(30, 30);
        for x in 5..25 {
            g.block(x, 15);
        }
        g.refresh();
        let mut s = Scratch::default();
        let from = Vec2Fx::new(
            Fx::from_ratio(15, 1) + Fx::from_ratio(3, 10),
            Fx::from_int(5),
        );
        let goal = Vec2Fx::new(
            Fx::from_ratio(15, 1) + Fx::from_ratio(7, 10),
            Fx::from_int(25),
        );
        let way = g.find_path(from, goal, 20_000, &mut s).expect("path");
        assert_eq!(*way.last().unwrap(), goal, "ends on the exact goal point");
        assert!(way.len() >= 2, "must bend around the wall: {way:?}");
        let clear = g
            .find_path(from, Vec2Fx::from_int(20, 5), 20_000, &mut s)
            .unwrap();
        assert_eq!(clear, vec![Vec2Fx::from_int(20, 5)], "LOS shortcut");
        assert!(
            g.find_path(from, Vec2Fx::from_int(10, 15), 20_000, &mut s)
                .is_none(),
            "blocked goal"
        );
    }

    #[test]
    fn maze_is_solved() {
        // A serpentine of walls forces a long path.
        let mut g = grid(21, 21);
        for row in (2..18).step_by(4) {
            wall_h(&mut g, row, 0, 18);
            wall_h(&mut g, row + 2, 2, 20);
        }
        g.refresh();
        let mut s = Scratch::default();
        let path = g
            .astar((0, 0), (20, 20), 50_000, &mut s)
            .expect("maze has a path");
        assert!(path.len() > 60, "serpentine should be long: {}", path.len());
        assert!(g.connected((0, 0), (20, 20)));
    }

    fn wall_h(g: &mut NavGrid, y: i32, x0: i32, x1: i32) {
        for x in x0..=x1 {
            g.block(x, y);
        }
    }

    #[test]
    fn footprints_and_centres() {
        assert_eq!(footprint_tiles(5, 5, 1), vec![(5, 5)]);
        assert_eq!(
            footprint_tiles(5, 5, 2),
            vec![(4, 4), (5, 4), (4, 5), (5, 5)]
        );
        assert_eq!(footprint_tiles(5, 5, 3).len(), 9);
        assert!(
            footprint_tiles(5, 5, 3).contains(&(4, 4))
                && footprint_tiles(5, 5, 3).contains(&(6, 6))
        );
        assert_eq!(tile_of(centre((3, 4))), (3, 4));
        assert_eq!(centre((0, 0)), Vec2Fx::new(Fx::HALF, Fx::HALF));
        for fp in 1..=4 {
            for &(x, y) in &[(5, 5), (0, 0), (7, 2)] {
                let c = building_centre(x, y, fp);
                assert_eq!(
                    anchor_tile(c, fp),
                    (x, y),
                    "fp {fp} at {x},{y}: centre {c:?}"
                );
                let tiles = footprint_tiles(x, y, fp);
                let (sx, sy) = tiles.iter().fold((Fx::ZERO, Fx::ZERO), |a, &t| {
                    (a.0 + centre(t).x, a.1 + centre(t).y)
                });
                assert_eq!(sx / tiles.len() as i32, c.x, "fp {fp}: x centre");
                assert_eq!(sy / tiles.len() as i32, c.y, "fp {fp}: y centre");
            }
        }
    }
}
