//! Fog of war as the renderers draw it (`docs/02` §9 `GD-FOG-01`).
//!
//! The simulation knows three states per tile. Here they become a light per
//! tile corner: black for ground never seen, half for ground seen once and
//! out of sight now, full for ground in sight. Terrain vertices carry the
//! light of their corner and both renderers shade across the tile from it,
//! so the fog edge is soft over one tile while the simulation stays
//! tile-exact and elevation costs nothing extra: the vertex already knows
//! its height. Sprites take one light each: what is in sight is drawn as it
//! is, what is remembered is drawn in the explored light, and what is
//! neither is not drawn.

use sim::{Fog, Visibility};

/// Light on ground seen once and out of sight now, out of 255.
pub const EXPLORED: u8 = 128;
/// Light on ground in sight.
pub const VISIBLE: u8 = 255;

/// The light at every tile corner of a map: `(width + 1) × (height + 1)`
/// bytes, row-major. This is also the texture the GPU samples.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FogLights {
    /// Corners across.
    pub width: u32,
    /// Corners down.
    pub height: u32,
    /// One byte per corner.
    pub lights: Vec<u8>,
}

/// The light a thing standing on a tile in `state` is drawn in.
pub fn tile_light(state: Visibility) -> u8 {
    match state {
        Visibility::Unexplored => 0,
        Visibility::Explored => EXPLORED,
        Visibility::Visible => VISIBLE,
    }
}

impl FogLights {
    /// Every corner lit: the view with no fog.
    pub fn lit(map_width: i32, map_height: i32) -> FogLights {
        let (w, h) = (map_width.max(0) as u32 + 1, map_height.max(0) as u32 + 1);
        FogLights {
            width: w,
            height: h,
            lights: vec![VISIBLE; (w * h) as usize],
        }
    }

    /// From a player's fog. A corner touching a tile never seen is black,
    /// so the edge of the known world fades on the known side and never
    /// shows a sliver of ground the player has not seen. Any other corner
    /// is the mean of the tiles around it, so the edge of sight is soft.
    pub fn from_fog(fog: &Fog) -> FogLights {
        let (w, h) = (fog.width().max(0), fog.height().max(0));
        let tiles: Vec<u8> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| tile_light(fog.state(x, y)))
            .collect();
        let at = |x: i32, y: i32| -> Option<u8> {
            (x >= 0 && y >= 0 && x < w && y < h).then(|| tiles[(y * w + x) as usize])
        };
        let mut lights = Vec::with_capacity(((w + 1) * (h + 1)) as usize);
        for cy in 0..=h {
            for cx in 0..=w {
                let (mut sum, mut n, mut dark) = (0u32, 0u32, false);
                for (tx, ty) in [(cx - 1, cy - 1), (cx, cy - 1), (cx - 1, cy), (cx, cy)] {
                    match at(tx, ty) {
                        Some(0) => dark = true,
                        Some(l) => {
                            sum += u32::from(l);
                            n += 1;
                        }
                        None => {}
                    }
                }
                lights.push(if dark || n == 0 { 0 } else { (sum / n) as u8 });
            }
        }
        FogLights {
            width: (w + 1) as u32,
            height: (h + 1) as u32,
            lights,
        }
    }

    /// The light at a corner; black outside the map.
    pub fn at(&self, cx: i32, cy: i32) -> u8 {
        if cx < 0 || cy < 0 || cx as u32 >= self.width || cy as u32 >= self.height {
            0
        } else {
            self.lights[(cy as u32 * self.width + cx as u32) as usize]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_are_black_beside_the_unseen_and_soft_between_seen_and_in_sight() {
        // An 8×4 map: the left half seen once, of which the leftmost two
        // columns are in sight now; the right half never seen.
        let mut fog = Fog::new(8, 4);
        for y in 0..4 {
            for x in 0..4 {
                fog.see(x, y);
            }
        }
        fog.clear_visible();
        for y in 0..4 {
            for x in 0..2 {
                fog.see(x, y);
            }
        }
        let l = FogLights::from_fog(&fog);
        assert_eq!((l.width, l.height), (9, 5));
        assert_eq!(l.at(0, 0), VISIBLE, "inside the sight disc");
        assert_eq!(l.at(1, 1), VISIBLE);
        assert_eq!(
            l.at(2, 1),
            ((u32::from(VISIBLE) + u32::from(EXPLORED)) / 2) as u8,
            "between in sight and seen once: the mean"
        );
        assert_eq!(l.at(3, 1), EXPLORED, "seen once");
        assert_eq!(l.at(4, 1), 0, "touching a tile never seen: black");
        assert_eq!(l.at(6, 2), 0);
        assert_eq!(l.at(-1, 0), 0, "outside the map");
        assert_eq!(l.at(9, 0), 0);
        let lit = FogLights::lit(8, 4);
        assert!(lit.lights.iter().all(|&v| v == VISIBLE));
        assert_eq!(lit.lights.len(), 45);
    }

    #[test]
    fn tile_lights() {
        assert_eq!(tile_light(Visibility::Unexplored), 0);
        assert_eq!(tile_light(Visibility::Explored), EXPLORED);
        assert_eq!(tile_light(Visibility::Visible), VISIBLE);
    }
}
