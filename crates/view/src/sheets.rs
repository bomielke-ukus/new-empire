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
use crate::sprites::Architectures;

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

/// What happens at a building's attachment point
/// (`docs/look/buildings.md`, "Smoke, flags and fire").
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AttachKind {
    /// A hearth's or chimney's mouth: pale smoke rises from it.
    Smoke,
    /// A forge's: sooty smoke.
    SmokeDark,
    /// A pole's top: the owner's flag waves from it.
    Flag,
    /// Where the building burns when it is hurt, in the order the fires
    /// catch.
    Fire,
}

/// One attachment point of a colour set: where in the cell, in authored
/// pixels like the anchor (it may lie outside the cell: smoke from a
/// chimney's mouth at the cell's top).
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Attach {
    /// What happens there.
    pub kind: AttachKind,
    /// Where, in authored pixels in the cell.
    pub at: (i32, i32),
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
    /// A building's attachment points, of its standing (idle) look.
    #[serde(default)]
    attach: Vec<Attach>,
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
    /// Where smoke, flags and fire go on the standing building, if anywhere.
    pub attach: Vec<Attach>,
}

impl Sheet {
    /// Loads a set from its manifest path.
    pub fn load(manifest_path: &Path) -> Result<Sheet, String> {
        Sheet::load_if(manifest_path, &|_| true).map(|s| s.expect("every set is wanted"))
    }

    /// Loads a set from its manifest path if `wanted` says so of a colour
    /// set's name, reading only the manifest otherwise: a colour set left
    /// out is never decoded (`docs/look/buildings.md`, "Memory"). An
    /// indexed set is always loaded.
    pub fn load_if(
        manifest_path: &Path,
        wanted: &(dyn Fn(&str) -> bool + Sync),
    ) -> Result<Option<Sheet>, String> {
        let text = std::fs::read_to_string(manifest_path)
            .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        let m: Manifest =
            ron::from_str(&text).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        if m.colour && !wanted(&m.name) {
            return Ok(None);
        }
        Sheet::from_manifest(manifest_path, m).map(Some)
    }

    /// Checks a manifest and decodes the images it names.
    fn from_manifest(manifest_path: &Path, m: Manifest) -> Result<Sheet, String> {
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
            attach: m.attach,
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
    load_paths(&find_sets(dir), &|_| true)
}

/// Loads the sets at `paths`, in that order, leaving out the colour sets
/// whose names `wanted` refuses. The images are decoded on as many
/// threads as the machine has, which is most of the time a match's art
/// takes to load.
fn load_paths(
    paths: &[PathBuf],
    wanted: &(dyn Fn(&str) -> bool + Sync),
) -> (Vec<Sheet>, Vec<String>) {
    // The biggest first, so no thread is left decoding a Wonder alone at
    // the end: by the bytes of the images beside each manifest.
    let weight = |p: &PathBuf| -> u64 {
        let Some(dir) = p.parent() else { return 0 };
        std::fs::read_dir(dir).map_or(0, |files| {
            files
                .flatten()
                .filter(|f| f.path().extension().is_some_and(|x| x == "png"))
                .filter_map(|f| f.metadata().ok())
                .map(|m| m.len())
                .sum()
        })
    };
    let mut order: Vec<(u64, usize)> = paths.iter().map(weight).zip(0..).collect();
    order.sort_by(|a, b| b.cmp(a));
    let mut loaded: Vec<(usize, Result<Option<Sheet>, String>)> =
        par_map(&order, |&(_, i)| (i, Sheet::load_if(&paths[i], wanted)));
    loaded.sort_by_key(|(i, _)| *i);
    let loaded = loaded.into_iter().map(|(_, r)| r);
    let mut sheets = Vec::new();
    let mut errors = Vec::new();
    for r in loaded {
        match r {
            Ok(Some(s)) => sheets.push(s),
            Ok(None) => {}
            Err(e) => errors.push(e),
        }
    }
    (sheets, errors)
}

/// `f` of every item, in order, on up to as many threads as the machine
/// has, each taking the next item as it finishes the last (a Wonder's
/// sheet takes many times a house's).
pub(crate) fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(items.len());
    if threads <= 1 {
        return items.iter().map(f).collect();
    }
    let next = AtomicUsize::new(0);
    let mut done: Vec<(usize, R)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(i) else {
                            break out;
                        };
                        out.push((i, f(item)));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().expect("a loading thread panicked"))
            .collect()
    });
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, r)| r).collect()
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
    load_art_for(sprites, look, new_art, Architectures::ALL)
}

/// [`load_art`] for a match whose sides build in `archs`
/// (`docs/look/buildings.md`, "Memory"): the colour sets of the other
/// architectures are not read beyond their manifests. Everything with no
/// architecture (trees, scenery, units) and the first architecture's sets
/// are always loaded.
pub fn load_art_for(
    sprites: Option<&Path>,
    look: Option<&Path>,
    new_art: bool,
    archs: Architectures,
) -> (Vec<Sheet>, Vec<String>) {
    let mut paths = sprites.map(find_sets).unwrap_or_default();
    if new_art {
        if let Some(dir) = look {
            paths.extend(find_sets(dir));
        }
    }
    load_paths(&paths, &|name| archs.keeps(name))
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

    /// A construction stage's colour in the building fixture: stage `s`
    /// is a row of `s + 1` pixels of this at (2, 4).
    pub fn stage_colour(s: u32) -> [u8; 4] {
        [40 + 60 * s as u8, 90, 10, 255]
    }
    /// The building fixture's rubble: two pixels at (3, 4).
    pub const RUBBLE: [u8; 4] = [150, 140, 120, 255];

    /// Writes the colour building `name` under `dir/name/` as
    /// `tools/look/export.py` lays one out (`docs/look/buildings.md`):
    /// three construction stages, the idle frame (as [`pixels`]) and its
    /// rubble, one row each, every cell [`FRAME`] with one anchor. Returns
    /// the manifest's path.
    pub fn write_building(dir: &Path, name: &str) -> PathBuf {
        let set = dir.join(name);
        std::fs::create_dir_all(&set).unwrap();
        let (w, h) = FRAME;
        let (cols, rows) = (3, 3);
        let sw = w * cols;
        let mut rgba = vec![0u8; (sw * h * rows * 4) as usize];
        let mut mask = vec![0u8; (sw * h * rows) as usize];
        let mut put = |cell: (u32, u32), x: u32, y: u32, c: [u8; 4], m: u8| {
            let i = ((cell.1 * h + y) * sw + cell.0 * w + x) as usize;
            rgba[i * 4..i * 4 + 4].copy_from_slice(&c);
            mask[i] = m;
        };
        for s in 0..3 {
            for x in 0..=s {
                put((s, 0), 2 + x, 4, stage_colour(s), 0);
            }
        }
        let (idle, idle_mask) = pixels();
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                let c = [
                    idle[i * 4],
                    idle[i * 4 + 1],
                    idle[i * 4 + 2],
                    idle[i * 4 + 3],
                ];
                put((0, 1), x, y, c, idle_mask[i]);
            }
        }
        put((0, 2), 3, 4, RUBBLE, 0);
        put((0, 2), 4, 4, RUBBLE, 0);
        write_png(
            &set.join(format!("{name}.png")),
            sw,
            h * rows,
            png::ColorType::Rgba,
            &rgba,
        );
        write_png(
            &set.join(format!("{name}.mask.png")),
            sw,
            h * rows,
            png::ColorType::Grayscale,
            &mask,
        );
        let manifest = set.join(format!("{name}.ron"));
        std::fs::write(
            &manifest,
            format!(
                r#"SpriteSet(
    name: "{name}",
    class: MediumBuilding,
    scale: 2,
    palette: "rgba",
    sheet: "{name}.png",
    anchor: ({}, {}),
    animations: [
        (name: "construction", frames: 3, frame_ms: 200, loops: false, impact: None),
        (name: "idle", frames: 1, frame_ms: 160, loops: true, impact: None),
        (name: "rubble", frames: 1, frame_ms: 200, loops: false, impact: None),
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

    /// Where the attached building fixture's smoke rises, in its cell.
    pub const SMOKE_AT: (i32, i32) = (6, -4);
    /// Where it burns, in the order the fires catch.
    pub const FIRE_AT: [(i32, i32); 3] = [(4, 2), (2, 3), (6, 3)];

    /// The building fixture `name` ([`write_building`]) with attachment
    /// points: smoke at [`SMOKE_AT`] (above the cell) and fire at each of
    /// [`FIRE_AT`]. Returns the manifest's path.
    pub fn write_attached_building(dir: &Path, name: &str) -> PathBuf {
        let manifest = write_building(dir, name);
        let text = std::fs::read_to_string(&manifest).unwrap();
        let mut attach = format!(
            "    attach: [\n        (kind: Smoke, at: ({}, {})),\n",
            SMOKE_AT.0, SMOKE_AT.1
        );
        for (x, y) in FIRE_AT {
            attach += &format!("        (kind: Fire, at: ({x}, {y})),\n");
        }
        attach += "    ],\n)";
        let i = text.rfind(')').unwrap();
        std::fs::write(&manifest, format!("{}{attach}", &text[..i])).unwrap();
        manifest
    }

    /// An effect's colour set `name` (`docs/look/buildings.md`, "Smoke,
    /// flags and fire"): a looping idle of `frames` frames of 100 ms, frame
    /// `k` one pixel of [`fx_colour`]`(k)` at its anchor's column, a row up.
    pub fn write_fx(dir: &Path, name: &str, frames: u32) -> PathBuf {
        let set = dir.join(name);
        std::fs::create_dir_all(&set).unwrap();
        let (w, h) = FRAME;
        let sw = w * frames;
        let mut rgba = vec![0u8; (sw * h * 4) as usize];
        for k in 0..frames {
            let i = ((ANCHOR.1 - 1) * sw + k * w + ANCHOR.0) as usize;
            rgba[i * 4..i * 4 + 4].copy_from_slice(&fx_colour(k));
        }
        write_png(
            &set.join(format!("{name}.png")),
            sw,
            h,
            png::ColorType::Rgba,
            &rgba,
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
        (name: "idle", frames: {frames}, frame_ms: 100, loops: true, impact: None),
    ],
    anchor_overrides: [],
    colour: true,
    mask: None,
    frame: Some(({w}, {h})),
)"#,
                ANCHOR.0, ANCHOR.1
            ),
        )
        .unwrap();
        manifest
    }

    /// The colour of an effect fixture's frame `k`: soft, as smoke is.
    pub fn fx_colour(k: u32) -> [u8; 4] {
        [200, 10 * k as u8, 30, 128]
    }

    /// The building fixture `name`, loaded.
    pub fn building(test: &str, name: &str) -> super::Sheet {
        let dir = scratch(&format!("{test}-{name}"));
        super::Sheet::load(&write_building(&dir, name)).unwrap()
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

    /// The exported buildings carry their attachment points
    /// (`docs/look/buildings.md`, "Smoke, flags and fire"): every one with
    /// stages but a farm can burn, every architecture's house and town
    /// centre smokes, and the effects' sets are there, each a looping idle.
    #[test]
    fn the_new_buildings_smoke_and_burn_where_their_models_say() {
        let look = repo_root().join("assets/look");
        let manifests: Vec<(String, Manifest)> = find_sets(&look)
            .into_iter()
            .map(|p| {
                let m: Manifest = ron::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
                (m.name.clone(), m)
            })
            .collect();
        let get = |name: &str| &manifests.iter().find(|(n, _)| n == name).unwrap().1;
        for (name, m) in &manifests {
            let staged = m.animations.iter().any(|a| a.name == "construction");
            if staged && !name.starts_with("farm") {
                assert!(
                    m.attach.iter().any(|a| a.kind == AttachKind::Fire),
                    "{name} has nowhere to burn"
                );
            }
        }
        for arch in ["", "_egyptian", "_mesopotamian", "_asian"] {
            for kind in ["house", "town_center", "temple"] {
                let m = get(&format!("{kind}{arch}"));
                assert!(
                    m.attach.iter().any(|a| a.kind == AttachKind::Smoke),
                    "{kind}{arch} does not smoke"
                );
            }
            let m = get(&format!("siege_workshop{arch}"));
            assert!(m.attach.iter().any(|a| a.kind == AttachKind::SmokeDark));
            assert!(
                !get(&format!("storehouse{arch}"))
                    .attach
                    .iter()
                    .any(|a| matches!(a.kind, AttachKind::Smoke | AttachKind::SmokeDark)),
                "a store has no hearth"
            );
        }
        for name in crate::sprites::FX_SETS {
            let m = get(name);
            assert!(m.colour);
            assert_eq!(m.animations.len(), 1);
            let idle = &m.animations[0];
            assert!(
                idle.name == "idle" && idle.loops && idle.frames >= 8,
                "{name}"
            );
            // The loop divides the renderers' clock, so it wraps unseen.
            let period = idle.frames * idle.frame_ms;
            assert_eq!((crate::ground::PERIOD as u32 * 1000) % period, 0, "{name}");
        }
        assert!(
            get("fx_flag").mask.is_some(),
            "the flag takes its owner's colour"
        );
    }

    /// A unit's colour set (`docs/look/units.md`) plays exactly what the
    /// old set of its name played: the same animations in the same order,
    /// with the same frame counts, timing, looping and impact frames, so
    /// nothing that times itself by them changes; and it turns, takes its
    /// owner's colour and is laid out as its manifest says.
    #[test]
    fn the_new_units_keep_the_old_units_timing() {
        let look = repo_root().join("assets/look");
        let old = repo_root().join("assets/sprites");
        let mut units = 0;
        for path in find_sets(&look) {
            let m: Manifest = ron::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            if !m.class.turns() {
                continue;
            }
            units += 1;
            let was: Manifest = ron::from_str(
                &std::fs::read_to_string(old.join(&m.name).join(format!("{}.ron", m.name)))
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(m.animations, was.animations, "{}", m.name);
            assert_eq!(m.class, was.class, "{}", m.name);
            assert!(m.colour && m.mask.is_some(), "{}", m.name);
            let sheet = Sheet::load(&path).unwrap();
            assert_eq!(sheet.facings().len(), 5);
            assert_eq!(
                sheet.height,
                m.frame.unwrap().1 * 5 * m.animations.len() as u32
            );
        }
        assert!(units >= 2, "the villager and the hoplite at least");
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
