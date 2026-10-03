//! The ground's grain: a greyscale layer per ground type, multiplied over
//! the blended terrain colours (`terrain.rs`) by the GPU's terrain shader
//! and the software rasteriser alike.
//!
//! `assets/terrain/detail.png` holds one layer per [`Terrain`], stacked top
//! to bottom in its order (`atlas detail` writes it from renders). A layer
//! is a tile's diamond, 128 x 64, inscribed in its rectangle, and tiles with
//! itself on every side. A texel of [`NEUTRAL`] leaves the colour as it is,
//! and every layer averages that, so the ground keeps its colour and gains
//! grain. Without the file the ground is drawn as before.

use sim::Terrain;
use std::path::{Path, PathBuf};

/// A layer's size, texels.
pub const LAYER_W: u16 = 128;
/// A layer's size, texels.
pub const LAYER_H: u16 = 64;
/// The texel that leaves a colour as it is.
pub const NEUTRAL: u8 = 128;

/// The grain sheet.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Detail {
    /// Width, texels.
    pub width: u32,
    /// Height, texels.
    pub height: u32,
    /// One byte per texel, row-major.
    pub texels: Vec<u8>,
}

impl Detail {
    /// No grain: one neutral texel, which every lookup clamps to.
    pub fn flat() -> Detail {
        Detail {
            width: 1,
            height: 1,
            texels: vec![NEUTRAL],
        }
    }

    /// Reads the sheet at `path`, checking it has a layer per ground type.
    pub fn load(path: &Path) -> Result<Detail, String> {
        let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut reader = png::Decoder::new(file)
            .read_info()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let (width, height) = (reader.info().width, reader.info().height);
        let colour = reader.info().color_type;
        let want = (LAYER_W as u32, LAYER_H as u32 * Terrain::ALL.len() as u32);
        if colour != png::ColorType::Grayscale
            || reader.info().bit_depth != png::BitDepth::Eight
            || (width, height) != want
        {
            return Err(format!(
                "{}: is {width}x{height} {colour:?}; the grain is {}x{} 8-bit greyscale",
                path.display(),
                want.0,
                want.1
            ));
        }
        let mut texels = vec![0u8; reader.output_buffer_size()];
        reader
            .next_frame(&mut texels)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        texels.truncate((width * height) as usize);
        Ok(Detail {
            width,
            height,
            texels,
        })
    }

    /// The sheet beside the sprite sets (`assets/terrain/detail.png`), or
    /// no grain if there is none; a broken sheet is reported and skipped.
    pub fn find() -> Detail {
        let Some(path) = default_path() else {
            return Detail::flat();
        };
        if !path.exists() {
            return Detail::flat();
        }
        Detail::load(&path).unwrap_or_else(|e| {
            eprintln!("warning: {e}");
            Detail::flat()
        })
    }

    /// The texel at `(x, y)`, clamped to the sheet's edges.
    pub fn texel(&self, x: i32, y: i32) -> u8 {
        let x = x.clamp(0, self.width as i32 - 1) as u32;
        let y = y.clamp(0, self.height as i32 - 1) as u32;
        self.texels[(y * self.width + x) as usize]
    }
}

/// Where the sheet is looked for: `terrain/detail.png` beside the sprites.
pub fn default_path() -> Option<PathBuf> {
    crate::sheets::default_dir()?
        .parent()
        .map(|assets| assets.join("terrain").join("detail.png"))
}

/// The sheet texel a tile of `terrain` maps its corner to: `corner` 0 to 3
/// is the tile's top, right, bottom and left, in [`crate::terrain`]'s order.
pub fn corner_texel(terrain: Terrain, corner: usize) -> [u16; 2] {
    let (u, v) = [
        (LAYER_W / 2, 0),
        (LAYER_W, LAYER_H / 2),
        (LAYER_W / 2, LAYER_H),
        (0, LAYER_H / 2),
    ][corner & 3];
    [u, v + LAYER_H * terrain as u16]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_grain_loads_with_a_layer_per_ground() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/terrain/detail.png");
        let d = Detail::load(&path).unwrap();
        assert_eq!((d.width, d.height), (128, 64 * 8));
        // Neutral outside each diamond, and about neutral on average inside.
        assert_eq!(d.texel(0, 0), NEUTRAL);
        let centre: u32 = (0..8).map(|l| d.texel(64, 32 + 64 * l) as u32).sum();
        assert!((60 * 8..=200 * 8).contains(&centre));
    }

    #[test]
    fn a_tile_maps_onto_its_own_layer_and_flat_is_neutral() {
        assert_eq!(corner_texel(Terrain::Grass, 0), [64, 0]);
        assert_eq!(corner_texel(Terrain::Dirt, 2), [64, 128]);
        assert_eq!(corner_texel(Terrain::Snow, 3), [0, 7 * 64 + 32]);
        let flat = Detail::flat();
        assert_eq!(flat.texel(500, -3), NEUTRAL);
    }
}
