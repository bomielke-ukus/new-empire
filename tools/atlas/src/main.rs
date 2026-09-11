//! The art conformance gate.
//!
//! ```text
//! atlas rig         [--rig FILE]
//! atlas palette     [--palette FILE]
//! atlas export      [--palette FILE] [--out DIR]
//! atlas validate    [--assets DIR] [--palette FILE]
//! atlas placeholder [--out DIR] [--palette FILE]
//! atlas quantize    --in FILE --out FILE [--downsample N] [--palette FILE]
//! atlas compose     --renders DIR --set NAME --class CLASS --out DIR
//! ```
//!
//! `validate` is the one CI runs. `docs/05` §6 requires that non-conformant art
//! fails the build, which means this exits non-zero and says what to fix.

mod colour;
mod compose;
mod image;
mod manifest;
mod palette;
mod placeholder;
mod quantize;
mod rig;
mod validate;

use colour::{simulate, Deficiency, Srgb};
use image::Indexed;
use palette::{Palette, PaletteSpec, PLAYER_RAMP_LEN, PLAYER_RAMP_START};
use rig::Rig;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const DEFAULT_PALETTE: &str = "assets/palette/ancient.ron";
/// Real art, committed. Rendered through Blender, which CI does not have, so
/// it cannot be regenerated on demand the way the palette and placeholders can.
const DEFAULT_ASSETS: &str = "assets/sprites";
/// Placeholder art, generated. Kept in its own tree so `atlas placeholder`
/// can never overwrite a rendered sprite set.
const DEFAULT_PLACEHOLDERS: &str = "assets/placeholder-sprites";
const DEFAULT_RIG: &str = "assets/render/rig.json";

struct Args {
    palette: PathBuf,
    rig: PathBuf,
    assets: PathBuf,
    out: PathBuf,
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    downsample: u32,
    renders: Option<PathBuf>,
    set: Option<String>,
    rust: Option<PathBuf>,
    class: Option<String>,
}

fn parse(args: &[String], default_out: &str) -> Result<Args, String> {
    let mut out = Args {
        palette: PathBuf::from(DEFAULT_PALETTE),
        rig: PathBuf::from(DEFAULT_RIG),
        assets: PathBuf::from(DEFAULT_ASSETS),
        out: PathBuf::from(default_out),
        input: None,
        output: None,
        downsample: 2,
        renders: None,
        set: None,
        rust: None,
        class: None,
    };
    let mut i = 0;
    while i < args.len() {
        let key = &args[i];
        let val = args
            .get(i + 1)
            .ok_or_else(|| format!("{key} needs a value"))?;
        match key.as_str() {
            "--palette" => out.palette = PathBuf::from(val),
            "--rig" => out.rig = PathBuf::from(val),
            "--assets" => out.assets = PathBuf::from(val),
            "--out" => out.out = PathBuf::from(val),
            "--in" => out.input = Some(PathBuf::from(val)),
            "--renders" => out.renders = Some(PathBuf::from(val)),
            "--set" => out.set = Some(val.clone()),
            "--rust" => out.rust = Some(PathBuf::from(val)),
            "--class" => out.class = Some(val.clone()),
            "--downsample" => {
                out.downsample = val.parse().map_err(|e| format!("--downsample: {e}"))?
            }
            _ => return Err(format!("unknown flag {key}")),
        }
        i += 2;
    }
    // `quantize` uses --out as a file rather than a directory.
    out.output = Some(out.out.clone());
    Ok(out)
}

fn load_palette(path: &Path) -> Result<Palette, String> {
    PaletteSpec::load(path)?.bake()
}

/// Every `.ron` under `dir`, sorted, so output is stable across platforms.
fn find_manifests(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries = std::fs::read_dir(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("{}: {e}", d.display()))?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "ron") {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
}

/// Prints the rig and everything derived from it, and fails if any of it has
/// drifted from the spec. This is what CI runs; it is also the quickest way for
/// someone setting up Blender by hand to see the numbers they need.
fn rig_report(args: &[String]) -> ExitCode {
    let a = match parse(args, "") {
        Ok(a) => a,
        Err(e) => return usage(&e),
    };
    let rig = match Rig::load(&a.rig) {
        Ok(r) => r,
        Err(e) => return fail(&e),
    };
    let b = rig.basis();

    println!("rig v{} — {}", rig.version, a.rig.display());
    println!(
        "\ncamera: orthographic, {}° above the horizon, euler {:?}",
        rig.camera.elevation_deg, rig.camera.rotation_euler_xyz_deg
    );
    println!("  location {:?}", rig.camera.location);
    println!("  right   {:?}", b.right);
    println!("  up      {:?}", b.up);
    println!("  forward {:?}", b.forward);
    println!(
        "  1x scale: {:.4} px per world unit across, {:.4} px per world unit of height",
        rig.px_per_unit_1x(),
        rig.px_per_unit_1x() * rig.camera.elevation_deg.to_radians().cos()
    );

    println!("\nfacings (the subject turns; the camera never does):");
    for f in &rig.facings {
        println!(
            "  {:<3} subject Z {:>7.1}°   screen ({:+.3}, {:+.3})",
            f.name, f.subject_z_rotation_deg, f.screen_direction[0], f.screen_direction[1]
        );
    }

    println!("\nlights (placed by where they sit on screen, not in the world):");
    for l in &rig.lights {
        println!(
            "  {:<5} energy {:>4.1}   euler {:>8.3}, {:.1}, {:>9.3}   screen ({:+.3}, {:+.3})",
            l.name,
            l.energy,
            l.rotation_euler_xyz_deg[0],
            l.rotation_euler_xyz_deg[1],
            l.rotation_euler_xyz_deg[2],
            l.screen_position.right,
            l.screen_position.up
        );
    }

    println!("\nlight reaching each visible face (energy x Lambert):");
    for (label, normal) in [
        ("screen-left  (+X)", [1.0, 0.0, 0.0]),
        ("screen-right (+Y)", [0.0, 1.0, 0.0]),
        ("top          (+Z)", [0.0, 0.0, 1.0]),
    ] {
        println!("  {label}  {:.3}", rig.illumination(normal));
    }

    println!("\nsize classes:");
    for (name, c) in &rig.classes {
        println!(
            "  {name:<15} sprite {:>3}x{:<3} render {:>4}x{:<4} ortho_scale {:.6}",
            c.sprite_px[0], c.sprite_px[1], c.render_px[0], c.render_px[1], c.ortho_scale
        );
    }

    let problems = rig.problems();
    if problems.is_empty() {
        println!("\nthe rig agrees with docs/05 and the size class table");
        ExitCode::SUCCESS
    } else {
        println!();
        for p in &problems {
            println!("  FAIL {p}");
        }
        ExitCode::FAILURE
    }
}

fn palette_report(args: &[String]) -> ExitCode {
    let a = match parse(args, "") {
        Ok(a) => a,
        Err(e) => return usage(&e),
    };
    let palette = match load_palette(&a.palette) {
        Ok(p) => p,
        Err(e) => return fail(&e),
    };

    println!("palette '{}' baked: 256 indices, all claimed", palette.name);
    println!("\nplayer colours, worst separation over the ramp body:");
    let models: [(&str, Option<Deficiency>); 3] = [
        ("normal", None),
        (Deficiency::Protanopia.name(), Some(Deficiency::Protanopia)),
        (
            Deficiency::Deuteranopia.name(),
            Some(Deficiency::Deuteranopia),
        ),
    ];
    for (label, deficiency) in models {
        let seen = |c: Srgb| match deficiency {
            Some(d) => simulate(c, d),
            None => c,
        };
        let mut worst = (f64::MAX, String::new());
        for (i, (name_a, ramp_a)) in palette.players.iter().enumerate() {
            for (name_b, ramp_b) in palette.players.iter().skip(i + 1) {
                for step in 2..6 {
                    use colour::{Linear, Oklab};
                    let d = Oklab::from(Linear::from(seen(ramp_a[step])))
                        .distance(Oklab::from(Linear::from(seen(ramp_b[step]))));
                    if d < worst.0 {
                        worst = (d, format!("{name_a} vs {name_b}"));
                    }
                }
            }
        }
        println!("  {label:<13} {:.3}   ({})", worst.0, worst.1);
    }

    let collisions = palette.player_collisions();
    if collisions.is_empty() {
        println!("\nall pairs clear their bar");
        ExitCode::SUCCESS
    } else {
        println!();
        for c in &collisions {
            println!(
                "  FAIL {} vs {} under {}: {:.3}, needs {:.3}",
                c.a, c.b, c.vision, c.distance, c.required
            );
        }
        ExitCode::FAILURE
    }
}

/// Writes the palette in the two forms an artist actually needs: a swatch to
/// look at, and a `.gpl` that Aseprite, GIMP and Krita import directly. Art
/// drawn against an imported `.gpl` conforms by construction.
fn export(args: &[String]) -> ExitCode {
    let a = match parse(args, "assets/palette") {
        Ok(a) => a,
        Err(e) => return usage(&e),
    };
    let palette = match load_palette(&a.palette) {
        Ok(p) => p,
        Err(e) => return fail(&e),
    };

    // 16x16 grid of 8x8 blocks, index 0 top-left, reading order.
    const BLOCK: u32 = 8;
    let mut swatch = Indexed::new(16 * BLOCK, 16 * BLOCK);
    for index in 0..256u32 {
        let (bx, by) = ((index % 16) * BLOCK, (index / 16) * BLOCK);
        for y in 0..BLOCK {
            for x in 0..BLOCK {
                swatch.set(bx + x, by + y, index as u8);
            }
        }
    }
    let swatch_path = a.out.join(format!("{}-swatch.png", palette.name));
    if let Err(e) = swatch.write_png(&swatch_path, &palette) {
        return fail(&e);
    }

    let mut gpl = format!(
        "GIMP Palette\nName: new-empire {}\nColumns: 16\n#\n# Generated by `atlas export`. \
         Import this into Aseprite, GIMP or Krita and draw against it.\n\
         # Indices 240-247 are player colour: draw ownership marks there and nowhere else.\n#\n",
        palette.name
    );
    for (i, c) in palette.entries.iter().enumerate() {
        let owner = palette.owner[i].as_deref().unwrap_or("?");
        gpl.push_str(&format!("{:3} {:3} {:3}\t{i:03} {owner}\n", c.r, c.g, c.b));
    }
    let gpl_path = a.out.join(format!("{}.gpl", palette.name));
    if let Err(e) =
        std::fs::write(&gpl_path, gpl).map_err(|e| format!("{}: {e}", gpl_path.display()))
    {
        return fail(&e);
    }

    println!("wrote {}", swatch_path.display());
    println!("wrote {}", gpl_path.display());

    if let Some(rust_path) = &a.rust {
        if let Err(e) = std::fs::write(rust_path, rust_table(&palette))
            .map_err(|e| format!("{}: {e}", rust_path.display()))
        {
            return fail(&e);
        }
        println!("wrote {}", rust_path.display());
    }
    ExitCode::SUCCESS
}

/// The baked palette as a Rust source file, for the renderer to compile in.
///
/// One palette, one source of truth: the renderer must draw an index as the
/// colour the validator checked it against, and the way to guarantee that is
/// for both to read this file's ancestor. The table is committed and
/// `scripts/check-generated.sh` fails if it drifts from `ancient.ron`.
pub fn rust_table(palette: &Palette) -> String {
    let mut out = String::new();
    out.push_str("//! Generated by `atlas export --rust`. Do not edit by hand.\n");
    out.push_str("//!\n");
    out.push_str(&format!(
        "//! The `{}` palette from `assets/palette/ancient.ron`, baked: 256 sRGB\n",
        palette.name
    ));
    out.push_str("//! entries, the ramps that own them, the specials, and the eight owner\n");
    out.push_str("//! ramps that replace the player slot at draw time.\n\n");
    out.push_str(&format!(
        "/// Palette name.\npub const NAME: &str = \"{}\";\n\n",
        palette.name
    ));

    out.push_str("/// sRGB colour of every index. Index 0 is transparent; alpha is the\n");
    out.push_str("/// renderer's business and is not encoded here.\n");
    out.push_str("pub const ENTRIES: [[u8; 3]; 256] = [\n");
    for row in palette.entries.chunks(8) {
        out.push_str("    ");
        for c in row {
            out.push_str(&format!("[{}, {}, {}], ", c.r, c.g, c.b));
        }
        out.push('\n');
    }
    out.push_str("];\n\n");

    // Ramps: contiguous runs of one owner name.
    out.push_str("/// Every named ramp or special as `(name, first index, length)`, in\n");
    out.push_str("/// index order. Look a colour up by name and step rather than by number.\n");
    out.push_str("pub const RAMPS: &[(&str, u8, u8)] = &[\n");
    let mut i = 0usize;
    while i < 256 {
        let name = palette.owner[i].as_deref().unwrap_or("unclaimed");
        let mut j = i + 1;
        while j < 256 && palette.owner[j].as_deref().unwrap_or("unclaimed") == name {
            j += 1;
        }
        out.push_str(&format!("    (\"{name}\", {i}, {}),\n", j - i));
        i = j;
    }
    out.push_str("];\n\n");

    out.push_str("/// First and last index of the player-colour slot.\n");
    out.push_str(&format!(
        "pub const PLAYER_SLOT: (u8, u8) = ({}, {});\n\n",
        PLAYER_RAMP_START,
        PLAYER_RAMP_START as usize + PLAYER_RAMP_LEN - 1
    ));

    out.push_str("/// The eight owner ramps, dark to light, in assignment order.\n");
    out.push_str(&format!(
        "pub const PLAYERS: [(&str, [[u8; 3]; {}]); {}] = [\n",
        PLAYER_RAMP_LEN,
        palette.players.len()
    ));
    for (name, ramp) in &palette.players {
        out.push_str(&format!("    (\"{name}\", ["));
        for c in ramp {
            out.push_str(&format!("[{}, {}, {}], ", c.r, c.g, c.b));
        }
        out.push_str("]),\n");
    }
    out.push_str("];\n");
    out
}

fn validate_all(args: &[String]) -> ExitCode {
    let a = match parse(args, "") {
        Ok(a) => a,
        Err(e) => return usage(&e),
    };
    let palette = match load_palette(&a.palette) {
        Ok(p) => p,
        Err(e) => return fail(&e),
    };
    // Both trees: real art and the generated placeholders go through the same
    // gate, which is the point of generating placeholders as files at all.
    let mut roots = vec![a.assets.clone()];
    if a.assets == Path::new(DEFAULT_ASSETS) {
        roots.push(PathBuf::from(DEFAULT_PLACEHOLDERS));
    }
    let mut manifests = Vec::new();
    for root in roots.iter().filter(|r| r.exists()) {
        match find_manifests(root) {
            Ok(m) => manifests.extend(m),
            Err(e) => return fail(&e),
        }
    }
    if manifests.is_empty() {
        println!("{}: nothing to validate yet", a.assets.display());
        return ExitCode::SUCCESS;
    }

    let mut failed = 0;
    for path in &manifests {
        match validate::validate(path, &palette) {
            Ok(report) if report.ok() => println!("ok    {}", report.set),
            Ok(report) => {
                failed += 1;
                println!("FAIL  {}", report.set);
                for p in &report.problems {
                    println!("        {p}");
                }
            }
            Err(e) => {
                failed += 1;
                println!("FAIL  {}\n        {e}", path.display());
            }
        }
    }

    println!("\n{} sprite set(s), {failed} failing", manifests.len());
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn placeholders(args: &[String]) -> ExitCode {
    let a = match parse(args, DEFAULT_PLACEHOLDERS) {
        Ok(a) => a,
        Err(e) => return usage(&e),
    };
    let palette = match load_palette(&a.palette) {
        Ok(p) => p,
        Err(e) => return fail(&e),
    };
    // A set that has real art does not need a placeholder, and generating one
    // anyway invites someone to load the wrong file.
    let real = PathBuf::from(DEFAULT_ASSETS);
    let written = match placeholder::generate_except(&a.out, &palette, &real) {
        Ok(w) => w,
        Err(e) => return fail(&e),
    };

    // Generating art that does not pass the gate would be worse than
    // generating none, so the generator checks its own output.
    let mut failed = 0;
    for path in &written {
        match validate::validate(path, &palette) {
            Ok(r) if r.ok() => {}
            Ok(r) => {
                failed += 1;
                println!("FAIL  {}", r.set);
                for p in &r.problems {
                    println!("        {p}");
                }
            }
            Err(e) => {
                failed += 1;
                println!("FAIL  {e}");
            }
        }
    }

    println!(
        "wrote {} placeholder sprite set(s) to {}",
        written.len(),
        a.out.display()
    );
    let skipped = placeholder::catalogue_len() - written.len();
    if skipped > 0 {
        println!(
            "skipped {skipped} set(s) that already have real art in {}",
            real.display()
        );
    }
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        fail("generated placeholders do not pass validation")
    }
}

fn quantize_render(args: &[String]) -> ExitCode {
    let a = match parse(args, "") {
        Ok(a) => a,
        Err(e) => return usage(&e),
    };
    let (Some(input), Some(output)) = (a.input.as_ref(), a.output.as_ref()) else {
        return usage("quantize needs --in and --out");
    };
    if output.as_os_str().is_empty() {
        return usage("quantize needs --out");
    }
    let palette = match load_palette(&a.palette) {
        Ok(p) => p,
        Err(e) => return fail(&e),
    };
    let opts = quantize::Options {
        downsample: a.downsample,
        ..Default::default()
    };
    let indexed = match quantize::quantize(input, &palette, &opts) {
        Ok(i) => i,
        Err(e) => return fail(&e),
    };
    if let Err(e) = indexed.write_png(output, &palette) {
        return fail(&e);
    }
    println!(
        "{} -> {} ({}x{}, indexed)",
        input.display(),
        output.display(),
        indexed.width,
        indexed.height
    );
    ExitCode::SUCCESS
}

/// Turns a directory of renders into a validated sprite set. The other half of
/// `tools/render/render_sheet.py`, and the point at which rendered art becomes
/// indistinguishable from any other art as far as the game is concerned.
fn compose_set(args: &[String]) -> ExitCode {
    let a = match parse(args, "") {
        Ok(a) => a,
        Err(e) => return usage(&e),
    };
    let (Some(renders), Some(name), Some(class)) =
        (a.renders.as_ref(), a.set.as_ref(), a.class.as_ref())
    else {
        return usage("compose needs --renders, --set, --class and --out");
    };
    let class = match compose::parse_class(class) {
        Ok(c) => c,
        Err(e) => return usage(&e),
    };
    let out = match a.output.as_ref() {
        Some(o) if !o.as_os_str().is_empty() => o,
        _ => return usage("compose needs --out"),
    };
    let palette = match load_palette(&a.palette) {
        Ok(p) => p,
        Err(e) => return fail(&e),
    };
    let rig = match Rig::load(&a.rig) {
        Ok(r) => r,
        Err(e) => return fail(&e),
    };

    let done = match compose::compose(renders, name, class, out, &palette, &rig) {
        Ok(d) => d,
        Err(e) => return fail(&e),
    };
    println!(
        "composed {} frame(s) into {}",
        done.frames,
        done.sheet.display()
    );

    match validate::validate(&done.manifest, &palette) {
        Ok(r) if r.ok() => {
            println!("ok    {}", r.set);
            ExitCode::SUCCESS
        }
        Ok(r) => {
            println!("FAIL  {}", r.set);
            for p in &r.problems {
                println!("        {p}");
            }
            ExitCode::FAILURE
        }
        Err(e) => fail(&e),
    }
}

/// Rewrites every sprite sheet's PLTE against the current palette, leaving
/// its indices untouched.
///
/// A sheet's indices are the art; its PLTE is a copy of the palette at the
/// moment it was exported. When a palette *colour* changes (an index keeps
/// its meaning but its sRGB moves), every committed sheet still carries the
/// old colour and `validate` rightly refuses it. Rendered sheets cannot be
/// re-exported on demand — they need Blender — so this refreshes the copy.
/// It never changes an index: if the *layout* changed, that is a re-render.
fn repalette(args: &[String]) -> ExitCode {
    let a = match parse(args, DEFAULT_ASSETS) {
        Ok(a) => a,
        Err(e) => return usage(&e),
    };
    let palette = match load_palette(&a.palette) {
        Ok(p) => p,
        Err(e) => return fail(&e),
    };
    let manifests = match find_manifests(&a.assets) {
        Ok(m) => m,
        Err(e) => return fail(&e),
    };
    let mut rewritten = 0;
    for manifest_path in manifests {
        let set = match manifest::SpriteSet::load(&manifest_path) {
            Ok(s) => s,
            Err(e) => return fail(&e),
        };
        let sheet_path = manifest_path.with_file_name(&set.sheet);
        let loaded = match image::read_png(&sheet_path) {
            Ok(l) => l,
            Err(e) => return fail(&e),
        };
        let stale = loaded
            .palette
            .iter()
            .enumerate()
            .any(|(i, c)| i < 256 && *c != palette.entries[i]);
        if !stale {
            println!("ok       {}", sheet_path.display());
            continue;
        }
        if let Err(e) = loaded.image.write_png(&sheet_path, &palette) {
            return fail(&e);
        }
        println!("rewrote  {}", sheet_path.display());
        rewritten += 1;
    }
    println!("\n{rewritten} sheet(s) rewritten");
    ExitCode::SUCCESS
}

fn usage(err: &str) -> ExitCode {
    eprintln!("error: {err}\n");
    eprintln!("usage:");
    eprintln!("  atlas rig         [--rig FILE]");
    eprintln!("  atlas palette     [--palette FILE]");
    eprintln!("  atlas export      [--palette FILE] [--out DIR]");
    eprintln!("  atlas validate    [--assets DIR] [--palette FILE]");
    eprintln!("  atlas repalette   [--assets DIR] [--palette FILE]");
    eprintln!("  atlas placeholder [--out DIR] [--palette FILE]");
    eprintln!("  atlas quantize    --in FILE --out FILE [--downsample N] [--palette FILE]");
    eprintln!("  atlas compose     --renders DIR --set NAME --class CLASS --out DIR");
    ExitCode::FAILURE
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("rig") => rig_report(&args[1..]),
        Some("palette") => palette_report(&args[1..]),
        Some("export") => export(&args[1..]),
        Some("validate") => validate_all(&args[1..]),
        Some("repalette") => repalette(&args[1..]),
        Some("placeholder") => placeholders(&args[1..]),
        Some("quantize") => quantize_render(&args[1..]),
        Some("compose") => compose_set(&args[1..]),
        Some(other) => usage(&format!("unknown command {other}")),
        None => usage("no command given"),
    }
}
