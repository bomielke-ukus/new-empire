//! Turning a render into a conformant sprite.
//!
//! This is the back half of the render-to-sprite pipeline in
//! `docs/08-art-production.md`: a 3D frame comes out of the renderer as RGBA,
//! and has to arrive in the game as palette indices with player colour in
//! 240..=247. Two problems have to be solved on the way.
//!
//! **Downsampling.** Rendering at the authoring size directly gives ragged
//! edges. Rendering larger and box-filtering down is what produces the soft,
//! dense look of a late-90s pre-rendered sprite, and it is why the original's
//! units read cleanly at 40 px tall.
//!
//! **Player colour.** A render has no notion of palette indices, so the model's
//! player-coloured surfaces are textured in a key hue that appears nowhere else
//! in the palette. Pixels near that hue are mapped into the reserved ramp *by
//! lightness*, preserving the shading the renderer produced; everything else is
//! matched to the nearest ordinary palette entry. Without this step a render
//! either has no player colour or has it baked in one fixed colour.

use crate::colour::{Linear, Oklab, Srgb};
use crate::image::Indexed;
use crate::palette::{Palette, PLAYER_RAMP_LEN, PLAYER_RAMP_START, TRANSPARENT};
use std::path::Path;

pub struct Options {
    /// Render-to-authoring downsample factor. 1 renders at final size.
    pub downsample: u32,
    /// The hue used on the model for player-coloured surfaces.
    pub player_key: Srgb,
    /// How close a pixel's hue must be to the key to count as player colour.
    /// Generous, because the renderer will have shaded it.
    pub player_tolerance: f64,
    /// Alpha at or below this is transparent. Anything above is opaque: an
    /// indexed sprite has no partial alpha to fall back on.
    pub alpha_cutoff: u8,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            downsample: 2,
            // Pure magenta: not in the ancient palette, and loud enough in a
            // 3D viewport that nobody paints it by accident.
            player_key: Srgb {
                r: 255,
                g: 0,
                b: 255,
            },
            player_tolerance: 0.25,
            alpha_cutoff: 127,
        }
    }
}

struct Rgba {
    width: u32,
    height: u32,
    /// Four bytes per pixel.
    pixels: Vec<u8>,
}

fn read_rgba(path: &Path) -> Result<Rgba, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut reader = png::Decoder::new(file)
        .read_info()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let (width, height) = (reader.info().width, reader.info().height);
    let colour = reader.info().color_type;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    buf.truncate(frame.buffer_size());

    let pixels = match colour {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        other => {
            return Err(format!(
                "{}: renders must be RGB or RGBA, found {other:?}",
                path.display()
            ))
        }
    };
    Ok(Rgba {
        width,
        height,
        pixels,
    })
}

/// Box-filters `factor`×`factor` blocks down to one pixel, in linear light.
///
/// Averaging in sRGB darkens edges; averaging in linear light does not. Fully
/// transparent samples are excluded from the colour average so a sprite's edge
/// does not average toward whatever the renderer left in the background.
fn downsample(src: &Rgba, factor: u32) -> Rgba {
    if factor <= 1 {
        return Rgba {
            width: src.width,
            height: src.height,
            pixels: src.pixels.clone(),
        };
    }
    let (w, h) = (src.width / factor, src.height / factor);
    let mut out = vec![0u8; (w * h * 4) as usize];

    for y in 0..h {
        for x in 0..w {
            let (mut r, mut g, mut b, mut a, mut covered) = (0.0, 0.0, 0.0, 0.0f64, 0u32);
            for sy in 0..factor {
                for sx in 0..factor {
                    let i = (((y * factor + sy) * src.width + x * factor + sx) * 4) as usize;
                    let alpha = src.pixels[i + 3] as f64 / 255.0;
                    a += alpha;
                    if alpha > 0.0 {
                        let lin = Linear::from(Srgb {
                            r: src.pixels[i],
                            g: src.pixels[i + 1],
                            b: src.pixels[i + 2],
                        });
                        r += lin.r * alpha;
                        g += lin.g * alpha;
                        b += lin.b * alpha;
                        covered += 1;
                    }
                }
            }
            let n = (factor * factor) as f64;
            let weight = if a > 0.0 { a } else { 1.0 };
            let _ = covered;
            let colour = Srgb::from(Linear {
                r: r / weight,
                g: g / weight,
                b: b / weight,
            });
            let i = ((y * w + x) * 4) as usize;
            out[i] = colour.r;
            out[i + 1] = colour.g;
            out[i + 2] = colour.b;
            out[i + 3] = ((a / n) * 255.0).round() as u8;
        }
    }
    Rgba {
        width: w,
        height: h,
        pixels: out,
    }
}

/// How far `colour` is from the key hue, ignoring lightness.
fn hue_distance(colour: Srgb, key: Srgb) -> f64 {
    let c = Oklab::from(Linear::from(colour));
    let k = Oklab::from(Linear::from(key));
    // Compare chroma direction only. A shaded magenta and a bright magenta are
    // the same surface; a grey is not, and falls out because its chroma is ~0.
    let cc = (c.a * c.a + c.b * c.b).sqrt();
    let kc = (k.a * k.a + k.b * k.b).sqrt();
    if cc < 0.02 || kc < 0.02 {
        return f64::MAX;
    }
    let (ca, cb) = (c.a / cc, c.b / cc);
    let (ka, kb) = (k.a / kc, k.b / kc);
    ((ca - ka).powi(2) + (cb - kb).powi(2)).sqrt()
}

/// Maps a lightness in 0..1 onto a step of the reserved player ramp.
fn player_step(lightness: f64) -> u8 {
    let step = (lightness * PLAYER_RAMP_LEN as f64).floor() as i64;
    PLAYER_RAMP_START + step.clamp(0, PLAYER_RAMP_LEN as i64 - 1) as u8
}

/// Quantises one render to indices against `palette`.
pub fn quantize(path: &Path, palette: &Palette, opts: &Options) -> Result<Indexed, String> {
    let src = read_rgba(path)?;
    if opts.downsample > 1
        && (src.width % opts.downsample != 0 || src.height % opts.downsample != 0)
    {
        return Err(format!(
            "{}: is {}x{}, which does not divide by the downsample factor {}. \
             Render at a whole multiple of the authoring size.",
            path.display(),
            src.width,
            src.height,
            opts.downsample
        ));
    }
    let src = downsample(&src, opts.downsample);

    let mut out = Indexed::new(src.width, src.height);
    // Nearest-index lookups repeat heavily across a sprite; cache them.
    let mut cache: std::collections::HashMap<(u8, u8, u8), u8> = std::collections::HashMap::new();

    for y in 0..src.height {
        for x in 0..src.width {
            let i = ((y * src.width + x) * 4) as usize;
            if src.pixels[i + 3] <= opts.alpha_cutoff {
                out.set(x, y, TRANSPARENT);
                continue;
            }
            let colour = Srgb {
                r: src.pixels[i],
                g: src.pixels[i + 1],
                b: src.pixels[i + 2],
            };
            let index = if hue_distance(colour, opts.player_key) <= opts.player_tolerance {
                player_step(Oklab::from(Linear::from(colour)).l)
            } else {
                *cache
                    .entry((colour.r, colour.g, colour.b))
                    .or_insert_with(|| palette.nearest(colour))
            };
            out.set(x, y, index);
        }
    }
    Ok(out)
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

    /// Writes a small RGBA PNG to feed the quantiser.
    fn write_rgba(path: &Path, w: u32, h: u32, pixels: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let file = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .unwrap()
            .write_image_data(pixels)
            .unwrap();
    }

    #[test]
    fn transparent_render_pixels_become_index_zero() {
        let dir = std::env::temp_dir().join("atlas-quantize-alpha");
        let path = dir.join("r.png");
        // Two pixels: one clear, one opaque brown.
        write_rgba(&path, 2, 1, &[0, 0, 0, 0, 0x8f, 0x60, 0x35, 255]);
        let out = quantize(
            &path,
            &palette(),
            &Options {
                downsample: 1,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(out.get(0, 0), TRANSPARENT);
        assert_ne!(out.get(1, 0), TRANSPARENT);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_key_hue_becomes_player_colour_and_keeps_its_shading() {
        let dir = std::env::temp_dir().join("atlas-quantize-player");
        let path = dir.join("r.png");
        // Dark, mid and bright magenta: one surface under three light levels.
        write_rgba(
            &path,
            3,
            1,
            &[40, 0, 40, 255, 140, 0, 140, 255, 250, 40, 250, 255],
        );
        let out = quantize(
            &path,
            &palette(),
            &Options {
                downsample: 1,
                ..Default::default()
            },
        )
        .unwrap();
        for x in 0..3 {
            assert!(
                Palette::is_player_index(out.get(x, 0)),
                "pixel {x} should be player colour, got {}",
                out.get(x, 0)
            );
        }
        // Shading must survive: darker in, darker step out.
        assert!(out.get(0, 0) < out.get(1, 0));
        assert!(out.get(1, 0) < out.get(2, 0));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ordinary_colours_never_land_in_the_player_ramp() {
        let dir = std::env::temp_dir().join("atlas-quantize-ordinary");
        let path = dir.join("r.png");
        // Greens, browns and greys: nothing here is the key hue.
        write_rgba(
            &path,
            4,
            1,
            &[
                0x7b, 0xa8, 0x4a, 255, 0x8f, 0x60, 0x35, 255, 0x9f, 0xa3, 0xab, 255, 0x2a, 0x63,
                0x82, 255,
            ],
        );
        let out = quantize(
            &path,
            &palette(),
            &Options {
                downsample: 1,
                ..Default::default()
            },
        )
        .unwrap();
        for x in 0..4 {
            assert!(!Palette::is_player_index(out.get(x, 0)));
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn downsampling_halves_the_render_and_rejects_ragged_sizes() {
        let dir = std::env::temp_dir().join("atlas-quantize-downsample");
        let path = dir.join("r.png");
        write_rgba(&path, 4, 2, &[0x7b, 0xa8, 0x4a, 255].repeat(8));
        let out = quantize(
            &path,
            &palette(),
            &Options {
                downsample: 2,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!((out.width, out.height), (2, 1));

        let odd = dir.join("odd.png");
        write_rgba(&odd, 3, 2, &[0x7b, 0xa8, 0x4a, 255].repeat(6));
        let err = quantize(
            &odd,
            &palette(),
            &Options {
                downsample: 2,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.contains("does not divide"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
