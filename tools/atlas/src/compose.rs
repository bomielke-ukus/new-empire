//! Turning a directory of renders into a validated sprite set.
//!
//! `tools/render/render_sheet.py` writes one RGBA PNG per frame, named
//! `<animation>_<facing>_<index>.png`, and knows nothing else. Everything after
//! that — quantising to the palette, the sheet grid, the manifest, the anchors
//! — happens here, so the layout is defined once (in `manifest`) rather than in
//! a Rust file and a Python file that drift apart.
//!
//! This is also the half of the render pipeline that can be tested without
//! Blender: synthesise renders, compose them, and check the result passes the
//! same gate real art does.

use crate::image::Indexed;
use crate::manifest::{default_animation, sheet_order, AnimationSpec, Class, Kind, SpriteSet};
use crate::palette::Palette;
use crate::quantize::{quantize, Options};
use crate::rig::Rig;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One rendered frame, located by where it belongs in the sheet.
#[derive(Debug)]
struct Render {
    animation: String,
    facing: String,
    index: u32,
    path: PathBuf,
}

/// Parses `<animation>_<facing>_<index>.png`. The animation name may itself
/// contain underscores (`carry_wood`), so the facing and index are taken from
/// the end.
fn parse_name(path: &Path) -> Option<Render> {
    let stem = path.file_stem()?.to_str()?;
    let (rest, index) = stem.rsplit_once('_')?;
    let (animation, facing) = rest.rsplit_once('_')?;
    Some(Render {
        animation: animation.to_string(),
        facing: facing.to_string(),
        index: index.parse().ok()?,
        path: path.to_path_buf(),
    })
}

fn collect(dir: &Path) -> Result<Vec<Render>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut out = Vec::new();
    let mut skipped = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.extension().is_some_and(|e| e == "png") {
            match parse_name(&path) {
                Some(r) => out.push(r),
                None => skipped.push(path),
            }
        }
    }
    if !skipped.is_empty() {
        return Err(format!(
            "{}: {} file(s) are not named <animation>_<facing>_<index>.png, \
             starting with {}",
            dir.display(),
            skipped.len(),
            skipped[0].display()
        ));
    }
    if out.is_empty() {
        return Err(format!("{}: no renders found", dir.display()));
    }
    Ok(out)
}

#[derive(Debug)]
pub struct Composed {
    pub manifest: PathBuf,
    pub sheet: PathBuf,
    pub frames: usize,
}

/// Composes the renders in `renders` into `out_dir` as `<name>.png` plus
/// `<name>.ron`.
pub fn compose(
    renders: &Path,
    name: &str,
    class: Class,
    out_dir: &Path,
    palette: &Palette,
    rig: &Rig,
) -> Result<Composed, String> {
    let found = collect(renders)?;

    // Group by animation, then facing, then frame index.
    let mut by_anim: BTreeMap<String, BTreeMap<String, Vec<&Render>>> = BTreeMap::new();
    for r in &found {
        by_anim
            .entry(r.animation.clone())
            .or_default()
            .entry(r.facing.clone())
            .or_default()
            .push(r);
    }

    let class_rig = rig
        .classes
        .get(class_key(class))
        .ok_or_else(|| format!("the rig has no size class {}", class_key(class)))?;

    // The facings a set of this class stores, as names.
    let want_facings: Vec<&str> = match class.kind() {
        Kind::Mobile => crate::manifest::AUTHORED_FACINGS
            .iter()
            .map(|f| f.name())
            .collect(),
        _ => vec!["S"],
    };

    let present: Vec<String> = by_anim.keys().cloned().collect();
    let order = sheet_order(class, &present);

    // Each animation must have every facing, with the same frame count.
    let mut animations: Vec<AnimationSpec> = Vec::new();
    for anim in &order {
        let facings = &by_anim[anim];
        let mut frames: Option<u32> = None;
        for facing in &want_facings {
            let Some(rendered) = facings.get(*facing) else {
                return Err(format!(
                    "'{anim}' has no frames for facing {facing}; a {:?} set needs {:?}",
                    class.kind(),
                    want_facings
                ));
            };
            let count = rendered.len() as u32;
            // Indices must run 0..count with nothing missing.
            let mut seen: Vec<u32> = rendered.iter().map(|r| r.index).collect();
            seen.sort_unstable();
            if seen != (0..count).collect::<Vec<_>>() {
                return Err(format!(
                    "'{anim}' facing {facing} has frame indices {seen:?}; they must \
                     run 0..{count} with none missing"
                ));
            }
            match frames {
                None => frames = Some(count),
                Some(n) if n != count => {
                    return Err(format!(
                        "'{anim}' has {n} frames for {} but {count} for {facing}; \
                         every facing of an animation must be the same length",
                        want_facings[0]
                    ))
                }
                Some(_) => {}
            }
        }
        for extra in facings.keys() {
            if !want_facings.contains(&extra.as_str()) {
                return Err(format!(
                    "'{anim}' was rendered for facing {extra}, which a {:?} set does \
                     not store; SE, E and NE are mirrored at draw time, not authored",
                    class.kind()
                ));
            }
        }
        animations.push(default_animation(anim, frames.unwrap()));
    }

    // Build the sheet from the manifest's own layout, so composition and
    // validation cannot disagree about where a frame goes.
    let set = SpriteSet {
        name: name.to_string(),
        class,
        scale: rig.projection.authoring_scale,
        palette: palette.name.clone(),
        sheet: format!("{name}.png"),
        anchor: (class_rig.anchor_px[0], class_rig.anchor_px[1]),
        animations,
        anchor_overrides: vec![],
    };

    let (fw, fh) = set.frame_size();
    let (sw, sh) = set.sheet_size();
    let mut sheet = Indexed::new(sw, sh);
    let opts = Options {
        downsample: rig.projection.supersample,
        ..Default::default()
    };

    let mut count = 0;
    for (ai, anim) in set.animations.iter().enumerate() {
        for (fi, facing) in set.facings().iter().enumerate() {
            let row = set.row(ai, fi);
            for frame in 0..anim.frames {
                let render = by_anim[&anim.name][facing.name()]
                    .iter()
                    .find(|r| r.index == frame)
                    .expect("frame indices were checked above");
                let cell = quantize(&render.path, palette, &opts)?;
                if (cell.width, cell.height) != (fw, fh) {
                    return Err(format!(
                        "{}: quantises to {}x{}, but a {:?} frame is {fw}x{fh}. The \
                         render should be {}x{} — check `atlas rig` for the size this \
                         class renders at.",
                        render.path.display(),
                        cell.width,
                        cell.height,
                        class,
                        class_rig.render_px[0],
                        class_rig.render_px[1]
                    ));
                }
                for y in 0..fh {
                    for x in 0..fw {
                        let index = cell.get(x, y);
                        if index != 0 {
                            sheet.set(frame * fw + x, row * fh + y, index);
                        }
                    }
                }
                count += 1;
            }
        }
    }

    let sheet_path = out_dir.join(&set.sheet);
    sheet.write_png(&sheet_path, palette)?;
    let manifest_path = out_dir.join(format!("{name}.ron"));
    std::fs::write(&manifest_path, render_manifest(&set))
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;

    Ok(Composed {
        manifest: manifest_path,
        sheet: sheet_path,
        frames: count,
    })
}

fn class_key(class: Class) -> &'static str {
    match class {
        Class::Foot => "Foot",
        Class::Mounted => "Mounted",
        Class::Heavy => "Heavy",
        Class::SmallBuilding => "SmallBuilding",
        Class::MediumBuilding => "MediumBuilding",
        Class::LargeBuilding => "LargeBuilding",
        Class::Wonder => "Wonder",
        Class::Terrain => "Terrain",
    }
}

pub fn parse_class(name: &str) -> Result<Class, String> {
    match name {
        "Foot" => Ok(Class::Foot),
        "Mounted" => Ok(Class::Mounted),
        "Heavy" => Ok(Class::Heavy),
        "SmallBuilding" => Ok(Class::SmallBuilding),
        "MediumBuilding" => Ok(Class::MediumBuilding),
        "LargeBuilding" => Ok(Class::LargeBuilding),
        "Wonder" => Ok(Class::Wonder),
        "Terrain" => Ok(Class::Terrain),
        other => Err(format!(
            "unknown size class '{other}'; docs/05 §2.3 has Foot, Mounted, Heavy, \
             SmallBuilding, MediumBuilding, LargeBuilding, Wonder, Terrain"
        )),
    }
}

fn render_manifest(set: &SpriteSet) -> String {
    let mut s = String::new();
    s.push_str(
        "// Generated by `atlas compose` from a directory of renders.\n\
         // Edit the models and re-render; anchor overrides can be added by hand.\n",
    );
    s.push_str("SpriteSet(\n");
    s.push_str(&format!("    name: \"{}\",\n", set.name));
    s.push_str(&format!("    class: {:?},\n", set.class));
    s.push_str(&format!("    scale: {},\n", set.scale));
    s.push_str(&format!("    palette: \"{}\",\n", set.palette));
    s.push_str(&format!("    sheet: \"{}\",\n", set.sheet));
    s.push_str(&format!(
        "    anchor: ({}, {}),\n",
        set.anchor.0, set.anchor.1
    ));
    s.push_str("    animations: [\n");
    for a in &set.animations {
        let impact = match a.impact {
            Some(f) => format!("Some({f})"),
            None => "None".to_string(),
        };
        s.push_str(&format!(
            "        (name: \"{}\", frames: {}, frame_ms: {}, loops: {}, impact: {impact}),\n",
            a.name, a.frames, a.frame_ms, a.loops
        ));
    }
    s.push_str("    ],\n    anchor_overrides: [],\n)\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::Srgb;
    use crate::manifest::REQUIRED_MOBILE;
    use crate::palette::PaletteSpec;
    use crate::validate::validate;

    fn palette() -> Palette {
        PaletteSpec::load(Path::new("../../assets/palette/ancient.ron"))
            .unwrap()
            .bake()
            .unwrap()
    }

    fn rig() -> Rig {
        Rig::load(Path::new("../../assets/render/rig.json")).unwrap()
    }

    /// Stands in for Blender: writes an RGBA PNG the size the rig says this
    /// class renders at, with a figure standing on the class's anchor and a
    /// magenta patch where player colour belongs.
    fn fake_render(path: &Path, w: u32, h: u32, anchor_y: u32, seed: u32) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut px = vec![0u8; (w * h * 4) as usize];
        let (cx, base) = (w / 2, anchor_y);
        let body_h = base.min(h / 2) - 4;
        for y in (base - body_h)..base {
            for x in (cx - w / 8)..(cx + w / 8) {
                let i = ((y * w + x) * 4) as usize;
                // Lower half magenta (player colour), upper half a brown tunic.
                let magenta = y > base - body_h / 2;
                let c = if magenta {
                    Srgb {
                        r: 200,
                        g: 0,
                        b: 200,
                    }
                } else {
                    Srgb {
                        r: 0x8f,
                        g: (0x60 + (seed % 8) as u8),
                        b: 0x35,
                    }
                };
                px[i] = c.r;
                px[i + 1] = c.g;
                px[i + 2] = c.b;
                px[i + 3] = 255;
            }
        }
        let file = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&px).unwrap();
    }

    fn render_a_villager(dir: &Path, rig: &Rig) {
        let c = &rig.classes["Foot"];
        let ss = rig.projection.supersample;
        for (anim, frames) in REQUIRED_MOBILE {
            for facing in crate::manifest::AUTHORED_FACINGS {
                for i in 0..frames {
                    fake_render(
                        &dir.join(format!("{anim}_{}_{i:02}.png", facing.name())),
                        c.render_px[0],
                        c.render_px[1],
                        c.anchor_px[1] * ss,
                        i,
                    );
                }
            }
        }
    }

    #[test]
    fn renders_compose_into_a_sprite_set_that_passes_the_gate() {
        let (palette, rig) = (palette(), rig());
        let base = std::env::temp_dir().join("atlas-compose-ok");
        std::fs::remove_dir_all(&base).ok();
        let (renders, out) = (base.join("renders"), base.join("out"));
        render_a_villager(&renders, &rig);

        let done = compose(&renders, "villager", Class::Foot, &out, &palette, &rig)
            .expect("compose must succeed");
        // 30 frames per facing x 5 authored facings.
        assert_eq!(done.frames, 150);

        let report = validate(&done.manifest, &palette).expect("manifest must load");
        assert!(
            report.ok(),
            "composed art must pass validation:\n  {}",
            report.problems.join("\n  ")
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn the_anchor_comes_from_the_rig_not_from_a_guess() {
        let (palette, rig) = (palette(), rig());
        let base = std::env::temp_dir().join("atlas-compose-anchor");
        std::fs::remove_dir_all(&base).ok();
        let (renders, out) = (base.join("renders"), base.join("out"));
        render_a_villager(&renders, &rig);
        let done = compose(&renders, "villager", Class::Foot, &out, &palette, &rig).unwrap();

        let text = std::fs::read_to_string(&done.manifest).unwrap();
        let c = &rig.classes["Foot"];
        assert!(
            text.contains(&format!("anchor: ({}, {})", c.anchor_px[0], c.anchor_px[1])),
            "manifest should anchor where the rig aims the camera:\n{text}"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_missing_facing_is_reported_rather_than_silently_dropped() {
        let (palette, rig) = (palette(), rig());
        let base = std::env::temp_dir().join("atlas-compose-missing");
        std::fs::remove_dir_all(&base).ok();
        let (renders, out) = (base.join("renders"), base.join("out"));
        render_a_villager(&renders, &rig);
        for i in 0..8 {
            std::fs::remove_file(renders.join(format!("walk_NW_{i:02}.png"))).unwrap();
        }
        let err = compose(&renders, "villager", Class::Foot, &out, &palette, &rig).unwrap_err();
        assert!(err.contains("no frames for facing NW"), "got: {err}");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_render_at_the_wrong_size_says_what_the_right_size_is() {
        let (palette, rig) = (palette(), rig());
        let base = std::env::temp_dir().join("atlas-compose-size");
        std::fs::remove_dir_all(&base).ok();
        let (renders, out) = (base.join("renders"), base.join("out"));
        render_a_villager(&renders, &rig);
        // Re-render one frame at the Mounted size.
        let m = &rig.classes["Mounted"];
        fake_render(
            &renders.join("idle_S_00.png"),
            m.render_px[0],
            m.render_px[1],
            m.anchor_px[1] * rig.projection.supersample,
            0,
        );
        let err = compose(&renders, "villager", Class::Foot, &out, &palette, &rig).unwrap_err();
        assert!(err.contains("check `atlas rig`"), "got: {err}");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn misnamed_files_are_rejected_up_front() {
        let base = std::env::temp_dir().join("atlas-compose-naming");
        std::fs::remove_dir_all(&base).ok();
        let renders = base.join("renders");
        fake_render(&renders.join("villager0001.png"), 160, 192, 186, 0);
        let err = collect(&renders).unwrap_err();
        assert!(
            err.contains("<animation>_<facing>_<index>.png"),
            "got: {err}"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn animation_names_with_underscores_survive_parsing() {
        let r = parse_name(Path::new("/x/carry_wood_SW_03.png")).unwrap();
        assert_eq!(r.animation, "carry_wood");
        assert_eq!(r.facing, "SW");
        assert_eq!(r.index, 3);
    }
}
