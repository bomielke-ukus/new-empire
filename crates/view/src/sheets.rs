//! Loading rendered sprite sets from `assets/sprites`.
//!
//! A set is a manifest (`name.ron`, written by `atlas compose`) beside an
//! 8-bit indexed PNG laid out one row per (animation, facing) and one column
//! per frame. This module reads both, checks the sheet was exported against
//! the palette this build draws with, and hands the frames to the atlas
//! packer. It knows nothing about kinds; `sprites.rs` decides which set draws
//! which kind.
//!
//! A colour set (`docs/look/colour-sets.md`, under `assets/look`) is the
//! same manifest with `colour: true`: its sheet is full-colour RGBA with
//! straight alpha, and an optional greyscale mask beside it says where the
//! owner's colour goes. It replaces the indexed set of its name.

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
    /// Boats, drawn by the middle of the hull.
    Ship,
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
            Class::Ship => (80, 80),
            Class::SmallBuilding => (64, 64),
            Class::MediumBuilding => (128, 96),
            Class::LargeBuilding => (192, 144),
            Class::Wonder => (384, 320),
            Class::Terrain => (64, 32),
        }
    }

    /// Whether the set has five facings or one.
    pub fn turns(self) -> bool {
        matches!(
            self,
            Class::Foot | Class::Mounted | Class::Heavy | Class::Ship
        )
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
    /// A full-colour set rather than an indexed one.
    #[serde(default)]
    colour: bool,
    /// The colour set's player-colour mask, beside the sheet.
    #[serde(default)]
    mask: Option<String>,
    /// The cell size in authored pixels, when the class's is too small.
    #[serde(default)]
    frame: Option<(u32, u32)>,
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
    /// Palette indices, row-major; empty for a colour set.
    pub indices: Vec<u8>,
    /// A colour set's pixels, RGBA with straight alpha, row-major; empty
    /// for an indexed set.
    pub rgba: Vec<u8>,
    /// A colour set's player-colour mask, a byte a pixel (255 takes the
    /// owner's colour); empty when nothing does, or for an indexed set.
    pub mask: Vec<u8>,
    /// Whether this is a colour set (`docs/look/colour-sets.md`).
    pub colour: bool,
    /// The cell size in authored pixels, if the manifest gives one.
    frame: Option<(u32, u32)>,
}

impl Sheet {
    /// Loads a set from its manifest path.
    pub fn load(manifest_path: &Path) -> Result<Sheet, String> {
        let text = std::fs::read_to_string(manifest_path)
            .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        let m: Manifest =
            ron::from_str(&text).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        // A colour set's pixels are their own colours: no palette to match.
        if !m.colour && m.palette != crate::palette_table::NAME {
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
        if m.frame.is_some_and(|(w, h)| w == 0 || h == 0) {
            return Err(format!(
                "{}: frame must be at least 1x1",
                manifest_path.display()
            ));
        }
        let sheet_path = manifest_path.with_file_name(&m.sheet);
        let (width, height, indices, rgba, mask) = if m.colour {
            let (w, h, rgba) = read_rgba_png(&sheet_path)?;
            let mask = match &m.mask {
                Some(name) => {
                    let mask_path = manifest_path.with_file_name(name);
                    let (mw, mh, mask) = read_grey_png(&mask_path)?;
                    if (mw, mh) != (w, h) {
                        return Err(format!(
                            "{}: mask is {mw}×{mh} but the sheet is {w}×{h}",
                            mask_path.display()
                        ));
                    }
                    mask
                }
                None => Vec::new(),
            };
            (w, h, Vec::new(), rgba, mask)
        } else {
            let (w, h, indices) = read_indexed_png(&sheet_path)?;
            (w, h, indices, Vec::new(), Vec::new())
        };
        let (fw, fh) = m
            .frame
            .unwrap_or_else(|| (m.class.size().0 * m.scale, m.class.size().1 * m.scale));
        let cols = m.animations.iter().map(|a| a.frames).max().unwrap_or(0);
        let rows = m.animations.len() as u32 * if m.class.turns() { 5 } else { 1 };
        let want = (cols * fw, rows * fh);
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
            rgba,
            mask,
            colour: m.colour,
            frame: m.frame,
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

    /// Frame size in authored pixels: the manifest's `frame` if it gives
    /// one, else the class's size times the scale.
    pub fn frame_size(&self) -> (u32, u32) {
        self.frame.unwrap_or_else(|| {
            let (w, h) = self.class.size();
            (w * self.scale, h * self.scale)
        })
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

    /// Index at a sheet pixel; 0 for a colour set.
    pub fn index_at(&self, x: u32, y: u32) -> u8 {
        self.indices
            .get((y * self.width + x) as usize)
            .copied()
            .unwrap_or(0)
    }

    /// RGBA at a sheet pixel of a colour set; transparent for an indexed one.
    pub fn rgba_at(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y * self.width + x) as usize * 4;
        self.rgba
            .get(i..i + 4)
            .map_or([0; 4], |p| [p[0], p[1], p[2], p[3]])
    }

    /// The player-colour mask at a sheet pixel; 0 where there is none.
    pub fn mask_at(&self, x: u32, y: u32) -> u8 {
        self.mask
            .get((y * self.width + x) as usize)
            .copied()
            .unwrap_or(0)
    }

    /// Whether anything is drawn at a sheet pixel: a non-transparent
    /// index, or for a colour set any alpha at all (a soft edge or a
    /// shadow is drawn too).
    pub fn drawn_at(&self, x: u32, y: u32) -> bool {
        if self.colour {
            self.rgba_at(x, y)[3] != 0
        } else {
            self.index_at(x, y) != 0
        }
    }
}

/// Decodes a PNG to 8 bits a channel, palettes and low bit depths
/// expanded: its size, colour type and bytes.
fn read_png8(path: &Path) -> Result<(u32, u32, png::ColorType, Vec<u8>), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    buf.truncate(frame.buffer_size());
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => {
            return Err(format!("{}: the palette did not expand", path.display()))
        }
    };
    if frame.bit_depth != png::BitDepth::Eight
        || buf.len() != (frame.width * frame.height * channels) as usize
    {
        return Err(format!("{}: unexpected buffer size", path.display()));
    }
    Ok((frame.width, frame.height, frame.color_type, buf))
}

/// Reads a colour set's sheet as RGBA8 with straight alpha. An RGB or grey
/// sheet is opaque; a grey one with alpha keeps it.
pub(crate) fn read_rgba_png(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let (w, h, colour, buf) = read_png8(path)?;
    let rgba = match colour {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        _ => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
    };
    Ok((w, h, rgba))
}

/// Reads a colour set's mask as a byte a pixel. A grey mask is taken as it
/// is (its alpha, if any, ignored); a colour one by its luma.
pub(crate) fn read_grey_png(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let (w, h, colour, buf) = read_png8(path)?;
    let luma =
        |p: &[u8]| ((299 * p[0] as u32 + 587 * p[1] as u32 + 114 * p[2] as u32 + 500) / 1000) as u8;
    let grey = match colour {
        png::ColorType::Grayscale => buf,
        png::ColorType::GrayscaleAlpha => buf.as_chunks::<2>().0.iter().map(|p| p[0]).collect(),
        png::ColorType::Rgb => buf.as_chunks::<3>().0.iter().map(|p| luma(p)).collect(),
        _ => buf.as_chunks::<4>().0.iter().map(|p| luma(p)).collect(),
    };
    Ok((w, h, grey))
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
        for (i, rgb) in plte.as_chunks::<3>().0.iter().enumerate().take(256) {
            if i == 0 {
                continue; // index 0 is transparent; its colour is irrelevant
            }
            if *rgb != ENTRIES[i] {
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

/// The sprite directory: the first of [`candidates`] that exists. An
/// installed game finds the art it shipped with, beside its binary or in
/// its bundle's `Resources`; run from the repository, the game and its
/// tools find `assets/sprites` from the root, a crate directory or
/// `target/release` alike.
pub fn default_dir() -> Option<PathBuf> {
    first_dir("sprites")
}

/// The colour sets' directory, `assets/look`, found as [`default_dir`]
/// finds `assets/sprites`. Read only when the new art is switched on.
pub fn look_dir() -> Option<PathBuf> {
    first_dir("look")
}

/// The first of [`candidates_for`] `leaf` that exists.
fn first_dir(leaf: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok();
    let cwd = std::env::current_dir().ok();
    candidates_for(exe.as_deref(), cwd.as_deref(), leaf)
        .into_iter()
        .find(|c| c.is_dir())
}

/// Where `assets/sprites` is looked for, in order, for a binary at `exe`
/// run from `cwd`: beside the binary; in a macOS bundle's
/// `Contents/Resources`, which sits beside the binary's `Contents/MacOS`;
/// then `cwd` and up to five ancestors. Pure, so the layouts are testable.
pub fn candidates(exe: Option<&Path>, cwd: Option<&Path>) -> Vec<PathBuf> {
    candidates_for(exe, cwd, "sprites")
}

/// Where `assets/<leaf>` is looked for, in the order of [`candidates`].
pub fn candidates_for(exe: Option<&Path>, cwd: Option<&Path>, leaf: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(bin_dir) = exe.and_then(Path::parent) {
        out.push(bin_dir.join("assets").join(leaf));
        if let Some(contents) = bin_dir.parent() {
            out.push(contents.join("Resources").join("assets").join(leaf));
        }
    }
    if let Some(cwd) = cwd {
        let mut dir = cwd.to_path_buf();
        for _ in 0..6 {
            out.push(dir.join("assets").join(leaf));
            if !dir.pop() {
                break;
            }
        }
    }
    out
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

/// The sets the game draws with: every indexed set under `sprites`, and
/// with the new art on (`Settings::new_art`) every colour set under
/// `look` after them, each to replace the indexed set of its name when the
/// atlas is built ([`crate::Atlas::with_sheets`]). With it off, `look` is
/// not read at all.
pub fn load_art(
    sprites: Option<&Path>,
    look: Option<&Path>,
    new_art: bool,
) -> (Vec<Sheet>, Vec<String>) {
    let (mut sheets, mut errors) = sprites.map(load_all).unwrap_or_default();
    if new_art {
        if let Some(dir) = look {
            let (colour, more) = load_all(dir);
            sheets.extend(colour);
            errors.extend(more);
        }
    }
    (sheets, errors)
}

/// A tiny colour set for tests, written to disk and loaded as the game
/// loads one.
#[cfg(test)]
pub(crate) mod fixture {
    use std::path::{Path, PathBuf};

    /// The fixture's cell: 8 x 6 authored pixels at scale 2.
    pub const FRAME: (u32, u32) = (8, 6);
    /// Its anchor in the cell.
    pub const ANCHOR: (u32, u32) = (4, 5);
    /// Grey cloth, lit at 128, fully the owner's colour.
    pub const CLOTH: [u8; 4] = [128, 128, 128, 255];
    /// Ordinary red, no player colour.
    pub const RED: [u8; 4] = [200, 30, 20, 255];
    /// The object's shadow: translucent black.
    pub const SHADOW: [u8; 4] = [0, 0, 0, 128];
    /// Bright grey, half the owner's colour.
    pub const HALF: [u8; 4] = [200, 200, 200, 255];
    /// Ordinary green.
    pub const GREEN: [u8; 4] = [20, 160, 40, 255];

    /// The cell's pixels and mask: transparent but for a 4 x 2 block at
    /// (2, 2), whose top row is [`CLOTH`], [`RED`], [`SHADOW`], [`HALF`]
    /// and bottom row [`GREEN`].
    pub fn pixels() -> (Vec<u8>, Vec<u8>) {
        let (w, h) = FRAME;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        let mut mask = vec![0u8; (w * h) as usize];
        let mut put = |x: u32, y: u32, c: [u8; 4], m: u8| {
            let i = (y * w + x) as usize;
            rgba[i * 4..i * 4 + 4].copy_from_slice(&c);
            mask[i] = m;
        };
        for (x, c, m) in [(2, CLOTH, 255), (3, RED, 0), (4, SHADOW, 0), (5, HALF, 128)] {
            put(x, 2, c, m);
        }
        for x in 2..6 {
            put(x, 3, GREEN, 0);
        }
        (rgba, mask)
    }

    /// Writes a PNG of `colour` type, 8 bits a channel.
    pub fn write_png(path: &Path, w: u32, h: u32, colour: png::ColorType, data: &[u8]) {
        let file = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(colour);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(data).unwrap();
    }

    /// A fresh scratch directory for one test.
    pub fn scratch(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ne-look-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Writes the colour set `name` under `dir/name/`, with its mask, and
    /// returns the manifest's path.
    pub fn write_set(dir: &Path, name: &str) -> PathBuf {
        let set = dir.join(name);
        std::fs::create_dir_all(&set).unwrap();
        let (rgba, mask) = pixels();
        let (w, h) = FRAME;
        write_png(
            &set.join(format!("{name}.png")),
            w,
            h,
            png::ColorType::Rgba,
            &rgba,
        );
        write_png(
            &set.join(format!("{name}.mask.png")),
            w,
            h,
            png::ColorType::Grayscale,
            &mask,
        );
        let manifest = set.join(format!("{name}.ron"));
        std::fs::write(
            &manifest,
            format!(
                r#"SpriteSet(
    name: "{name}",
    class: SmallBuilding,
    scale: 2,
    palette: "rgba",
    sheet: "{name}.png",
    anchor: ({}, {}),
    animations: [
        (name: "idle", frames: 1, frame_ms: 160, loops: true, impact: None),
    ],
    anchor_overrides: [],
    colour: true,
    mask: Some("{name}.mask.png"),
    frame: Some(({w}, {h})),
)"#,
                ANCHOR.0, ANCHOR.1
            ),
        )
        .unwrap();
        manifest
    }

    /// The fixture `house`, loaded.
    pub fn house(test: &str) -> super::Sheet {
        let dir = scratch(test);
        super::Sheet::load(&write_set(&dir, "house")).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// A double-clicked bundle runs from `/`, where no ancestor holds
    /// the art: the binary's own bundle is looked in first, then a plain
    /// install beside the binary, then the working directory and its
    /// ancestors, which is how a checkout finds the committed sets.
    #[test]
    fn a_bundle_and_an_install_find_their_own_art_before_the_working_directory() {
        let exe = Path::new("/Applications/Brenden's Empires.app/Contents/MacOS/new-empire");
        let c = candidates(Some(exe), Some(Path::new("/")));
        assert_eq!(
            c[0],
            Path::new("/Applications/Brenden's Empires.app/Contents/MacOS/assets/sprites")
        );
        assert_eq!(
            c[1],
            Path::new("/Applications/Brenden's Empires.app/Contents/Resources/assets/sprites")
        );
        assert_eq!(c[2], Path::new("/assets/sprites"));
        assert_eq!(c.len(), 3, "the root has no ancestors");
        let c = candidates(
            Some(Path::new("/repo/target/release/mapview")),
            Some(Path::new("/repo/crates/view")),
        );
        assert!(c.contains(&PathBuf::from("/repo/assets/sprites")));
        assert_eq!(candidates(None, None), Vec::<PathBuf>::new());
        assert_eq!(
            default_dir().as_deref().map(|d| d.file_name()),
            Some(Some(std::ffi::OsStr::new("sprites"))),
            "the test binary runs in the checkout, which has the art"
        );
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
            [
                "idle",
                "walk",
                "attack",
                "death",
                "decay",
                "build",
                "carry_food",
                "carry_gold",
                "carry_stone",
                "carry_wood",
                "chop",
                "farm",
                "forage",
                "mine"
            ]
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

    /// A colour set (`docs/look/colour-sets.md`) loads its RGBA sheet and
    /// mask whatever its manifest's palette says, takes its cell size from
    /// `frame`, reads an RGB sheet as opaque, and is refused with a mask
    /// of the wrong size.
    #[test]
    fn a_colour_set_loads_its_sheet_and_mask() {
        let dir = fixture::scratch("load");
        let manifest = fixture::write_set(&dir, "house");
        let s = Sheet::load(&manifest).unwrap();
        assert!(s.colour);
        assert_eq!((s.width, s.height), fixture::FRAME);
        assert_eq!(s.frame_size(), fixture::FRAME);
        assert_eq!(s.facings(), &[0]);
        assert!(s.indices.is_empty());
        assert_eq!(s.rgba_at(2, 2), fixture::CLOTH);
        assert_eq!(s.rgba_at(4, 2), fixture::SHADOW);
        assert_eq!(s.rgba_at(0, 0), [0; 4]);
        assert_eq!(s.mask_at(2, 2), 255);
        assert_eq!(s.mask_at(5, 2), 128);
        assert_eq!(s.mask_at(3, 2), 0);
        assert!(s.drawn_at(4, 2) && !s.drawn_at(1, 2));
        assert_eq!(s.index_at(2, 2), 0);
        let (sheets, errors) = load_all(&dir);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(sheets.len(), 1);

        // Without a mask nothing takes the owner's colour; an RGB sheet is
        // opaque.
        let set = dir.join("house");
        let text = std::fs::read_to_string(&manifest).unwrap();
        let rgb: Vec<u8> = fixture::pixels()
            .0
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        let (w, h) = fixture::FRAME;
        fixture::write_png(&set.join("rgb.png"), w, h, png::ColorType::Rgb, &rgb);
        let plain = set.join("plain.ron");
        std::fs::write(
            &plain,
            text.replace("sheet: \"house.png\"", "sheet: \"rgb.png\"")
                .replace("mask: Some(\"house.mask.png\"),", ""),
        )
        .unwrap();
        let p = Sheet::load(&plain).unwrap();
        assert!(p.mask.is_empty());
        assert_eq!(p.mask_at(2, 2), 0);
        assert_eq!(p.rgba_at(4, 2), [0, 0, 0, 255]);
        assert_eq!(p.rgba_at(3, 2), fixture::RED);

        // A mask the wrong size is refused, by name.
        fixture::write_png(
            &set.join("small.png"),
            4,
            3,
            png::ColorType::Grayscale,
            &[0; 12],
        );
        let bad = set.join("bad.ron");
        std::fs::write(&bad, text.replace("house.mask.png", "small.png")).unwrap();
        let e = Sheet::load(&bad).unwrap_err();
        assert!(e.contains("small.png") && e.contains("4×3"), "{e}");
        // So is a sheet that does not lay out as the manifest says.
        let wrong = set.join("wrong.ron");
        std::fs::write(
            &wrong,
            text.replace("frame: Some((8, 6))", "frame: Some((8, 8))"),
        )
        .unwrap();
        assert!(Sheet::load(&wrong).unwrap_err().contains("lays out 8×8"));
        // An indexed set is still held to the palette.
        let indexed = set.join("indexed.ron");
        std::fs::write(&indexed, text.replace("colour: true", "colour: false")).unwrap();
        assert!(Sheet::load(&indexed)
            .unwrap_err()
            .contains("palette 'rgba'"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The new art is read only when it is on; then its sets follow the
    /// indexed ones. `assets/look` is found as `assets/sprites` is.
    #[test]
    fn the_look_directory_is_read_only_with_the_new_art_on() {
        let dir = fixture::scratch("art");
        fixture::write_set(&dir, "house");
        let sprites = repo_root().join("assets/sprites");
        let (off, _) = load_art(Some(&sprites), Some(&dir), false);
        assert!(off.iter().all(|s| !s.colour));
        let (on, errors) = load_art(Some(&sprites), Some(&dir), true);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(on.len(), off.len() + 1);
        assert!(on.last().unwrap().colour);
        assert_eq!(
            candidates_for(None, Some(Path::new("/repo/crates/view")), "look")[2],
            Path::new("/repo/assets/look")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_directory_is_empty_not_fatal() {
        let (sheets, errors) = load_all(Path::new("/definitely/not/here"));
        assert!(sheets.is_empty() && errors.is_empty());
    }
}
