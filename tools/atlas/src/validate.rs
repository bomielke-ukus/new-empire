//! The conformance gate.
//!
//! `docs/05` §6: "Every sprite goes through `tools/atlas`, which validates size,
//! anchor, facing count and palette conformance and **fails the build** on
//! violation. That validation is what keeps art from four sources looking like
//! one game."
//!
//! This is that. It reports every violation it finds rather than stopping at
//! the first, because an artist fixing a sheet wants the whole list, and it
//! phrases each one as what to change rather than as what is wrong.

use crate::image::read_png;
use crate::manifest::{Class, Kind, SpriteSet, REQUIRED_BUILDING, REQUIRED_MOBILE, VILLAGER_TASKS};
use crate::palette::{Palette, TRANSPARENT};
use std::path::Path;

pub struct Report {
    pub set: String,
    pub problems: Vec<String>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.problems.is_empty()
    }
}

/// Validates one sprite set against the palette and the specs.
pub fn validate(manifest_path: &Path, palette: &Palette) -> Result<Report, String> {
    let set = SpriteSet::load(manifest_path)?;
    let mut problems = Vec::new();

    if set.palette != palette.name {
        problems.push(format!(
            "declares palette '{}' but was validated against '{}'",
            set.palette, palette.name
        ));
    }
    if set.scale != 2 {
        problems.push(format!(
            "authored at {}x; docs/05 §1 says art is authored at 2x and downsampled",
            set.scale
        ));
    }

    check_animations(&set, &mut problems);

    let sheet_path = manifest_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(&set.sheet);
    let sheet = match read_png(&sheet_path) {
        Ok(s) => s,
        Err(e) => {
            problems.push(e);
            return Ok(Report {
                set: set.name,
                problems,
            });
        }
    };

    if sheet.transparent_index != Some(TRANSPARENT) {
        problems.push(format!(
            "{}: index {TRANSPARENT} must be the transparent entry in tRNS, found {:?}",
            set.sheet, sheet.transparent_index
        ));
    }
    if sheet.palette.len() != 256 {
        problems.push(format!(
            "{}: carries {} palette entries; sprites must ship all 256 so the \
             renderer can upload one palette for every sheet",
            set.sheet,
            sheet.palette.len()
        ));
    } else if let Some(i) = (0..256).find(|&i| sheet.palette[i] != palette.entries[i]) {
        problems.push(format!(
            "{}: palette entry {i} is {} but the '{}' palette says {}. The sheet \
             was exported against a different palette; re-export against \
             `atlas palette export`.",
            set.sheet,
            sheet.palette[i].to_hex(),
            palette.name,
            palette.entries[i].to_hex()
        ));
    }

    let (want_w, want_h) = set.sheet_size();
    if (sheet.image.width, sheet.image.height) != (want_w, want_h) {
        let (fw, fh) = set.frame_size();
        problems.push(format!(
            "{}: is {}x{} but the manifest describes a {}x{} sheet \
             ({} frames wide, {} rows of {}x{} at {}x)",
            set.sheet,
            sheet.image.width,
            sheet.image.height,
            want_w,
            want_h,
            want_w / fw.max(1),
            want_h / fh.max(1),
            fw,
            fh,
            set.scale
        ));
        // Every check below indexes into the sheet, so stop here.
        return Ok(Report {
            set: set.name,
            problems,
        });
    }

    check_pixels(&set, &sheet.image, palette, &mut problems);
    check_frames_and_anchors(&set, &sheet.image, &mut problems);
    check_anchor_overrides(&set, &mut problems);

    Ok(Report {
        set: set.name,
        problems,
    })
}

/// The animation set is a contract with the simulation: it schedules damage on
/// a tick and the renderer has to land the blow on the matching frame.
fn check_animations(set: &SpriteSet, problems: &mut Vec<String>) {
    let required: &[(&str, u32)] = match set.class.kind() {
        Kind::Mobile => &REQUIRED_MOBILE,
        Kind::Building => &REQUIRED_BUILDING,
        Kind::Terrain => &[],
    };

    for (name, frames) in required {
        match set.animations.iter().find(|a| a.name == *name) {
            None => problems.push(format!(
                "is missing the '{name}' animation, which docs/05 §2.2 requires \
                 of every {:?} set",
                set.class.kind()
            )),
            Some(a) if a.frames != *frames => problems.push(format!(
                "'{name}' has {} frames; docs/05 §2.2 specifies {frames}",
                a.frames
            )),
            Some(_) => {}
        }
    }

    for a in &set.animations {
        if a.frames == 0 {
            problems.push(format!("'{}' has no frames", a.name));
        }
        if a.frame_ms == 0 {
            problems.push(format!("'{}' has a frame time of 0 ms", a.name));
        }
        // The impact frame is what makes a hit feel like it connected. Without
        // it the renderer has to guess, and it will guess the middle.
        if a.name == "attack" {
            match a.impact {
                None => problems.push(
                    "'attack' does not tag an impact frame; docs/05 §2.2 requires \
                     one so damage and animation agree"
                        .to_string(),
                ),
                Some(f) if f >= a.frames => problems.push(format!(
                    "'attack' tags impact on frame {f} but only has {} frames",
                    a.frames
                )),
                Some(_) => {}
            }
        } else if a.impact.is_some() {
            problems.push(format!(
                "'{}' tags an impact frame, which only means something on 'attack'",
                a.name
            ));
        }
    }

    // Animation names are looked up by the simulation, which has no way to
    // report a miss: a villager whose chopping animation is called "choop" just
    // stands still while it works. So the vocabulary is closed.
    for a in &set.animations {
        let known = required.iter().any(|(n, _)| *n == a.name)
            || VILLAGER_TASKS.contains(&a.name.as_str())
            // Carry variants are per-resource: carry_wood, carry_food, ...
            || a.name.starts_with("carry_")
            || (set.class == Class::Terrain && a.name == "variants");
        if !known {
            problems.push(format!(
                "'{}' is not an animation name the simulation knows. Expected one \
                 of {:?}, a villager task {:?}, or carry_<resource>.",
                a.name,
                required.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
                VILLAGER_TASKS
            ));
        }
    }

    let mut seen: Vec<&str> = set.animations.iter().map(|a| a.name.as_str()).collect();
    seen.sort_unstable();
    for pair in seen.windows(2) {
        if pair[0] == pair[1] {
            problems.push(format!("'{}' is declared twice", pair[0]));
        }
    }
}

/// Palette conformance, pixel by pixel.
fn check_pixels(
    set: &SpriteSet,
    image: &crate::image::Indexed,
    palette: &Palette,
    problems: &mut Vec<String>,
) {
    let mut reserve = Vec::new();
    let mut error = 0usize;
    let mut player_pixels = 0usize;

    for &index in &image.pixels {
        match palette.owner[index as usize].as_deref() {
            Some("reserve") => {
                if !reserve.contains(&index) {
                    reserve.push(index);
                }
            }
            Some("error") => error += 1,
            Some("player") => player_pixels += 1,
            _ => {}
        }
    }

    if !reserve.is_empty() {
        reserve.sort_unstable();
        problems.push(format!(
            "{}: uses reserved indices {reserve:?}, which the palette has not \
             assigned a colour yet. Pick a material ramp, or claim the reserve \
             in assets/palette/{}.ron.",
            set.sheet, palette.name
        ));
    }
    if error > 0 {
        problems.push(format!(
            "{}: has {error} pixels in the error index. Something wrote a colour \
             the palette does not define.",
            set.sheet
        ));
    }

    // Ownership has to be visible. A unit with no player colour anywhere on it
    // belongs to nobody as far as the player is concerned.
    let needs_owner = matches!(set.class.kind(), Kind::Mobile | Kind::Building);
    if needs_owner && player_pixels == 0 {
        problems.push(format!(
            "{}: uses no player colour at all. Draw some of it in indices \
             240..=247 or its owner will be invisible (docs/05 §2.4).",
            set.sheet
        ));
    }
    if set.class == Class::Terrain && player_pixels > 0 {
        problems.push(format!(
            "{}: terrain uses {player_pixels} player-colour pixels; terrain has \
             no owner and would recolour itself per player.",
            set.sheet
        ));
    }
}

/// Every declared frame must hold a sprite, and every anchor must sit under it.
fn check_frames_and_anchors(
    set: &SpriteSet,
    image: &crate::image::Indexed,
    problems: &mut Vec<String>,
) {
    let (fw, fh) = set.frame_size();
    let facings = set.facings();

    for (ai, anim) in set.animations.iter().enumerate() {
        for (fi, facing) in facings.iter().enumerate() {
            let row = set.row(ai, fi);
            for frame in 0..anim.frames {
                let cell = image.crop(frame * fw, row * fh, fw, fh);
                let Some((bx, by, bw, bh)) = cell.content_bounds() else {
                    problems.push(format!(
                        "{} {} frame {frame} is empty; the sheet grid is one row \
                         per (animation, facing) and one column per frame",
                        anim.name,
                        facing.name()
                    ));
                    continue;
                };

                let (ax, ay) = set.anchor_for(&anim.name, *facing, frame);
                if ax >= fw || ay >= fh {
                    problems.push(format!(
                        "{} {} frame {frame}: anchor ({ax}, {ay}) is outside the \
                         {fw}x{fh} frame",
                        anim.name,
                        facing.name()
                    ));
                    continue;
                }

                // The anchor is the ground contact point (docs/05 §2.3). If it
                // is not horizontally under the sprite, the sprite will sit
                // beside where the simulation thinks the unit is.
                if ax < bx || ax >= bx + bw {
                    problems.push(format!(
                        "{} {} frame {frame}: anchor x {ax} is outside the drawn \
                         sprite (x {bx}..{}), so it would not stand where the \
                         simulation puts it",
                        anim.name,
                        facing.name(),
                        bx + bw
                    ));
                }
                // Decay is a corpse settling into the ground, so it is allowed
                // to sink below its own anchor. Nothing else is.
                if anim.name != "decay" && ay + 1 < by + bh {
                    problems.push(format!(
                        "{} {} frame {frame}: anchor y {ay} is above the sprite's \
                         lowest pixel ({}), so it would float",
                        anim.name,
                        facing.name(),
                        by + bh - 1
                    ));
                }
            }
        }
    }
}

fn check_anchor_overrides(set: &SpriteSet, problems: &mut Vec<String>) {
    for o in &set.anchor_overrides {
        let Some(anim) = set.animations.iter().find(|a| a.name == o.animation) else {
            problems.push(format!(
                "anchor override names animation '{}', which this set does not have",
                o.animation
            ));
            continue;
        };
        if o.frame >= anim.frames {
            problems.push(format!(
                "anchor override for '{}' frame {} is past its {} frames",
                o.animation, o.frame, anim.frames
            ));
        }
        if !set.facings().contains(&o.facing) {
            problems.push(format!(
                "anchor override for '{}' names facing {}, which a {:?} set does \
                 not store",
                o.animation,
                o.facing.name(),
                set.class.kind()
            ));
        }
    }
}
