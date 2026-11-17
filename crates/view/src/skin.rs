//! The new look's UI skin (`docs/look/ui.md`): a painted, sunlit
//! parchment-and-bronze dress for the same HUD and screens, drawn with the
//! new art on (`Settings::new_art`). Off, nothing here is used and the UI is
//! the old one to the pixel.
//!
//! Everything the skin draws is a frame on the atlas's colour pages, so the
//! GPU renderer and the software rasteriser draw it exactly as they draw the
//! new art: no shader or pipeline of its own. The frames are made when the
//! atlas is built ([`frames`]):
//!
//! - **Text**: glyphs of two open-licensed faces (`assets/ui/OFL.txt`),
//!   baked by `tools/look/ui_font.py` into a coverage atlas at every cap
//!   height the UI asks for, coloured here with the skin's inks. Text is
//!   always drawn at a bake's own pixel size, so it is crisp at any UI
//!   scale ([`Font`]).
//! - **Boxes**: panels, buttons, chips, troughs and plates as nine-slice
//!   pieces, rendered for each of a few device scales ([`BUCKETS`]) so their
//!   rounded corners and lines land on whole device pixels; and their soft
//!   drop shadow, the sheen that bevels them and the parchment's grain.
//! - **Icons**: the resources' and the units' and buildings' own new art,
//!   area-filtered down to icon size at the same device scales.
//!
//! The UI is laid out in HUD pixels as before; only how a box or a line of
//! text is painted changes, so every hit box is the old one.

use std::collections::HashMap;
use std::sync::LazyLock;

use sim::entity::KindId;

/// An sRGB colour with straight alpha.
pub type Rgba = [u8; 4];

// ----- palette (`docs/look/ui.md`, "Palette"), from the buildings'
// pigments in `tools/look/lib.py` `PAL` -----------------------------------

/// The panels' parchment: `PAL["plaster"]`, warmed.
pub const PARCHMENT: Rgba = [241, 229, 199, 255];
/// A button at rest: `PAL["limestone"]` lifted toward the parchment.
pub const STONE: Rgba = [228, 207, 160, 255];
/// A hovered button: the lit side of the limestone.
pub const STONE_LIT: Rgba = [247, 232, 190, 255];
/// A greyed button: the parchment with the colour drained.
pub const STONE_DIM: Rgba = [204, 196, 180, 255];
/// The deep ink the text is in: `PAL["wood_dark"]` toward black.
pub const INK: Rgba = [44, 28, 18, 255];
/// Headings and the gold role on parchment: `PAL["tile_dark"]` deepened,
/// a rubric.
pub const RUBRIC: Rgba = [128, 42, 20, 255];
/// Text on a dark chip: `PAL["linen"]`.
pub const CREAM: Rgba = [250, 243, 226, 255];
/// Text on a greyed button.
pub const MUTED: Rgba = [84, 74, 62, 255];
/// The frames' outer edge: `PAL["bronze"]` in shadow.
pub const BRONZE_DARK: Rgba = [96, 60, 26, 255];
/// The frames' bronze: `PAL["bronze"]`.
pub const BRONZE: Rgba = [204, 143, 61, 255];
/// The thin gold line inside a panel's edge: `PAL["gold"]`, a touch deeper.
pub const GOLD_LINE: Rgba = [222, 172, 62, 255];
/// The accent: `PAL["lapis"]`.
pub const LAPIS: Rgba = [31, 56, 158, 255];
/// Warnings: `PAL["terracotta"]` deepened so cream text reads on it.
pub const TERRACOTTA: Rgba = [166, 60, 32, 255];
/// A softer warning wash, for a flashed resource.
pub const TERRACOTTA_LIGHT: Rgba = [236, 178, 150, 255];
/// The gold chip (the idle count, the controls hint): `PAL["yellow_ochre"]`.
pub const OCHRE: Rgba = [236, 190, 92, 255];
/// Health, full: `PAL["malachite"]`.
pub const MALACHITE: Rgba = [46, 150, 84, 255];
/// Health, low: `PAL["red_ochre"]`.
pub const RED_OCHRE: Rgba = [196, 58, 38, 255];
/// Progress: `PAL["gold"]`.
pub const GOLD: Rgba = [240, 180, 48, 255];
/// A bar's trough, the minimap's well: `PAL["wood_dark"]`.
pub const TROUGH: Rgba = [66, 44, 26, 255];
/// A grey for what is lost: `PAL["iron"]` warmed.
pub const PEWTER: Rgba = [150, 140, 126, 255];
/// A light grey.
pub const PEWTER_LIGHT: Rgba = [200, 192, 178, 255];
/// A deep green.
pub const MALACHITE_DARK: Rgba = [30, 100, 56, 255];
/// The backdrop behind a screen with no world behind it.
pub const BACKDROP: Rgba = [34, 26, 20, 255];

/// The text-on-surface pairs the skin draws, each of which must meet WCAG
/// AA (4.5:1): `(text, surface, what)`.
pub const TEXT_PAIRS: [(Rgba, Rgba, &str); 13] = [
    (INK, PARCHMENT, "text on a panel"),
    (RUBRIC, PARCHMENT, "a heading on a panel"),
    (INK, STONE, "a button's label"),
    (INK, STONE_LIT, "a hovered button's label"),
    (MUTED, STONE_DIM, "a greyed button's label"),
    (CREAM, TERRACOTTA, "a warning"),
    (INK, TERRACOTTA_LIGHT, "a flashed resource"),
    (INK, OCHRE, "a gold chip"),
    (RUBRIC, STONE, "a heading on a button"),
    (CREAM, LAPIS, "text on the accent"),
    (INK, GOLD, "a value on a progress bar"),
    (CREAM, TROUGH, "a key on its key cap"),
    (RUBRIC, STONE_DIM, "LOCKED on a greyed button"),
];

/// WCAG 2 contrast ratio of two opaque sRGB colours.
pub fn contrast(a: Rgba, b: Rgba) -> f32 {
    let lum = |c: Rgba| {
        let ch = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * ch(c[0]) + 0.7152 * ch(c[1]) + 0.0722 * ch(c[2])
    };
    let (la, lb) = (lum(a), lum(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

// ----- inks ---------------------------------------------------------------

/// The colours text is baked in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ink {
    /// [`INK`], on parchment and stone.
    Ink = 0,
    /// [`RUBRIC`], headings and the old gold.
    Rubric = 1,
    /// [`CREAM`], on a dark chip.
    Cream = 2,
    /// [`MUTED`], on a greyed button.
    Muted = 3,
}

impl Ink {
    /// Every ink, in frame order.
    pub const ALL: [Ink; 4] = [Ink::Ink, Ink::Rubric, Ink::Cream, Ink::Muted];

    /// Its colour.
    pub const fn rgba(self) -> Rgba {
        match self {
            Ink::Ink => INK,
            Ink::Rubric => RUBRIC,
            Ink::Cream => CREAM,
            Ink::Muted => MUTED,
        }
    }
}

// ----- the font -----------------------------------------------------------

/// Which face.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Face {
    /// Carlito Bold: the HUD's text.
    Sans = 0,
    /// Caladea Bold: headings and titles.
    Serif = 1,
}

/// One glyph of a bake, in device pixels.
#[derive(Clone, Copy, Debug, Default)]
pub struct Glyph {
    /// Its rectangle in `assets/ui/font.png`; `w == 0` for a blank.
    pub x: u16,
    /// Atlas rectangle.
    pub y: u16,
    /// Atlas rectangle.
    pub w: u16,
    /// Atlas rectangle.
    pub h: u16,
    /// Its top left from the pen on the baseline.
    pub bx: i16,
    /// Its top left from the pen on the baseline.
    pub by: i16,
    /// How far the pen moves.
    pub advance: f32,
}

/// A face at one cap height.
#[derive(Clone, Debug)]
pub struct Bake {
    /// Which face.
    pub face: Face,
    /// The cap height it was baked for, device px.
    pub cap: u16,
    /// The height its capitals are drawn, device px.
    pub drawn: u16,
    /// Its size in px (the em).
    pub size: f32,
    /// Only the title set: no lower punctuation, two inks.
    pub title: bool,
    /// Glyphs by ASCII code; `None` where not baked.
    pub glyphs: Vec<Option<Glyph>>,
}

/// The two faces' metrics and every bake (`assets/ui/font.txt`).
#[derive(Debug)]
pub struct Font {
    /// Tracking, em, by [`Face`].
    pub tracking: [f32; 2],
    /// Kerning, em, by face and pair of ASCII codes.
    pub kern: [HashMap<(u8, u8), f32>; 2],
    /// Every bake, in file order.
    pub bakes: Vec<Bake>,
}

/// The baked metrics.
pub const FONT_TXT: &str = include_str!("../../../assets/ui/font.txt");
/// The baked coverage.
pub const FONT_PNG: &[u8] = include_bytes!("../../../assets/ui/font.png");

/// The font, parsed once.
pub static FONT: LazyLock<Font> = LazyLock::new(|| Font::parse(FONT_TXT));

impl Font {
    /// Parses the metrics file `tools/look/ui_font.py` writes.
    pub fn parse(text: &str) -> Font {
        let mut font = Font {
            tracking: [0.0; 2],
            kern: [HashMap::new(), HashMap::new()],
            bakes: Vec::new(),
        };
        let mut face = Face::Sans;
        let face_of = |s: &str| {
            if s == "serif" {
                Face::Serif
            } else {
                Face::Sans
            }
        };
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            let num = |i: usize| f.get(i).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
            match f.first().copied() {
                Some("face") => {
                    face = face_of(f[1]);
                    font.tracking[face as usize] = num(2);
                }
                Some("kern") => {
                    font.kern[face as usize].insert((num(1) as u8, num(2) as u8), num(3));
                }
                Some("bake") => font.bakes.push(Bake {
                    face: face_of(f[1]),
                    cap: num(2) as u16,
                    size: num(3),
                    drawn: num(5) as u16,
                    title: false,
                    glyphs: vec![None; 128],
                }),
                Some("glyph") => {
                    if let Some(b) = font.bakes.last_mut() {
                        let code = num(1) as usize;
                        if code < 128 {
                            b.glyphs[code] = Some(Glyph {
                                x: num(2) as u16,
                                y: num(3) as u16,
                                w: num(4) as u16,
                                h: num(5) as u16,
                                bx: num(6) as i16,
                                by: num(7) as i16,
                                advance: num(8),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        for b in &mut font.bakes {
            b.title = b.glyphs[b';' as usize].is_none();
        }
        font
    }

    /// The bake of `face` nearest `cap` device px that has `ink`: the one
    /// whose drawn capitals are closest by ratio. Its index.
    pub fn pick(&self, face: Face, cap: f32, ink: Ink) -> Option<usize> {
        self.pick_in(face, cap, ink)
            .or_else(|| self.pick_in(Face::Sans, cap, ink))
    }

    fn pick_in(&self, face: Face, cap: f32, ink: Ink) -> Option<usize> {
        self.bakes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.face == face && has_ink(b, ink))
            .min_by(|(_, a), (_, b)| {
                let d = |x: &Bake| (x.drawn as f32 / cap.max(1.0)).ln().abs();
                d(a).total_cmp(&d(b))
            })
            .map(|(i, _)| i)
    }

    /// The pen positions of `text` set in bake `bake`: each character's
    /// ASCII code (upper case) and its pen x in whole device px from the
    /// start, and the whole run's advance. Characters the bake lacks take
    /// a space's room.
    pub fn layout(&self, bake: usize, text: &str) -> (Vec<(u8, i32)>, f32) {
        let b = &self.bakes[bake];
        let face = b.face as usize;
        // Whole pixels, as the hinted advances are, so every pen lands on
        // the grid the glyphs were hinted for.
        let track = (self.tracking[face] * b.size).round();
        // Capitals want a wider word space than the face's own.
        let space = b.glyphs[b' ' as usize]
            .map_or(0.0, |g| g.advance)
            .max((b.size * 0.3).round());
        let mut out = Vec::with_capacity(text.len());
        let mut pen = 0.0f32;
        let mut prev: Option<u8> = None;
        for ch in text.chars() {
            let c = ch.to_ascii_uppercase();
            let code = if c.is_ascii() { c as u8 } else { b'?' };
            let g = b.glyphs.get(code as usize).copied().flatten();
            if let (Some(p), Some(_)) = (prev, g) {
                pen += (self.kern[face].get(&(p, code)).copied().unwrap_or(0.0) * b.size).round();
            }
            out.push((code, pen.round() as i32));
            pen += match g {
                Some(_) if code == b' ' => space,
                Some(g) => g.advance,
                None => space,
            } + track;
            prev = g.map(|_| code);
        }
        let width = if out.is_empty() { 0.0 } else { pen - track };
        (out, width.max(0.0))
    }
}

/// Whether bake `b` was coloured in `ink`, to spare the atlas what is
/// never drawn: the sans in every ink, the serif's headings in ink and
/// rubric (and cream, for a warning in the resource bar), its title sizes
/// (the title and the banner) in rubric only.
/// Text in an ink a size lacks is set in the nearest size that has it.
pub fn has_ink(b: &Bake, ink: Ink) -> bool {
    match (b.face, b.title) {
        (Face::Sans, _) => true,
        (Face::Serif, false) => ink != Ink::Muted,
        (Face::Serif, true) => ink == Ink::Rubric,
    }
}

/// The face a text scale is set in: headings (twice the HUD's text and
/// up) in the serif, the rest in the sans. ([`Font::pick`] falls back to
/// the sans for an ink the serif is not baked in.)
pub fn face_for(scale: f32) -> Face {
    if scale >= 1.75 {
        Face::Serif
    } else {
        Face::Sans
    }
}

// ----- frame ids ----------------------------------------------------------

/// Glyphs: `FONT + bake`, facing the [`Ink`], index the ASCII code.
pub const FONT_ID: KindId = 64_000;
/// Box pieces: `BOX + style * 8 + bucket`, facing the [`Piece`].
pub const BOX_ID: KindId = 64_100;
/// Drop shadows: `SHADOW + bucket`, facing the [`Piece`].
pub const SHADOW_ID: KindId = 64_300;
/// Fills, the sheen and the grain: facing a [`Misc`].
pub const MISC_ID: KindId = 64_320;
/// Flat colours: `SOLID`, facing the colour's index in [`SOLIDS`].
pub const SOLID_ID: KindId = 64_330;
/// Resource icons: `ICON + resource index`, facing the bucket.
pub const ICON_ID: KindId = 64_340;
/// A kind's portrait: `PORTRAIT + kind`, facing the bucket. Kinds stay
/// under 1000.
pub const PORTRAIT_ID: KindId = 64_400;

/// The device scales the skin's pieces and icons are made for; a UI scale
/// between them uses the nearest.
pub const BUCKETS: [f32; 7] = [1.0, 1.25, 1.5, 2.0, 2.5, 3.0, 4.0];

/// The icons and portraits are made for the buckets up to this one (2x):
/// above it they are drawn at 2x's size, smaller than the layout's room
/// for them but crisp, to spare the atlas.
pub const ICON_BUCKETS: usize = 4;

/// The bucket nearest a device scale.
pub fn bucket(s: f32) -> usize {
    (0..BUCKETS.len())
        .min_by(|&a, &b| {
            let d = |i: usize| (BUCKETS[i] / s.max(0.1)).ln().abs();
            d(a).total_cmp(&d(b))
        })
        .unwrap_or(0)
}

/// The flat colours the skin fills with, by index.
pub const SOLIDS: [Rgba; 25] = [
    PARCHMENT,
    STONE,
    STONE_LIT,
    STONE_DIM,
    INK,
    RUBRIC,
    CREAM,
    MUTED,
    BRONZE_DARK,
    BRONZE,
    GOLD_LINE,
    LAPIS,
    TERRACOTTA,
    TERRACOTTA_LIGHT,
    OCHRE,
    MALACHITE,
    RED_OCHRE,
    GOLD,
    TROUGH,
    BACKDROP,
    PEWTER,
    PEWTER_LIGHT,
    MALACHITE_DARK,
    [0, 0, 0, 96],
    [255, 255, 255, 255],
];

/// The index of a colour in [`SOLIDS`], if it is one.
pub fn solid_index(c: Rgba) -> Option<u8> {
    SOLIDS.iter().position(|s| *s == c).map(|i| i as u8)
}

/// The skin colour an old palette index paints as, for the rectangles the
/// UI still draws one by one; `None` keeps the palette's (a player's
/// colour, laid on by its row).
pub fn remap(idx: u8) -> Option<Rgba> {
    use crate::palette as p;
    Some(match idx {
        p::BLACK => BRONZE_DARK,
        p::BROWN_DARK | p::SAND => PARCHMENT,
        p::BROWN => STONE,
        p::TAN => STONE_LIT,
        p::GREY_DARK => STONE_DIM,
        p::GREY => PEWTER,
        p::GREY_LIGHT => PEWTER_LIGHT,
        p::GOLD_DARK => OCHRE,
        p::GOLD => GOLD,
        p::GOLD_LIGHT => GOLD_LINE,
        p::RED => RED_OCHRE,
        p::RED_DARK => TERRACOTTA,
        p::GREEN_LIGHT | p::GREEN => MALACHITE,
        p::GREEN_DARK => MALACHITE_DARK,
        p::WHITE => CREAM,
        _ => return None,
    })
}

/// The look of a box.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    /// A panel: parchment in a bronze frame with a thin gold line inside.
    Panel = 0,
    /// A button at rest: limestone, bevelled, a bronze edge.
    Button = 1,
    /// A hovered button: lit, a gold edge.
    Hover = 2,
    /// A greyed button: drained, a soft edge.
    Disabled = 3,
    /// A warning chip: terracotta.
    Warn = 4,
    /// A gold chip.
    Gold = 5,
    /// A sunken trough, for a bar or the minimap.
    Trough = 6,
    /// A plate: a tooltip, a notice, a line of text over the world.
    Plate = 7,
    /// A soft warning wash: a flashed resource.
    Flash = 8,
    /// The accent: the chosen one of a row.
    Accent = 9,
}

impl Style {
    /// Every style, in id order.
    pub const ALL: [Style; 10] = [
        Style::Panel,
        Style::Button,
        Style::Hover,
        Style::Disabled,
        Style::Warn,
        Style::Gold,
        Style::Trough,
        Style::Plate,
        Style::Flash,
        Style::Accent,
    ];

    /// The ink text on it is in.
    pub const fn ink(self) -> Ink {
        match self {
            Style::Warn | Style::Accent => Ink::Cream,
            Style::Disabled => Ink::Muted,
            Style::Trough => Ink::Cream,
            _ => Ink::Ink,
        }
    }
}

/// The nine pieces of a box, and the frames a shadow is cut into.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Piece {
    /// Corner.
    TopLeft = 0,
    /// Edge, stretched across.
    Top = 1,
    /// Corner.
    TopRight = 2,
    /// Edge, stretched down.
    Left = 3,
    /// Fill.
    Fill = 4,
    /// Edge, stretched down.
    Right = 5,
    /// Corner.
    BottomLeft = 6,
    /// Edge, stretched across.
    Bottom = 7,
    /// Corner.
    BottomRight = 8,
}

/// The misc frames.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Misc {
    /// A vertical light-to-shade wash, laid over a box's inside.
    Sheen = 0,
    /// Parchment grain, tiled 1:1 in device px.
    Grain = 1,
}

/// The grain tile's side, device px.
pub const GRAIN: u32 = 96;

/// The tallest sheen baked to its own height ([`Misc::Sheen`], index
/// the height in device px); a taller box stretches index 0.
pub const SHEEN_EXACT: u32 = 96;

/// A box's corner piece size at bucket `b`, device px: the radius and the
/// lines inside it.
pub fn corner(style: Style, b: usize) -> u32 {
    let s = BUCKETS[b];
    let r = match style {
        Style::Panel => 6.0,
        Style::Plate | Style::Button | Style::Hover | Style::Disabled | Style::Accent => 4.0,
        _ => 3.0,
    };
    (r * s).round().max(2.0) as u32
}

/// A drop shadow's reach beyond its box, device px, at bucket `b`.
pub fn shadow_reach(b: usize) -> u32 {
    (4.0 * BUCKETS[b]).round() as u32
}

/// How far a drop shadow falls below its box, device px.
pub fn shadow_drop(b: usize) -> f32 {
    (2.0 * BUCKETS[b]).round()
}

/// Coverage of a pixel by a rounded rectangle `[0, w) x [0, h)` of corner
/// radius `r` inset by `k` px, 4 x 4 samples.
fn cover(px: u32, py: u32, w: f32, h: f32, r: f32, k: f32) -> f32 {
    let (x0, y0, x1, y1) = (k, k, w - k, h - k);
    let r = (r - k).max(0.0);
    let mut n = 0;
    for sy in 0..4 {
        for sx in 0..4 {
            let x = px as f32 + (sx as f32 + 0.5) / 4.0;
            let y = py as f32 + (sy as f32 + 0.5) / 4.0;
            if x < x0 || x >= x1 || y < y0 || y >= y1 {
                continue;
            }
            let cx = x.clamp(x0 + r, x1 - r);
            let cy = y.clamp(y0 + r, y1 - r);
            let (dx, dy) = (x - cx, y - cy);
            if dx * dx + dy * dy <= r * r {
                n += 1;
            }
        }
    }
    n as f32 / 16.0
}

/// `top` over `under`, straight alpha.
fn over(top: [f32; 4], under: [f32; 4]) -> [f32; 4] {
    let a = top[3] + under[3] * (1.0 - top[3]);
    if a <= 0.0 {
        return [0.0; 4];
    }
    let ch = |i: usize| (top[i] * top[3] + under[i] * under[3] * (1.0 - top[3])) / a;
    [ch(0), ch(1), ch(2), a]
}

fn f(c: Rgba) -> [f32; 4] {
    [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32 / 255.0]
}

fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let ch = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8;
    [ch(0), ch(1), ch(2), ch(3)]
}

/// A style's layers, outside in: `(inset in device px, colour)`; each
/// covers the rounded rectangle at its inset. The last is the fill.
fn layers(style: Style, s: f32) -> Vec<(f32, Rgba)> {
    let px = |v: f32| (v * s).round().max(1.0);
    let edge = |c: Rgba, fill: Rgba| {
        vec![
            (0.0, c),
            // A bevel: a lit line inside the edge, then the fill.
            (px(1.0), mix(fill, [255, 255, 255, 255], 0.45)),
            (px(1.0) + px(1.0).min(1.0 * s), fill),
        ]
    };
    match style {
        Style::Panel => vec![
            (0.0, BRONZE_DARK),
            (px(1.0), BRONZE),
            (px(2.0), PARCHMENT),
            (px(3.0), GOLD_LINE),
            (px(3.0) + px(1.0), PARCHMENT),
        ],
        Style::Button => edge(BRONZE_DARK, STONE),
        Style::Hover => vec![
            (0.0, BRONZE_DARK),
            (px(1.0), GOLD_LINE),
            (px(2.0), STONE_LIT),
        ],
        Style::Disabled => edge(PEWTER, STONE_DIM),
        Style::Warn => edge([110, 36, 18, 255], TERRACOTTA),
        Style::Gold => edge(BRONZE_DARK, OCHRE),
        Style::Trough => vec![(0.0, BRONZE_DARK), (px(1.0), TROUGH)],
        Style::Plate => vec![
            (0.0, BRONZE_DARK),
            (px(1.0), mix(PARCHMENT, [255, 255, 255, 255], 0.5)),
            (px(2.0), PARCHMENT),
        ],
        Style::Flash => edge(TERRACOTTA, TERRACOTTA_LIGHT),
        Style::Accent => edge([16, 28, 90, 255], LAPIS),
    }
}

/// A frame the skin adds to the atlas: RGBA with straight alpha, and the
/// player-colour mask (zero but on portraits).
pub struct SkinFrame {
    /// Filed under.
    pub kind: KindId,
    /// Filed under.
    pub facing: u8,
    /// Filed under.
    pub index: u8,
    /// Size.
    pub w: u32,
    /// Size.
    pub h: u32,
    /// RGBA8, straight alpha.
    pub rgba: Vec<u8>,
    /// The owner's colour, a byte a pixel.
    pub mask: Vec<u8>,
}

impl SkinFrame {
    fn new(kind: KindId, facing: u8, index: u8, w: u32, h: u32) -> SkinFrame {
        SkinFrame {
            kind,
            facing,
            index,
            w,
            h,
            rgba: vec![0; (w * h * 4) as usize],
            mask: vec![0; (w * h) as usize],
        }
    }

    fn set(&mut self, x: u32, y: u32, c: Rgba) {
        let i = ((y * self.w + x) * 4) as usize;
        self.rgba[i..i + 4].copy_from_slice(&c);
    }

    fn get(&self, x: u32, y: u32) -> Rgba {
        let i = ((y * self.w + x) * 4) as usize;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    /// The `(x, y, w, h)` part of it, filed under another key.
    fn cut(&self, key: (KindId, u8), x: u32, y: u32, w: u32, h: u32) -> SkinFrame {
        let mut out = SkinFrame::new(key.0, key.1, 0, w.max(1), h.max(1));
        for yy in 0..h {
            for xx in 0..w {
                out.set(xx, yy, self.get(x + xx, y + yy));
            }
        }
        out
    }
}

/// A box drawn whole at `w` x `h` device px in `style` at device scale `s`,
/// for cutting into its nine pieces (and for tests).
pub fn render_box(style: Style, s: f32, r: f32, w: u32, h: u32) -> SkinFrame {
    let mut img = SkinFrame::new(0, 0, 0, w, h);
    let ls = layers(style, s);
    for y in 0..h {
        for x in 0..w {
            let mut c = [0.0f32; 4];
            for &(k, col) in &ls {
                let a = cover(x, y, w as f32, h as f32, r, k);
                if a > 0.0 {
                    let mut l = f(col);
                    l[3] *= a;
                    c = over(l, c);
                }
            }
            img.set(
                x,
                y,
                [
                    c[0].round() as u8,
                    c[1].round() as u8,
                    c[2].round() as u8,
                    (c[3] * 255.0).round() as u8,
                ],
            );
        }
    }
    img
}

/// A box's nine pieces, filed under `kind`, from a whole box `2c + 1`
/// square: the corners `c` square, the edges a pixel thick, the fill one
/// pixel.
fn nine(whole: &SkinFrame, kind: KindId, c: u32) -> Vec<SkinFrame> {
    let t = 2 * c + 1;
    debug_assert_eq!(whole.w, t);
    let k = |p: Piece| (kind, p as u8);
    vec![
        whole.cut(k(Piece::TopLeft), 0, 0, c, c),
        whole.cut(k(Piece::Top), c, 0, 1, c),
        whole.cut(k(Piece::TopRight), c + 1, 0, c, c),
        whole.cut(k(Piece::Left), 0, c, c, 1),
        whole.cut(k(Piece::Fill), c, c, 1, 1),
        whole.cut(k(Piece::Right), c + 1, c, c, 1),
        whole.cut(k(Piece::BottomLeft), 0, c + 1, c, c),
        whole.cut(k(Piece::Bottom), c, c + 1, 1, c),
        whole.cut(k(Piece::BottomRight), c + 1, c + 1, c, c),
    ]
}

/// A soft shadow `t` square: a rounded rectangle inset by `reach`, blurred
/// out to the edge.
fn shadow_whole(b: usize) -> (SkinFrame, u32) {
    let reach = shadow_reach(b);
    let c = reach * 2 + corner(Style::Panel, b);
    let t = 2 * c + 1;
    let mut img = SkinFrame::new(0, 0, 0, t, t);
    let r = corner(Style::Panel, b) as f32;
    for y in 0..t {
        for x in 0..t {
            // Distance outside the inner rounded rectangle.
            let (x0, x1) = (reach as f32 + r, t as f32 - reach as f32 - r);
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let dx = (x0 - px).max(px - x1).max(0.0);
            let dy = (x0 - py).max(py - x1).max(0.0);
            let d = (dx * dx + dy * dy).sqrt() - r;
            let t = (1.0 - (d / reach as f32).clamp(-1.0, 1.0)) * 0.5;
            let a = t * t * (3.0 - 2.0 * t);
            img.set(x, y, [24, 14, 6, (a * 110.0).round() as u8]);
        }
    }
    (img, c)
}

/// The resources, in [`sim::kinds::Resource::ALL`] order, by the kind whose
/// new art draws each one's icon.
pub const RESOURCE_ART: [KindId; 4] = [
    sim::kinds::BERRY_BUSH,
    sim::kinds::TREE,
    sim::kinds::STONE_MINE,
    sim::kinds::GOLD_MINE,
];

/// A resource icon's height, HUD px.
pub const ICON_H: f32 = 16.0;
/// A portrait's height, HUD px.
pub const PORTRAIT_H: f32 = 22.0;

/// A colour frame the icons are made from: a kind's own new art.
pub struct Source {
    /// Whose.
    pub kind: KindId,
    /// Size.
    pub w: u32,
    /// Size.
    pub h: u32,
    /// RGBA8, straight alpha.
    pub rgba: Vec<u8>,
    /// The owner's colour.
    pub mask: Vec<u8>,
}

/// The kinds whose art the icons and portraits are made from: the
/// resources', and every kind that can be built or trained.
pub fn source_kinds() -> Vec<KindId> {
    let mut out: Vec<KindId> = RESOURCE_ART.to_vec();
    for k in sim::kinds::all() {
        if (k.buildable || k.trained_at.is_some()) && !out.contains(&k.id) && k.id < 1000 {
            out.push(k.id);
        }
    }
    out
}

/// `src` area-filtered into a frame no larger than `tw` x `th`, its
/// aspect kept, colour averaged by coverage, the mask with it; trimmed of
/// the shadow on the ground (nearly black and see-through).
fn shrink(src: &Source, key: (KindId, u8), tw: u32, th: u32) -> SkinFrame {
    // Trim to what is drawn and not shadow.
    let solid = |x: u32, y: u32| {
        let i = ((y * src.w + x) * 4) as usize;
        let p = &src.rgba[i..i + 4];
        p[3] > 160 || (p[3] > 24 && (p[0] as u32 + p[1] as u32 + p[2] as u32) > 60)
    };
    let (mut x0, mut y0, mut x1, mut y1) = (src.w, src.h, 0, 0);
    for y in 0..src.h {
        for x in 0..src.w {
            if solid(x, y) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return SkinFrame::new(key.0, key.1, 0, 1, 1);
    }
    let (sw, sh) = ((x1 - x0) as f32, (y1 - y0) as f32);
    let k = (tw as f32 / sw).min(th as f32 / sh);
    let (dw, dh) = (
        (sw * k).round().max(1.0) as u32,
        (sh * k).round().max(1.0) as u32,
    );
    let mut out = SkinFrame::new(key.0, key.1, 0, dw, dh);
    for dy in 0..dh {
        for dx in 0..dw {
            let (fx0, fx1) = (x0 as f32 + dx as f32 / k, x0 as f32 + (dx + 1) as f32 / k);
            let (fy0, fy1) = (y0 as f32 + dy as f32 / k, y0 as f32 + (dy + 1) as f32 / k);
            let mut acc = [0.0f32; 5];
            let mut wsum = 0.0;
            let mut y = fy0.floor() as u32;
            while (y as f32) < fy1 && y < y1 {
                let wy = (fy1.min(y as f32 + 1.0) - fy0.max(y as f32)).max(0.0);
                let mut x = fx0.floor() as u32;
                while (x as f32) < fx1 && x < x1 {
                    let wx = (fx1.min(x as f32 + 1.0) - fx0.max(x as f32)).max(0.0);
                    let w = wx * wy;
                    let i = (y * src.w + x) as usize;
                    let a = src.rgba[i * 4 + 3] as f32 / 255.0;
                    let a = if solid(x, y) { a } else { 0.0 };
                    for (c, v) in acc.iter_mut().take(3).enumerate() {
                        *v += src.rgba[i * 4 + c] as f32 * a * w;
                    }
                    acc[3] += a * w;
                    acc[4] += src.mask[i] as f32 * a * w;
                    wsum += w;
                    x += 1;
                }
                y += 1;
            }
            if acc[3] > 0.0 && wsum > 0.0 {
                let a = acc[3] / wsum;
                out.set(
                    dx,
                    dy,
                    [
                        (acc[0] / acc[3]).round() as u8,
                        (acc[1] / acc[3]).round() as u8,
                        (acc[2] / acc[3]).round() as u8,
                        (a * 255.0).round() as u8,
                    ],
                );
                out.mask[(dy * dw + dx) as usize] = (acc[4] / acc[3]).round() as u8;
            }
        }
    }
    out
}

/// Every frame the skin adds to the atlas: the glyphs in every ink, the
/// boxes' pieces and shadows at every bucket, the flat colours, the sheen
/// and the grain, and the icons and portraits made from `sources`.
pub fn frames(sources: &[Source]) -> Vec<SkinFrame> {
    let mut out = Vec::new();
    // Glyphs.
    let decoder = png::Decoder::new(std::io::Cursor::new(FONT_PNG));
    let mut reader = decoder.read_info().expect("assets/ui/font.png decodes");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .expect("assets/ui/font.png decodes");
    let (aw, chans) = (info.width, info.color_type.samples() as u32);
    let cov = |x: u32, y: u32| buf[((y * aw + x) * chans) as usize];
    for (bi, b) in FONT.bakes.iter().enumerate() {
        for ink in Ink::ALL.into_iter().filter(|&i| has_ink(b, i)) {
            let rgb = ink.rgba();
            for (code, g) in b.glyphs.iter().enumerate() {
                let Some(g) = g.filter(|g| g.w > 0) else {
                    continue;
                };
                let mut fr = SkinFrame::new(
                    FONT_ID + bi as KindId,
                    ink as u8,
                    code as u8,
                    g.w as u32,
                    g.h as u32,
                );
                for y in 0..g.h as u32 {
                    for x in 0..g.w as u32 {
                        let a = cov(g.x as u32 + x, g.y as u32 + y);
                        fr.set(x, y, [rgb[0], rgb[1], rgb[2], a]);
                    }
                }
                out.push(fr);
            }
        }
    }
    // Boxes and shadows.
    for (bi, &s) in BUCKETS.iter().enumerate() {
        for style in Style::ALL {
            let c = corner(style, bi);
            let whole = render_box(style, s, c as f32, 2 * c + 1, 2 * c + 1);
            out.extend(nine(&whole, BOX_ID + style as KindId * 8 + bi as KindId, c));
        }
        let (whole, c) = shadow_whole(bi);
        out.extend(nine(&whole, SHADOW_ID + bi as KindId, c));
        if bi >= ICON_BUCKETS {
            continue;
        }
        for (ri, &k) in RESOURCE_ART.iter().enumerate() {
            if let Some(src) = sources.iter().find(|x| x.kind == k) {
                let h = (ICON_H * s).round() as u32;
                out.push(shrink(
                    src,
                    (ICON_ID + ri as KindId, bi as u8),
                    h + h / 3,
                    h,
                ));
            }
        }
        for src in sources {
            let h = (PORTRAIT_H * s).round() as u32;
            out.push(shrink(src, (PORTRAIT_ID + src.kind, bi as u8), h * 2, h));
        }
    }
    // Flat colours.
    for (i, &c) in SOLIDS.iter().enumerate() {
        let mut fr = SkinFrame::new(SOLID_ID, i as u8, 0, 2, 2);
        for y in 0..2 {
            for x in 0..2 {
                fr.set(x, y, c);
            }
        }
        out.push(fr);
    }
    // The sheen: light at the top fading out by a third, shade gathering
    // over the last third. Index 0 is a tall one to stretch; index `h`
    // (up to [`SHEEN_EXACT`]) is `h` device px tall, laid texel for pixel
    // so that a short box's sheen is not a resampling of the tall one's
    // (which the GPU, sampling at pixel centres, and the rasteriser,
    // at pixel corners, would pick differently).
    let wash = |t: f32| -> Rgba {
        if t < 0.35 {
            let a = (1.0 - t / 0.35).powf(1.5) * 70.0;
            [255, 252, 240, a.round() as u8]
        } else if t > 0.6 {
            let a = ((t - 0.6) / 0.4).powf(1.3) * 34.0;
            [90, 56, 20, a.round() as u8]
        } else {
            [0, 0, 0, 0]
        }
    };
    for index in 0..=SHEEN_EXACT as u8 {
        let h = if index == 0 { 64 } else { index as u32 };
        let mut sheen = SkinFrame::new(MISC_ID, Misc::Sheen as u8, index, 1, h);
        for y in 0..h {
            sheen.set(0, y, wash((y as f32 + 0.5) / h as f32));
        }
        out.push(sheen);
    }
    // The grain: soft fibres and specks, a hash of the pixel, tiling.
    let mut grain = SkinFrame::new(MISC_ID, Misc::Grain as u8, 0, GRAIN, GRAIN);
    for y in 0..GRAIN {
        for x in 0..GRAIN {
            let h = hash(x, y);
            // Fibres run across: a slow wave along x, a quick one down.
            let fibre = ((x as f32 * 0.21 + (hash(x / 7, y) % 7) as f32).sin()
                * (y as f32 * 1.7).sin())
            .abs();
            let speck = if h.is_multiple_of(61) { 26.0 } else { 0.0 };
            let a = (fibre * 9.0 + (h % 9) as f32 + speck).min(40.0);
            grain.set(x, y, [120, 84, 40, a.round() as u8]);
        }
    }
    out.push(grain);
    out
}

fn hash(x: u32, y: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every text-on-surface pair the skin draws meets WCAG AA (4.5:1).
    #[test]
    fn every_text_pair_meets_wcag_aa() {
        for (text, surface, what) in TEXT_PAIRS {
            let r = contrast(text, surface);
            assert!(r >= 4.5, "{what}: {r:.2}:1");
        }
        // The reference values: black on white is 21:1.
        assert!((contrast([0, 0, 0, 255], [255, 255, 255, 255]) - 21.0).abs() < 0.01);
        // Every style's own ink reads on its own fill.
        for style in Style::ALL {
            let fill = layers(style, 1.0).last().unwrap().1;
            let r = contrast(style.ink().rgba(), fill);
            assert!(r >= 4.5, "{style:?}: {r:.2}:1");
        }
    }

    /// The font parses, both faces are baked at the HUD's sizes, and the
    /// text the UI draws is covered.
    #[test]
    fn the_font_covers_the_ui() {
        let f = &*FONT;
        assert!(f.bakes.len() >= 10);
        for face in [Face::Sans, Face::Serif] {
            assert!(f.bakes.iter().any(|b| b.face == face && !b.title));
        }
        let sans7 = f.pick(Face::Sans, 7.0, Ink::Ink).unwrap();
        assert_eq!(f.bakes[sans7].drawn, 7);
        for c in crate::font::CHARS.chars() {
            assert!(
                f.bakes[sans7].glyphs[c as usize].is_some(),
                "{c:?} is not baked"
            );
        }
        // The title sizes carry the title.
        let big = f.pick(Face::Serif, 56.0, Ink::Rubric).unwrap();
        assert!(f.bakes[big].title);
        for c in crate::shell::TITLE.chars() {
            assert!(f.bakes[big].glyphs[c as usize].is_some(), "{c:?}");
        }
        // A title bake is rubric only; ink falls back to the largest
        // heading size, and the serif has no cream at all.
        let ink = f.pick(Face::Serif, 56.0, Ink::Ink).unwrap();
        assert!(!f.bakes[ink].title && f.bakes[ink].drawn >= 28);
        let muted = f.pick(Face::Serif, 14.0, Ink::Muted).unwrap();
        assert_eq!(f.bakes[muted].face, Face::Sans, "falls back to the sans");
        assert!(f.pick(Face::Sans, 7.0, Ink::Cream).is_some());
    }

    /// Setting text: pens move right, kerning pulls AV together, lower
    /// case is set as capitals, and a doubled string is a little over
    /// twice as wide (the tracking between them).
    #[test]
    fn layout_measures_and_kerns() {
        let f = &*FONT;
        let b = f.pick(Face::Sans, 14.0, Ink::Ink).unwrap();
        let (pens, w) = f.layout(b, "FOOD 200");
        assert_eq!(pens.len(), 8);
        assert!(pens.windows(2).all(|p| p[1].1 > p[0].1));
        assert!(w > 0.0);
        assert_eq!(f.layout(b, "food 200"), f.layout(b, "FOOD 200"));
        let (_, one) = f.layout(b, "AB");
        let (_, two) = f.layout(b, "ABAB");
        assert!(two > 2.0 * one && two < 2.0 * one + 3.0, "{one} {two}");
        let (_, av) = f.layout(b, "AV");
        let (_, a) = f.layout(b, "A");
        let (_, v) = f.layout(b, "V");
        let track = (f.tracking[0] * f.bakes[b].size).round();
        assert!(
            av < a + v + track,
            "AV is kerned: {av} vs {}",
            a + v + track
        );
        assert_eq!(f.layout(b, "").1, 0.0);
    }

    /// An atlas with the skin and none of the new art: what the shell's
    /// screens draw on before a match, and enough for the HUD.
    static SKINNED: LazyLock<crate::Atlas> = LazyLock::new(|| {
        crate::Atlas::with_sheets_skinned(&[], crate::sprites::ATLAS_PAGE_HEIGHT, true)
    });

    /// The painter's text: drawn as wide as it measures, a fitted line
    /// within its width, crisp (every glyph on whole device pixels at any
    /// scale), and without the skin exactly the old font's arithmetic.
    #[test]
    fn the_painter_measures_fits_and_sets_text_on_the_pixel_grid() {
        use crate::hud::Painter;
        let old = crate::Atlas::placeholder();
        assert!(SKINNED.has_skin() && !old.has_skin());
        for s in [1.0, 1.25, 1.5, 2.0, 3.0] {
            let mut p = Painter::scaled(&SKINNED, s);
            let line = "ATTACK 5 PIERCE, RANGE 5";
            let w = p.measure(line, 1.0);
            assert!(w > 0.0);
            let drawn = p.type_in(10.0, 10.0, line, Ink::Ink, 1.0);
            assert!((drawn - w).abs() < 1e-3, "{drawn} {w}");
            for sp in &p.out {
                for v in [sp.x * s, sp.y * s, sp.w * s, sp.h * s] {
                    assert!((v - v.round()).abs() < 1e-3, "off the grid at {s}: {v}");
                }
            }
            // Headings are larger, and set in the serif.
            assert!(p.measure("SKIRMISH", 2.0) > p.measure("SKIRMISH", 1.0) * 1.5);
            // Fitting: within the width, a prefix, and whole when it fits.
            let fitted = p.fit(line, 60.0);
            assert!(p.measure(&fitted, 1.0) <= 60.0 && line.starts_with(&fitted));
            assert!(fitted.len() < line.len());
            assert_eq!(p.fit(line, w + 1.0), line);
        }
        // The skin's text is no wider than the old font's for the HUD's
        // words, so what fitted a box still fits it.
        let p = Painter::scaled(&SKINNED, 1.0);
        let q = Painter::scaled(&old, 1.0);
        for word in [
            "VILLAGER",
            "TOWN CENTER",
            "STONE AGE: 0/2 BUILDINGS",
            "400F 200W",
        ] {
            assert!(p.measure(word, 1.0) <= q.measure(word, 1.0), "{word}");
            assert_eq!(q.measure(word, 1.0), crate::font::width(word) as f32);
            assert_eq!(q.fit(word, 30.0), crate::hud::fit(word, 30.0));
        }
    }

    /// The skin changes only how things look: every button of the HUD
    /// and of every shell screen is where it was and does what it did, at
    /// every UI scale.
    #[test]
    fn hit_boxes_are_the_same_in_both_skins() {
        use crate::hud::{Hud, HudInput};
        use crate::shell::{self, ShellInput};
        use sim::{SimConfig, Simulation};
        let old = crate::Atlas::placeholder();
        let sim = Simulation::new(5, SimConfig::default());
        let settings = crate::Settings::default();
        let mine: Vec<u32> = sim
            .world()
            .slots()
            .filter(|s| sim.world().owner[s.index()] == 0)
            .map(|s| s.index() as u32)
            .collect();
        let notices = [crate::Notice {
            kind: crate::NoticeKind::Attack,
            text: "YOUR TOWN IS UNDER ATTACK".into(),
            tile: Some((10.0, 10.0)),
            tick: 0,
        }];
        for (w, h, scale) in [
            (1280.0, 720.0, 1.0),
            (2560.0, 1440.0, 2.0),
            (1920.0, 1080.0, 1.5),
        ] {
            let camera = crate::Camera::new(sim.map().width(), sim.map().height(), (w, h));
            for selected in [&mine[..1], &mine[..], &[]] {
                let input = HudInput {
                    sim: &sim,
                    player: 0,
                    camera: &camera,
                    selected,
                    build_mode: None,
                    fps: 60.0,
                    paused: false,
                    speed: 1.0,
                    status: "T0",
                    hover: Some((w * 0.3, h - 100.0 * scale)),
                    banner: Some(crate::hud::Banner::Victory),
                    ui_scale: scale,
                    help: false,
                    targeting: false,
                    defences: false,
                    settings: &settings,
                    notices: &notices,
                    hint: Some("VILLAGERS ARE IDLE"),
                    narration: None,
                    flash: [true, false, false, false],
                    perf: None,
                    fresh: &[],
                };
                let (a, b) = (Hud::build(&old, &input), Hud::build(&SKINNED, &input));
                assert!(!a.buttons.is_empty());
                assert_eq!(a.buttons, b.buttons, "the HUD at {w}x{h} x{scale}");
                assert_ne!(a.sprites, b.sprites, "the skin paints");
            }
            let input = ShellInput {
                viewport: (w, h),
                ui_scale: scale,
                hover: Some((w / 2.0, h / 2.0)),
                world_behind: false,
            };
            let setup = shell::Setup::new(3);
            let screens = |atlas: &crate::Atlas| {
                vec![
                    shell::title(atlas, &input),
                    shell::setup(atlas, &input, &setup, Some("too many players")),
                    shell::settings_screen(
                        atlas,
                        &input,
                        &settings,
                        shell::SettingsPage::Keys,
                        None,
                        Some("that key is taken"),
                    ),
                    shell::load_screen(atlas, &input, &[], None, false),
                    shell::pause_menu(atlas, &input, false, None, None, false, false),
                ]
            };
            for (a, b) in screens(&old).iter().zip(screens(&SKINNED).iter()) {
                assert_eq!(a.buttons, b.buttons, "a screen at {w}x{h} x{scale}");
                assert_eq!(a.preview, b.preview);
            }
        }
    }

    /// The nine pieces of a box put back together are the box.
    #[test]
    fn nine_pieces_rebuild_the_box() {
        for style in [Style::Panel, Style::Button, Style::Warn] {
            let b = bucket(1.5);
            let c = corner(style, b);
            let t = 2 * c + 1;
            let whole = render_box(style, 1.5, c as f32, t, t);
            let pieces = nine(&whole, 1, c);
            assert_eq!(pieces.len(), 9);
            assert_eq!(pieces[Piece::TopLeft as usize].get(0, 0)[3], 0, "rounded");
            let fill = pieces[Piece::Fill as usize].get(0, 0);
            assert_eq!(fill[3], 255);
            assert_eq!(
                pieces[Piece::BottomRight as usize].get(c - 1, c - 1),
                whole.get(t - 1, t - 1)
            );
        }
        assert_eq!(bucket(1.0), 0);
        assert_eq!(bucket(1.5), 2);
        assert_eq!(bucket(2.0), 3);
        assert_eq!(BUCKETS[bucket(1.875)], 2.0);
    }
}
