//! The ground's grain: a render of one tile of each ground type, turned into
//! the greyscale layer the game multiplies over its blended ground colours
//! (`crates/view/src/detail.rs`).
//!
//! The sheet is one layer per ground type, stacked top to bottom in
//! [`GROUNDS`] order, which is `sim::Terrain`'s. A layer is a tile's diamond
//! at the art's authoring scale, inscribed in its 128 x 64 rectangle; a
//! texel of 128 leaves the ground's colour as it is. Colour does not
//! survive, only light and shade: each layer is set to average 128 over the
//! diamond, so the ground keeps its colour, and to vary by its ground's
//! [`CONTRAST`], so the grain is the same strength whatever the render's.

use crate::quantize::read_rgba;
use std::path::Path;

/// The ground types, in `sim::Terrain` order, as `tools/render/slice.py`
/// names their subjects (`ground_<name>`).
pub const GROUNDS: [&str; 8] = [
    "grass",
    "dirt",
    "desert",
    "sand",
    "shallow_water",
    "deep_water",
    "forest_floor",
    "snow",
];

/// How much each ground's grain varies: the standard deviation of its factor
/// about 1. Strongest where ground is busy, faint on water and snow, and
/// never so strong that a unit on it stops reading.
pub const CONTRAST: [f32; 8] = [0.12, 0.10, 0.07, 0.07, 0.05, 0.04, 0.12, 0.05];

/// A layer's size, texels.
pub const LAYER_W: u32 = 128;
/// A layer's size, texels.
pub const LAYER_H: u32 = 64;
/// The texel that leaves a colour as it is.
pub const NEUTRAL: u8 = 128;
/// The darkest and lightest a texel may make the ground.
const RANGE: (f32, f32) = (0.55, 1.45);

/// [`RANGE`] in texels.
fn bounds() -> (u8, u8) {
    (
        (RANGE.0 * NEUTRAL as f32).ceil() as u8,
        (RANGE.1 * NEUTRAL as f32).floor() as u8,
    )
}

/// True if texel `(x, y)` of a layer is inside the tile's diamond.
pub fn inside(x: u32, y: u32) -> bool {
    let u = (x as f32 + 0.5) / LAYER_W as f32 * 2.0 - 1.0;
    let v = (y as f32 + 0.5) / LAYER_H as f32 * 2.0 - 1.0;
    u.abs() + v.abs() <= 1.0
}

/// One layer from a render of ground `index`, any whole multiple of the
/// layer's size: averaged down, its brightness made a factor about 1 at the
/// ground's contrast, the texels outside the diamond neutral.
pub fn layer(width: u32, height: u32, rgba: &[u8], index: usize) -> Result<Vec<u8>, String> {
    let k = width / LAYER_W;
    if k == 0 || width != LAYER_W * k || height != LAYER_H * k {
        return Err(format!(
            "a ground render is a multiple of {LAYER_W}x{LAYER_H}; this one is {width}x{height}"
        ));
    }
    let luma = |x: u32, y: u32| {
        let i = ((y * width + x) * 4) as usize;
        let c = |v: u8| v as f32 / 255.0;
        0.2126 * c(rgba[i]) + 0.7152 * c(rgba[i + 1]) + 0.0722 * c(rgba[i + 2])
    };
    let mut lum = vec![0.0f32; (LAYER_W * LAYER_H) as usize];
    for y in 0..LAYER_H {
        for x in 0..LAYER_W {
            let mut sum = 0.0;
            for dy in 0..k {
                for dx in 0..k {
                    sum += luma(x * k + dx, y * k + dy);
                }
            }
            lum[(y * LAYER_W + x) as usize] = sum / (k * k) as f32;
        }
    }
    let cells: Vec<f32> = (0..LAYER_H)
        .flat_map(|y| (0..LAYER_W).map(move |x| (x, y)))
        .filter(|&(x, y)| inside(x, y))
        .map(|(x, y)| lum[(y * LAYER_W + x) as usize])
        .collect();
    let mean = cells.iter().sum::<f32>() / cells.len() as f32;
    if mean <= 0.0 {
        return Err("the render is black".to_string());
    }
    let sd =
        (cells.iter().map(|l| (l / mean - 1.0).powi(2)).sum::<f32>() / cells.len() as f32).sqrt();
    let gain = if sd > 1e-6 { CONTRAST[index] / sd } else { 0.0 };
    let mut out = vec![NEUTRAL; (LAYER_W * LAYER_H) as usize];
    for y in 0..LAYER_H {
        for x in 0..LAYER_W {
            if inside(x, y) {
                let i = (y * LAYER_W + x) as usize;
                let f = (1.0 + (lum[i] / mean - 1.0) * gain).clamp(RANGE.0, RANGE.1);
                out[i] = (f * NEUTRAL as f32).round() as u8;
            }
        }
    }
    // Clamping and rounding move the mean a little; put it back on 128.
    let total: i64 = (0..out.len())
        .filter(|&i| inside(i as u32 % LAYER_W, i as u32 / LAYER_W))
        .map(|i| out[i] as i64 - NEUTRAL as i64)
        .sum();
    let shift = (total as f32 / cells.len() as f32).round() as i32;
    let (lo, hi) = bounds();
    for (i, t) in out.iter_mut().enumerate() {
        if inside(i as u32 % LAYER_W, i as u32 / LAYER_W) {
            *t = (*t as i32 - shift).clamp(lo as i32, hi as i32) as u8;
        }
    }
    Ok(out)
}

/// Builds the sheet from `renders/ground_<name>/ground_S_00.png` for every
/// ground type and writes it to `out`.
pub fn compose(renders: &Path, out: &Path) -> Result<(), String> {
    let mut texels = Vec::with_capacity((LAYER_W * LAYER_H) as usize * GROUNDS.len());
    for (i, name) in GROUNDS.iter().enumerate() {
        let path = renders.join(format!("ground_{name}/ground_S_00.png"));
        if !path.exists() {
            return Err(format!(
                "{} is missing: render it with `scripts/render-sprites.sh ground_{name}`",
                path.display()
            ));
        }
        let r = read_rgba(&path)?;
        let l = layer(r.width, r.height, &r.pixels, i).map_err(|e| format!("{name}: {e}"))?;
        texels.extend(l);
    }
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let file = std::fs::File::create(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let height = LAYER_H * GROUNDS.len() as u32;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), LAYER_W, height);
    enc.set_color(png::ColorType::Grayscale);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| w.write_image_data(&texels))
        .map_err(|e| format!("{}: {e}", out.display()))
}

/// What is wrong with the sheet at `path`, if anything: its size, and each
/// layer's mean and range.
pub fn check(path: &Path) -> Result<Vec<String>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut reader = png::Decoder::new(file)
        .read_info()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let (w, h) = (reader.info().width, reader.info().height);
    let colour = reader.info().color_type;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    reader
        .next_frame(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut problems = Vec::new();
    let want = (LAYER_W, LAYER_H * GROUNDS.len() as u32);
    if colour != png::ColorType::Grayscale || (w, h) != want {
        problems.push(format!(
            "is {w}x{h} {colour:?}; it should be {}x{} greyscale, a layer per ground type",
            want.0, want.1
        ));
        return Ok(problems);
    }
    let (lo, hi) = bounds();
    for (i, name) in GROUNDS.iter().enumerate() {
        let layer = &buf[(i as u32 * LAYER_W * LAYER_H) as usize..][..(LAYER_W * LAYER_H) as usize];
        let cells: Vec<u8> = (0..LAYER_H)
            .flat_map(|y| (0..LAYER_W).map(move |x| (x, y)))
            .filter(|&(x, y)| inside(x, y))
            .map(|(x, y)| layer[(y * LAYER_W + x) as usize])
            .collect();
        let mean = cells.iter().map(|&t| t as f32).sum::<f32>() / cells.len() as f32;
        if (mean - NEUTRAL as f32).abs() > 2.0 {
            problems.push(format!(
                "{name}'s layer averages {mean:.1}, not {NEUTRAL}: it would change the ground's colour"
            ));
        }
        if cells.iter().any(|&t| t < lo || t > hi) {
            problems.push(format!(
                "{name}'s layer goes outside {lo}..={hi}, past what the grain may do to the ground"
            ));
        }
    }
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layer_averages_neutral_at_its_contrast_and_is_neutral_outside_the_diamond() {
        // A render of stripes, twice the layer's size.
        let (w, h) = (LAYER_W * 2, LAYER_H * 2);
        let mut rgba = Vec::new();
        for y in 0..h {
            for _ in 0..w {
                let v = if (y / 8) % 2 == 0 { 90 } else { 160 };
                rgba.extend([v, v, v, 255]);
            }
        }
        let l = layer(w, h, &rgba, 0).unwrap();
        let cells: Vec<f32> = (0..LAYER_H)
            .flat_map(|y| (0..LAYER_W).map(move |x| (x, y)))
            .filter(|&(x, y)| inside(x, y))
            .map(|(x, y)| l[(y * LAYER_W + x) as usize] as f32)
            .collect();
        let mean = cells.iter().sum::<f32>() / cells.len() as f32;
        assert!((mean - 128.0).abs() <= 1.0, "{mean}");
        let sd = (cells.iter().map(|c| (c / mean - 1.0).powi(2)).sum::<f32>() / cells.len() as f32)
            .sqrt();
        assert!((sd - CONTRAST[0]).abs() < 0.02, "{sd}");
        assert_eq!(l[0], NEUTRAL, "a corner is outside the diamond");
        assert!(layer(100, 50, &rgba, 0).is_err());
    }

    #[test]
    fn the_diamond_is_the_tile() {
        assert!(inside(LAYER_W / 2, LAYER_H / 2));
        assert!(inside(LAYER_W / 2, 0) && inside(LAYER_W - 2, LAYER_H / 2 - 1));
        assert!(!inside(0, 0) && !inside(LAYER_W - 1, LAYER_H - 1));
    }
}
