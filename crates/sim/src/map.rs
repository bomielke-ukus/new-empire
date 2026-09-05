//! The tile map: terrain per tile, elevation per tile *corner*.
//!
//! Elevation lives on corners (a `(w+1) × (h+1)` grid) rather than tiles so
//! the isometric terrain can deform smoothly, the way the second game's did.
//! A tile's gameplay elevation is the rounded mean of its four corners.
//! Neighbouring corners never differ by more than one level; the map
//! generator enforces that, and [`TileMap::validate`] checks it.

use crate::hash::{HashState, StateHasher};
use serde::{Deserialize, Serialize};

/// Highest elevation level.
pub const MAX_ELEVATION: u8 = 3;

/// Two adjacent corner coordinates, as reported by [`TileMap::validate`].
pub type CornerPair = ((i32, i32), (i32, i32));

/// Ground type of a tile.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum Terrain {
    /// Temperate grassland, the default.
    #[default]
    Grass = 0,
    /// Bare earth.
    Dirt = 1,
    /// Arid sand.
    Desert = 2,
    /// Beach sand next to water.
    Sand = 3,
    /// Wadeable water; villagers can fish from the shore.
    ShallowWater = 4,
    /// Open water; boats only.
    DeepWater = 5,
    /// Ground under a forest canopy.
    ForestFloor = 6,
    /// Snow-covered ground.
    Snow = 7,
}

impl Terrain {
    /// Every terrain, in `repr` order.
    pub const ALL: [Terrain; 8] = [
        Terrain::Grass,
        Terrain::Dirt,
        Terrain::Desert,
        Terrain::Sand,
        Terrain::ShallowWater,
        Terrain::DeepWater,
        Terrain::ForestFloor,
        Terrain::Snow,
    ];

    /// From the `repr` value; unknown values become grass.
    pub const fn from_u8(v: u8) -> Terrain {
        match v {
            1 => Terrain::Dirt,
            2 => Terrain::Desert,
            3 => Terrain::Sand,
            4 => Terrain::ShallowWater,
            5 => Terrain::DeepWater,
            6 => Terrain::ForestFloor,
            7 => Terrain::Snow,
            _ => Terrain::Grass,
        }
    }

    /// Whether land units can stand on it.
    pub const fn walkable(self) -> bool {
        !matches!(self, Terrain::ShallowWater | Terrain::DeepWater)
    }

    /// Whether it is water of any depth.
    pub const fn is_water(self) -> bool {
        matches!(self, Terrain::ShallowWater | Terrain::DeepWater)
    }
}

/// Terrain and elevation for a rectangular map.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TileMap {
    width: u16,
    height: u16,
    terrain: Vec<Terrain>,
    /// Corner heights, `(width + 1) * (height + 1)`, row-major.
    corners: Vec<u8>,
}

impl TileMap {
    /// A flat grass map.
    pub fn new(width: u16, height: u16) -> TileMap {
        let w = width as usize;
        let h = height as usize;
        TileMap {
            width,
            height,
            terrain: vec![Terrain::Grass; w * h],
            corners: vec![0; (w + 1) * (h + 1)],
        }
    }

    /// Width in tiles.
    pub const fn width(&self) -> i32 {
        self.width as i32
    }

    /// Height in tiles.
    pub const fn height(&self) -> i32 {
        self.height as i32
    }

    /// True if `(x, y)` is a tile on the map.
    pub const fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32
    }

    fn tile_index(&self, x: i32, y: i32) -> usize {
        debug_assert!(self.in_bounds(x, y), "tile ({x}, {y}) out of bounds");
        y as usize * self.width as usize + x as usize
    }

    fn corner_index(&self, cx: i32, cy: i32) -> usize {
        debug_assert!(cx >= 0 && cy >= 0 && cx <= self.width as i32 && cy <= self.height as i32);
        cy as usize * (self.width as usize + 1) + cx as usize
    }

    /// Terrain of a tile. Out-of-bounds reads as deep water, so the map edge
    /// behaves like a coast for anything that asks.
    pub fn terrain(&self, x: i32, y: i32) -> Terrain {
        if self.in_bounds(x, y) {
            self.terrain[self.tile_index(x, y)]
        } else {
            Terrain::DeepWater
        }
    }

    /// Sets a tile's terrain. Out-of-bounds writes are ignored.
    pub fn set_terrain(&mut self, x: i32, y: i32, t: Terrain) {
        if self.in_bounds(x, y) {
            let i = self.tile_index(x, y);
            self.terrain[i] = t;
        }
    }

    /// Height of a corner; corners run `0..=width` and `0..=height`.
    /// Out of range clamps to the nearest edge corner.
    pub fn corner(&self, cx: i32, cy: i32) -> u8 {
        let cx = cx.clamp(0, self.width as i32);
        let cy = cy.clamp(0, self.height as i32);
        self.corners[self.corner_index(cx, cy)]
    }

    /// Sets a corner height, clamped to [`MAX_ELEVATION`]. Out of range ignored.
    pub fn set_corner(&mut self, cx: i32, cy: i32, h: u8) {
        if cx >= 0 && cy >= 0 && cx <= self.width as i32 && cy <= self.height as i32 {
            let i = self.corner_index(cx, cy);
            self.corners[i] = h.min(MAX_ELEVATION);
        }
    }

    /// The four corner heights of a tile: `[nw, ne, se, sw]` in corner-grid
    /// terms, i.e. `(x,y) (x+1,y) (x+1,y+1) (x,y+1)`.
    pub fn tile_corners(&self, x: i32, y: i32) -> [u8; 4] {
        [
            self.corner(x, y),
            self.corner(x + 1, y),
            self.corner(x + 1, y + 1),
            self.corner(x, y + 1),
        ]
    }

    /// Gameplay elevation of a tile: rounded mean of its corners.
    pub fn elevation(&self, x: i32, y: i32) -> u8 {
        let c = self.tile_corners(x, y);
        ((c[0] as u16 + c[1] as u16 + c[2] as u16 + c[3] as u16 + 2) / 4) as u8
    }

    /// True if the tile is land a unit can stand on.
    pub fn walkable(&self, x: i32, y: i32) -> bool {
        self.in_bounds(x, y) && self.terrain(x, y).walkable()
    }

    /// Checks the invariant that neighbouring corners differ by at most one
    /// level. Returns the first violating corner pair if any.
    pub fn validate(&self) -> Result<(), CornerPair> {
        let w = self.width as i32;
        let h = self.height as i32;
        for cy in 0..=h {
            for cx in 0..=w {
                let a = self.corner(cx, cy) as i32;
                if cx < w && (a - self.corner(cx + 1, cy) as i32).abs() > 1 {
                    return Err(((cx, cy), (cx + 1, cy)));
                }
                if cy < h && (a - self.corner(cx, cy + 1) as i32).abs() > 1 {
                    return Err(((cx, cy), (cx, cy + 1)));
                }
            }
        }
        Ok(())
    }

    /// Lowers corners until no neighbouring pair differs by more than one.
    /// Only ever lowers, so it terminates; used by the generator as the
    /// final guarantee after any shaping it does.
    pub fn enforce_max_step(&mut self) {
        let w = self.width as i32;
        let h = self.height as i32;
        loop {
            let mut changed = false;
            for cy in 0..=h {
                for cx in 0..=w {
                    let mut lowest = u8::MAX;
                    for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                        let nx = cx + dx;
                        let ny = cy + dy;
                        if nx >= 0 && ny >= 0 && nx <= w && ny <= h {
                            lowest = lowest.min(self.corner(nx, ny));
                        }
                    }
                    let i = self.corner_index(cx, cy);
                    if self.corners[i] > lowest + 1 {
                        self.corners[i] = lowest + 1;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// Terrain counts per type, for tests and the map viewer.
    pub fn terrain_histogram(&self) -> [usize; 8] {
        let mut out = [0; 8];
        for t in &self.terrain {
            out[*t as usize] += 1;
        }
        out
    }
}

impl HashState for TileMap {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u16(self.width);
        h.write_u16(self.height);
        for t in &self.terrain {
            h.write_u8(*t as u8);
        }
        h.write_bytes(&self.corners);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_map_basics() {
        let m = TileMap::new(4, 3);
        assert_eq!(m.width(), 4);
        assert_eq!(m.height(), 3);
        assert!(m.in_bounds(3, 2));
        assert!(!m.in_bounds(4, 2));
        assert!(!m.in_bounds(-1, 0));
        assert_eq!(m.terrain(1, 1), Terrain::Grass);
        assert_eq!(m.terrain(9, 9), Terrain::DeepWater);
        assert_eq!(m.elevation(0, 0), 0);
        assert!(m.walkable(0, 0));
        assert!(!m.walkable(4, 0));
        assert!(m.validate().is_ok());
    }

    #[test]
    fn terrain_round_trip_and_walkability() {
        for t in Terrain::ALL {
            assert_eq!(Terrain::from_u8(t as u8), t);
        }
        assert_eq!(Terrain::from_u8(200), Terrain::Grass);
        assert!(!Terrain::DeepWater.walkable());
        assert!(!Terrain::ShallowWater.walkable());
        assert!(Terrain::Sand.walkable());
        let mut m = TileMap::new(2, 2);
        m.set_terrain(1, 1, Terrain::DeepWater);
        assert!(!m.walkable(1, 1));
        m.set_terrain(5, 5, Terrain::Snow); // ignored
        assert_eq!(m.terrain_histogram()[Terrain::DeepWater as usize], 1);
    }

    #[test]
    fn corners_and_elevation() {
        let mut m = TileMap::new(2, 2);
        m.set_corner(1, 1, 2);
        assert_eq!(m.corner(1, 1), 2);
        assert_eq!(m.tile_corners(0, 0), [0, 0, 2, 0]);
        assert_eq!(m.elevation(0, 0), 1); // (0+0+2+0+2)/4 = 1
        m.set_corner(0, 0, 9);
        assert_eq!(m.corner(0, 0), MAX_ELEVATION);
        assert_eq!(m.corner(-5, -5), MAX_ELEVATION, "clamps to edge corner");
        assert_eq!(m.validate(), Err(((0, 0), (1, 0))));
    }

    #[test]
    fn enforce_max_step_lowers_until_valid() {
        let mut m = TileMap::new(6, 1);
        m.set_corner(3, 0, 3);
        m.set_corner(3, 1, 3);
        m.enforce_max_step();
        assert!(m.validate().is_ok());
        assert_eq!(m.corner(3, 0), 1, "isolated peak collapses to a bump");
        // A wide plateau keeps its interior.
        let mut p = TileMap::new(10, 10);
        for cy in 2..=8 {
            for cx in 2..=8 {
                p.set_corner(cx, cy, 2);
            }
        }
        p.enforce_max_step();
        assert!(p.validate().is_ok());
        assert_eq!(p.corner(5, 5), 2);
        assert_eq!(p.corner(2, 5), 1);
        assert_eq!(p.corner(1, 5), 0);
    }

    #[test]
    fn hash_reflects_terrain_and_corners() {
        let a = TileMap::new(3, 3);
        let mut b = a.clone();
        let mut ha = StateHasher::new();
        a.hash_state(&mut ha);
        b.set_corner(1, 1, 1);
        let mut hb = StateHasher::new();
        b.hash_state(&mut hb);
        assert_ne!(ha.finish(), hb.finish());
    }
}
