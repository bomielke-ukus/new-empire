//! Indexed PNG in and out.
//!
//! Every sprite in the game is an 8-bit indexed PNG carrying the full 256-entry
//! palette in its `PLTE` chunk and marking index 0 transparent in `tRNS`. That
//! is not a storage optimisation — it is how player colour works. The renderer
//! uploads indices, not colours, and remaps 240..=247 per owner in the fragment
//! shader (`docs/05` §2.4). An RGBA sprite cannot do that.

use crate::colour::Srgb;
use crate::palette::{Palette, TRANSPARENT};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

/// An 8-bit indexed image: one palette index per pixel.
#[derive(Debug)]
pub struct Indexed {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Indexed {
    /// A fully transparent image.
    pub fn new(width: u32, height: u32) -> Self {
        Indexed {
            width,
            height,
            pixels: vec![TRANSPARENT; (width * height) as usize],
        }
    }

    pub fn get(&self, x: u32, y: u32) -> u8 {
        self.pixels[(y * self.width + x) as usize]
    }

    pub fn set(&mut self, x: u32, y: u32, index: u8) {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize] = index;
        }
    }

    /// Copies a rectangle out, for per-frame inspection.
    pub fn crop(&self, x: u32, y: u32, w: u32, h: u32) -> Indexed {
        let mut out = Indexed::new(w, h);
        for row in 0..h {
            for col in 0..w {
                out.set(col, row, self.get(x + col, y + row));
            }
        }
        out
    }

    /// Bounding box of non-transparent pixels: (x, y, w, h), or `None` when the
    /// image is empty. Used to check that a frame actually contains a sprite
    /// and that its anchor sits under it.
    pub fn content_bounds(&self) -> Option<(u32, u32, u32, u32)> {
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
        for y in 0..self.height {
            for x in 0..self.width {
                if self.get(x, y) != TRANSPARENT {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        (x0 != u32::MAX).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
    }

    pub fn write_png(&self, path: &Path, palette: &Palette) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let file = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut encoder = png::Encoder::new(BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);

        let mut plte = Vec::with_capacity(768);
        for c in &palette.entries {
            plte.extend_from_slice(&[c.r, c.g, c.b]);
        }
        encoder.set_palette(plte);
        // Index 0 fully transparent, everything else opaque.
        let mut trns = vec![255u8; 256];
        trns[TRANSPARENT as usize] = 0;
        encoder.set_trns(trns);

        encoder
            .write_header()
            .map_err(|e| format!("{}: {e}", path.display()))?
            .write_image_data(&self.pixels)
            .map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// A sheet as read back off disk, with the palette it was stored with, so the
/// validator can prove the art was drawn against the palette we ship rather
/// than one an artist's tool invented on export.
pub struct LoadedSheet {
    pub image: Indexed,
    pub palette: Vec<Srgb>,
    pub transparent_index: Option<u8>,
}

pub fn read_png(path: &Path) -> Result<LoadedSheet, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("{}: {e}", path.display()))?;

    let info = reader.info();
    if info.color_type != png::ColorType::Indexed {
        return Err(format!(
            "{}: is {:?}, but sprites must be 8-bit indexed so player colour \
             can be remapped at draw time",
            path.display(),
            info.color_type
        ));
    }
    if info.bit_depth != png::BitDepth::Eight {
        return Err(format!(
            "{}: is {:?}, but sprites must be 8 bits per pixel",
            path.display(),
            info.bit_depth
        ));
    }

    let palette: Vec<Srgb> = info
        .palette
        .as_ref()
        .ok_or_else(|| format!("{}: indexed PNG with no PLTE chunk", path.display()))?
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| Srgb {
            r: c[0],
            g: c[1],
            b: c[2],
        })
        .collect();

    let transparent_index = info
        .trns
        .as_ref()
        .and_then(|t| t.iter().position(|&a| a == 0))
        .map(|i| i as u8);

    let (width, height) = (info.width, info.height);
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    buf.truncate(frame.buffer_size());

    Ok(LoadedSheet {
        image: Indexed {
            width,
            height,
            pixels: buf,
        },
        palette,
        transparent_index,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::PaletteSpec;

    fn palette() -> Palette {
        PaletteSpec::load(Path::new("../../assets/palette/ancient.ron"))
            .unwrap()
            .bake()
            .unwrap()
    }

    #[test]
    fn indexed_pngs_round_trip_with_their_palette() {
        let pal = palette();
        let mut img = Indexed::new(9, 5);
        img.set(1, 1, 17);
        img.set(4, 2, 240);
        img.set(8, 4, 254);

        let dir = std::env::temp_dir().join("atlas-image-round-trip");
        let path = dir.join("sheet.png");
        img.write_png(&path, &pal).unwrap();

        let back = read_png(&path).unwrap();
        assert_eq!(back.image.width, 9);
        assert_eq!(back.image.height, 5);
        assert_eq!(back.image.pixels, img.pixels);
        assert_eq!(back.palette.len(), 256);
        assert_eq!(back.palette[17], pal.entries[17]);
        assert_eq!(back.transparent_index, Some(0));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn content_bounds_finds_the_sprite_and_reports_empty_frames() {
        let mut img = Indexed::new(10, 10);
        assert_eq!(img.content_bounds(), None);
        img.set(3, 4, 20);
        img.set(6, 8, 20);
        assert_eq!(img.content_bounds(), Some((3, 4, 4, 5)));
    }

    #[test]
    fn crop_lifts_one_frame_out_of_a_sheet() {
        let mut img = Indexed::new(8, 4);
        img.set(5, 1, 99);
        let frame = img.crop(4, 0, 4, 4);
        assert_eq!(frame.width, 4);
        assert_eq!(frame.get(1, 1), 99);
        assert_eq!(frame.get(0, 0), 0);
    }
}
