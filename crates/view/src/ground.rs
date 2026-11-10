//! The textured ground (`docs/look/ground.md`): with the new art on, every
//! pixel of ground is painted from one tiling texture per ground type,
//! blended where types meet along an edge the noise wobbles, the water
//! drifting and foaming at the shore.
//!
//! This module holds the art ([`GroundArt`]), the map's types as the
//! renderers read them ([`TypeMap`]), and the maths both renderers do per
//! pixel, written once here as the reference: [`Ground::colour`]. The
//! terrain shader (`crates/render/src/shaders/ground.wgsl`) does the same
//! sums in the same order; the software rasteriser calls this directly.
//!
//! Every pixel knows its world position in tiles, interpolated across its
//! tile's diamond from the corners (`TerrainVertex::corner`), so elevation
//! lifts the ground without sliding the texture over it. The slope's shade
//! and the fog's light ride on the vertices as before and multiply the
//! result.

use sim::{Terrain, TileMap};
use std::path::Path;

/// A ground texture's size, texels a side.
pub const TEX_SIZE: u32 = 512;
/// Texels of a ground texture a tile: 512 covers 4 x 4 tiles.
pub const TEX_PER_TILE: f32 = 128.0;
/// The noise texture's size, texels a side.
pub const NOISE_SIZE: u32 = 256;
/// Texels of the noise a tile: 256 covers 8 x 8 tiles.
pub const NOISE_PER_TILE: f32 = 32.0;
/// How far the noise pushes the blend's sample point, tiles at most.
pub const WOBBLE: f32 = 0.35;
/// The renderers' clock wraps at this many seconds: every drift and every
/// breath of the foam is a whole number of cycles in it, so the wrap is
/// invisible and an `f32` keeps its precision over a long session.
pub const PERIOD: f32 = 512.0;
/// How far each of the two samples of the water drifts a second, texels.
/// Whole numbers, so each comes back to where it started after [`PERIOD`].
pub const DRIFT: [[f32; 2]; 2] = [[5.0, 2.0], [-3.0, 4.0]];
/// Seconds for the foam to breathe in and out once; divides [`PERIOD`].
pub const BREATH: f32 = 4.0;
/// How far the foam's band moves as it breathes, in water weight.
pub const BREATH_DEPTH: f32 = 0.04;
/// The foam's colour, sRGB 0 to 1.
pub const FOAM: [f32; 3] = [0.97, 0.99, 1.0];
/// Mip levels of a ground texture, 512 down to 1.
pub const LEVELS: u32 = 10;
/// The file names under `assets/look/terrain`, in [`Terrain`]'s order.
pub const NAMES: [&str; 8] = [
    "grass", "dirt", "desert", "sand", "shallow", "deep", "forest", "snow",
];

/// The ground's textures, each with its mip chain, and the noise.
#[derive(Clone, PartialEq, Debug)]
pub struct GroundArt {
    /// Per ground type in [`Terrain`]'s order, its [`LEVELS`] mip levels,
    /// 512 x 512 RGBA8 down to 1 x 1, each a 2 x 2 box filter of the one
    /// above in sRGB bytes, as everything else is blended.
    pub textures: Vec<Vec<Vec<[u8; 4]>>>,
    /// The noise, 256 x 256, a byte a texel.
    pub noise: Vec<u8>,
}

/// The ground type of every tile, a byte a tile in [`Terrain`]'s order:
/// the texture the GPU reads it from.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TypeMap {
    /// Tiles across.
    pub width: u32,
    /// Tiles down.
    pub height: u32,
    /// Row-major.
    pub types: Vec<u8>,
}

impl TypeMap {
    /// A map's types.
    pub fn from_map(map: &TileMap) -> TypeMap {
        let (w, h) = (map.width().max(1), map.height().max(1));
        let types = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| map.terrain(x, y) as u8)
            .collect();
        TypeMap {
            width: w as u32,
            height: h as u32,
            types,
        }
    }

    /// The type at a tile, clamped to the map's edge: past it, the ground
    /// goes on as it ends.
    #[inline]
    pub fn at(&self, x: i32, y: i32) -> u8 {
        let x = x.clamp(0, self.width as i32 - 1) as u32;
        let y = y.clamp(0, self.height as i32 - 1) as u32;
        self.types[(y * self.width + x) as usize]
    }
}

/// The ground types blended at a point and how much of each: up to four,
/// a type once, the weights summing to one. Unused entries weigh zero.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Blend {
    /// Ground types, in [`Terrain`]'s order.
    pub types: [u8; 4],
    /// Their weights.
    pub weights: [f32; 4],
}

impl Blend {
    /// The weight of one type.
    pub fn weight(&self, t: Terrain) -> f32 {
        (0..4)
            .filter(|&i| self.types[i] == t as u8)
            .map(|i| self.weights[i])
            .sum()
    }

    /// The weight of the water, shallow and deep together.
    pub fn water(&self) -> f32 {
        self.weight(Terrain::ShallowWater) + self.weight(Terrain::DeepWater)
    }
}

/// Whether a type is water, which drifts.
#[inline]
pub fn is_water(t: u8) -> bool {
    t == Terrain::ShallowWater as u8 || t == Terrain::DeepWater as u8
}

/// The mip level the ground textures are read at, from how far the world
/// position moves (in tiles) across a window pixel to the right (`dx`)
/// and down (`dy`): constant over a triangle, which is flat. The longer
/// of the two in texels, over the square root of two. On flat ground a
/// tile is 128 texels a side and 64 x 32 px at 1x, so down the screen a
/// pixel crosses 5.7 texels and across it 2.8, and this is their
/// geometric mean, 4 at 1x: crisp, and the box-filtered levels keep it
/// from shimmering, where isotropic filtering by the longer axis alone
/// would blur the ground across. Every flat tile gets the same level, so
/// no chunk or tile edge shows; a slope facing away, squeezed into a
/// sliver, gets a coarser one rather than streaks.
pub fn lod(dx: (f32, f32), dy: (f32, f32)) -> f32 {
    let len = |d: (f32, f32)| (d.0 * d.0 + d.1 * d.1).sqrt() * TEX_PER_TILE;
    (len(dx).max(len(dy)) * std::f32::consts::FRAC_1_SQRT_2)
        .max(1e-6)
        .log2()
        .clamp(0.0, (LEVELS - 1) as f32)
}

/// The renderers' clock: seconds, wrapped at [`PERIOD`].
pub fn wrap_time(secs: f64) -> f32 {
    secs.rem_euclid(PERIOD as f64) as f32
}

impl GroundArt {
    /// Reads `dir/{grass,…,snow}.png` (512 x 512 RGB) and `dir/noise.png`
    /// (256 x 256 grey).
    pub fn load(dir: &Path) -> Result<GroundArt, String> {
        let mut textures = Vec::with_capacity(NAMES.len());
        for name in NAMES {
            let path = dir.join(format!("{name}.png"));
            let (w, h, rgba) = crate::sheets::read_rgba_png(&path)?;
            if (w, h) != (TEX_SIZE, TEX_SIZE) {
                return Err(format!(
                    "{}: is {w}x{h}; a ground texture is {TEX_SIZE}x{TEX_SIZE}",
                    path.display()
                ));
            }
            let base: Vec<[u8; 4]> = rgba
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| [p[0], p[1], p[2], 255])
                .collect();
            textures.push(mip_chain(base, TEX_SIZE));
        }
        let path = dir.join("noise.png");
        let (w, h, noise) = crate::sheets::read_grey_png(&path)?;
        if (w, h) != (NOISE_SIZE, NOISE_SIZE) {
            return Err(format!(
                "{}: is {w}x{h}; the noise is {NOISE_SIZE}x{NOISE_SIZE}",
                path.display()
            ));
        }
        Ok(GroundArt { textures, noise })
    }

    /// The ground art under the look directory (`assets/look/terrain`),
    /// or `None` with a warning if it is missing or broken: the ground is
    /// then drawn as before.
    pub fn find(look: Option<&Path>) -> Option<GroundArt> {
        let Some(dir) = look.map(|l| l.join("terrain")) else {
            eprintln!("warning: no assets/look; the ground is drawn without its textures");
            return None;
        };
        GroundArt::load(&dir)
            .map_err(|e| eprintln!("warning: {e}; the ground is drawn without its textures"))
            .ok()
    }

    /// Bytes the textures take on the GPU: every level of every type, and
    /// the noise.
    pub fn gpu_bytes(&self) -> usize {
        let tex: usize = self
            .textures
            .iter()
            .flat_map(|levels| levels.iter().map(|l| l.len() * 4))
            .sum();
        tex + self.noise.len()
    }

    /// The noise at texel position `(u, v)`, bilinear between texel
    /// centres and wrapping, 0 to 1: what a linear sampler returns.
    #[inline]
    pub fn noise_at(&self, u: f32, v: f32) -> f32 {
        let n = NOISE_SIZE as i32;
        let (u, v) = (u - 0.5, v - 0.5);
        let (u0, v0) = (u.floor(), v.floor());
        let (fu, fv) = (u - u0, v - v0);
        let (x0, y0) = ((u0 as i32).rem_euclid(n), (v0 as i32).rem_euclid(n));
        let (x1, y1) = ((x0 + 1) % n, (y0 + 1) % n);
        let at = |x: i32, y: i32| self.noise[(y * n + x) as usize] as f32;
        let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * fu;
        let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * fu;
        (top + (bottom - top) * fv) / 255.0
    }

    /// How far the noise pushes the blend's sample point at world `(x, y)`,
    /// each axis within [`WOBBLE`]: two reads of the noise, the second
    /// transposed and offset so the axes do not move together.
    #[inline]
    pub fn wobble(&self, x: f32, y: f32) -> (f32, f32) {
        let a = self.noise_at(x * NOISE_PER_TILE, y * NOISE_PER_TILE);
        let b = self.noise_at(y * NOISE_PER_TILE + 101.0, x * NOISE_PER_TILE + 57.0);
        ((a - 0.5) * 2.0 * WOBBLE, (b - 0.5) * 2.0 * WOBBLE)
    }

    /// Type `t`'s texture at texture coordinates `(u, v)` (1.0 is the
    /// texture's width, wrapping), trilinear at `lod`, sRGB 0 to 1.
    #[inline]
    pub fn sample(&self, t: u8, u: f32, v: f32, lod: f32) -> [f32; 3] {
        let levels = &self.textures[t as usize];
        let top = levels.len() - 1;
        let l0 = (lod.floor().max(0.0) as usize).min(top);
        let f = (lod - l0 as f32).clamp(0.0, 1.0);
        let a = bilinear(&levels[l0], TEX_SIZE >> l0, u, v);
        if f == 0.0 || l0 == top {
            return a;
        }
        let b = bilinear(&levels[l0 + 1], TEX_SIZE >> (l0 + 1), u, v);
        [
            a[0] + (b[0] - a[0]) * f,
            a[1] + (b[1] - a[1]) * f,
            a[2] + (b[2] - a[2]) * f,
        ]
    }
}

/// A level of `size` a side at texture coordinates `(u, v)`, bilinear
/// between texel centres and wrapping, as a linear sampler reads it.
#[inline]
fn bilinear(level: &[[u8; 4]], size: u32, u: f32, v: f32) -> [f32; 3] {
    let n = size as i32;
    let (u, v) = (u * size as f32 - 0.5, v * size as f32 - 0.5);
    let (u0, v0) = (u.floor(), v.floor());
    let (fu, fv) = (u - u0, v - v0);
    let (x0, y0) = ((u0 as i32).rem_euclid(n), (v0 as i32).rem_euclid(n));
    let (x1, y1) = ((x0 + 1) % n, (y0 + 1) % n);
    let at = |x: i32, y: i32| level[(y * n + x) as usize];
    let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
    let mut out = [0.0; 3];
    for (k, o) in out.iter_mut().enumerate() {
        let top = a[k] as f32 + (b[k] as f32 - a[k] as f32) * fu;
        let bottom = c[k] as f32 + (d[k] as f32 - c[k] as f32) * fu;
        *o = (top + (bottom - top) * fv) / 255.0;
    }
    out
}

/// A texture's levels from `size` a side down to 1, each texel of a level
/// the mean of the 2 x 2 below it, rounded.
pub fn mip_chain(base: Vec<[u8; 4]>, size: u32) -> Vec<Vec<[u8; 4]>> {
    let mut levels = vec![base];
    let mut n = size;
    while n > 1 {
        let half = n / 2;
        let prev = levels.last().expect("a level");
        let mut next = Vec::with_capacity((half * half) as usize);
        for y in 0..half {
            for x in 0..half {
                let at = |dx: u32, dy: u32| prev[((2 * y + dy) * n + 2 * x + dx) as usize];
                let (a, b, c, d) = (at(0, 0), at(1, 0), at(0, 1), at(1, 1));
                let mut p = [0u8; 4];
                for k in 0..4 {
                    let sum = a[k] as u32 + b[k] as u32 + c[k] as u32 + d[k] as u32;
                    p[k] = ((sum + 2) / 4) as u8;
                }
                next.push(p);
            }
        }
        levels.push(next);
        n = half;
    }
    levels
}

/// The art and a map's types: everything a pixel of ground is made from.
#[derive(Clone, Copy, Debug)]
pub struct Ground<'a> {
    /// The textures and noise.
    pub art: &'a GroundArt,
    /// The map's types.
    pub types: &'a TypeMap,
}

impl Ground<'_> {
    /// The types blended at world `(x, y)`: the four tiles whose centres
    /// surround the point, after the noise has pushed it, bilinearly.
    pub fn blend(&self, x: f32, y: f32) -> Blend {
        let (ox, oy) = self.art.wobble(x, y);
        blend_at(self.types, x + ox, y + oy)
    }

    /// The ground's colour at world `(x, y)` at `time` seconds (wrapped by
    /// [`wrap_time`]), its textures read at `lod`, sRGB 0 to 1, before the
    /// slope's shade and the fog's light.
    pub fn colour(&self, x: f32, y: f32, time: f32, lod: f32) -> [f32; 3] {
        let b = self.blend(x, y);
        // The texture at the pixel's own position, not the pushed one: the
        // wobble moves the edge between types, not the types themselves.
        let (u, v) = (
            x * TEX_PER_TILE / TEX_SIZE as f32,
            y * TEX_PER_TILE / TEX_SIZE as f32,
        );
        let mut col = [0.0f32; 3];
        for i in 0..4 {
            let w = b.weights[i];
            if w <= 0.0 {
                continue;
            }
            let t = b.types[i];
            let c = if is_water(t) {
                // Two reads drifting apart and back, averaged.
                let d = |k: usize| {
                    let s = time / TEX_SIZE as f32;
                    (u + DRIFT[k][0] * s, v + DRIFT[k][1] * s)
                };
                let (a, bb) = (d(0), d(1));
                let p = self.art.sample(t, a.0, a.1, lod);
                let q = self.art.sample(t, bb.0, bb.1, lod);
                [
                    (p[0] + q[0]) * 0.5,
                    (p[1] + q[1]) * 0.5,
                    (p[2] + q[2]) * 0.5,
                ]
            } else {
                self.art.sample(t, u, v, lod)
            };
            for k in 0..3 {
                col[k] += c[k] * w;
            }
        }
        let f = foam(self.art, b.water(), x, y, time);
        if f > 0.0 {
            for k in 0..3 {
                col[k] += (FOAM[k] - col[k]) * f;
            }
        }
        col
    }
}

/// The types of the four tile centres around world `(x, y)` (already
/// pushed by the wobble), weighted bilinearly, each type once.
pub fn blend_at(types: &TypeMap, x: f32, y: f32) -> Blend {
    let (px, py) = (x - 0.5, y - 0.5);
    let (fx0, fy0) = (px.floor(), py.floor());
    let (fx, fy) = (px - fx0, py - fy0);
    let (i0, j0) = (fx0 as i32, fy0 as i32);
    let mut b = Blend {
        types: [
            types.at(i0, j0),
            types.at(i0 + 1, j0),
            types.at(i0, j0 + 1),
            types.at(i0 + 1, j0 + 1),
        ],
        weights: [
            (1.0 - fx) * (1.0 - fy),
            fx * (1.0 - fy),
            (1.0 - fx) * fy,
            fx * fy,
        ],
    };
    // A type met twice is read once, with both weights.
    for i in 1..4 {
        for j in 0..i {
            if b.weights[j] > 0.0 && b.types[j] == b.types[i] {
                b.weights[j] += b.weights[i];
                b.weights[i] = 0.0;
                break;
            }
        }
    }
    b
}

/// How much foam at world `(x, y)`, 0 to 1, for `water`, the blended
/// weight of the water there: a line where it is between about 0.35 and
/// 0.6, the shore, broken where the finer noise is low, the band moving a
/// little toward the land and back as it breathes.
pub fn foam(art: &GroundArt, water: f32, x: f32, y: f32, time: f32) -> f32 {
    let shift = BREATH_DEPTH * (time * std::f32::consts::TAU / BREATH).sin();
    let (lo, hi) = (0.35 - shift, 0.6 - shift);
    let band = ((water - lo) / 0.05).clamp(0.0, 1.0) * ((hi - water) / 0.05).clamp(0.0, 1.0);
    if band <= 0.0 {
        return 0.0;
    }
    let breakup = art.noise_at(
        x * NOISE_PER_TILE * 3.1 + 17.0,
        y * NOISE_PER_TILE * 3.1 + 5.0,
    );
    band * ((breakup - 0.3) / 0.25).clamp(0.0, 1.0) * 0.85
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A tiny art set: each type a flat colour (its index in red), noise
    /// flat at the middle, so nothing wobbles.
    pub(crate) fn flat_art() -> GroundArt {
        let textures = (0..8u8)
            .map(|t| {
                mip_chain(
                    vec![[t * 30, 100, 200, 255]; (TEX_SIZE * TEX_SIZE) as usize],
                    TEX_SIZE,
                )
            })
            .collect();
        GroundArt {
            textures,
            noise: vec![128; (NOISE_SIZE * NOISE_SIZE) as usize],
        }
    }

    /// The committed art.
    pub(crate) fn committed_art() -> GroundArt {
        let dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/look/terrain");
        GroundArt::load(&dir).unwrap()
    }

    #[test]
    fn the_committed_art_loads_with_its_mips() {
        let art = committed_art();
        assert_eq!(art.textures.len(), 8);
        for levels in &art.textures {
            assert_eq!(levels.len(), LEVELS as usize);
            assert_eq!(levels[0].len(), 512 * 512);
            assert_eq!(levels[9].len(), 1);
        }
        assert_eq!(art.noise.len(), 256 * 256);
        // 8 textures with their mips (a third more) and the noise.
        assert_eq!(art.gpu_bytes(), 8 * 4 * 349_525 + 65_536);
    }

    #[test]
    fn missing_art_falls_back_without_a_crash() {
        let dir = std::env::temp_dir().join(format!("ne-ground-none-{}", std::process::id()));
        assert!(GroundArt::find(Some(&dir)).is_none());
        assert!(GroundArt::find(None).is_none());
    }

    #[test]
    fn the_type_map_clamps_at_the_edge() {
        let mut map = TileMap::new(4, 3);
        map.set_terrain(0, 0, Terrain::Sand);
        map.set_terrain(3, 2, Terrain::DeepWater);
        let t = TypeMap::from_map(&map);
        assert_eq!((t.width, t.height), (4, 3));
        assert_eq!(t.at(0, 0), Terrain::Sand as u8);
        assert_eq!(t.at(-5, -1), Terrain::Sand as u8);
        assert_eq!(t.at(9, 7), Terrain::DeepWater as u8);
        assert_eq!(t.at(1, 1), Terrain::Grass as u8);
    }

    /// At a tile's centre the tile is all there is; on the edge between two
    /// tiles each is half; at a corner where four meet, a quarter each.
    #[test]
    fn blend_weights_at_centres_edges_and_corners() {
        let mut map = TileMap::new(4, 4);
        map.set_terrain(2, 1, Terrain::Desert);
        let t = TypeMap::from_map(&map);
        let b = blend_at(&t, 2.5, 1.5);
        assert_eq!(b.weight(Terrain::Desert), 1.0);
        assert_eq!(b.weight(Terrain::Grass), 0.0);
        let b = blend_at(&t, 2.0, 1.5);
        assert_eq!(b.weight(Terrain::Desert), 0.5);
        assert_eq!(b.weight(Terrain::Grass), 0.5);
        let b = blend_at(&t, 2.0, 1.0);
        assert_eq!(b.weight(Terrain::Desert), 0.25);
        let b = blend_at(&t, 1.2, 3.7);
        assert_eq!(b.weight(Terrain::Grass), 1.0);
        assert_eq!(b.weights.iter().sum::<f32>(), 1.0, "{b:?}");
        // Each type once.
        assert_eq!(b.weights.iter().filter(|&&w| w > 0.0).count(), 1);
    }

    #[test]
    fn the_wobble_is_bounded_and_moves() {
        let art = committed_art();
        let (mut lo, mut hi) = (0.0f32, 0.0f32);
        for i in 0..2000 {
            let (x, y) = (i as f32 * 0.137, i as f32 * 0.071 + 3.0);
            let (ox, oy) = art.wobble(x, y);
            lo = lo.min(ox.min(oy));
            hi = hi.max(ox.max(oy));
            assert!(ox.abs() <= WOBBLE && oy.abs() <= WOBBLE, "{ox} {oy}");
        }
        assert!(hi - lo > WOBBLE, "the edge should wander: {lo}..{hi}");
        // Flat noise leaves the point where it is.
        assert!(flat_art().wobble(3.3, 4.4).0.abs() < 0.003);
    }

    #[test]
    fn a_tile_is_its_texture_and_an_edge_is_both() {
        let art = flat_art();
        let mut map = TileMap::new(4, 4);
        map.set_terrain(2, 1, Terrain::Desert);
        let types = TypeMap::from_map(&map);
        let g = Ground {
            art: &art,
            types: &types,
        };
        let red = |c: [f32; 3]| (c[0] * 255.0).round();
        assert_eq!(red(g.colour(2.5, 1.5, 0.0, 0.0)), 60.0);
        assert_eq!(red(g.colour(0.5, 0.5, 0.0, 0.0)), 0.0);
        let edge = red(g.colour(2.0, 1.5, 0.0, 0.0));
        assert!((edge - 30.0).abs() <= 1.0, "{edge}");
    }

    /// Foam lies along the shore and nowhere else, and breathes.
    #[test]
    fn foam_is_on_the_shore() {
        let art = committed_art();
        let mut map = TileMap::new(8, 4);
        for y in 0..4 {
            for x in 4..8 {
                map.set_terrain(x, y, Terrain::ShallowWater);
            }
        }
        let types = TypeMap::from_map(&map);
        let g = Ground {
            art: &art,
            types: &types,
        };
        let along = |x: f32, t: f32| -> f32 {
            (0..64)
                .map(|i| {
                    foam(
                        &art,
                        g.blend(x, 0.5 + i as f32 * 0.05).water(),
                        x,
                        0.5 + i as f32 * 0.05,
                        t,
                    )
                })
                .sum::<f32>()
        };
        assert_eq!(along(1.5, 0.0), 0.0, "inland: no foam");
        assert_eq!(along(6.5, 0.0), 0.0, "open water: no foam");
        let shore: f32 = (0..20).map(|i| along(3.5 + i as f32 * 0.05, 0.0)).sum();
        assert!(shore > 1.0, "foam at the shore: {shore}");
        let later: f32 = (0..20)
            .map(|i| along(3.5 + i as f32 * 0.05, BREATH * 0.25))
            .sum();
        assert_ne!(shore, later, "the foam breathes");
    }

    /// The water drifts with time, and comes back to itself when the clock
    /// wraps, so the wrap is invisible.
    #[test]
    fn water_drifts_and_the_clock_wraps_seamlessly() {
        let art = committed_art();
        let mut map = TileMap::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                map.set_terrain(x, y, Terrain::DeepWater);
            }
        }
        let types = TypeMap::from_map(&map);
        let g = Ground {
            art: &art,
            types: &types,
        };
        let a = g.colour(1.3, 2.2, 0.0, 0.0);
        let b = g.colour(1.3, 2.2, 7.0, 0.0);
        assert_ne!(a, b, "it moves");
        let c = g.colour(1.3, 2.2, PERIOD, 0.0);
        for k in 0..3 {
            assert!((a[k] - c[k]).abs() < 1e-3, "{a:?} {c:?}");
        }
        assert_eq!(wrap_time(PERIOD as f64 * 3.0 + 1.5), 1.5);
        // An hour in, the clock still counts in small steps.
        let t = wrap_time(3600.0 + 1.0 / 60.0);
        assert!((t - (3600.0f32 % PERIOD + 1.0 / 60.0)).abs() < 1e-4, "{t}");
    }

    #[test]
    fn levels_shrink_and_average() {
        let mut base = vec![[0u8, 0, 0, 255]; 4];
        base[0] = [200, 100, 0, 255];
        let m = mip_chain(base, 2);
        assert_eq!(m.len(), 2);
        assert_eq!(m[1][0], [50, 25, 0, 255]);
        // Flat ground at a zoom: a pixel right and a pixel down.
        let flat = |zoom: f32| {
            let (ax, ay) = crate::iso::unproject(1.0 / zoom, 0.0);
            let (bx, by) = crate::iso::unproject(0.0, 1.0 / zoom);
            lod((ax, ay), (bx, by))
        };
        assert!(flat(4.0).abs() < 1e-5);
        assert!((flat(1.0) - 2.0).abs() < 1e-5);
        assert!((flat(2.0) - 1.0).abs() < 1e-5);
        assert!((flat(0.5) - 3.0).abs() < 1e-5);
        // A sliver: three times the texels down the screen, a coarser level.
        let (ax, ay) = crate::iso::unproject(1.0, 0.0);
        let (bx, by) = crate::iso::unproject(0.0, 3.0);
        assert!(lod((ax, ay), (bx, by)) > flat(1.0) + 1.0);
    }
}
