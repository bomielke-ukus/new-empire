//! Procedural placeholder art.
//!
//! `docs/05` §6 step 1: "Flat coloured diamonds and boxes ... with correct
//! sizes, anchors and facings. Everything is playable and testable before a
//! single sprite exists."
//!
//! These are generated as files rather than at runtime on purpose. Runtime
//! shapes prove that the renderer can draw a diamond; files prove the whole
//! pipeline — manifest, sheet layout, indexed palette, player-colour ramp,
//! anchors, five authored facings — which is step 2 of the same production
//! plan. The placeholders go through `atlas validate` exactly as real art will,
//! so the gate is load-bearing from the first milestone instead of from the
//! first sprite.
//!
//! They are also meant to be *readable*: different silhouettes, heights and
//! accent ramps per unit, so a playtest can tell a bowman from a clubman
//! without a single drawn pixel.

use crate::image::Indexed;
use crate::manifest::{
    AnimationSpec, Class, Facing, Kind, AUTHORED_FACINGS, REQUIRED_BUILDING, REQUIRED_MOBILE,
};
use crate::palette::{Palette, PLAYER_RAMP_START};
use std::path::Path;

/// One entry in the placeholder catalogue.
struct Entry {
    name: &'static str,
    class: Class,
    /// Palette index of the accent used for the unit's distinguishing marks.
    /// Different per unit so a playtest can tell them apart at a glance.
    accent: u8,
    /// Silhouette height as a fraction of the frame, in eighths. A clubman is
    /// squat, a bowman is tall and thin, an elephant fills the cell.
    height_8ths: u32,
    /// Silhouette width, in eighths of the frame.
    width_8ths: u32,
}

const fn e(
    name: &'static str,
    class: Class,
    accent: u8,
    height_8ths: u32,
    width_8ths: u32,
) -> Entry {
    Entry {
        name,
        class,
        accent,
        height_8ths,
        width_8ths,
    }
}

/// The vertical-slice inventory from `docs/02`, plus the terrain pages the map
/// generator needs. Accent indices are mid-ramp entries from `assets/palette`.
const CATALOGUE: &[Entry] = &[
    // Economic and support
    e("villager", Class::Foot, 52, 5, 3),
    e("scout", Class::Foot, 60, 5, 2),
    e("priest", Class::Foot, 68, 6, 3),
    // Infantry: hides, then cloth, then bronze, then iron
    e("clubman", Class::Foot, 92, 5, 4),
    e("axeman", Class::Foot, 148, 5, 4),
    e("spearman", Class::Foot, 156, 6, 3),
    e("swordsman", Class::Foot, 157, 6, 4),
    // Ranged
    e("slinger", Class::Foot, 132, 5, 3),
    e("bowman", Class::Foot, 116, 6, 2),
    // Mounted and siege
    e("light_cavalry", Class::Mounted, 76, 5, 5),
    e("heavy_cavalry", Class::Mounted, 158, 6, 5),
    e("stone_thrower", Class::Heavy, 94, 5, 6),
    // Buildings
    e("house", Class::SmallBuilding, 108, 5, 6),
    e("farm", Class::SmallBuilding, 180, 2, 8),
    e("storehouse", Class::MediumBuilding, 96, 5, 6),
    e("barracks", Class::MediumBuilding, 118, 6, 7),
    e("archery_range", Class::MediumBuilding, 120, 6, 7),
    e("stable", Class::MediumBuilding, 100, 6, 7),
    e("watch_tower", Class::MediumBuilding, 140, 8, 3),
    e("town_center", Class::LargeBuilding, 134, 6, 7),
    e("temple", Class::LargeBuilding, 136, 7, 6),
    e("wonder", Class::Wonder, 164, 8, 7),
    // Terrain pages: four variants each, per docs/05 §3
    e("terrain_grass", Class::Terrain, 172, 0, 0),
    e("terrain_dirt", Class::Terrain, 188, 0, 0),
    e("terrain_sand", Class::Terrain, 204, 0, 0),
    e("terrain_water_shallow", Class::Terrain, 212, 0, 0),
];

/// Deterministic noise. Placeholders are regenerated in CI and compared, so
/// nothing here may depend on hashing order, time or the platform.
fn noise(seed: u32, x: u32, y: u32) -> u32 {
    let mut h = seed
        .wrapping_mul(0x9E37_79B9)
        .wrapping_add(x.wrapping_mul(0x85EB_CA6B))
        .wrapping_add(y.wrapping_mul(0xC2B2_AE35));
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_F491);
    h ^= h >> 13;
    h
}

fn seed_of(name: &str) -> u32 {
    name.bytes().fold(0x811C_9DC5u32, |h, b| {
        (h ^ b as u32).wrapping_mul(0x0100_0193)
    })
}

/// Screen-space direction of each authored facing under the 2:1 projection in
/// `docs/05` §1, scaled to the frame. All five point left or straight up and
/// down; the renderer mirrors them for SE, E and NE.
fn facing_vector(f: Facing) -> (f32, f32) {
    match f {
        Facing::S => (0.0, 1.0),
        Facing::SW => (-0.894, 0.447),
        Facing::W => (-1.0, 0.0),
        Facing::NW => (-0.894, -0.447),
        Facing::N => (0.0, -1.0),
    }
}

fn fill_ellipse(img: &mut Indexed, cx: i32, cy: i32, rx: i32, ry: i32, index: u8) {
    if rx <= 0 || ry <= 0 {
        return;
    }
    for y in -ry..=ry {
        for x in -rx..=rx {
            let (fx, fy) = (x as f32 / rx as f32, y as f32 / ry as f32);
            if fx * fx + fy * fy <= 1.0 && cx + x >= 0 && cy + y >= 0 {
                img.set((cx + x) as u32, (cy + y) as u32, index);
            }
        }
    }
}

/// An isometric diamond: the footprint every building and tile sits on.
fn fill_diamond(img: &mut Indexed, cx: i32, cy: i32, half_w: i32, index: u8) {
    let half_h = (half_w / 2).max(1);
    for y in -half_h..=half_h {
        let span = half_w - (y.abs() * half_w) / half_h.max(1);
        for x in -span..=span {
            if cx + x >= 0 && cy + y >= 0 {
                img.set((cx + x) as u32, (cy + y) as u32, index);
            }
        }
    }
}

fn fill_rect(img: &mut Indexed, x0: i32, y0: i32, w: i32, h: i32, index: u8) {
    for y in y0.max(0)..(y0 + h).max(0) {
        for x in x0.max(0)..(x0 + w).max(0) {
            img.set(x as u32, y as u32, index);
        }
    }
}

/// Which step of the player ramp to use. Keeping placeholders spread across the
/// ramp means an unremapped sheet still looks like a shaded object, and it
/// exercises every reserved index so a broken remap shows up immediately.
fn player(step: u32) -> u8 {
    PLAYER_RAMP_START + (step.min(7) as u8)
}

struct Frame<'a> {
    entry: &'a Entry,
    /// Frame cell size.
    w: u32,
    h: u32,
    /// The ground contact point. Nothing is ever drawn below this row, which is
    /// what makes the anchor check in `validate` pass by construction.
    anchor: (u32, u32),
    outline: u8,
    shadow: u8,
}

impl Frame<'_> {
    /// A mobile unit: shadow, body in player colour, accent mark, and a nose
    /// pointing the way it faces.
    fn mobile(&self, img: &mut Indexed, facing: Facing, anim: &str, frame: u32, frames: u32) {
        let (ax, ay) = (self.anchor.0 as i32, self.anchor.1 as i32);
        let body_h = (self.h * self.entry.height_8ths / 8) as i32;
        let body_w = (self.w * self.entry.width_8ths / 8).max(4) as i32;
        let (dx, dy) = facing_vector(facing);

        // How the animation deforms the body. `lift` bobs, `lean` shifts
        // toward the facing, `squash` collapses it on death.
        let t = if frames > 1 {
            frame as f32 / (frames - 1) as f32
        } else {
            0.0
        };
        let (lift, lean, squash) = match anim {
            "idle" => (((frame % 4) as f32 / 3.0 * 2.0 - 1.0).abs() - 0.5, 0.0, 1.0),
            "walk" => (
                if frame % 4 < 2 { -1.0 } else { 0.0 },
                (frame as f32 * 0.9).sin() * 2.0,
                1.0,
            ),
            "attack" => (0.0, t * 5.0, 1.0),
            "death" => (0.0, 0.0, 1.0 - t * 0.75),
            "decay" => (0.0, 0.0, 0.25),
            // Villager task animations and anything else: a small working bob.
            _ => (if frame % 2 == 0 { 0.0 } else { -1.0 }, 1.0, 1.0),
        };
        let body_h = ((body_h as f32 * squash) as i32).max(3);
        let lean_x = (lean * dx) as i32;
        let top = ay - body_h + lift as i32;

        // Shadow first, on the ground, unaffected by the bob. Its centre sits
        // one radius above the anchor so its lowest row is the ground contact
        // point exactly — nothing in the frame may fall below it.
        let shadow_ry = (body_w / 5).max(1);
        fill_ellipse(img, ax, ay - shadow_ry, body_w / 2, shadow_ry, self.shadow);

        // Body: a shaded column in the reserved player ramp, dark at the base.
        for row in 0..body_h {
            let step = 2 + (row * 5) / body_h.max(1);
            let inset = if row < body_h / 6 { 1 } else { 0 };
            fill_rect(
                img,
                ax - body_w / 2 + lean_x + inset,
                top + row,
                body_w - inset * 2,
                1,
                player(step as u32),
            );
        }

        // Accent band: the unit's identifying mark, at chest height.
        let band = top + body_h / 3;
        fill_rect(
            img,
            ax - body_w / 2 + lean_x,
            band,
            body_w,
            (body_h / 8).max(1),
            self.entry.accent,
        );

        // Nose: which way it is facing, pushed out along the screen-space
        // facing vector. The vertical throw is deliberately large, because S
        // and N have no horizontal component at all and would otherwise be the
        // same picture — facing S drops the mark onto the chest, facing N lifts
        // it clear above the head.
        let head = top + body_h / 4;
        let reach = (body_w as f32 * 0.55) as i32;
        // The nose shrinks with the body, or a collapsed death frame would end
        // up with a full-sized head on a flattened corpse.
        let nose_r = ((body_w / 5) as f32 * squash).max(1.0) as i32;
        fill_ellipse(
            img,
            ax + lean_x + (dx * reach as f32) as i32,
            (head as f32 + dy * (body_h as f32 * 0.38)) as i32,
            nose_r,
            nose_r,
            self.outline,
        );

        // The impact frame gets a flash, so the "hit lands here" tag in the
        // manifest is visible while playtesting rather than only in data.
        if anim == "attack" && frame == 3 {
            fill_ellipse(
                img,
                ax + (dx * body_w as f32) as i32,
                band,
                (body_w / 3).max(2),
                (body_w / 3).max(2),
                248,
            );
        }

        // Decay fades by dithering the corpse away rather than by alpha: the
        // pipeline is indexed, so there is no alpha to fade.
        if anim == "decay" {
            let keep = 4 - frame.min(3);
            for y in top..=ay {
                for x in (ax - body_w)..=(ax + body_w) {
                    if x < 0 || y < 0 {
                        continue;
                    }
                    if noise(seed_of(self.entry.name), x as u32, y as u32) % 4 >= keep {
                        img.set(x as u32, y as u32, 0);
                    }
                }
            }
        }
    }

    /// A building: an isometric box on its footprint diamond, with a
    /// player-colour banner so ownership reads at a distance.
    fn building(&self, img: &mut Indexed, anim: &str, frame: u32, frames: u32) {
        let (ax, ay) = (self.anchor.0 as i32, self.anchor.1 as i32);
        let half_w = (self.w * self.entry.width_8ths / 16).max(4) as i32;
        let full_h = (self.h * self.entry.height_8ths / 8) as i32;

        // Construction rises out of the ground; rubble is a flat pile.
        let height = match anim {
            "construction" => (full_h * (frame + 1) as i32) / (frames.max(1) as i32 + 1),
            "rubble" => full_h / 6,
            _ => full_h,
        }
        .max(3);

        // Footprint, always drawn, so an unbuilt plot still occupies the tile.
        // Centred half its own height above the anchor, so the diamond's front
        // corner lands exactly on the ground contact point rather than below it.
        fill_diamond(img, ax, ay - half_w / 2, half_w, self.shadow);

        let top = ay - half_w / 2 - height;
        for row in 0..height {
            let y = top + row;
            let span = half_w - (half_w * (height - row)) / (height * 8);
            // Left face darker than right: one key light, high and to the left,
            // which is the relationship the whole palette is built around.
            fill_rect(
                img,
                ax - span,
                y,
                span,
                1,
                self.entry.accent.saturating_sub(2),
            );
            fill_rect(img, ax, y, span, 1, self.entry.accent);
        }

        // Roof diamond caps the box.
        fill_diamond(img, ax, top, half_w, self.entry.accent.saturating_add(2));

        // Banner in player colour. Every building must carry some, or
        // `validate` rejects it.
        let banner_h = (height / 4).max(2);
        fill_rect(img, ax - 1, top - banner_h, 3, banner_h, player(5));
        fill_rect(
            img,
            ax - half_w / 3,
            top + height / 3,
            2,
            height / 3,
            player(3),
        );

        if anim == "construction" {
            // Scaffold dither over the structure itself, so an in-progress
            // building reads as in progress. Bounded to the box's own silhouette
            // rather than the whole cell, or the hatch reads as a crate the
            // building sits inside.
            for y in top..ay {
                let span = if y < top + half_w / 2 {
                    // Taper across the roof diamond.
                    half_w - (top + half_w / 2 - y) * 2
                } else {
                    half_w
                };
                for x in (ax - span)..=(ax + span) {
                    if x < 0 || y < 0 || span <= 0 {
                        continue;
                    }
                    if (x + y) % 4 == 0 && img.get(x as u32, y as u32) != 0 {
                        img.set(x as u32, y as u32, self.outline);
                    }
                }
            }
        }
    }

    /// A terrain tile: the base diamond with per-variant noise so four tiles of
    /// the same type do not visibly repeat.
    fn terrain(&self, img: &mut Indexed, variant: u32) {
        let (ax, ay) = (self.anchor.0 as i32, self.anchor.1 as i32);
        let half_w = (self.w / 2) as i32 - 1;
        let base = self.entry.accent;
        // Centred on the anchor, which for terrain is the tile's placement point.
        fill_diamond(img, ax, ay, half_w, base);

        let seed = seed_of(self.entry.name).wrapping_add(variant.wrapping_mul(0x9E37));
        for y in 0..self.h {
            for x in 0..self.w {
                if img.get(x, y) == 0 {
                    continue;
                }
                match noise(seed, x, y) % 8 {
                    0 => img.set(x, y, base.saturating_add(1)),
                    1 => img.set(x, y, base.saturating_sub(1)),
                    _ => {}
                }
            }
        }
    }
}

/// The animation list a placeholder set declares. Timings come from the shared
/// table so placeholders and rendered art play at the same speed.
fn animations(class: Class) -> Vec<AnimationSpec> {
    let required: &[(&str, u32)] = match class.kind() {
        Kind::Mobile => &REQUIRED_MOBILE,
        Kind::Building => &REQUIRED_BUILDING,
        Kind::Terrain => &[("variants", 4)],
    };
    required
        .iter()
        .map(|(name, frames)| crate::manifest::default_animation(name, *frames))
        .collect()
}

/// Generates the whole placeholder catalogue into `out_dir`.
///
/// Returns the manifest paths written, so the caller can validate them and
/// prove the generator produces conformant art rather than merely plausible art.
/// Skips any set that already has real art under `real_art`: a rendered sprite
/// set is not something a placeholder should quietly sit beside. Pass an empty
/// path to generate the whole catalogue.
pub fn generate_except(
    out_dir: &Path,
    palette: &Palette,
    real_art: &Path,
) -> Result<Vec<std::path::PathBuf>, String> {
    let mut written = Vec::new();

    for entry in CATALOGUE {
        if real_art
            .join(entry.name)
            .join(format!("{}.ron", entry.name))
            .exists()
        {
            continue;
        }
        let (fw, fh) = {
            let (w, h) = entry.class.size();
            (w * 2, h * 2)
        };
        let anims = animations(entry.class);
        let facings: &[Facing] = match entry.class.kind() {
            Kind::Mobile => &AUTHORED_FACINGS,
            _ => &AUTHORED_FACINGS[..1],
        };

        let cols = anims.iter().map(|a| a.frames).max().unwrap_or(1);
        let rows = anims.len() as u32 * facings.len() as u32;
        let mut sheet = Indexed::new(cols * fw, rows * fh);

        // Ground contact sits three rows above the bottom edge, leaving room
        // for the shadow without anything crossing the cell boundary — the same
        // convention assets/render/rig.json shifts the render camera to hit.
        // Terrain is the exception: a tile is placed by its centre.
        let anchor = match entry.class.kind() {
            Kind::Terrain => (fw / 2, fh / 2),
            _ => (fw / 2, fh - 3),
        };
        let painter = Frame {
            entry,
            w: fw,
            h: fh,
            anchor,
            outline: 254,
            shadow: 3,
        };

        for (ai, anim) in anims.iter().enumerate() {
            for (fi, facing) in facings.iter().enumerate() {
                let row = ai as u32 * facings.len() as u32 + fi as u32;
                for frame in 0..anim.frames {
                    let mut cell = Indexed::new(fw, fh);
                    match entry.class.kind() {
                        Kind::Mobile => {
                            painter.mobile(&mut cell, *facing, &anim.name, frame, anim.frames)
                        }
                        Kind::Building => {
                            painter.building(&mut cell, &anim.name, frame, anim.frames)
                        }
                        Kind::Terrain => painter.terrain(&mut cell, frame),
                    }
                    // The invariant the anchor check depends on: nothing is
                    // composited below the ground contact point, whatever a
                    // shape routine drew there. Terrain is exempt — its diamond
                    // surrounds the anchor rather than standing on it.
                    let floor = match entry.class.kind() {
                        Kind::Terrain => fh - 1,
                        _ => anchor.1,
                    };
                    for y in 0..=floor {
                        for x in 0..fw {
                            let index = cell.get(x, y);
                            if index != 0 {
                                sheet.set(frame * fw + x, row * fh + y, index);
                            }
                        }
                    }
                }
            }
        }

        let dir = out_dir.join(entry.name);
        let sheet_name = format!("{}.png", entry.name);
        sheet.write_png(&dir.join(&sheet_name), palette)?;

        let manifest = render_manifest(entry, &anims, anchor, &sheet_name, &palette.name);
        let manifest_path = dir.join(format!("{}.ron", entry.name));
        std::fs::write(&manifest_path, manifest)
            .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        written.push(manifest_path);
    }

    Ok(written)
}

fn render_manifest(
    entry: &Entry,
    anims: &[AnimationSpec],
    anchor: (u32, u32),
    sheet: &str,
    palette: &str,
) -> String {
    let mut s = String::new();
    s.push_str(
        "// Generated by `atlas placeholder`. Do not edit: regenerate.\n\
         // Replaced wholesale when real art lands; the format is the contract.\n",
    );
    s.push_str("SpriteSet(\n");
    s.push_str(&format!("    name: \"{}\",\n", entry.name));
    s.push_str(&format!("    class: {:?},\n", entry.class));
    s.push_str("    scale: 2,\n");
    s.push_str(&format!("    palette: \"{palette}\",\n"));
    s.push_str(&format!("    sheet: \"{sheet}\",\n"));
    s.push_str(&format!("    anchor: ({}, {}),\n", anchor.0, anchor.1));
    s.push_str("    animations: [\n");
    for a in anims {
        let impact = match a.impact {
            Some(f) => format!("Some({f})"),
            None => "None".to_string(),
        };
        s.push_str(&format!(
            "        (name: \"{}\", frames: {}, frame_ms: {}, loops: {}, impact: {impact}),\n",
            a.name, a.frames, a.frame_ms, a.loops
        ));
    }
    s.push_str("    ],\n");
    s.push_str("    anchor_overrides: [],\n");
    s.push_str(")\n");
    s
}

/// How many sets the placeholder catalogue holds, for the CLI summary.
pub fn catalogue_len() -> usize {
    CATALOGUE.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::PaletteSpec;
    use crate::validate::validate;

    #[test]
    fn every_placeholder_passes_the_gate_it_will_ship_through() {
        let palette = PaletteSpec::load(Path::new("../../assets/palette/ancient.ron"))
            .unwrap()
            .bake()
            .unwrap();
        let dir = std::env::temp_dir().join("atlas-placeholder-conformance");
        std::fs::remove_dir_all(&dir).ok();

        let manifests =
            generate_except(&dir, &palette, Path::new("")).expect("generation must succeed");
        assert_eq!(manifests.len(), CATALOGUE.len());

        let mut failures = Vec::new();
        for m in &manifests {
            let report = validate(m, &palette).expect("manifest must load");
            if !report.ok() {
                failures.push(format!(
                    "{}:\n    {}",
                    report.set,
                    report.problems.join("\n    ")
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "placeholder art must pass its own validator:\n{}",
            failures.join("\n")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn generation_is_reproducible() {
        let palette = PaletteSpec::load(Path::new("../../assets/palette/ancient.ron"))
            .unwrap()
            .bake()
            .unwrap();
        let base = std::env::temp_dir().join("atlas-placeholder-repeatable");
        std::fs::remove_dir_all(&base).ok();
        let (a, b) = (base.join("a"), base.join("b"));
        generate_except(&a, &palette, Path::new("")).unwrap();
        generate_except(&b, &palette, Path::new("")).unwrap();

        for entry in CATALOGUE {
            let name = entry.name;
            for file in [format!("{name}.png"), format!("{name}.ron")] {
                let pa = std::fs::read(a.join(name).join(&file)).unwrap();
                let pb = std::fs::read(b.join(name).join(&file)).unwrap();
                assert_eq!(pa, pb, "{file} differs between runs");
            }
        }
        std::fs::remove_dir_all(&base).ok();
    }
}
