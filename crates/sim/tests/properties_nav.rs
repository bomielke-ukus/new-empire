//! Pathfinding properties over generated obstacle layouts.
//!
//! `docs/06` makes [RM-M2-03] an acceptance criterion for M2 — "pathfinding
//! property tests pass on adversarial maps: mazes, single-tile gaps, full
//! enclosure". `nav.rs` already has example tests for a wall, a maze and a
//! blocked goal, and examples do not generalise: they check the layouts
//! somebody thought of.
//!
//! These target `NavGrid` directly rather than a whole `Simulation`, because
//! these are properties of the search and the component labelling, and a
//! simulation would add units, orders and separation between the property and
//! what it is about.

use proptest::prelude::*;
use sim::nav::{self, NavGrid, Scratch};
use sim::TileMap;

/// Generous: the point is to prove the search is *correct*, not that it fits a
/// per-tick allowance. The budget assertions are in `behaviour_pathfinding`.
const BUDGET: usize = 2_000_000;

fn grid(w: i32, h: i32) -> NavGrid {
    NavGrid::from_map(&TileMap::new(w as u16, h as u16))
}

/// A layout: the grid size, and the tiles to block.
#[derive(Debug, Clone)]
struct Layout {
    size: i32,
    blocked: Vec<(i32, i32)>,
}

impl Layout {
    /// Every passable tile, in a stable order.
    ///
    /// Tests index into this rather than generating coordinates and discarding
    /// the ones that land on a wall: a maze layout blocks most of the grid, so
    /// rejection sampling throws away the majority of cases and proptest gives
    /// up before it has tested much.
    fn open_tiles(g: &NavGrid) -> Vec<(i32, i32)> {
        (0..g.height())
            .flat_map(|y| (0..g.width()).map(move |x| (x, y)))
            .filter(|&(x, y)| g.passable(x, y))
            .collect()
    }

    fn build(&self) -> NavGrid {
        let mut g = grid(self.size, self.size);
        for &(x, y) in &self.blocked {
            g.block(x, y);
        }
        g.refresh();
        g
    }
}

/// Scattered blockers, walls with gaps, and serpentine mazes — the three
/// shapes that break naive pathfinding in different ways.
fn any_layout() -> impl Strategy<Value = Layout> {
    let scatter = (12i32..40).prop_flat_map(|size| {
        prop::collection::vec((0..size, 0..size), 0..(size as usize * 4))
            .prop_map(move |blocked| Layout { size, blocked })
    });
    let walls_with_gaps = (16i32..40, 1usize..5, any::<u64>()).prop_map(|(size, walls, seed)| {
        let mut blocked = Vec::new();
        let mut s = seed;
        for k in 0..walls {
            // A deterministic pseudo-random gap per wall, from the seed.
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let x = ((k + 1) as i32 * size) / (walls as i32 + 1);
            let gap = (s >> 33) as i32 % size;
            for y in 0..size {
                if y != gap {
                    blocked.push((x, y));
                }
            }
        }
        Layout { size, blocked }
    });
    let maze = (17i32..41).prop_map(|size| {
        let mut blocked = Vec::new();
        let mut row = 2;
        let mut from_left = true;
        while row < size - 2 {
            for x in 0..size {
                let is_gap = if from_left { x == size - 1 } else { x == 0 };
                if !is_gap {
                    blocked.push((x, row));
                }
            }
            row += 3;
            from_left = !from_left;
        }
        Layout { size, blocked }
    });
    prop_oneof![scatter, walls_with_gaps, maze]
}

proptest! {
    /// The component labelling and the search must never disagree. If
    /// `connected` says yes, a path exists; if it says no, none does. A
    /// disagreement either strands units that could walk or sends them on
    /// searches that cannot succeed.
    ///
    /// REQ: RM-M2-03
    #[test]
    fn a_path_exists_exactly_when_the_tiles_are_connected(
        layout in any_layout(),
        a in 0usize..4096,
        b in 0usize..4096,
    ) {
        let g = layout.build();
        let open = Layout::open_tiles(&g);
        prop_assume!(!open.is_empty());
        let (from, to) = (open[a % open.len()], open[b % open.len()]);

        let mut s = Scratch::default();
        let found = g.astar(from, to, BUDGET, &mut s).is_some();
        prop_assert_eq!(
            found,
            g.connected(from, to),
            "astar {:?} -> {:?} says {}, connected says {}",
            from, to, found, g.connected(from, to)
        );
    }

    /// A returned path must be walkable: every tile passable, every step to an
    /// adjacent tile, ending on the goal. A path that teleports or clips a
    /// corner is worse than no path, because the unit will walk it.
    ///
    /// REQ: RM-M2-03
    #[test]
    fn a_returned_path_is_contiguous_and_passable(
        layout in any_layout(),
        a in 0usize..4096,
        b in 0usize..4096,
    ) {
        let g = layout.build();
        let open = Layout::open_tiles(&g);
        prop_assume!(!open.is_empty());
        let (from, to) = (open[a % open.len()], open[b % open.len()]);

        let mut s = Scratch::default();
        // Unreachable pairs are the other property's business; here there is
        // simply nothing to check, and skipping is cheaper than rejecting.
        let Some(path) = g.astar(from, to, BUDGET, &mut s) else {
            return Ok(());
        };
        if from == to {
            // Already there: no steps to take, and an empty path is the right
            // answer rather than a one-element path to where you stand.
            prop_assert!(path.is_empty(), "a path to the current tile: {:?}", path);
            return Ok(());
        }
        prop_assert_eq!(path.last().copied(), Some(to), "must end on the goal");

        let mut prev = from;
        for &t in &path {
            prop_assert!(g.passable(t.0, t.1), "path crosses a blocked tile {:?}", t);
            let (dx, dy) = ((t.0 - prev.0).abs(), (t.1 - prev.1).abs());
            prop_assert!(
                dx <= 1 && dy <= 1 && (dx + dy) > 0,
                "step from {:?} to {:?} is not to an adjacent tile",
                prev, t
            );
            prev = t;
        }
    }

    /// A unit sealed inside a ring must be told so, promptly. Returning `None`
    /// is the requirement; doing it without exhausting the node budget is what
    /// keeps a walled-in villager from costing a frame.
    ///
    /// REQ: RM-M2-03
    #[test]
    fn full_enclosure_reports_no_path_without_a_long_search(size in 15i32..40) {
        let mut g = grid(size, size);
        // A ring around the centre, and a goal outside it.
        let c = size / 2;
        for dx in -2i32..=2 {
            for dy in -2i32..=2 {
                if dx.abs() == 2 || dy.abs() == 2 {
                    g.block(c + dx, c + dy);
                }
            }
        }
        g.refresh();
        prop_assume!(g.passable(c, c));
        prop_assume!(g.passable(1, 1));

        prop_assert!(!g.connected((c, c), (1, 1)), "the ring must seal the centre");

        let mut s = Scratch::default();
        prop_assert!(
            g.astar((c, c), (1, 1), BUDGET, &mut s).is_none(),
            "a sealed unit must be told there is no path"
        );
        // The enclosed region is 3x3; a search that answers from the component
        // label rather than by exploring cannot have expanded much more.
        prop_assert!(
            s.expanded <= 64,
            "answering an enclosed query took {} node expansions",
            s.expanded
        );
    }

    /// A wall with exactly one hole in it. The gap is the only way through, so
    /// the path must use it — this is the case where an off-by-one in the
    /// neighbour or corner rules shows up as "the unit refuses to walk through
    /// a doorway".
    ///
    /// REQ: RM-M2-03
    #[test]
    fn a_single_tile_gap_is_found_and_used(size in 12i32..40, gap_at in 0usize..4096) {
        let mut g = grid(size, size);
        let wall_x = size / 2;
        // Keep the gap off the very edge, where a wall tile and the map border
        // would leave a diagonal-only squeeze the corner rules forbid.
        let gap = 1 + (gap_at as i32 % (size - 2));
        for y in 0..size {
            if y != gap {
                g.block(wall_x, y);
            }
        }
        g.refresh();

        let (from, to) = ((1, gap), (size - 2, gap));
        prop_assume!(g.passable(from.0, from.1) && g.passable(to.0, to.1));

        prop_assert!(
            g.connected(from, to),
            "one gap still connects the two halves"
        );
        let mut s = Scratch::default();
        let path = g
            .astar(from, to, BUDGET, &mut s)
            .expect("the gap is the way through");
        prop_assert!(
            path.contains(&(wall_x, gap)),
            "the path must pass through the gap at {:?}, got {:?}",
            (wall_x, gap), path
        );
    }

    /// The smoothed waypoints `find_path` hands the simulation must be
    /// reachable in straight lines, or a unit will walk into a wall between
    /// two of them.
    ///
    /// REQ: RM-M2-03
    #[test]
    fn smoothed_waypoints_have_line_of_sight_between_them(layout in any_layout()) {
        let g = layout.build();
        let open = Layout::open_tiles(&g);
        prop_assume!(open.len() >= 2);
        let (from, to) = (open[0], open[open.len() - 1]);

        let mut s = Scratch::default();
        let Some(way) = g.find_path(nav::centre(from), nav::centre(to), BUDGET, &mut s) else {
            return Ok(());
        };
        let mut prev = nav::centre(from);
        for &p in &way {
            prop_assert!(
                g.line_of_sight(prev, p),
                "no clear line from {:?} to {:?}",
                prev, p
            );
            prev = p;
        }
        prop_assert_eq!(way.last().copied(), Some(nav::centre(to)));
    }
}
