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
/// The age-up glow, a gold footprint, at this id plus the footprint.
pub const UI_FOOT_GLOW: KindId = 58_400;
/// An arrow in flight.
pub const UI_ARROW: KindId = 58_500;
/// Rubble where a building stood, by footprint (1..=3).
pub const UI_RUBBLE: KindId = 58_600;
/// Light glyphs: `UI_GLYPH + index into font::CHARS`.
pub const UI_GLYPH: KindId = 60_000;
/// Dark glyphs: `UI_GLYPH_DARK + index into font::CHARS`.
pub const UI_GLYPH_DARK: KindId = 61_000;
/// Gold glyphs, for the age banner, at this id plus the character index.
pub const UI_GLYPH_GOLD: KindId = 62_000;
/// Age-styled placeholder variants live above this: `AGE_VARIANT_BASE +
/// age * 1000 + kind`. See [`Atlas::variant`].
pub const AGE_VARIANT_BASE: KindId = 50_000;

/// The colour text is drawn in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ink {
    /// On dark panels.
    White,
    /// On light buttons.
    Black,
    /// The age banner.
    Gold,
}

/// Palette indices that get a solid fill frame, for panels and bars.
pub const SOLIDS: &[u8] = &[
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
    GOLD_LIGHT,
    RED,
    RED_DARK,
    P_BASE,
    P_LIGHT,
    P_DARK,
];

/// Animations a kind may have. Military placeholders include a brief strike
/// and all mobile placeholders have corpses; rendered sets supply their own.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Anim {
    /// Standing.
    Idle = 0,
    /// Walking.
    Walk = 1,
    /// Working or attacking (the art's `attack`).
    Work = 2,
    /// Dying.
    Death = 3,
    /// Lying dead.
    Decay = 4,
}

impl Anim {
    /// The art pipeline's name for this animation.
    pub fn from_name(name: &str) -> Option<Anim> {
        match name {
            "idle" => Some(Anim::Idle),
            "walk" => Some(Anim::Walk),
            "attack" => Some(Anim::Work),
            "death" => Some(Anim::Death),
            "decay" => Some(Anim::Decay),
            _ => None,
        }
    }
}

/// Timing of one kind's animation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AnimInfo {
    /// Frame count.
    pub frames: u32,
    /// Milliseconds per frame.
    pub frame_ms: u32,
    /// Whether it repeats.
    pub loops: bool,
}

/// One image in the atlas.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Frame {
    /// Which kind.
    pub kind: KindId,
    /// Authored facing (`facing8` index); 0 for static things.
    pub facing: u8,
    /// Which animation.
    pub anim: Anim,
    /// Frame index within the animation.
    pub index: u8,
    /// Authored pixels per 1× pixel. A frame drawn at 1× is `w / scale` wide.
    pub scale: u8,
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

impl Frame {
    /// Draw width at 1× zoom.
    pub fn draw_w(&self) -> f32 {
        self.w as f32 / self.scale.max(1) as f32
    }

    /// Draw height at 1× zoom.
    pub fn draw_h(&self) -> f32 {
        self.h as f32 / self.scale.max(1) as f32
    }

    /// Anchor in 1× pixels.
    pub fn draw_anchor(&self) -> (f32, f32) {
        let sc = self.scale.max(1) as f32;
        (self.anchor_x as f32 / sc, self.anchor_y as f32 / sc)
    }
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
    lookup: HashMap<(KindId, u8, Anim, u8), usize>,
    anims: HashMap<(KindId, Anim), AnimInfo>,
    /// `(kind, age index)` to the id its age-styled frames are filed under.
    variants: HashMap<(KindId, u8), KindId>,
    /// Names of the rendered sets that replaced placeholders.
    pub loaded_sets: Vec<String>,
}

impl Atlas {
    /// Finds the idle frame for a kind at a facing, and whether to mirror it.
    /// Static kinds ignore the facing.
    pub fn frame(&self, kind: KindId, facing8: u8) -> Option<(&Frame, bool)> {
        self.frame_at(kind, facing8, Anim::Idle, 0)
    }

    /// The frame of `anim` to show at `time_ms` into it, falling back to idle
    /// and then to whatever single frame the kind has.
    pub fn frame_at(
        &self,
        kind: KindId,
        facing8: u8,
        anim: Anim,
        time_ms: u32,
    ) -> Option<(&Frame, bool)> {
        let index = |a: Anim| -> u8 {
            match self.anims.get(&(kind, a)) {
                Some(info) if info.frames > 0 && info.frame_ms > 0 => {
                    let f = time_ms / info.frame_ms;
                    if info.loops {
                        (f % info.frames) as u8
                    } else {
                        f.min(info.frames - 1) as u8
                    }
                }
                _ => 0,
            }
        };
        for a in [anim, Anim::Idle] {
            let idx = index(a);
            if let Some(&i) = self.lookup.get(&(kind, 0, a, idx)) {
                return Some((&self.frames[i], false));
            }
            let (f, flip) = source_facing(facing8);
            if let Some(&i) = self.lookup.get(&(kind, f, a, idx)) {
                return Some((&self.frames[i], flip));
            }
        }
        None
    }

    /// The id to look a kind up under for an owner in age `age` (by
    /// [`sim::Age::index`]): its age-styled variant if one is drawn, else the
    /// kind itself. Rendered sprite sets carry no variants yet, so a kind
    /// with a set always answers with itself.
    pub fn variant(&self, kind: KindId, age: u8) -> KindId {
        self.variants.get(&(kind, age)).copied().unwrap_or(kind)
    }

    /// Timing of an animation, if the kind has it.
    pub fn anim_info(&self, kind: KindId, anim: Anim) -> Option<AnimInfo> {
        self.anims.get(&(kind, anim)).copied()
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
        Atlas::with_sheets(&[])
    }

    /// The placeholder atlas, with every kind that has a rendered sprite set
    /// in `sheets` drawn from that set instead. Sets whose name matches no
    /// kind are ignored.
    pub fn with_sheets(sheets: &[crate::sheets::Sheet]) -> Atlas {
        let mut canvases: Vec<Entry> = Vec::new();
        let mut anims: HashMap<(KindId, Anim), AnimInfo> = HashMap::new();
        let mut variants: HashMap<(KindId, u8), KindId> = HashMap::new();
        let mut loaded_sets = Vec::new();
        let mut covered: Vec<KindId> = Vec::new();
        for sheet in sheets {
            let Some(kind) = kind_for_set(&sheet.name) else {
                continue;
            };
            covered.push(kind);
            loaded_sets.push(sheet.name.clone());
            for (ai, animation) in sheet.animations.iter().enumerate() {
                let Some(anim) = Anim::from_name(&animation.name) else {
                    continue;
                };
                anims.insert(
                    (kind, anim),
                    AnimInfo {
                        frames: animation.frames,
                        frame_ms: animation.frame_ms,
                        loops: animation.loops,
                    },
                );
                for (fi, &facing) in sheet.facings().iter().enumerate() {
                    for frame in 0..animation.frames {
                        let (x, y, w, h) = sheet.frame_rect(ai, fi, frame);
                        let (ax, ay) = sheet.anchor_for(ai, fi, frame);
                        let mut c = Canvas::new(w, h, (ax as i16, ay as i16));
                        for yy in 0..h {
                            for xx in 0..w {
                                c.set(xx as i32, yy as i32, sheet.index_at(x + xx, y + yy));
                            }
                        }
                        canvases.push(Entry {
                            kind,
                            facing,
                            anim,
                            index: frame as u8,
                            scale: sheet.scale as u8,
                            canvas: c,
                        });
                    }
                }
            }
        }
        let still = |kind: KindId, facing: u8, canvas: Canvas| Entry {
            kind,
            facing,
            anim: Anim::Idle,
            index: 0,
            scale: 1,
            canvas,
        };
        // A corpse for every mobile placeholder: one frame, any facing, that
        // the death and decay animations both show.
        type Anims = HashMap<(KindId, Anim), AnimInfo>;
        let fallen =
            |kind: KindId, base: KindId, age: u8, canvases: &mut Vec<Entry>, anims: &mut Anims| {
                for anim in [Anim::Death, Anim::Decay] {
                    canvases.push(Entry {
                        kind,
                        facing: 0,
                        anim,
                        index: 0,
                        scale: 1,
                        canvas: draw_fallen(base, age),
                    });
                    anims.insert(
                        (kind, anim),
                        AnimInfo {
                            frames: 1,
                            frame_ms: 1000,
                            loops: false,
                        },
                    );
                }
            };
        for k in kinds::all() {
            if covered.contains(&k.id) {
                continue;
            }
            if k.mobile {
                for f in AUTHORED {
                    canvases.push(still(k.id, f, draw_kind(k.id, f)));
                    if is_military_foot(k.id) {
                        for index in 0..2 {
                            canvases.push(Entry {
                                kind: k.id,
                                facing: f,
                                anim: Anim::Work,
                                index,
                                scale: 1,
                                canvas: military_foot(k.id, f, 0, index == 0),
                            });
                        }
                    }
                }
                if is_military_foot(k.id) {
                    anims.insert(
                        (k.id, Anim::Work),
                        AnimInfo {
                            frames: 2,
                            frame_ms: 150,
                            loops: false,
                        },
                    );
                }
                fallen(k.id, k.id, 0, &mut canvases, &mut anims);
            } else {
                canvases.push(still(k.id, 0, draw_kind(k.id, 1)));
                if k.id == kinds::GATE {
                    // The gate standing open, filed under the work animation.
                    canvases.push(Entry {
                        kind: k.id,
                        facing: 0,
                        anim: Anim::Work,
                        index: 0,
                        scale: 1,
                        canvas: gate(true),
                    });
                    anims.insert(
                        (k.id, Anim::Work),
                        AnimInfo {
                            frames: 1,
                            frame_ms: 1000,
                            loops: false,
                        },
                    );
                }
            }
            if !has_age_variants(k.id) {
                continue;
            }
            // The Stone Age look is the kind itself; the three later ages
            // are drawn in their materials and filed under variant ids.
            for age in 1..=3u8 {
                let id = variant_id(k.id, age);
                if k.mobile {
                    for f in AUTHORED {
                        canvases.push(still(id, f, draw_kind_aged(k.id, f, age)));
                        if is_military_foot(k.id) {
                            for index in 0..2 {
                                canvases.push(Entry {
                                    kind: id,
                                    facing: f,
                                    anim: Anim::Work,
                                    index,
                                    scale: 1,
                                    canvas: military_foot(k.id, f, age, index == 0),
                                });
                            }
                        }
                    }
                    if is_military_foot(k.id) {
                        anims.insert(
                            (id, Anim::Work),
                            AnimInfo {
                                frames: 2,
                                frame_ms: 150,
                                loops: false,
                            },
                        );
                    }
                    fallen(id, k.id, age, &mut canvases, &mut anims);
                } else {
                    canvases.push(still(id, 0, draw_kind_aged(k.id, 1, age)));
                }
                variants.insert((k.id, age), id);
            }
        }
        for &idx in SOLIDS {
            let mut c = Canvas::new(4, 4, (0, 0));
            c.rect(0, 0, 4, 4, idx);
            canvases.push(still(UI_SOLID + idx as KindId, 0, c));
        }
        for facing in AUTHORED {
            let (dx, dy) = facing_dir(facing);
            let mut c = Canvas::new(32, 32, (16, 16));
            let hand = (16.0 - dx * 9.0, 16.0 - dy * 9.0);
            shaft(&mut c, hand, (dx, dy), weapon(18.0, 4.0, BLACK, BLACK));
            shaft(&mut c, hand, (dx, dy), weapon(17.0, 2.0, LINEN, WHITE));
            c.circle(hand.0, hand.1, 3.0, P_BASE);
            canvases.push(still(UI_ARROW, facing, c));
        }
        for fp in 0..=3u32 {
            canvases.push(still(UI_RING + fp as KindId, 0, ring(fp)));
            if fp > 0 {
                canvases.push(still(
                    UI_FOOT_OK + fp as KindId,
                    0,
                    footprint(fp, GREEN_LIGHT),
                ));
                canvases.push(still(UI_FOOT_BAD + fp as KindId, 0, footprint(fp, RED)));
                canvases.push(still(
                    UI_FOOT_GLOW + fp as KindId,
                    0,
                    footprint(fp, GOLD_LIGHT),
                ));
                canvases.push(still(UI_SITE + fp as KindId, 0, site(fp)));
                canvases.push(still(UI_RUBBLE + fp as KindId, 0, draw_rubble(fp)));
            }
        }
        for (i, ch) in font::CHARS.chars().enumerate() {
            canvases.push(still(UI_GLYPH + i as KindId, 0, glyph(ch, WHITE)));
            canvases.push(still(UI_GLYPH_DARK + i as KindId, 0, glyph(ch, BLACK)));
            canvases.push(still(UI_GLYPH_GOLD + i as KindId, 0, glyph(ch, GOLD_LIGHT)));
        }
        let mut atlas = pack(canvases, ATLAS_WIDTH);
        atlas.anims = anims;
        atlas.variants = variants;
        atlas.loaded_sets = loaded_sets;
        atlas
    }

    /// The arrow drawn for a projectile in flight.
    pub fn arrow(&self, facing: u8) -> Option<(&Frame, bool)> {
        self.frame(UI_ARROW, facing)
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
        self.glyph_ink(c, if dark { Ink::Black } else { Ink::White })
    }

    /// The glyph for a character in an ink, if the font has it.
    pub fn glyph_ink(&self, c: char, ink: Ink) -> Option<&Frame> {
        let i = font::CHARS.find(c.to_ascii_uppercase())? as KindId;
        let base = match ink {
            Ink::White => UI_GLYPH,
            Ink::Black => UI_GLYPH_DARK,
            Ink::Gold => UI_GLYPH_GOLD,
        };
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

    /// Rubble for a footprint: what a fallen building leaves.
    pub fn rubble(&self, footprint: u8) -> Option<&Frame> {
        self.frame(UI_RUBBLE + footprint.clamp(1, 3) as KindId, 0)
            .map(|(f, _)| f)
    }

    /// Placement footprint overlay.
    /// The age-up glow for a footprint: the placement hatch in gold.
    pub fn glow(&self, footprint: u8) -> Option<&Frame> {
        self.frame(UI_FOOT_GLOW + footprint as KindId, 0)
            .map(|(f, _)| f)
    }

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

/// Atlas texture width. Height grows to fit, up to the GPU's limit.
pub const ATLAS_WIDTH: u32 = 2048;

/// A frame waiting to be packed.
struct Entry {
    kind: KindId,
    facing: u8,
    anim: Anim,
    index: u8,
    scale: u8,
    canvas: Canvas,
}

/// Shelf-packs canvases into an atlas of the given width. Taller frames go
/// first so shelves waste less.
fn pack(mut canvases: Vec<Entry>, width: u32) -> Atlas {
    canvases.sort_by_key(|e| std::cmp::Reverse(e.canvas.h));
    let mut frames = Vec::new();
    let mut lookup = HashMap::new();
    let mut placed: Vec<(u32, u32, Canvas)> = Vec::new();
    let (mut x, mut y, mut shelf_h) = (0u32, 0u32, 0u32);
    for e in canvases {
        let c = e.canvas;
        if x + c.w > width {
            x = 0;
            y += shelf_h + 1;
            shelf_h = 0;
        }
        frames.push(Frame {
            kind: e.kind,
            facing: e.facing,
            anim: e.anim,
            index: e.index,
            scale: e.scale,
            x: x as u16,
            y: y as u16,
            w: c.w as u16,
            h: c.h as u16,
            anchor_x: c.anchor.0,
            anchor_y: c.anchor.1,
        });
        lookup.insert((e.kind, e.facing, e.anim, e.index), frames.len() - 1);
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
        anims: HashMap::new(),
        variants: HashMap::new(),
        loaded_sets: Vec::new(),
    }
}

/// Which kind a rendered sprite set draws, by the set's name.
pub fn kind_for_set(name: &str) -> Option<KindId> {
    Some(match name {
        "villager" => kinds::VILLAGER,
        "scout" => kinds::SCOUT,
        "clubman" => kinds::CLUBMAN,
        "axeman" => kinds::AXEMAN,
        "spearman" => kinds::SPEARMAN,
        "slinger" => kinds::SLINGER,
        "bowman" => kinds::BOWMAN,
        "light_cavalry" => kinds::LIGHT_CAVALRY,
        "town_center" => kinds::TOWN_CENTER,
        "house" => kinds::HOUSE,
        "storehouse" => kinds::STOREHOUSE,
        "tree" => kinds::TREE,
        "berry_bush" => kinds::BERRY_BUSH,
        "gold_mine" => kinds::GOLD_MINE,
        "stone_mine" => kinds::STONE_MINE,
        "gazelle" => kinds::GAZELLE,
        _ => return None,
    })
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

    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, idx: u8) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len > 0.0 {
            shaft(self, a, (dx / len, dy / len), weapon(len, width, idx, idx));
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

/// The id age-styled frames of `kind` are filed under.
pub fn variant_id(kind: KindId, age: u8) -> KindId {
    AGE_VARIANT_BASE + age as KindId * 1000 + kind
}

/// Kinds whose placeholder changes with the owner's age: what players build,
/// bar the Farm (a field is a field), and the costumes of villagers and
/// infantry (`docs/02` §4: "villager and infantry sprites swap").
fn has_age_variants(kind: KindId) -> bool {
    let k = kinds::info(kind);
    kind == kinds::VILLAGER
        || k.class == kinds::Class::Infantry
        || (k.buildable
            && !k.mobile
            && kind != kinds::FARM
            && !kinds::is_wall(kind)
            && kind != kinds::GATE)
}

/// The costume band a foot unit wears at an age: hides, then linen,
/// bronze, iron.
fn costume(age: u8) -> Option<u8> {
    match age {
        0 => None,
        1 => Some(LINEN),
        2 => Some(BRONZE),
        _ => Some(IRON),
    }
}

/// A foot unit's body, shared by the villager and the infantry: shadow,
/// tunic in the player colour, the age's costume band, and a head turned
/// to the facing. Weapons are drawn over it by the caller.
fn foot_body(c: &mut Canvas, dx: f32, dy: f32, age: u8) {
    c.ellipse(20.0, 42.0, 11.0, 5.0, SHADOW);
    c.ellipse(20.0, 30.0, 8.0, 12.0, BLACK);
    c.ellipse(20.0, 30.0, 7.0, 11.0, P_BASE);
    c.ellipse(22.0, 27.0, 3.5, 6.0, P_LIGHT);
    c.ellipse(16.5, 32.0, 3.0, 7.0, P_DARK);
    if let Some(band) = costume(age) {
        c.ellipse(20.0, 34.0, 7.0, 2.0, band);
    }
    c.circle(20.0, 14.0, 7.0, BLACK);
    c.circle(20.0, 14.0, 6.0, SKIN);
    c.circle(20.0 + dx * 5.0, 14.0 + dy * 5.0, 2.0, BLACK);
}

/// A straight weapon: length and width in px, the shaft's colour and the
/// head's.
#[derive(Clone, Copy)]
struct Weapon {
    len: f32,
    w: f32,
    shaft: u8,
    head: u8,
}

const fn weapon(len: f32, w: f32, shaft: u8, head: u8) -> Weapon {
    Weapon {
        len,
        w,
        shaft,
        head,
    }
}

/// A straight weapon held out from the hand along the facing `dir`.
fn shaft(c: &mut Canvas, hand: (f32, f32), dir: (f32, f32), weapon: Weapon) {
    let (hx, hy) = hand;
    let (dx, dy) = dir;
    let Weapon {
        len,
        w,
        shaft: idx,
        head,
    } = weapon;
    // The facing's perpendicular, for the shaft's width.
    let (px, py) = (-dy * w * 0.5, dx * w * 0.5);
    let (ex, ey) = (hx + dx * len, hy + dy * len);
    c.convex(
        &[
            (hx + px, hy + py),
            (ex + px, ey + py),
            (ex - px, ey - py),
            (hx - px, hy - py),
        ],
        idx,
    );
    c.circle(ex, ey, w * 0.5 + 1.0, head);
}

/// The materials a building is drawn in at each age (`docs/05` §3): timber
/// and thatch, mudbrick, dressed limestone, then granite with iron trim.
#[derive(Clone, Copy)]
struct Style {
    /// The lit wall.
    wall: u8,
    /// The wall in shadow.
    wall_dark: u8,
    /// Ridge and edge trim; `None` in the Stone Age, which has no trim.
    trim: Option<u8>,
}

fn style(age: u8) -> Style {
    match age {
        0 => Style {
            wall: BROWN,
            wall_dark: BROWN_DARK,
            trim: None,
        },
        1 => Style {
            wall: MUDBRICK,
            wall_dark: MUDBRICK_DARK,
            trim: Some(THATCH_DARK),
        },
        2 => Style {
            wall: LIMESTONE,
            wall_dark: LIMESTONE_DARK,
            trim: Some(BRONZE),
        },
        _ => Style {
            wall: GREY_LIGHT,
            wall_dark: GREY_DARK,
            trim: Some(IRON),
        },
    }
}

fn draw_kind(kind: KindId, facing: u8) -> Canvas {
    draw_kind_aged(kind, facing, 0)
}

/// A kind's placeholder lying dead: the body along the ground in the
/// player colour, the head at one end, a mount on its side under a rider.
fn draw_fallen(kind: KindId, age: u8) -> Canvas {
    let mounted = kinds::info(kind).class == kinds::Class::Cavalry;
    if mounted {
        let mut c = Canvas::new(56, 32, (28, 26));
        c.ellipse(28.0, 26.0, 22.0, 6.0, SHADOW);
        c.ellipse(28.0, 22.0, 19.0, 7.0, BLACK);
        c.ellipse(28.0, 22.0, 18.0, 6.0, HIDE);
        c.ellipse(14.0, 18.0, 6.0, 4.0, P_BASE);
        c.circle(8.0, 16.0, 4.0, BLACK);
        c.circle(8.0, 16.0, 3.0, SKIN);
        return c;
    }
    let mut c = Canvas::new(40, 24, (20, 20));
    c.ellipse(20.0, 20.0, 15.0, 5.0, SHADOW);
    c.ellipse(22.0, 16.0, 12.0, 5.0, BLACK);
    c.ellipse(22.0, 16.0, 11.0, 4.0, P_DARK);
    if let Some(band) = costume(age) {
        c.ellipse(24.0, 16.0, 3.0, 4.0, band);
    }
    c.circle(8.0, 15.0, 5.0, BLACK);
    c.circle(8.0, 15.0, 4.0, SKIN);
    c
}

/// Keep recognisable weapons outside the body silhouette at normal play zoom.
fn is_military_foot(kind: KindId) -> bool {
    matches!(
        kind,
        kinds::CLUBMAN | kinds::AXEMAN | kinds::SPEARMAN | kinds::SLINGER | kinds::BOWMAN
    )
}

fn military_foot(kind: KindId, facing: u8, age: u8, striking: bool) -> Canvas {
    let (dx, dy) = facing_dir(facing);
    let mut c = Canvas::new(64, 64, (32, 56));
    let mut body = Canvas::new(40, 48, (20, 42));
    foot_body(&mut body, dx, dy, age);
    for y in 0..48 {
        for x in 0..40 {
            c.set(x + 12, y + 14, body.px[(y * 40 + x) as usize]);
        }
    }
    let side = if dx < -0.1 { -1.0 } else { 1.0 };
    let (hx, hy) = (32.0 + side * 10.0, 40.0);
    // Idle weapons are held clear of the head. A strike/release extends toward
    // the actual facing for 150 ms, then returns, timed by the reload counter.
    let dir = if striking {
        (dx, dy)
    } else {
        (side * 0.45, -0.89)
    };
    match kind {
        kinds::CLUBMAN | kinds::AXEMAN => {
            // Broad hide shoulders and a club or unmistakable axe blade.
            c.ellipse(32.0, 37.0, 10.0, 4.0, BROWN_DARK);
            c.ellipse(32.0, 37.0, 8.0, 2.0, HIDE);
            shaft(&mut c, (hx, hy), dir, weapon(16.0, 4.0, BLACK, BLACK));
            shaft(&mut c, (hx, hy), dir, weapon(15.0, 2.0, BROWN, BROWN));
            let (ex, ey) = (hx + dir.0 * 15.0, hy + dir.1 * 15.0);
            if kind == kinds::AXEMAN {
                c.ellipse(ex, ey, 6.0, 5.0, BLACK);
                c.ellipse(ex, ey, 5.0, 4.0, GREY_LIGHT);
                c.rect(ex as i32, ey as i32 - 3, 2, 6, WHITE);
            } else {
                c.ellipse(ex, ey, 4.0, 6.0, BLACK);
                c.ellipse(ex, ey, 3.0, 5.0, BROWN);
            }
        }
        kinds::SPEARMAN => {
            shaft(
                &mut c,
                (if striking { 32.0 } else { hx }, hy + 4.0),
                dir,
                weapon(28.0, 4.0, BLACK, BLACK),
            );
            shaft(
                &mut c,
                (if striking { 32.0 } else { hx }, hy + 4.0),
                dir,
                weapon(27.0, 2.0, BROWN, GREY_LIGHT),
            );
            // A small shield on the opposite arm, still carrying team colour.
            c.ellipse(32.0 - side * 9.0, 44.0, 5.0, 8.0, BLACK);
            c.ellipse(32.0 - side * 9.0, 44.0, 4.0, 7.0, P_DARK);
            c.circle(32.0 - side * 9.0, 44.0, 2.0, GREY_LIGHT);
        }
        kinds::SLINGER => {
            c.rect(26, 25, 12, 3, P_DARK);
            c.ellipse(32.0 - side * 8.0, 48.0, 4.0, 5.0, BROWN_DARK);
            let (ex, ey) = if striking {
                (hx + dx * 15.0, hy + dy * 15.0)
            } else {
                (hx, 18.0)
            };
            c.line((hx, hy), (ex, ey), 3.0, BLACK);
            c.line((hx, hy), (ex, ey), 1.0, LINEN);
            c.circle(ex, ey, 4.0, BLACK);
            c.circle(ex, ey, 3.0, GREY_LIGHT);
        }
        kinds::BOWMAN => {
            // A tall curved stave and separate string, plus a quiver.
            c.line(
                (32.0 - side * 7.0, 35.0),
                (32.0 - side * 10.0, 23.0),
                5.0,
                BROWN_DARK,
            );
            let bx = hx + if striking { side * 5.0 } else { 0.0 };
            let top = (bx, hy - 16.0);
            let mid = (bx + side * 7.0, hy);
            let bottom = (bx, hy + 14.0);
            c.line(top, mid, 4.0, BLACK);
            c.line(mid, bottom, 4.0, BLACK);
            c.line(top, mid, 2.0, BROWN);
            c.line(mid, bottom, 2.0, BROWN);
            let string = (bx - if striking { side * 4.0 } else { 0.0 }, hy);
            c.line(top, string, 1.0, LINEN);
            c.line(string, bottom, 1.0, LINEN);
        }
        _ => unreachable!(),
    }
    c
}

/// A kind's placeholder as its owner's age (0 Stone .. 3 Iron) draws it.
fn draw_kind_aged(kind: KindId, facing: u8, age: u8) -> Canvas {
    let (dx, dy) = facing_dir(facing);
    let st = style(age);
    let fp = kinds::info(kind).footprint.max(1) as u32;
    match kind {
        kinds::VILLAGER => {
            let mut c = Canvas::new(40, 48, (20, 42));
            foot_body(&mut c, dx, dy, age);
            c
        }
        k if is_military_foot(k) => military_foot(k, facing, age, false),
        kinds::LIGHT_CAVALRY => {
            // The scout's horse with a rider in the player colour and a
            // lance: the same silhouette as the scout, armed.
            let mut c = Canvas::new(56, 56, (28, 50));
            c.ellipse(28.0, 50.0, 19.0, 7.0, SHADOW);
            c.ellipse(28.0, 37.0, 18.0, 10.0, BLACK);
            c.ellipse(28.0, 37.0, 17.0, 9.0, HIDE);
            c.circle(28.0 + dx * 15.0, 37.0 + dy * 11.0, 5.0, BLACK);
            c.circle(28.0 + dx * 15.0, 37.0 + dy * 11.0, 4.0, BROWN_DARK);
            c.ellipse(28.0, 32.0, 8.0, 5.0, P_BASE);
            c.ellipse(28.0, 24.0, 5.0, 7.0, BLACK);
            c.ellipse(28.0, 24.0, 4.0, 6.0, P_BASE);
            c.ellipse(29.0, 22.0, 2.0, 4.0, P_LIGHT);
            c.circle(28.0, 16.0, 5.0, BLACK);
            c.circle(28.0, 16.0, 4.0, SKIN);
            shaft(
                &mut c,
                (28.0 + dx * 4.0, 24.0 + dy * 3.0),
                (dx, dy),
                weapon(14.0, 1.5, BROWN, GREY_LIGHT),
            );
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
        kinds::TOWN_CENTER => building(192, 144, 3, 58.0, true, st),
        kinds::HOUSE => building(128, 96, 2, 34.0, false, st),
        kinds::STOREHOUSE => storehouse(age),
        kinds::FARM => farm(fp),
        kinds::BARRACKS => {
            let mut c = block(fp, 40.0, false, st);
            let (x, y) = wall_point(&c, fp, 0.55);
            c.circle(x, y - 22.0, 7.0, BLACK);
            c.circle(x, y - 22.0, 6.0, RED);
            c.circle(x, y - 22.0, 2.0, WHITE);
            c
        }
        kinds::ARCHERY_RANGE => {
            let mut c = block(fp, 30.0, false, st);
            let (x, y) = (c.w as f32 * 0.82, c.h as f32 - 14.0);
            c.circle(x, y, 8.0, BLACK);
            c.circle(x, y, 7.0, WHITE);
            c.circle(x, y, 5.0, RED);
            c.circle(x, y, 3.0, WHITE);
            c.circle(x, y, 1.2, RED);
            c
        }
        kinds::STABLE => {
            let mut c = block(fp, 28.0, false, st);
            let (x, y) = (c.w as f32 * 0.2, c.h as f32 - 16.0);
            c.ellipse(x, y, 13.0, 6.0, BLACK);
            c.ellipse(x, y, 12.0, 5.0, BROWN);
            c.circle(x + 12.0, y - 6.0, 4.0, BLACK);
            c.circle(x + 12.0, y - 6.0, 3.0, BROWN_DARK);
            c
        }
        kinds::MARKET => {
            let mut c = block(fp, 30.0, false, st);
            let (x, y) = wall_point(&c, fp, 0.5);
            for k in 0..6 {
                let idx = if k % 2 == 0 { RED } else { WHITE };
                c.rect(x as i32 - 15 + k * 5, y as i32 - 26, 5, 6, idx);
            }
            c.rect(x as i32 - 15, y as i32 - 27, 30, 1, BLACK);
            c
        }
        kinds::WATCH_TOWER => {
            let mut c = block(1, 70.0, false, st);
            let (cx, top) = (c.w as f32 / 2.0, c.h as f32 - 16.0 - 70.0);
            for k in [-16.0, -6.0, 4.0, 14.0] {
                c.rect((cx + k) as i32, (top - 20.0) as i32, 3, 7, st.wall_dark);
            }
            c
        }
        kinds::TEMPLE => {
            let mut c = block(fp, 36.0, false, st);
            let (x, y) = wall_point(&c, fp, 0.5);
            for k in 0..4 {
                let px = x as i32 - 18 + k * 12;
                c.rect(px, y as i32 - 32, 4, 30, BLACK);
                c.rect(px + 1, y as i32 - 31, 2, 28, LINEN);
            }
            c
        }
        kinds::ACADEMY => {
            let mut c = block(fp, 34.0, false, st);
            let (x, y) = wall_point(&c, fp, 0.5);
            c.rect(x as i32 - 20, y as i32 - 4, 40, 3, GREY_LIGHT);
            c.rect(x as i32 - 24, y as i32 - 1, 48, 3, GREY);
            c
        }
        kinds::SIEGE_WORKSHOP => {
            let mut c = block(fp, 30.0, false, st);
            let (x, y) = (c.w as f32 * 0.2, c.h as f32 - 18.0);
            c.circle(x, y, 9.0, BLACK);
            c.circle(x, y, 7.0, BROWN);
            c.rect(x as i32 - 7, y as i32 - 1, 14, 2, BLACK);
            c.rect(x as i32 - 1, y as i32 - 7, 2, 14, BLACK);
            c.circle(x, y, 2.0, BLACK);
            c
        }
        kinds::GOVERNMENT_CENTRE => {
            let mut c = block(fp, 44.0, true, st);
            let (x, y) = wall_point(&c, fp, 0.5);
            c.rect(x as i32 - 12, y as i32 - 40, 24, 3, GOLD);
            c
        }
        kinds::PALISADE_WALL => palisade(),
        kinds::STONE_WALL => stone_wall(),
        kinds::GATE => gate(false),
        _ => {
            let mut c = Canvas::new(32, 32, (16, 28));
            c.diamond(16.0, 28.0, 14.0, 7.0, SHADOW);
            c.rect(8, 6, 16, 22, BLACK);
            c.rect(9, 7, 14, 20, P_BASE);
            c
        }
    }
}

/// A one-tile wall segment: a low block filling the tile, `rise` high, in
/// two materials, with a player-colour pennant so an owner can be told.
fn segment(rise: f32, lit: u8, dark: u8, cap: u8) -> Canvas {
    let (w, h) = (64, 32 + rise as u32 + 4);
    let cx = 32.0;
    let base_cy = h as f32 - 16.0;
    let mut c = Canvas::new(w, h, (cx as i16, base_cy as i16));
    let (hw, hh) = (31.0, 15.0);
    c.diamond(cx, base_cy, hw, hh, BLACK);
    let inset = 0.9;
    let (bl, bb, br) = (
        (cx - hw * inset, base_cy),
        (cx, base_cy + hh * inset),
        (cx + hw * inset, base_cy),
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
        lit,
    );
    c.convex(
        &[
            up(bb, 1.0),
            up(br, 1.0),
            up(br, rise - 1.0),
            up(bb, rise - 1.0),
        ],
        dark,
    );
    let top = base_cy - rise;
    c.diamond(cx, top, hw * inset + 1.0, hh * inset + 1.0, BLACK);
    c.diamond(cx, top, hw * inset, hh * inset, cap);
    c.rect(cx as i32 - 1, (top - 10.0) as i32, 2, 10, BLACK);
    c.rect(cx as i32 + 1, (top - 10.0) as i32, 6, 4, P_BASE);
    c
}

/// Sharpened timber, lashed: uprights along the top edge.
fn palisade() -> Canvas {
    let mut c = segment(22.0, BROWN, BROWN_DARK, BROWN_DARK);
    let top = c.h as f32 - 16.0 - 22.0;
    for k in 0..5 {
        let x = 12 + k * 10;
        c.rect(x, (top - 6.0) as i32, 3, 8, BLACK);
        c.rect(x + 1, (top - 5.0) as i32, 1, 6, TAN);
    }
    c
}

/// Dressed stone, with a course line.
fn stone_wall() -> Canvas {
    let mut c = segment(26.0, LIMESTONE, LIMESTONE_DARK, GREY_LIGHT);
    let base_cy = c.h as f32 - 16.0;
    for k in 0..3 {
        let y = (base_cy - 6.0 - k as f32 * 8.0) as i32;
        c.rect(6, y, 52, 1, GREY_DARK);
    }
    c
}

/// A gate: a stone segment with an arch, the doors shut or swung open.
fn gate(open: bool) -> Canvas {
    let mut c = segment(30.0, LIMESTONE, LIMESTONE_DARK, GREY_LIGHT);
    let base_cy = c.h as f32 - 16.0;
    let (cx, cy) = (32.0, base_cy + 6.0);
    // The opening, through the front faces.
    c.convex(
        &[
            (cx - 9.0, cy - 2.0),
            (cx + 9.0, cy - 2.0),
            (cx + 9.0, cy - 22.0),
            (cx - 9.0, cy - 22.0),
        ],
        BLACK,
    );
    if open {
        c.convex(
            &[
                (cx - 8.0, cy - 3.0),
                (cx + 8.0, cy - 3.0),
                (cx + 8.0, cy - 21.0),
                (cx - 8.0, cy - 21.0),
            ],
            SAND,
        );
        // Doors swung back against the posts.
        c.rect(cx as i32 - 11, (cy - 22.0) as i32, 3, 20, BROWN_DARK);
        c.rect(cx as i32 + 8, (cy - 22.0) as i32, 3, 20, BROWN_DARK);
    } else {
        c.convex(
            &[
                (cx - 8.0, cy - 3.0),
                (cx - 1.0, cy - 3.0),
                (cx - 1.0, cy - 21.0),
                (cx - 8.0, cy - 21.0),
            ],
            BROWN,
        );
        c.convex(
            &[
                (cx + 1.0, cy - 3.0),
                (cx + 8.0, cy - 3.0),
                (cx + 8.0, cy - 21.0),
                (cx + 1.0, cy - 21.0),
            ],
            BROWN_DARK,
        );
        c.rect(cx as i32 - 7, (cy - 14.0) as i32, 14, 1, IRON_DARK);
    }
    c
}

/// Rubble over a footprint: a scatter of broken stone and charred timber
/// on a dust-coloured ground, no taller than a unit's knee.
fn draw_rubble(fp: u32) -> Canvas {
    let (w, h) = (64 * fp, 32 * fp + 12);
    let cx = w as f32 / 2.0;
    let cy = h as f32 - 16.0 * fp as f32;
    let mut c = Canvas::new(w, h, (cx as i16, cy as i16));
    let (hw, hh) = (cx - 1.0, 16.0 * fp as f32 - 1.0);
    c.diamond(cx, cy, hw, hh, SHADOW);
    c.diamond(cx, cy, hw - 3.0, hh - 2.0, DIRT);
    // Heaps, placed by a fixed pattern so every footprint reads the same.
    let heaps = [
        (0.0, 0.0, 9.0, 5.0, GREY),
        (-0.45, -0.1, 6.0, 3.0, GREY_DARK),
        (0.4, 0.15, 7.0, 4.0, GREY_LIGHT),
        (-0.15, 0.4, 5.0, 3.0, BROWN_DARK),
        (0.2, -0.45, 6.0, 3.0, GREY),
        (-0.5, 0.3, 4.0, 2.0, GREY_DARK),
        (0.55, -0.2, 5.0, 3.0, BROWN_DARK),
    ];
    for (u, v, rx, ry, idx) in heaps {
        let x = cx + u * hw * 0.9;
        let y = cy + v * hh * 0.9;
        c.ellipse(
            x,
            y,
            rx * fp as f32 * 0.7 + 1.0,
            ry * fp as f32 * 0.7 + 1.0,
            BLACK,
        );
        c.ellipse(x, y - 1.0, rx * fp as f32 * 0.7, ry * fp as f32 * 0.7, idx);
    }
    c
}

/// A point on the right-hand (door-side) wall of a `block` building, `t`
/// of the way along it, at ground level.
fn wall_point(c: &Canvas, fp: u32, t: f32) -> (f32, f32) {
    let cx = c.w as f32 / 2.0;
    let base_cy = c.h as f32 - fp as f32 * iso::TILE_H / 2.0;
    let hw = fp as f32 * iso::TILE_W / 2.0 - 1.0;
    let hh = fp as f32 * iso::TILE_H / 2.0 - 1.0;
    let inset = 0.8;
    let bb = (cx, base_cy + hh * inset);
    let br = (cx + hw * inset, base_cy);
    (bb.0 + (br.0 - bb.0) * t, bb.1 + (br.1 - bb.1) * t)
}

/// A `building` sized from its footprint rather than by hand.
fn block(fp: u32, rise: f32, flag: bool, st: Style) -> Canvas {
    let hh = fp as f32 * iso::TILE_H / 2.0 - 1.0;
    let above = (hh * 0.8).max(if flag { 30.0 } else { 0.0 });
    let w = fp * iso::TILE_W as u32;
    let h = (fp as f32 * iso::TILE_H / 2.0 + rise + above + 2.0).ceil() as u32;
    building(w, h, fp, rise, flag, st)
}

/// A field: tilled earth in furrows, with a row of sprouts. No walls, no
/// age variants, and no player colour but the boundary stakes.
fn farm(fp: u32) -> Canvas {
    let (w, h) = (64 * fp, 32 * fp + 8);
    let cx = w as f32 / 2.0;
    let cy = h as f32 - 16.0 * fp as f32;
    let mut c = Canvas::new(w, h, (cx as i16, cy as i16));
    let (hw, hh) = (cx - 1.0, 16.0 * fp as f32 - 1.0);
    c.diamond(cx, cy, hw, hh, BLACK);
    c.diamond(cx, cy, hw - 1.0, hh - 1.0, DIRT);
    // Furrows run parallel to the left-top edge; a rhombus makes each one
    // exactly one edge-vector long.
    let (lx, ly) = (cx - hw, cy);
    for k in 1..(3 * fp as i32) {
        let t = k as f32 / (3.0 * fp as f32);
        let (sx, sy) = (lx + t * hw, ly + t * hh);
        c.convex(
            &[
                (sx + 2.0, sy),
                (sx + hw - 2.0, sy - hh),
                (sx + hw - 2.0, sy - hh + 2.0),
                (sx + 2.0, sy + 2.0),
            ],
            DIRT_DARK,
        );
        for j in 1..8 {
            let u = j as f32 / 8.0;
            c.circle(sx + u * hw, sy - u * hh - 1.0, 1.5, GREEN);
        }
    }
    for (px, py) in [
        (cx, cy - hh + 2.0),
        (cx + hw - 3.0, cy),
        (cx, cy + hh - 2.0),
        (cx - hw + 3.0, cy),
    ] {
        c.rect(px as i32 - 1, py as i32 - 8, 3, 9, BLACK);
        c.rect(px as i32, py as i32 - 7, 1, 7, P_BASE);
    }
    c
}

/// The storehouse: a low, wide shed with a thatched roof and a player-colour
/// banner, so it reads as "goods go here" rather than "people live here".
fn storehouse(age: u8) -> Canvas {
    let (wall_l, wall_r, roof) = match age {
        0 => (TAN, BROWN, BROWN),
        1 => (MUDBRICK, MUDBRICK_DARK, THATCH),
        2 => (LIMESTONE, LIMESTONE_DARK, GREY),
        _ => (GREY_LIGHT, GREY_DARK, IRON_DARK),
    };
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
        wall_l,
    );
    c.convex(
        &[
            up(bb, 1.0),
            up(br, 1.0),
            up(br, rise - 1.0),
            up(bb, rise - 1.0),
        ],
        wall_r,
    );
    // Thatch: a diamond roof in browns with a ridge.
    let roof_cy = base_cy - rise;
    c.diamond(cx, roof_cy, 63.0 * inset + 1.0, 31.0 * inset + 1.0, BLACK);
    c.diamond(cx, roof_cy, 63.0 * inset, 31.0 * inset, roof);
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
/// roof diamond, and a door. `rise` is wall height in px; `st` picks the
/// wall materials for the owner's age.
fn building(w: u32, h: u32, footprint: u32, rise: f32, flag: bool, st: Style) -> Canvas {
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
        st.wall,
    );
    c.convex(
        &[
            ins(bb, 1.0),
            ins(br, 1.0),
            ins(up(br), -1.0),
            ins(up(bb), -1.0),
        ],
        st.wall_dark,
    );
    // Roof.
    let roof_cy = base_cy - rise;
    c.diamond(cx, roof_cy, hw * inset + 1.0, hh * inset + 1.0, BLACK);
    c.diamond(cx, roof_cy, hw * inset, hh * inset, P_BASE);
    if let Some(trim) = st.trim {
        // A band along the eaves, in the age's material.
        c.diamond(cx, roof_cy, hw * inset, hh * inset, trim);
        c.diamond(cx, roof_cy, hw * inset - 4.0, hh * inset - 2.0, P_BASE);
    }
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
        assert_eq!(a.width, ATLAS_WIDTH);
        assert!(a.height <= 1024, "placeholder atlas too tall: {}", a.height);
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
            if f.kind < UI_RING && f.scale == 1 {
                assert!(
                    painted > (f.w as usize * f.h as usize) / 8,
                    "frame {i} ({}) is nearly empty",
                    f.kind
                );
            } else {
                let space = UI_GLYPH + 36;
                let dark_space = UI_GLYPH_DARK + 36;
                let gold_space = UI_GLYPH_GOLD + 36;
                assert!(
                    painted > 0 || f.kind == space || f.kind == dark_space || f.kind == gold_space,
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
        assert_eq!(a.rubble(3).unwrap().w, 192);
        assert!(
            a.frame_at(kinds::GATE, 0, Anim::Work, 0).is_some(),
            "the gate opens"
        );
        assert!(a.frame(kinds::PALISADE_WALL, 0).is_some());
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
    fn rendered_sets_replace_placeholders_and_animate() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/sprites");
        let (sheets, errors) = crate::sheets::load_all(&dir);
        assert!(errors.is_empty(), "{errors:?}");
        let a = Atlas::with_sheets(&sheets);
        assert_eq!(a.loaded_sets, vec!["villager".to_string()]);
        assert!(
            a.width == ATLAS_WIDTH && a.height <= 8192,
            "{}x{}",
            a.width,
            a.height
        );
        let (idle, flip) = a.frame(kinds::VILLAGER, 1).unwrap();
        assert!(!flip);
        assert_eq!((idle.w, idle.h, idle.scale), (80, 96, 2));
        assert_eq!((idle.draw_w(), idle.draw_h()), (40.0, 48.0));
        let walk = a.anim_info(kinds::VILLAGER, Anim::Walk).unwrap();
        assert_eq!((walk.frames, walk.frame_ms, walk.loops), (8, 100, true));
        let (f0, _) = a.frame_at(kinds::VILLAGER, 2, Anim::Walk, 0).unwrap();
        let (f2, _) = a.frame_at(kinds::VILLAGER, 2, Anim::Walk, 250).unwrap();
        let (f9, _) = a.frame_at(kinds::VILLAGER, 2, Anim::Walk, 950).unwrap();
        assert_eq!((f0.anim, f0.index), (Anim::Walk, 0));
        assert_eq!((f2.anim, f2.index), (Anim::Walk, 2));
        assert_eq!(f9.index, 1, "walk loops");
        let (d, _) = a.frame_at(kinds::VILLAGER, 1, Anim::Death, 10_000).unwrap();
        assert_eq!(d.index, 7, "death holds its last frame");
        let (e, flip) = a.frame_at(kinds::VILLAGER, 7, Anim::Idle, 0).unwrap();
        assert!(flip && e.facing == 3, "east mirrors west");
        // Kinds without a set still get placeholders, and UI frames still exist.
        let (tc, _) = a.frame(kinds::TOWN_CENTER, 0).unwrap();
        assert_eq!(tc.scale, 1);
        assert!(a.glyph('A', false).is_some());
        // A kind with only placeholders falls back to its single frame for any anim.
        let (g, _) = a.frame_at(kinds::GAZELLE, 1, Anim::Walk, 500).unwrap();
        assert_eq!((g.anim, g.index), (Anim::Idle, 0));
    }

    #[test]
    fn arrow_heads_point_forward_in_all_eight_facings() {
        let atlas = Atlas::placeholder();
        for facing in 0..8 {
            let (frame, flip) = atlas.arrow(facing).unwrap();
            let (dx, dy) = facing_dir(facing);
            let mut heads = Vec::new();
            let mut tails = Vec::new();
            for y in 0..frame.h as u32 {
                for x in 0..frame.w as u32 {
                    let index = atlas.index_at(frame.x as u32 + x, frame.y as u32 + y);
                    let sx = if flip {
                        frame.w as f32 - 1.0 - x as f32
                    } else {
                        x as f32
                    };
                    let along = (sx - 15.5) * dx + (y as f32 - 15.5) * dy;
                    if index == WHITE {
                        heads.push(along);
                    }
                    if index == P_BASE {
                        tails.push(along);
                    }
                }
            }
            assert!(
                !heads.is_empty() && heads.iter().all(|x| *x > 0.0),
                "head {facing}"
            );
            assert!(
                !tails.is_empty() && tails.iter().all(|x| *x < 0.0),
                "tail {facing}"
            );
        }
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
