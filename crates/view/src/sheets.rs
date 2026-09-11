//! Loading rendered sprite sets from `assets/sprites`.
//!
//! A set is a manifest (`name.ron`, written by `atlas compose`) beside an
//! 8-bit indexed PNG laid out one row per (animation, facing) and one column
//! per frame. This module reads both, checks the sheet was exported against
//! the palette this build draws with, and hands the frames to the atlas
//! packer. It knows nothing about kinds; `sprites.rs` decides which set draws
//! which kind.

use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::palette_table::ENTRIES;

/// Size classes from `docs/05` §2.3, mirrored from `tools/atlas`.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    /// Villager, infantry.
    Foot,
    /// Cavalry, chariot.
    Mounted,
    /// Elephant, siege.
    Heavy,
    /// House, Farm.
    SmallBuilding,
    /// Barracks, Storehouse.
    MediumBuilding,
    /// Town Center, Temple.
    LargeBuilding,
    /// Wonder.
    Wonder,
    /// Terrain pages.
    Terrain,
}

impl Class {
    /// Frame size at 1× zoom.
    pub fn size(self) -> (u32, u32) {
        match self {
            Class::Foot => (40, 48),
            Class::Mounted => (56, 56),
            Class::Heavy => (72, 72),
            Class::SmallBuilding => (64, 64),
            Class::MediumBuilding => (128, 96),
            Class::LargeBuilding => (192, 144),
            Class::Wonder => (384, 320),
            Class::Terrain => (64, 32),
        }
    }

    /// Whether the set has five facings or one.
    pub fn turns(self) -> bool {
        matches!(self, Class::Foot | Class::Mounted | Class::Heavy)
    }
}

/// The five authored facings, in sheet row order. The renderer's `facing8`
/// indices for these are 1..=5 in the same order.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Facing {
    /// South (`facing8` 1).
    S,
    /// South-west (2).
    SW,
    /// West (3).
    W,
    /// North-west (4).
    NW,
    /// North (5).
    N,
}

/// One animation's timing.
#[derive(Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Animation {
    /// Name: idle, walk, attack, death, decay, construction, rubble…
    pub name: String,
    /// Frame count.
    pub frames: u32,
    /// Milliseconds per frame.
    pub frame_ms: u32,
    /// Whether it repeats.
    pub loops: bool,
    /// Frame on which a hit lands, if any.
    pub impact: Option<u32>,
}

#[derive(Deserialize, Clone, PartialEq, Eq, Debug)]
struct AnchorOverride {
    animation: String,
    facing: Facing,
    frame: u32,
    anchor: (u32, u32),
}

/// The manifest as written by `atlas compose`; the RON type is `SpriteSet`.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename = "SpriteSet")]
struct Manifest {
    name: String,
    class: Class,
    scale: u32,
    palette: String,
    sheet: String,
    anchor: (u32, u32),
    animations: Vec<Animation>,
    #[serde(default)]
    anchor_overrides: Vec<AnchorOverride>,
}

/// A loaded sprite set: manifest plus the indexed pixels of its sheet.
#[derive(Clone, Debug)]
pub struct Sheet {
    /// Set name, e.g. `villager`.
    pub name: String,
    /// Size class.
    pub class: Class,
    /// Authored scale: pixels per 1× pixel.
    pub scale: u32,
    /// Animations in sheet row order.
    pub animations: Vec<Animation>,
    /// Ground-contact anchor in authored pixels, for every frame unless overridden.
    pub anchor: (u32, u32),
    anchor_overrides: Vec<AnchorOverride>,
    /// Sheet width in pixels.
    pub width: u32,
    /// Sheet height in pixels.
    pub height: u32,
    /// Palette indices, row-major.
    pub indices: Vec<u8>,
}

impl Sheet {
    /// Loads a set from its manifest path.
    pub fn load(manifest_path: &Path) -> Result<Sheet, String> {
        let text = std::fs::read_to_string(manifest_path)
            .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        let m: Manifest =
            ron::from_str(&text).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        if m.palette != crate::palette_table::NAME {
            return Err(format!(
                "{}: exported against palette '{}', this build draws with '{}'",
                manifest_path.display(),
                m.palette,
                crate::palette_table::NAME
            ));
        }
        if m.scale == 0 {
            return Err(format!(
                "{}: scale must be at least 1",
                manifest_path.display()
            ));
        }
        let sheet_path = manifest_path.with_file_name(&m.sheet);
        let (width, height, indices) = read_indexed_png(&sheet_path)?;
        let (fw, fh) = m.class.size();
        let cols = m.animations.iter().map(|a| a.frames).max().unwrap_or(0);
        let rows = m.animations.len() as u32 * if m.class.turns() { 5 } else { 1 };
        let want = (cols * fw * m.scale, rows * fh * m.scale);
        if (width, height) != want {
            return Err(format!(
                "{}: sheet is {width}×{height} but the manifest lays out {}×{}",
                sheet_path.display(),
                want.0,
                want.1
            ));
        }
        Ok(Sheet {
            name: m.name,
            class: m.class,
            scale: m.scale,
            animations: m.animations,
            anchor: m.anchor,
            anchor_overrides: m.anchor_overrides,
            width,
            height,
            indices,
        })
    }

    /// Facings this set has, as `facing8` indices.
    pub fn facings(&self) -> &'static [u8] {
        if self.class.turns() {
            &[1, 2, 3, 4, 5]
        } else {
            &[0]
        }
    }

    /// Frame size in authored pixels.
    pub fn frame_size(&self) -> (u32, u32) {
        let (w, h) = self.class.size();
        (w * self.scale, h * self.scale)
    }

    /// Pixel rectangle of a frame in the sheet: `(x, y, w, h)`.
    pub fn frame_rect(
        &self,
        animation: usize,
        facing_row: usize,
        frame: u32,
    ) -> (u32, u32, u32, u32) {
        let (fw, fh) = self.frame_size();
        let facings = self.facings().len();
        let row = animation * facings + facing_row;
        (frame * fw, row as u32 * fh, fw, fh)
    }

    /// Anchor for a frame after overrides, in authored pixels.
    pub fn anchor_for(&self, animation: usize, facing_row: usize, frame: u32) -> (u32, u32) {
        let anim = &self.animations[animation].name;
        let facing = [Facing::S, Facing::SW, Facing::W, Facing::NW, Facing::N][facing_row.min(4)];
        self.anchor_overrides
            .iter()
            .find(|o| &o.animation == anim && o.facing == facing && o.frame == frame)
            .map_or(self.anchor, |o| o.anchor)
    }

    /// Index at a sheet pixel.
    pub fn index_at(&self, x: u32, y: u32) -> u8 {
        self.indices[(y * self.width + x) as usize]
    }
}

/// Reads an 8-bit indexed PNG and checks its palette chunk is ours.
fn read_indexed_png(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut reader = png::Decoder::new(file)
        .read_info()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let info = reader.info();
    if info.color_type != png::ColorType::Indexed || info.bit_depth != png::BitDepth::Eight {
        return Err(format!(
            "{}: sprites must be 8-bit indexed PNGs",
            path.display()
        ));
    }
    let (width, height) = (info.width, info.height);
    if let Some(plte) = info.palette.as_deref() {
        for (i, rgb) in plte.chunks_exact(3).enumerate().take(256) {
            if i == 0 {
                continue; // index 0 is transparent; its colour is irrelevant
            }
            if rgb != ENTRIES[i] {
                return Err(format!(
                    "{}: palette entry {i} is #{:02x}{:02x}{:02x} but this build's palette says #{:02x}{:02x}{:02x}; run `atlas repalette`",
                    path.display(),
                    rgb[0],
                    rgb[1],
                    rgb[2],
                    ENTRIES[i][0],
                    ENTRIES[i][1],
                    ENTRIES[i][2]
                ));
            }
        }
    }
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    buf.truncate(frame.buffer_size());
    if buf.len() != (width * height) as usize {
        return Err(format!("{}: unexpected buffer size", path.display()));
    }
    Ok((width, height, buf))
}

/// The sprite directory: `assets/sprites` in the current directory or any
/// ancestor, so the game and its tools find the same art whether run from
/// the repository root, a crate directory, or an installed layout.
pub fn default_dir() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    for _ in 0..6 {
        let candidate = dir.join("assets").join("sprites");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Every manifest under `dir/*/*.ron`, sorted.
pub fn find_sets(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        if let Ok(files) = std::fs::read_dir(&p) {
            for f in files.flatten() {
                let fp = f.path();
                if fp.extension().is_some_and(|x| x == "ron") {
                    out.push(fp);
                }
            }
        }
    }
    out.sort();
    out
}

/// Loads every set under `dir`, reporting each failure by path rather than
/// stopping at the first: a bad sheet should cost one sprite, not the game.
pub fn load_all(dir: &Path) -> (Vec<Sheet>, Vec<String>) {
    let mut sheets = Vec::new();
    let mut errors = Vec::new();
    for path in find_sets(dir) {
        match Sheet::load(&path) {
            Ok(s) => sheets.push(s),
            Err(e) => errors.push(e),
        }
    }
    (sheets, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn the_committed_villager_loads_and_lays_out() {
        let dir = repo_root().join("assets/sprites");
        let (sheets, errors) = load_all(&dir);
        assert!(errors.is_empty(), "{errors:?}");
        let v = sheets
            .iter()
            .find(|s| s.name == "villager")
            .expect("villager set");
        assert_eq!(v.class, Class::Foot);
        assert_eq!(v.scale, 2);
        assert_eq!(v.frame_size(), (80, 96));
        assert_eq!(v.facings(), &[1, 2, 3, 4, 5]);
        assert_eq!(
            v.animations
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>(),
            ["idle", "walk", "attack", "death", "decay"]
        );
        let (x, y, w, h) = v.frame_rect(1, 2, 3);
        assert_eq!((x, y, w, h), (240, (5 + 2) * 96, 80, 96));
        assert!(x + w <= v.width && y + h <= v.height);
        // Frames are drawn in the palette: some player colour, some skin, no error index.
        let mut player = 0;
        let mut error = 0;
        for i in &v.indices {
            if (240..=247).contains(i) {
                player += 1;
            }
            if *i == 255 {
                error += 1;
            }
        }
        assert!(player > 100, "villager should carry player colour");
        assert_eq!(error, 0, "no pixel may use the error index");
        assert_eq!(v.anchor_for(0, 0, 0), v.anchor);
    }

    #[test]
    fn missing_directory_is_empty_not_fatal() {
        let (sheets, errors) = load_all(Path::new("/definitely/not/here"));
        assert!(sheets.is_empty() && errors.is_empty());
    }
}
