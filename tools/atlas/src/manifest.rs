//! The sprite set manifest: the authored data that says what a sheet contains.
//!
//! One manifest describes one sprite set — a villager, a barracks, a terrain
//! page — and sits next to its indexed PNG. Everything the validator checks and
//! everything the renderer needs at load time is declared here, because the
//! alternative is inferring it from the image, and inference is what produces
//! sprites that slide around (`docs/05` §2.3).

use serde::Deserialize;
use std::path::Path;

/// Size classes from `docs/05` §2.3, in pixels at 1× zoom.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    /// Villager, infantry.
    Foot,
    /// Cavalry, chariot.
    Mounted,
    /// Elephant, siege.
    Heavy,
    /// House, Farm — 1×1 or 2×2 tiles.
    SmallBuilding,
    /// Barracks, Storehouse — 2×2 tiles.
    MediumBuilding,
    /// Town Center, Temple — 3×3 tiles.
    LargeBuilding,
    /// 5×5 tiles.
    Wonder,
    /// Terrain and cliff pages: one 64×32 tile per frame.
    Terrain,
}

/// Whether a set animates and turns, or just stands there.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum Kind {
    Mobile,
    Building,
    Terrain,
}

impl Class {
    /// Frame size at 1× zoom: (width, height).
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

    pub fn kind(self) -> Kind {
        match self {
            Class::Foot | Class::Mounted | Class::Heavy => Kind::Mobile,
            Class::SmallBuilding | Class::MediumBuilding | Class::LargeBuilding | Class::Wonder => {
                Kind::Building
            }
            Class::Terrain => Kind::Terrain,
        }
    }
}

/// The five authored facings. The renderer mirrors these to get SE, E and NE,
/// which is why there are five here and eight on screen (`docs/05` §2.1).
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Facing {
    S,
    SW,
    W,
    NW,
    N,
}

pub const AUTHORED_FACINGS: [Facing; 5] = [Facing::S, Facing::SW, Facing::W, Facing::NW, Facing::N];

impl Facing {
    pub fn name(self) -> &'static str {
        match self {
            Facing::S => "S",
            Facing::SW => "SW",
            Facing::W => "W",
            Facing::NW => "NW",
            Facing::N => "N",
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct AnimationSpec {
    pub name: String,
    pub frames: u32,
    pub frame_ms: u32,
    pub loops: bool,
    /// The frame on which a hit lands, 0-based. Required for attack
    /// animations and meaningless elsewhere: the simulation applies damage on
    /// a tick, and the renderer has to agree with it about when that looks
    /// like it happened (`docs/05` §2.2).
    pub impact: Option<u32>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AnchorOverride {
    pub animation: String,
    pub facing: Facing,
    pub frame: u32,
    pub anchor: (u32, u32),
}

#[derive(Deserialize, Debug)]
pub struct SpriteSet {
    pub name: String,
    pub class: Class,
    /// Authoring scale. `docs/05` §1 says art is authored at 2× and
    /// downsampled, so this is 2 for everything we draw ourselves.
    pub scale: u32,
    /// Name of the palette this set is drawn against.
    pub palette: String,
    /// Indexed PNG holding every frame, relative to the manifest.
    pub sheet: String,
    /// The ground contact point within a frame, at the authored scale. Every
    /// frame uses this unless it appears in `anchor_overrides`.
    pub anchor: (u32, u32),
    pub animations: Vec<AnimationSpec>,
    #[serde(default)]
    pub anchor_overrides: Vec<AnchorOverride>,
}

impl SpriteSet {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        ron::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Facings actually stored in the sheet. Buildings and terrain do not turn.
    pub fn facings(&self) -> &'static [Facing] {
        match self.class.kind() {
            Kind::Mobile => &AUTHORED_FACINGS,
            Kind::Building | Kind::Terrain => &AUTHORED_FACINGS[..1],
        }
    }

    /// Frame cell size at the authored scale.
    pub fn frame_size(&self) -> (u32, u32) {
        let (w, h) = self.class.size();
        (w * self.scale, h * self.scale)
    }

    /// Sheet layout: one row per (animation, facing) in declaration order, one
    /// column per frame. The widest animation sets the sheet width.
    pub fn sheet_size(&self) -> (u32, u32) {
        let (fw, fh) = self.frame_size();
        let cols = self.animations.iter().map(|a| a.frames).max().unwrap_or(0);
        let rows = self.animations.len() as u32 * self.facings().len() as u32;
        (cols * fw, rows * fh)
    }

    /// Row index for a given animation and facing.
    pub fn row(&self, animation: usize, facing: usize) -> u32 {
        animation as u32 * self.facings().len() as u32 + facing as u32
    }

    /// The anchor for one frame, after overrides.
    pub fn anchor_for(&self, animation: &str, facing: Facing, frame: u32) -> (u32, u32) {
        self.anchor_overrides
            .iter()
            .find(|o| o.animation == animation && o.facing == facing && o.frame == frame)
            .map(|o| o.anchor)
            .unwrap_or(self.anchor)
    }
}

/// The animation set every mobile unit ships with (`docs/05` §2.2). Frame
/// counts are a contract, not a suggestion: the simulation's timings assume
/// them and a set that is short a frame will desync from its own sound.
pub const REQUIRED_MOBILE: [(&str, u32); 5] = [
    ("idle", 4),
    ("walk", 8),
    ("attack", 6),
    ("death", 8),
    ("decay", 4),
];

/// Buildings do not walk, but they are built, they stand, and they fall over.
/// `construction` frames are the progress silhouette from `docs/02` §6.
pub const REQUIRED_BUILDING: [(&str, u32); 3] = [("construction", 3), ("idle", 1), ("rubble", 1)];

/// Default timing for a known animation name, from `docs/05` §2.2.
///
/// Frame counts come from the art; the timings do not, and hard-coding them in
/// two places is how a walk cycle ends up playing at one speed in the
/// placeholder set and another in the rendered one.
pub fn default_animation(name: &str, frames: u32) -> AnimationSpec {
    let (frame_ms, loops, impact) = match name {
        "idle" => (160, true, None),
        "walk" => (100, true, None),
        // Hit lands on frame 4 of 6, one-based — index 3 (`docs/05` §2.2).
        "attack" => (90, false, Some(3)),
        "death" => (110, false, None),
        // Decay spans ~30 s across 4 frames.
        "decay" => (7500, false, None),
        "construction" => (200, false, None),
        "rubble" => (200, false, None),
        // Terrain variants are chosen by the map generator, never played.
        "variants" => (1000, false, None),
        // Villager tasks and carry variants: a working loop.
        _ => (140, true, None),
    };
    AnimationSpec {
        name: name.to_string(),
        frames,
        frame_ms,
        loops,
        impact,
    }
}

/// The order animations are laid out in a sheet: the required ones first, in
/// spec order, then anything else alphabetically. Deterministic, because the
/// row index is how the renderer finds a frame.
pub fn sheet_order(class: Class, present: &[String]) -> Vec<String> {
    let required: &[(&str, u32)] = match class.kind() {
        Kind::Mobile => &REQUIRED_MOBILE,
        Kind::Building => &REQUIRED_BUILDING,
        Kind::Terrain => &[],
    };
    let mut out: Vec<String> = required
        .iter()
        .filter(|(n, _)| present.iter().any(|p| p == n))
        .map(|(n, _)| n.to_string())
        .collect();
    let mut extra: Vec<String> = present
        .iter()
        .filter(|p| !out.contains(p))
        .cloned()
        .collect();
    extra.sort();
    out.extend(extra);
    out
}

/// Villager task animations from `docs/05` §2.2. Not required of every set —
/// only the villager has them — but the names are fixed so the simulation can
/// look one up by task without a per-unit table.
pub const VILLAGER_TASKS: [&str; 8] = [
    "chop", "mine", "forage", "farm", "fish", "build", "repair", "carry",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_sizes_match_the_spec_table() {
        // docs/05 §2.3, verbatim. If this table is edited, the spec was edited.
        assert_eq!(Class::Foot.size(), (40, 48));
        assert_eq!(Class::Mounted.size(), (56, 56));
        assert_eq!(Class::Heavy.size(), (72, 72));
        assert_eq!(Class::SmallBuilding.size(), (64, 64));
        assert_eq!(Class::MediumBuilding.size(), (128, 96));
        assert_eq!(Class::LargeBuilding.size(), (192, 144));
        assert_eq!(Class::Wonder.size(), (384, 320));
        // The base tile from docs/05 §1.
        assert_eq!(Class::Terrain.size(), (64, 32));
    }

    #[test]
    fn mobile_sets_store_five_facings_and_buildings_store_one() {
        let mobile = SpriteSet {
            name: "x".into(),
            class: Class::Foot,
            scale: 2,
            palette: "ancient".into(),
            sheet: "x.png".into(),
            anchor: (40, 90),
            animations: vec![AnimationSpec {
                name: "walk".into(),
                frames: 8,
                frame_ms: 100,
                loops: true,
                impact: None,
            }],
            anchor_overrides: vec![],
        };
        assert_eq!(mobile.facings().len(), 5);
        // 8 frames wide, 1 animation x 5 facings tall, at 2x of 40x48.
        assert_eq!(mobile.sheet_size(), (8 * 80, 5 * 96));
        assert_eq!(mobile.row(0, 3), 3);

        let building = SpriteSet {
            class: Class::MediumBuilding,
            ..mobile
        };
        assert_eq!(building.facings().len(), 1);
        assert_eq!(building.sheet_size(), (8 * 256, 192));
    }

    #[test]
    fn anchor_overrides_win_and_everything_else_falls_back() {
        let set = SpriteSet {
            name: "x".into(),
            class: Class::Foot,
            scale: 2,
            palette: "ancient".into(),
            sheet: "x.png".into(),
            anchor: (40, 90),
            animations: vec![],
            anchor_overrides: vec![AnchorOverride {
                animation: "attack".into(),
                facing: Facing::SW,
                frame: 3,
                anchor: (44, 92),
            }],
        };
        assert_eq!(set.anchor_for("attack", Facing::SW, 3), (44, 92));
        assert_eq!(set.anchor_for("attack", Facing::SW, 2), (40, 90));
        assert_eq!(set.anchor_for("attack", Facing::S, 3), (40, 90));
        assert_eq!(set.anchor_for("walk", Facing::SW, 3), (40, 90));
    }
}
