//! The sprite atlas, and the procedural placeholder art that fills it until
//! real sprites exist.
//!
//! Placeholders are deliberately built through the *real* pipeline: palette
//! indices, per-frame anchors, five authored facings mirrored to eight. When
//! drawn art arrives it replaces the canvases; nothing downstream changes.

use sim::entity::KindId;
use sim::kinds;
use std::collections::HashMap;

use crate::font;
use crate::iso;
use crate::palette::*;

/// Reserved kind ids for UI frames. Real kinds stay below these.
pub const UI_SOLID: KindId = 59_000;
/// Selection ring for a footprint of `n` tiles: `UI_RING + n` (0 = unit).
pub const UI_RING: KindId = 58_000;
/// Placement footprint, valid: `UI_FOOT_OK + footprint`.
pub const UI_FOOT_OK: KindId = 58_100;
/// Placement footprint, blocked: `UI_FOOT_BAD + footprint`.
pub const UI_FOOT_BAD: KindId = 58_200;
/// Construction site: `UI_SITE + footprint`.
pub const UI_SITE: KindId = 58_300;
/// Light glyphs: `UI_GLYPH + index into font::CHARS`.
pub const UI_GLYPH: KindId = 60_000;
/// Dark glyphs: `UI_GLYPH_DARK + index into font::CHARS`.
pub const UI_GLYPH_DARK: KindId = 61_000;

/// Palette indices that get a solid fill frame, for panels and bars.
pub const SOLIDS: [u8; 20] = [
    BLACK,
    WHITE,
    SHADOW,
    BROWN_DARK,
    BROWN,
    TAN,
    SAND,
    GREEN_DARK,
    GREEN,
    GREEN_LIGHT,
    GREY_DARK,
    GREY,
    GREY_LIGHT,
    GOLD_DARK,
    GOLD,
    RED,
    RED_DARK,
    P_BASE,
    P_LIGHT,
    P_DARK,
];

/// One image in the atlas.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Frame {
    /// Which kind.
    pub kind: KindId,
    /// Authored facing (`facing8` index); 0 for static things.
    pub facing: u8,
    /// Atlas rectangle.
    pub x: u16,
    /// Atlas rectangle.
    pub y: u16,
    /// Atlas rectangle.
    pub w: u16,
    /// Atlas rectangle.
    pub h: u16,
    /// Pixel of the frame that sits on the entity's ground point.
    pub anchor_x: i16,
    /// Pixel of the frame that sits on the entity's ground point.
    pub anchor_y: i16,
}

/// Facings that are authored; the other three are mirrors.
pub const AUTHORED: [u8; 5] = [1, 2, 3, 4, 5];

/// Which authored facing draws `facing8`, and whether to mirror it.
pub const fn source_facing(facing8: u8) -> (u8, bool) {
    match facing8 & 7 {
        0 => (2, true),
        7 => (3, true),
        6 => (4, true),
        f => (f, false),
    }
}

/// An indexed-colour texture atlas plus its frame table.
pub struct Atlas {
    /// Texture width.
    pub width: u32,
    /// Texture height.
    pub height: u32,
    /// Palette indices, row-major.
    pub indices: Vec<u8>,
    frames: Vec<Frame>,
    lookup: HashMap<(KindId, u8), usize>,
}

impl Atlas {
    /// Finds the frame for a kind at a facing, and whether to mirror it.
    /// Static kinds ignore the facing.
    pub fn frame(&self, kind: KindId, facing8: u8) -> Option<(&Frame, bool)> {
        if let Some(&i) = self.lookup.get(&(kind, 0)) {
            return Some((&self.frames[i], false));
        }
        let (f, flip) = source_facing(facing8);
        self.lookup
            .get(&(kind, f))
            .map(|&i| (&self.frames[i], flip))
    }

    /// Every frame.
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    /// Palette index at an atlas pixel; 0 outside.
    pub fn index_at(&self, x: u32, y: u32) -> u8 {
        if x < self.width && y < self.height {
            self.indices[(y * self.width + x) as usize]
        } else {
            0
        }
    }

    /// Builds the placeholder atlas: coloured shapes for every kind the
    /// simulation knows, at the sizes the art spec calls for, plus the UI
    /// frames (solid fills, glyphs, selection rings, placement footprints).
    pub fn placeholder() -> Atlas {
        let mut canvases: Vec<(KindId, u8, Canvas)> = Vec::new();
        for k in kinds::all() {
            if k.mobile {
                for f in AUTHORED {
                    canvases.push((k.id, f, draw_kind(k.id, f)));
                }
            } else {
                canvases.push((k.id, 0, draw_kind(k.id, 1)));
            }
        }
        for idx in SOLIDS {
            let mut c = Canvas::new(4, 4, (0, 0));
            c.rect(0, 0, 4, 4, idx);
            canvases.push((UI_SOLID + idx as KindId, 0, c));
        }
        for fp in 0..=3u32 {
            canvases.push((UI_RING + fp as KindId, 0, ring(fp)));
            if fp > 0 {
                canvases.push((UI_FOOT_OK + fp as KindId, 0, footprint(fp, GREEN_LIGHT)));
                canvases.push((UI_FOOT_BAD + fp as KindId, 0, footprint(fp, RED)));
                canvases.push((UI_SITE + fp as KindId, 0, site(fp)));
            }
        }
        for (i, ch) in font::CHARS.chars().enumerate() {
            canvases.push((UI_GLYPH + i as KindId, 0, glyph(ch, WHITE)));
            canvases.push((UI_GLYPH_DARK + i as KindId, 0, glyph(ch, BLACK)));
        }
        pack(canvases, 1024)
    }

    /// A 4×4 fill of a palette index, for stretching into rectangles.
    pub fn solid(&self, idx: u8) -> &Frame {
        self.frame(UI_SOLID + idx as KindId, 0)
            .or_else(|| self.frame(UI_SOLID + BLACK as KindId, 0))
            .map(|(f, _)| f)
            .expect("solid frames exist")
    }

    /// The glyph for a character, light or dark.
    pub fn glyph(&self, c: char, dark: bool) -> Option<&Frame> {
        let i = font::CHARS.find(c.to_ascii_uppercase())? as KindId;
        let base = if dark { UI_GLYPH_DARK } else { UI_GLYPH };
        self.frame(base + i, 0).map(|(f, _)| f)
    }

    /// Selection ring for a footprint (0 for units).
    pub fn ring(&self, footprint: u8) -> Option<&Frame> {
        self.frame(UI_RING + footprint.min(3) as KindId, 0)
            .map(|(f, _)| f)
    }

    /// Construction site for a footprint.
    pub fn site(&self, footprint: u8) -> Option<&Frame> {
        self.frame(UI_SITE + footprint.clamp(1, 3) as KindId, 0)
            .map(|(f, _)| f)
    }

    /// Placement footprint overlay.
    pub fn footprint(&self, footprint: u8, ok: bool) -> Option<&Frame> {
        let base = if ok { UI_FOOT_OK } else { UI_FOOT_BAD };
        self.frame(base + footprint.clamp(1, 3) as KindId, 0)
            .map(|(f, _)| f)
    }
}

/// A selection ellipse in the player ramp, anchored at its centre.
fn ring(footprint: u32) -> Canvas {
    let (w, h) = if footprint == 0 {
        (40, 20)
    } else {
        (64 * footprint + 8, 32 * footprint + 4)
    };
    let mut c = Canvas::new(w, h, ((w / 2) as i16, (h / 2) as i16));
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let (rx, ry) = (cx - 1.0, cy - 1.0);
    c.ellipse(cx, cy, rx, ry, P_LIGHT);
    c.ellipse(cx, cy, rx - 2.0, ry - 1.5, TRANSPARENT);
    c
}

/// A translucent-looking footprint diamond in a single colour.
fn footprint(fp: u32, idx: u8) -> Canvas {
    let (w, h) = (64 * fp, 32 * fp);
    let mut c = Canvas::new(w, h, ((w / 2) as i16, (h / 2) as i16));
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    c.diamond(cx, cy, cx - 1.0, cy - 1.0, idx);
    // Hatch so the ground shows through.
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if (x + y) % 3 != 0 {
                let i = (y as u32 * w + x as u32) as usize;
                if c.px[i] == idx {
                    c.px[i] = TRANSPARENT;
                }
            }
        }
    }
    c.diamond(cx, cy, cx - 1.0, cy - 1.0, BLACK);
    c.diamond(cx, cy, cx - 3.0, cy - 2.0, TRANSPARENT);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let i = (y as u32 * w + x as u32) as usize;
            if c.px[i] == TRANSPARENT && (x + y) % 3 == 0 {
                // Re-apply the hatch fill inside the outline.
                let dx = (x as f32 + 0.5 - cx).abs() / (cx - 3.0);
                let dy = (y as f32 + 0.5 - cy).abs() / (cy - 2.0);
                if dx + dy <= 1.0 {
                    c.px[i] = idx;
                }
            }
        }
    }
    c
}

/// A 5×7 glyph in one colour.
fn glyph(ch: char, idx: u8) -> Canvas {
    let mut c = Canvas::new(font::GLYPH_W, font::GLYPH_H, (0, 0));
    for (y, row) in font::rows(ch).iter().enumerate() {
        for (x, p) in row.chars().enumerate() {
            if p == '#' {
                c.set(x as i32, y as i32, idx);
            }
        }
    }
    c
}

/// Shelf-packs canvases into an atlas of the given width.
fn pack(canvases: Vec<(KindId, u8, Canvas)>, width: u32) -> Atlas {
    let mut frames = Vec::new();
    let mut lookup = HashMap::new();
    let mut placed: Vec<(u32, u32, Canvas)> = Vec::new();
    let (mut x, mut y, mut shelf_h) = (0u32, 0u32, 0u32);
    for (kind, facing, c) in canvases {
        if x + c.w > width {
            x = 0;
            y += shelf_h + 1;
            shelf_h = 0;
        }
        frames.push(Frame {
            kind,
            facing,
            x: x as u16,
            y: y as u16,
            w: c.w as u16,
            h: c.h as u16,
            anchor_x: c.anchor.0,
            anchor_y: c.anchor.1,
        });
        lookup.insert((kind, facing), frames.len() - 1);
        shelf_h = shelf_h.max(c.h);
        placed.push((x, y, c));
        x += placed.last().unwrap().2.w + 1;
    }
    let height = (y + shelf_h).next_power_of_two().max(1);
    let mut indices = vec![0u8; (width * height) as usize];
    for (px, py, c) in placed {
        for row in 0..c.h {
            let src = (row * c.w) as usize;
            let dst = ((py + row) * width + px) as usize;
            indices[dst..dst + c.w as usize].copy_from_slice(&c.px[src..src + c.w as usize]);
        }
    }
    Atlas {
        width,
        height,
        indices,
        frames,
        lookup,
    }
}

/// An indexed-colour drawing surface.
pub struct Canvas {
    w: u32,
    h: u32,
    anchor: (i16, i16),
    px: Vec<u8>,
}

impl Canvas {
    fn new(w: u32, h: u32, anchor: (i16, i16)) -> Canvas {
        Canvas {
            w,
            h,
            anchor,
            px: vec![0; (w * h) as usize],
        }
    }

    fn set(&mut self, x: i32, y: i32, idx: u8) {
        if x >= 0 && y >= 0 && (x as u32) < self.w && (y as u32) < self.h {
            self.px[(y as u32 * self.w + x as u32) as usize] = idx;
        }
    }

    fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, idx: u8) {
        let (x0, x1) = ((cx - rx).floor() as i32, (cx + rx).ceil() as i32);
        let (y0, y1) = ((cy - ry).floor() as i32, (cy + ry).ceil() as i32);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = (x as f32 + 0.5 - cx) / rx;
                let dy = (y as f32 + 0.5 - cy) / ry;
                if dx * dx + dy * dy <= 1.0 {
                    self.set(x, y, idx);
                }
            }
        }
    }

    fn circle(&mut self, cx: f32, cy: f32, r: f32, idx: u8) {
        self.ellipse(cx, cy, r, r, idx);
    }

    fn rect(&mut self, x0: i32, y0: i32, w: i32, h: i32, idx: u8) {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                self.set(x, y, idx);
            }
        }
    }

    /// Fills a convex polygon given in order.
    fn convex(&mut self, pts: &[(f32, f32)], idx: u8) {
        let y0 = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor() as i32;
        let y1 = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil() as i32;
        for y in y0..=y1 {
            let sy = y as f32 + 0.5;
            let mut xs: Vec<f32> = Vec::new();
            for i in 0..pts.len() {
                let (ax, ay) = pts[i];
                let (bx, by) = pts[(i + 1) % pts.len()];
                if (ay <= sy && by > sy) || (by <= sy && ay > sy) {
                    xs.push(ax + (sy - ay) / (by - ay) * (bx - ax));
                }
            }
            if xs.len() >= 2 {
                let lo = xs.iter().cloned().fold(f32::MAX, f32::min);
                let hi = xs.iter().cloned().fold(f32::MIN, f32::max);
                for x in lo.floor() as i32..=hi.ceil() as i32 {
                    if (x as f32 + 0.5) >= lo && (x as f32 + 0.5) <= hi {
                        self.set(x, y, idx);
                    }
                }
            }
        }
    }

    fn diamond(&mut self, cx: f32, cy: f32, hw: f32, hh: f32, idx: u8) {
        self.convex(
            &[(cx, cy - hh), (cx + hw, cy), (cx, cy + hh), (cx - hw, cy)],
            idx,
        );
    }
}

/// Screen-space unit vector for an authored facing.
fn facing_dir(facing: u8) -> (f32, f32) {
    // facing8: 0=+x, 1=+x+y, 2=+y, 3=-x+y, 4=-x, 5=-x-y, 6=-y, 7=+x-y
    let (wx, wy) = match facing & 7 {
        0 => (1.0, 0.0),
        1 => (1.0, 1.0),
        2 => (0.0, 1.0),
        3 => (-1.0, 1.0),
        4 => (-1.0, 0.0),
        5 => (-1.0, -1.0),
        6 => (0.0, -1.0),
        _ => (1.0, -1.0),
    };
    iso::direction(wx, wy)
}

fn draw_kind(kind: KindId, facing: u8) -> Canvas {
    let (dx, dy) = facing_dir(facing);
    match kind {
        kinds::VILLAGER => {
            let mut c = Canvas::new(40, 48, (20, 42));
            c.ellipse(20.0, 42.0, 11.0, 5.0, SHADOW);
            c.ellipse(20.0, 30.0, 8.0, 12.0, BLACK);
            c.ellipse(20.0, 30.0, 7.0, 11.0, P_BASE);
            c.ellipse(22.0, 27.0, 3.5, 6.0, P_LIGHT);
            c.ellipse(16.5, 32.0, 3.0, 7.0, P_DARK);
            c.circle(20.0, 14.0, 7.0, BLACK);
            c.circle(20.0, 14.0, 6.0, SKIN);
            c.circle(20.0 + dx * 5.0, 14.0 + dy * 5.0, 2.0, BLACK);
            c
        }
        kinds::SCOUT => {
            let mut c = Canvas::new(56, 56, (28, 50));
            c.ellipse(28.0, 50.0, 19.0, 7.0, SHADOW);
            c.ellipse(28.0, 37.0, 18.0, 10.0, BLACK);
            c.ellipse(28.0, 37.0, 17.0, 9.0, BROWN);
            c.circle(28.0 + dx * 15.0, 37.0 + dy * 11.0, 5.0, BLACK);
            c.circle(28.0 + dx * 15.0, 37.0 + dy * 11.0, 4.0, BROWN_DARK);
            c.ellipse(28.0, 32.0, 8.0, 5.0, P_BASE);
            c.ellipse(28.0, 24.0, 5.0, 7.0, BLACK);
            c.ellipse(28.0, 24.0, 4.0, 6.0, P_LIGHT);
            c.circle(28.0, 16.0, 5.0, BLACK);
            c.circle(28.0, 16.0, 4.0, SKIN);
            c
        }
        kinds::GAZELLE => {
            let mut c = Canvas::new(40, 32, (20, 28));
            c.ellipse(20.0, 28.0, 12.0, 4.0, SHADOW);
            c.ellipse(20.0, 18.0, 13.0, 7.0, BLACK);
            c.ellipse(20.0, 18.0, 12.0, 6.0, HIDE);
            c.ellipse(20.0, 16.0, 6.0, 3.0, WHITE);
            c.circle(20.0 + dx * 11.0, 16.0 + dy * 9.0, 3.5, BLACK);
            c.circle(20.0 + dx * 11.0, 16.0 + dy * 9.0, 2.5, BROWN);
            c
        }
        kinds::TREE => {
            let mut c = Canvas::new(64, 64, (32, 58));
            c.ellipse(32.0, 58.0, 15.0, 6.0, SHADOW);
            c.rect(28, 32, 8, 27, BLACK);
            c.rect(29, 33, 6, 25, BROWN_DARK);
            c.circle(32.0, 28.0, 23.0, BLACK);
            c.circle(34.0, 30.0, 21.0, GREEN_DARK);
            c.circle(30.0, 26.0, 19.0, GREEN);
            c.circle(25.0, 20.0, 8.0, GREEN_LIGHT);
            c
        }
        kinds::BERRY_BUSH => {
            let mut c = Canvas::new(48, 40, (24, 34));
            c.ellipse(24.0, 34.0, 16.0, 5.0, SHADOW);
            c.ellipse(24.0, 22.0, 19.0, 13.0, BLACK);
            c.ellipse(24.0, 22.0, 18.0, 12.0, GREEN_DARK);
            c.ellipse(22.0, 20.0, 14.0, 9.0, GREEN);
            for (bx, by) in [
                (14.0, 20.0),
                (22.0, 15.0),
                (30.0, 21.0),
                (19.0, 27.0),
                (28.0, 28.0),
                (35.0, 26.0),
            ] {
                c.circle(bx, by, 2.2, RED);
            }
            c
        }
        kinds::GOLD_MINE | kinds::STONE_MINE => {
            let mut c = Canvas::new(64, 44, (32, 30));
            c.diamond(32.0, 30.0, 30.0, 14.0, SHADOW);
            c.convex(
                &[
                    (4.0, 30.0),
                    (20.0, 8.0),
                    (46.0, 6.0),
                    (60.0, 28.0),
                    (32.0, 42.0),
                ],
                BLACK,
            );
            c.convex(
                &[
                    (6.0, 30.0),
                    (21.0, 10.0),
                    (45.0, 8.0),
                    (58.0, 28.0),
                    (32.0, 40.0),
                ],
                GREY,
            );
            c.convex(
                &[(8.0, 29.0), (21.0, 11.0), (34.0, 12.0), (30.0, 30.0)],
                GREY_LIGHT,
            );
            c.convex(
                &[(34.0, 14.0), (45.0, 10.0), (56.0, 28.0), (36.0, 36.0)],
                GREY_DARK,
            );
            let (a, b) = if kind == kinds::GOLD_MINE {
                (GOLD, GOLD_LIGHT)
            } else {
                (GREY_LIGHT, WHITE)
            };
            for (fx, fy) in [
                (18.0, 24.0),
                (30.0, 18.0),
                (42.0, 26.0),
                (26.0, 33.0),
                (48.0, 16.0),
            ] {
                c.circle(fx, fy, 3.0, a);
                c.circle(fx - 1.0, fy - 1.0, 1.2, b);
            }
            c
        }
        kinds::TOWN_CENTER => building(192, 144, 3, 58.0, true),
        kinds::HOUSE => building(128, 96, 2, 34.0, false),
        kinds::STOREHOUSE => storehouse(),
        _ => {
            let mut c = Canvas::new(32, 32, (16, 28));
            c.diamond(16.0, 28.0, 14.0, 7.0, SHADOW);
            c.rect(8, 6, 16, 22, BLACK);
            c.rect(9, 7, 14, 20, P_BASE);
            c
        }
    }
}

/// The storehouse: a low, wide shed with a thatched roof and a player-colour
/// banner, so it reads as "goods go here" rather than "people live here".
fn storehouse() -> Canvas {
    let (w, h) = (128, 80);
    let cx = 64.0;
    let base_cy = h as f32 - 32.0;
    let mut c = Canvas::new(w, h, (cx as i16, base_cy as i16));
    c.diamond(cx, base_cy, 63.0, 31.0, BLACK);
    c.diamond(cx, base_cy, 62.0, 30.0, SAND);
    let rise = 18.0;
    let inset = 0.85;
    let (bl, bb, br) = (
        (cx - 63.0 * inset, base_cy),
        (cx, base_cy + 31.0 * inset),
        (cx + 63.0 * inset, base_cy),
    );
    let up = |p: (f32, f32), d: f32| (p.0, p.1 - d);
    c.convex(&[bl, bb, up(bb, rise), up(bl, rise)], BLACK);
    c.convex(&[bb, br, up(br, rise), up(bb, rise)], BLACK);
    c.convex(
        &[
            up(bl, 1.0),
            up(bb, 1.0),
            up(bb, rise - 1.0),
            up(bl, rise - 1.0),
        ],
        TAN,
    );
    c.convex(
        &[
            up(bb, 1.0),
            up(br, 1.0),
            up(br, rise - 1.0),
            up(bb, rise - 1.0),
        ],
        BROWN,
    );
    // Thatch: a diamond roof in browns with a ridge.
    let roof_cy = base_cy - rise;
    c.diamond(cx, roof_cy, 63.0 * inset + 1.0, 31.0 * inset + 1.0, BLACK);
    c.diamond(cx, roof_cy, 63.0 * inset, 31.0 * inset, BROWN);
    c.diamond(cx - 12.0, roof_cy - 6.0, 30.0, 14.0, TAN);
    c.rect(
        cx as i32 - 1,
        (roof_cy - 31.0 * inset) as i32,
        2,
        (31.0 * inset * 2.0) as i32,
        BROWN_DARK,
    );
    // Sacks and a banner.
    c.ellipse(cx - 30.0, base_cy + 6.0, 7.0, 4.0, BLACK);
    c.ellipse(cx - 30.0, base_cy + 5.0, 6.0, 3.0, GOLD_DARK);
    c.ellipse(cx - 18.0, base_cy + 12.0, 7.0, 4.0, BLACK);
    c.ellipse(cx - 18.0, base_cy + 11.0, 6.0, 3.0, TAN);
    c.rect(cx as i32 + 30, roof_cy as i32 - 22, 2, 30, BLACK);
    c.convex(
        &[
            (cx + 32.0, roof_cy - 22.0),
            (cx + 46.0, roof_cy - 18.0),
            (cx + 32.0, roof_cy - 12.0),
        ],
        BLACK,
    );
    c.convex(
        &[
            (cx + 33.0, roof_cy - 21.0),
            (cx + 43.0, roof_cy - 18.0),
            (cx + 33.0, roof_cy - 14.0),
        ],
        P_BASE,
    );
    c
}

/// A construction site: the footprint pegged out, with corner posts.
fn site(fp: u32) -> Canvas {
    let (w, h) = (64 * fp, 32 * fp + 20);
    let cx = w as f32 / 2.0;
    let base_cy = h as f32 - 16.0 * fp as f32;
    let mut c = Canvas::new(w, h, (cx as i16, base_cy as i16));
    let (hw, hh) = (cx - 1.0, 16.0 * fp as f32 - 1.0);
    c.diamond(cx, base_cy, hw, hh, BLACK);
    c.diamond(cx, base_cy, hw - 1.0, hh - 1.0, BROWN_DARK);
    c.diamond(cx, base_cy, hw - 4.0, hh - 3.0, TAN);
    for (px, py) in [
        (cx, base_cy - hh + 4.0),
        (cx + hw - 6.0, base_cy),
        (cx, base_cy + hh - 4.0),
        (cx - hw + 6.0, base_cy),
    ] {
        c.rect(px as i32 - 2, py as i32 - 18, 4, 20, BLACK);
        c.rect(px as i32 - 1, py as i32 - 17, 2, 18, BROWN);
    }
    c
}

/// A block building: a footprint diamond, two shaded walls, a player-colour
/// roof diamond, and a door. `rise` is wall height in px.
fn building(w: u32, h: u32, footprint: u32, rise: f32, flag: bool) -> Canvas {
    let cx = w as f32 / 2.0;
    let base_cy = h as f32 - footprint as f32 * iso::TILE_H / 2.0;
    let hw = footprint as f32 * iso::TILE_W / 2.0 - 1.0;
    let hh = footprint as f32 * iso::TILE_H / 2.0 - 1.0;
    let mut c = Canvas::new(w, h, (cx as i16, base_cy as i16));
    // Footprint on the ground.
    c.diamond(cx, base_cy, hw, hh, BLACK);
    c.diamond(cx, base_cy, hw - 1.0, hh - 1.0, TAN);
    // Walls rise from the left, bottom and right base corners; the building
    // block is inset a little from the footprint.
    let inset = 0.8;
    let (bl, bb, br) = (
        (cx - hw * inset, base_cy),
        (cx, base_cy + hh * inset),
        (cx + hw * inset, base_cy),
    );
    let up = |p: (f32, f32)| (p.0, p.1 - rise);
    c.convex(&[bl, bb, up(bb), up(bl)], BLACK);
    c.convex(&[bb, br, up(br), up(bb)], BLACK);
    let ins = |p: (f32, f32), d: f32| (p.0, p.1 - d);
    c.convex(
        &[
            ins(bl, 1.0),
            ins(bb, 1.0),
            ins(up(bb), -1.0),
            ins(up(bl), -1.0),
        ],
        BROWN,
    );
    c.convex(
        &[
            ins(bb, 1.0),
            ins(br, 1.0),
            ins(up(br), -1.0),
            ins(up(bb), -1.0),
        ],
        BROWN_DARK,
    );
    // Roof.
    let roof_cy = base_cy - rise;
    c.diamond(cx, roof_cy, hw * inset + 1.0, hh * inset + 1.0, BLACK);
    c.diamond(cx, roof_cy, hw * inset, hh * inset, P_BASE);
    c.diamond(
        cx - hw * inset * 0.25,
        roof_cy - hh * inset * 0.25,
        hw * inset * 0.45,
        hh * inset * 0.45,
        P_LIGHT,
    );
    // Door on the right-hand wall.
    let (dx, dy) = ((bb.0 + br.0) * 0.5, (bb.1 + br.1) * 0.5);
    c.convex(
        &[
            (dx - 5.0, dy - 2.0),
            (dx + 5.0, dy - 7.0),
            (dx + 5.0, dy - 7.0 - rise * 0.45),
            (dx - 5.0, dy - 2.0 - rise * 0.45),
        ],
        BLACK,
    );
    if flag {
        // Pole rises from the roof centre; the canvas is sized so it fits.
        let top = roof_cy - 30.0;
        c.rect(cx as i32 - 1, top as i32, 3, 30, BLACK);
        c.convex(
            &[
                (cx + 2.0, top),
                (cx + 20.0, top + 5.0),
                (cx + 2.0, top + 11.0),
            ],
            BLACK,
        );
        c.convex(
            &[
                (cx + 3.0, top + 1.0),
                (cx + 17.0, top + 5.0),
                (cx + 3.0, top + 9.0),
            ],
            P_LIGHT,
        );
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirror_mapping_covers_all_eight_facings() {
        for f in 0..8u8 {
            let (src, flip) = source_facing(f);
            assert!(
                AUTHORED.contains(&src),
                "facing {f} maps to unauthored {src}"
            );
            assert_eq!(flip, matches!(f, 0 | 6 | 7));
        }
        assert_eq!(source_facing(0), (2, true));
        assert_eq!(source_facing(7), (3, true));
        assert_eq!(source_facing(6), (4, true));
    }

    #[test]
    fn placeholder_atlas_has_every_kind_at_spec_sizes() {
        let a = Atlas::placeholder();
        assert_eq!(a.width, 1024);
        assert!(a.height <= 1024, "atlas too tall: {}", a.height);
        for k in kinds::all() {
            for f in 0..8u8 {
                let (frame, flip) = a
                    .frame(k.id, f)
                    .unwrap_or_else(|| panic!("no frame for {}", k.name));
                assert_eq!(frame.kind, k.id);
                if k.mobile {
                    assert_eq!(flip, matches!(f, 0 | 6 | 7));
                } else {
                    assert!(!flip);
                }
                assert!(frame.anchor_x >= 0 && (frame.anchor_x as u16) < frame.w);
                assert!(frame.anchor_y >= 0 && (frame.anchor_y as u16) <= frame.h);
            }
        }
        let (tc, _) = a.frame(kinds::TOWN_CENTER, 0).unwrap();
        assert_eq!((tc.w, tc.h), (192, 144));
        assert_eq!(
            (tc.anchor_x, tc.anchor_y),
            (96, 96),
            "anchor is the footprint centre"
        );
        let (v, _) = a.frame(kinds::VILLAGER, 1).unwrap();
        assert_eq!((v.w, v.h), (40, 48));
        assert!(a.frame(9999, 0).is_none());
    }

    #[test]
    fn frames_do_not_overlap_and_contain_paint() {
        let a = Atlas::placeholder();
        let frames = a.frames();
        for (i, f) in frames.iter().enumerate() {
            let painted = (0..f.h as u32)
                .flat_map(|y| (0..f.w as u32).map(move |x| (x, y)))
                .filter(|&(x, y)| a.index_at(f.x as u32 + x, f.y as u32 + y) != 0)
                .count();
            if f.kind < UI_RING {
                assert!(
                    painted > (f.w as usize * f.h as usize) / 8,
                    "frame {i} ({}) is nearly empty",
                    f.kind
                );
            } else {
                let space = UI_GLYPH + 36;
                let dark_space = UI_GLYPH_DARK + 36;
                assert!(
                    painted > 0 || f.kind == space || f.kind == dark_space,
                    "UI frame {i} ({}) is empty",
                    f.kind
                );
            }
            for g in &frames[i + 1..] {
                let disjoint =
                    f.x + f.w <= g.x || g.x + g.w <= f.x || f.y + f.h <= g.y || g.y + g.h <= f.y;
                assert!(disjoint, "frames overlap: {f:?} {g:?}");
            }
        }
    }

    #[test]
    fn ui_frames_exist() {
        let a = Atlas::placeholder();
        assert_eq!(a.solid(BLACK).w, 4);
        assert!(
            a.glyph('A', false).is_some()
                && a.glyph('7', true).is_some()
                && a.glyph('#', false).is_none()
        );
        let (gw, gh) = (
            a.glyph('A', false).unwrap().w,
            a.glyph('A', false).unwrap().h,
        );
        assert_eq!((gw as u32, gh as u32), (font::GLYPH_W, font::GLYPH_H));
        assert_eq!(a.ring(0).unwrap().w, 40);
        assert_eq!(a.ring(3).unwrap().w, 200);
        assert_eq!(a.footprint(2, true).unwrap().w, 128);
        assert!(a.footprint(2, false).is_some());
        assert_eq!(a.site(3).unwrap().w, 192);
        assert_eq!(
            a.frame(kinds::STOREHOUSE, 0).unwrap().0.w,
            128,
            "storehouse has its own sprite"
        );
        // A glyph is drawn in its colour only.
        let g = a.glyph('I', false).unwrap();
        let mut seen = std::collections::HashSet::new();
        for y in 0..g.h as u32 {
            for x in 0..g.w as u32 {
                seen.insert(a.index_at(g.x as u32 + x, g.y as u32 + y));
            }
        }
        assert_eq!(seen, [TRANSPARENT, WHITE].into_iter().collect());
    }

    #[test]
    fn facings_differ_from_each_other() {
        let a = Atlas::placeholder();
        let pixels = |facing: u8| {
            let (f, _) = a.frame(kinds::VILLAGER, facing).unwrap();
            (0..f.h as u32)
                .flat_map(|y| (0..f.w as u32).map(move |x| (x, y)))
                .map(|(x, y)| a.index_at(f.x as u32 + x, f.y as u32 + y))
                .collect::<Vec<_>>()
        };
        assert_ne!(pixels(1), pixels(3));
        assert_ne!(pixels(2), pixels(5));
    }
}
