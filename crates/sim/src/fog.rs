//! Fog of war: what each player has seen and can see (`docs/02` §9
//! `GD-FOG-01`; `docs/04` §6).
//!
//! One [`Fog`] per player. `visibility` counts the player's units and
//! buildings that can see each tile right now and is recomputed every tick
//! from their positions, so it is derived state and stays out of the hash;
//! `explored` and the remembered buildings are history and are hashed. A
//! remembered building is kept at its anchor tile until the tile is seen
//! again, which is the information asymmetry the design asks for.

use crate::command::PlayerId;
use crate::entity::{EntityId, KindId};
use crate::hash::{HashState, StateHasher};
use crate::kinds;
use crate::nav;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The furthest anything sees, in tiles. Bounds the stamp table.
pub const MAX_SIGHT: i32 = 16;

/// What a player knows of a tile. Ordered: seeing a tile now is knowing
/// more than having seen it once.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Visibility {
    /// Never seen: nothing known.
    Unexplored,
    /// Seen once: terrain and last-known buildings, no units.
    Explored,
    /// In sight now.
    Visible,
}

/// A building or node as it was last seen, at its anchor tile.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Memory {
    /// Its handle, so what was seen can be ordered at: a villager sent to
    /// a remembered tree, an army to a remembered building. Stale once the
    /// thing is gone, and a stale handle is ignored, as a player's click
    /// on a memory of something destroyed does nothing.
    pub id: EntityId,
    /// What stood there.
    pub kind: KindId,
    /// Whose it was.
    pub owner: PlayerId,
    /// The owner's age when it was last seen (by [`crate::Age::index`]), so
    /// it is drawn as it was and an advance out of sight is not given away.
    pub age: u8,
    /// Still under construction when last seen.
    pub site: bool,
}

/// One player's knowledge of the map.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Fog {
    width: i32,
    height: i32,
    /// Units and buildings of the player seeing each tile now, kept
    /// incrementally by the simulation (a unit that has not moved costs
    /// nothing; one that has trades its old disc for its new one). Derived
    /// from the world, so not hashed; a loaded save rebuilds it.
    visibility: Vec<u16>,
    /// One bit per tile: seen at least once.
    explored: Vec<u64>,
    /// Static things last seen, by anchor tile index, kept until the tile
    /// is seen again.
    remembered: BTreeMap<u32, Memory>,
    /// One bit per tile: something is remembered there. Checked before the
    /// map, so stamping a sight disc costs a bit test per tile, not a
    /// lookup.
    marked: Vec<u64>,
}

impl Fog {
    /// Nothing known of a `width` by `height` map.
    pub fn new(width: i32, height: i32) -> Fog {
        let n = (width.max(0) * height.max(0)) as usize;
        Fog {
            width,
            height,
            visibility: vec![0; n],
            explored: vec![0; n.div_ceil(64)],
            remembered: BTreeMap::new(),
            marked: vec![0; n.div_ceil(64)],
        }
    }

    /// Map width in tiles.
    pub fn width(&self) -> i32 {
        self.width
    }

    /// Map height in tiles.
    pub fn height(&self) -> i32 {
        self.height
    }

    fn idx(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.width && y < self.height)
            .then(|| (y * self.width + x) as usize)
    }

    /// True if the player sees the tile now.
    pub fn visible(&self, x: i32, y: i32) -> bool {
        self.idx(x, y).is_some_and(|i| self.visibility[i] > 0)
    }

    /// True if the player has ever seen the tile.
    pub fn explored(&self, x: i32, y: i32) -> bool {
        self.idx(x, y)
            .is_some_and(|i| self.explored[i / 64] & (1u64 << (i % 64)) != 0)
    }

    /// What the player knows of the tile.
    pub fn state(&self, x: i32, y: i32) -> Visibility {
        if self.visible(x, y) {
            Visibility::Visible
        } else if self.explored(x, y) {
            Visibility::Explored
        } else {
            Visibility::Unexplored
        }
    }

    /// True if any tile of a `kind` anchored at `(x, y)` is in sight.
    pub fn in_sight(&self, kind: KindId, x: i32, y: i32) -> bool {
        let fp = kinds::info(kind).footprint as i32;
        nav::footprint_tiles(x, y, fp)
            .into_iter()
            .any(|(tx, ty)| self.visible(tx, ty))
    }

    /// The building or node last seen anchored on the tile, if no part of
    /// it is in sight and something was there when it last was. What is in
    /// sight is not a memory: what is there is there.
    pub fn remembered(&self, x: i32, y: i32) -> Option<Memory> {
        let i = self.idx(x, y)?;
        let m = *self.remembered.get(&(i as u32))?;
        (!self.in_sight(m.kind, x, y)).then_some(m)
    }

    /// Every remembered thing out of sight, with its anchor tile, in tile
    /// order.
    pub fn memories(&self) -> impl Iterator<Item = ((i32, i32), Memory)> + '_ {
        self.remembered
            .iter()
            .map(move |(&i, m)| {
                let i = i as i32;
                ((i % self.width, i / self.width), *m)
            })
            .filter(move |((x, y), m)| !self.in_sight(m.kind, *x, *y))
    }

    /// Tiles in sight now.
    pub fn visible_count(&self) -> usize {
        self.visibility.iter().filter(|&&v| v > 0).count()
    }

    /// Tiles ever seen.
    pub fn explored_count(&self) -> usize {
        self.explored.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Nobody sees anything until the next stamp.
    pub fn clear_visible(&mut self) {
        self.visibility.fill(0);
    }

    /// One more observer of the tile. The first one is the moment it
    /// comes into view: it is explored from then on, and what was
    /// remembered there is superseded by what is there. Returns true for
    /// that first observer, so the caller knows the tile is newly seen.
    pub fn see(&mut self, x: i32, y: i32) -> bool {
        let Some(i) = self.idx(x, y) else {
            return false;
        };
        let was = self.visibility[i];
        self.visibility[i] = was.saturating_add(1);
        if was != 0 {
            return false;
        }
        let (word, bit) = (i / 64, 1u64 << (i % 64));
        self.explored[word] |= bit;
        if self.marked[word] & bit != 0 {
            self.marked[word] &= !bit;
            self.remembered.remove(&(i as u32));
        }
        true
    }

    /// One observer fewer. Out of sight of the last one, the tile is
    /// explored and remembered as it was.
    pub fn unsee(&mut self, x: i32, y: i32) {
        if let Some(i) = self.idx(x, y) {
            self.visibility[i] = self.visibility[i].saturating_sub(1);
        }
    }

    /// How many of the player's units and buildings see the tile now.
    pub fn observers(&self, x: i32, y: i32) -> u16 {
        self.idx(x, y).map_or(0, |i| self.visibility[i])
    }

    /// Something static stands on a seen tile: remember it.
    pub fn remember(&mut self, x: i32, y: i32, m: Memory) {
        if let Some(i) = self.idx(x, y) {
            self.marked[i / 64] |= 1u64 << (i % 64);
            self.remembered.insert(i as u32, m);
        }
    }

    /// What stood on a seen tile is gone: nothing is remembered there.
    pub fn forget(&mut self, x: i32, y: i32) {
        if let Some(i) = self.idx(x, y) {
            let (word, bit) = (i / 64, 1u64 << (i % 64));
            if self.marked[word] & bit != 0 {
                self.marked[word] &= !bit;
                self.remembered.remove(&(i as u32));
            }
        }
    }
}

impl HashState for Fog {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_i32(self.width);
        h.write_i32(self.height);
        h.write(&self.explored);
        h.write_u64(self.remembered.len() as u64);
        for (&i, m) in &self.remembered {
            h.write_u32(i);
            h.write_u32(m.id.index() as u32);
            h.write_u32(m.id.generation());
            h.write_u16(m.kind);
            h.write_u8(m.owner);
            h.write_u8(m.age);
            h.write_u8(m.site as u8);
        }
    }
}

/// The tiles within `radius` of a point, as offsets: a disc, in row order.
pub fn stamp(radius: i32) -> Vec<(i32, i32)> {
    let r = radius.clamp(0, MAX_SIGHT);
    let mut out = Vec::new();
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r + r {
                out.push((dx, dy));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stamp_is_a_symmetric_disc_and_grows_with_the_radius() {
        assert_eq!(stamp(0), vec![(0, 0)]);
        for r in 1..=MAX_SIGHT {
            let s = stamp(r);
            assert!(s.len() > stamp(r - 1).len());
            for &(dx, dy) in &s {
                assert!(s.contains(&(-dx, dy)) && s.contains(&(dx, -dy)) && s.contains(&(dy, dx)));
                assert!(dx.abs() <= r && dy.abs() <= r);
            }
            assert!(s.contains(&(r, 0)) && s.contains(&(0, -r)));
        }
        assert_eq!(stamp(MAX_SIGHT + 5).len(), stamp(MAX_SIGHT).len());
    }

    #[test]
    fn seeing_explores_and_forgets_what_was_remembered() {
        let mut f = Fog::new(8, 4);
        assert_eq!(f.state(3, 2), Visibility::Unexplored);
        f.remember(
            3,
            2,
            Memory {
                id: EntityId::from_parts(0, 0),
                kind: 11,
                owner: 1,
                age: 0,
                site: false,
            },
        );
        assert_eq!(f.remembered(3, 2).map(|m| m.kind), Some(11));
        f.see(3, 2);
        assert_eq!(f.state(3, 2), Visibility::Visible);
        assert_eq!(f.remembered(3, 2), None, "seen again: the memory goes");
        f.clear_visible();
        assert_eq!(f.state(3, 2), Visibility::Explored);
        assert_eq!(f.state(0, 0), Visibility::Unexplored);
        f.see(-1, 0);
        f.see(8, 3);
        assert_eq!((f.visible_count(), f.explored_count()), (0, 1));
        let mut h1 = StateHasher::new();
        f.hash_state(&mut h1);
        let mut g = f.clone();
        g.see(0, 0);
        let mut h2 = StateHasher::new();
        g.hash_state(&mut h2);
        assert_ne!(h1.finish(), h2.finish(), "explored is state");
        let mut k = f.clone();
        k.see(3, 2);
        k.clear_visible();
        let mut h3 = StateHasher::new();
        k.hash_state(&mut h3);
        let mut h4 = StateHasher::new();
        f.hash_state(&mut h4);
        assert_eq!(h3.finish(), h4.finish(), "visibility is not");
    }

    /// The incremental bookkeeping: only the first observer of a tile is
    /// its coming into view, the last one leaving is its going out, and a
    /// memory made while it is in view survives further observers.
    #[test]
    fn observers_are_counted_and_only_the_first_supersedes_a_memory() {
        let m = Memory {
            id: EntityId::from_parts(4, 1),
            kind: 100,
            owner: 255,
            age: 0,
            site: false,
        };
        let mut f = Fog::new(8, 4);
        f.remember(3, 2, m);
        assert!(f.see(3, 2), "the first observer");
        assert_eq!(f.remembered(3, 2), None, "superseded by what is there");
        f.remember(3, 2, m);
        assert!(!f.see(3, 2), "the second is not news");
        assert_eq!(f.observers(3, 2), 2);
        f.unsee(3, 2);
        assert_eq!(f.state(3, 2), Visibility::Visible);
        assert_eq!(
            f.remembered(3, 2),
            None,
            "still in sight: what is there is there"
        );
        f.unsee(3, 2);
        assert_eq!(f.state(3, 2), Visibility::Explored);
        assert_eq!(
            f.remembered(3, 2).map(|m| m.kind),
            Some(100),
            "out of sight: the memory"
        );
        f.unsee(3, 2);
        assert_eq!(f.observers(3, 2), 0, "saturates at nobody");
        f.forget(3, 2);
        assert_eq!(f.remembered(3, 2), None);
        f.forget(0, 0);
        assert!(
            !f.see(-1, 0) && f.observers(-1, 0) == 0,
            "off the map is nothing"
        );
    }
}
