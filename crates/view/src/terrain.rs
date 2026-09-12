//! Terrain geometry: one diamond per tile, four vertices each, batched in
//! chunks so a camera move only touches the chunks it can see.
//!
//! Colour is per vertex. Each tile has its own colour (terrain type, a little
//! per-tile variation, slope shading); each vertex blends its tile's colour
//! with the tiles sharing that corner, which softens type boundaries without
//! any mask textures. Real blended textures arrive with real art.

use bytemuck::{Pod, Zeroable};
use sim::{Terrain, TileMap};

use crate::iso;

/// Tiles per chunk edge.
pub const CHUNK_TILES: i32 = 32;

/// A terrain vertex in world-screen space.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug, Pod, Zeroable)]
pub struct TerrainVertex {
    /// World-screen position.
    pub pos: [f32; 2],
    /// Colour, RGBA8.
    pub colour: [u8; 4],
}

/// The geometry for one chunk.
#[derive(Clone, PartialEq, Debug)]
pub struct ChunkMesh {
    /// Chunk column.
    pub cx: i32,
    /// Chunk row.
    pub cy: i32,
    /// Vertices.
    pub vertices: Vec<TerrainVertex>,
    /// Triangle list indices.
    pub indices: Vec<u32>,
    /// World-screen bounds `(min_x, min_y, max_x, max_y)`, for culling.
    pub bounds: (f32, f32, f32, f32),
}

impl ChunkMesh {
    /// True if the chunk overlaps a world-screen rectangle.
    pub fn overlaps(&self, rect: (f32, f32, f32, f32)) -> bool {
        let (l, t, r, b) = self.bounds;
        !(r < rect.0 || l > rect.2 || b < rect.1 || t > rect.3)
    }
}

/// Base colour of a terrain type: a step of the matching palette ramp, so
/// the ground and the sprites standing on it come from one palette.
pub fn terrain_colour(t: Terrain) -> [f32; 3] {
    use crate::palette::{index, rgb_f32};
    rgb_f32(match t {
        Terrain::Grass => index("grass", 5),
        Terrain::Dirt => index("dirt", 5),
        Terrain::Desert => index("sand", 4),
        Terrain::Sand => index("sand", 6),
        Terrain::ShallowWater => index("water_shallow", 5),
        Terrain::DeepWater => index("water_deep", 4),
        Terrain::ForestFloor => index("foliage", 3),
        Terrain::Snow => index("neutral", 15),
    })
}

fn hash2(x: i32, y: i32) -> u32 {
    let h = (x as u32).wrapping_mul(73_856_093) ^ (y as u32).wrapping_mul(19_349_663);
    h ^ (h >> 13)
}

/// Deterministic per-tile brightness variation in `[0.92, 1.08]`.
fn variation(x: i32, y: i32) -> f32 {
    0.92 + (hash2(x, y) % 1000) as f32 / 1000.0 * 0.16
}

/// How far a grass tile leans toward dry grass: patches a few tiles
/// across, so a meadow has drifts of paler growth in it rather than one
/// flat green. Zero for most tiles, up to about a third for the driest.
fn dryness(x: i32, y: i32) -> f32 {
    // Coarse cells give the patches their size; the fine hash breaks up
    // their edges so they do not read as a grid.
    let coarse = hash2(x.div_euclid(3), y.div_euclid(3)) % 100;
    let fine = hash2(x + 977, y + 331) % 100;
    let mix = (coarse as f32 * 0.7 + fine as f32 * 0.3) / 100.0;
    ((mix - 0.55) / 0.45).clamp(0.0, 1.0) * 0.35
}

/// A tile's own colour: type, variation, and slope shading with the light
/// coming from the upper-left of the screen.
pub fn tile_colour(map: &TileMap, x: i32, y: i32) -> [f32; 3] {
    let terrain = map.terrain(x, y);
    let mut base = terrain_colour(terrain);
    if terrain == Terrain::Grass {
        let dry = crate::palette::rgb_f32(crate::palette::index("grass_dry", 5));
        let d = dryness(x, y);
        for (b, d_c) in base.iter_mut().zip(dry) {
            *b += (d_c - *b) * d;
        }
    }
    let [top, right, bottom, left] = map.tile_corners(x, y).map(|h| h as f32);
    let slope = ((top + left) - (right + bottom)) * 0.5;
    let shade = (1.0 + 0.16 * slope).clamp(0.6, 1.4);
    let f = variation(x, y) * shade;
    [
        (base[0] * f).min(1.0),
        (base[1] * f).min(1.0),
        (base[2] * f).min(1.0),
    ]
}

/// Average colour of the in-bounds tiles touching corner `(cx, cy)`.
fn corner_colour(map: &TileMap, cx: i32, cy: i32) -> [f32; 3] {
    let mut sum = [0.0; 3];
    let mut n = 0.0;
    for (tx, ty) in [(cx - 1, cy - 1), (cx, cy - 1), (cx - 1, cy), (cx, cy)] {
        if map.in_bounds(tx, ty) {
            let c = tile_colour(map, tx, ty);
            sum[0] += c[0];
            sum[1] += c[1];
            sum[2] += c[2];
            n += 1.0;
        }
    }
    if n == 0.0 {
        [0.0; 3]
    } else {
        [sum[0] / n, sum[1] / n, sum[2] / n]
    }
}

fn to_rgba(c: [f32; 3]) -> [u8; 4] {
    [
        (c[0] * 255.0).round() as u8,
        (c[1] * 255.0).round() as u8,
        (c[2] * 255.0).round() as u8,
        255,
    ]
}

/// Builds the mesh for chunk `(cx, cy)`.
pub fn build_chunk(map: &TileMap, cx: i32, cy: i32) -> ChunkMesh {
    let x0 = cx * CHUNK_TILES;
    let y0 = cy * CHUNK_TILES;
    let x1 = (x0 + CHUNK_TILES).min(map.width());
    let y1 = (y0 + CHUNK_TILES).min(map.height());
    let mut vertices = Vec::with_capacity(((x1 - x0) * (y1 - y0) * 4) as usize);
    let mut indices = Vec::with_capacity(((x1 - x0) * (y1 - y0) * 6) as usize);
    let mut bounds = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for y in y0..y1 {
        for x in x0..x1 {
            let own = tile_colour(map, x, y);
            let corners = [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)];
            let base = vertices.len() as u32;
            for (cxi, cyi) in corners {
                let h = map.corner(cxi, cyi) as f32;
                let (sx, sy) = iso::project(cxi as f32, cyi as f32, h);
                let blend = corner_colour(map, cxi, cyi);
                let colour = [
                    own[0] * 0.55 + blend[0] * 0.45,
                    own[1] * 0.55 + blend[1] * 0.45,
                    own[2] * 0.55 + blend[2] * 0.45,
                ];
                vertices.push(TerrainVertex {
                    pos: [sx, sy],
                    colour: to_rgba(colour),
                });
                bounds.0 = bounds.0.min(sx);
                bounds.1 = bounds.1.min(sy);
                bounds.2 = bounds.2.max(sx);
                bounds.3 = bounds.3.max(sy);
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    ChunkMesh {
        cx,
        cy,
        vertices,
        indices,
        bounds,
    }
}

/// Builds every chunk of a map.
pub fn build_all(map: &TileMap) -> Vec<ChunkMesh> {
    let cols = (map.width() + CHUNK_TILES - 1) / CHUNK_TILES;
    let rows = (map.height() + CHUNK_TILES - 1) / CHUNK_TILES;
    let mut out = Vec::with_capacity((cols * rows) as usize);
    for cy in 0..rows {
        for cx in 0..cols {
            out.push(build_chunk(map, cx, cy));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_geometry_counts() {
        let map = TileMap::new(40, 40);
        let chunks = build_all(&map);
        assert_eq!(chunks.len(), 4);
        let full = &chunks[0];
        assert_eq!(full.vertices.len(), 32 * 32 * 4);
        assert_eq!(full.indices.len(), 32 * 32 * 6);
        let edge = &chunks[3];
        assert_eq!(edge.vertices.len(), 8 * 8 * 4);
        assert!(full
            .indices
            .iter()
            .all(|&i| (i as usize) < full.vertices.len()));
    }

    #[test]
    fn elevation_lifts_vertices() {
        let mut map = TileMap::new(4, 4);
        map.set_corner(1, 1, 2);
        let m = build_chunk(&map, 0, 0);
        // Tile (0,0)'s bottom corner is corner (1,1): index 2 of the first quad.
        let (_, flat_y) = iso::project(1.0, 1.0, 0.0);
        assert_eq!(m.vertices[2].pos[1], flat_y - 2.0 * iso::ELEV_PX);
        assert_eq!(m.vertices[0].pos, [0.0, 0.0]);
    }

    #[test]
    fn slope_shading_lights_the_upper_left() {
        let mut map = TileMap::new(4, 4);
        let flat = tile_colour(&map, 1, 1);
        map.set_corner(1, 1, 1);
        map.set_corner(1, 2, 1); // left-hand corners of tile (1,1) raised
        let lit = tile_colour(&map, 1, 1);
        assert!(lit[1] > flat[1], "tile facing the light should be brighter");
        let mut map2 = TileMap::new(4, 4);
        map2.set_corner(2, 1, 1);
        map2.set_corner(2, 2, 1); // right-hand corners raised
        let dim = tile_colour(&map2, 1, 1);
        assert!(dim[1] < flat[1], "tile facing away should be darker");
    }

    #[test]
    fn colours_blend_across_a_boundary() {
        let mut map = TileMap::new(4, 4);
        map.set_terrain(2, 1, Terrain::Desert);
        let m = build_chunk(&map, 0, 0);
        // Tile (1,1) right corner (index 1) touches the desert; its top corner
        // (index 0) does not. Desert is brighter, so the right corner is too.
        let q = (4 + 1) * 4; // tile (1,1) in a 4-wide map, 4 vertices per tile
        assert!(m.vertices[q + 1].colour[0] > m.vertices[q].colour[0]);
        assert!(m.overlaps((0.0, 0.0, 10.0, 10.0)));
        assert!(!m.overlaps((10_000.0, 10_000.0, 10_010.0, 10_010.0)));
    }

    #[test]
    fn variation_is_bounded_and_deterministic() {
        for y in 0..50 {
            for x in 0..50 {
                let v = variation(x, y);
                assert!((0.92..=1.08).contains(&v));
                assert_eq!(v, variation(x, y));
            }
        }
        assert_ne!(variation(3, 4), variation(4, 3));
    }

    #[test]
    fn meadows_have_dry_patches_but_most_grass_is_grass() {
        let mut dry = 0;
        for y in 0..60 {
            for x in 0..60 {
                let d = dryness(x, y);
                assert!((0.0..=0.35).contains(&d));
                assert_eq!(d, dryness(x, y));
                if d > 0.0 {
                    dry += 1;
                }
            }
        }
        let share = dry as f32 / 3600.0;
        assert!(
            (0.15..0.55).contains(&share),
            "a meadow is mostly green with some pale drifts, not {share:.2} dry"
        );
    }
}
