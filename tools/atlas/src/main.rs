//! The art conformance gate.
//!
//! ```text
//! atlas palette     [--palette FILE]
//! atlas export      [--palette FILE] [--out DIR]
//! atlas validate    [--assets DIR] [--palette FILE]
//! atlas placeholder [--out DIR] [--palette FILE]
//! atlas quantize    --in FILE --out FILE [--downsample N] [--palette FILE]
//! ```
//!
//! `validate` is the one CI runs. `docs/05` §6 requires that non-conformant art
//! fails the build, which means this exits non-zero and says what to fix.

mod colour;
mod image;
mod manifest;
mod palette;
mod placeholder;
mod quantize;
mod validate;

use colour::{simulate, Deficiency, Srgb};
use image::Indexed;
use palette::{Palette, PaletteSpec};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const DEFAULT_PALETTE: &str = "assets/palette/ancient.ron";
const DEFAULT_ASSETS: &str = "assets/sprites";

struct Args {
    palette: PathBuf,
    assets: PathBuf,
    out: PathBuf,
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    downsample: u32,
}

fn parse(args: &[String], default_out: &str) -> Result<Args, String> {
    let mut out = Args {
        palette: PathBuf::from(DEFAULT_PALETTE),
        assets: PathBuf::from(DEFAULT_ASSETS),
        out: PathBuf::from(default_out),
        input: None,
        output: None,
        downsample: 2,
    };
    let mut i = 0;
    while i < args.len() {
        let key = &args[i];
        let val = args
            .get(i + 1)
            .ok_or_else(|| format!("{key} needs a value"))?;
        match key.as_str() {
            "--palette" => out.palette = PathBuf::from(val),
            "--assets" => out.assets = PathBuf::from(val),
            "--out" => out.out = PathBuf::from(val),
            "--in" => out.input = Some(PathBuf::from(val)),
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
    ExitCode::SUCCESS
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
    if !a.assets.exists() {
        println!("{}: nothing to validate yet", a.assets.display());
        return ExitCode::SUCCESS;
    }
    let manifests = match find_manifests(&a.assets) {
        Ok(m) => m,
        Err(e) => return fail(&e),
    };

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
    let a = match parse(args, DEFAULT_ASSETS) {
        Ok(a) => a,
        Err(e) => return usage(&e),
    };
    let palette = match load_palette(&a.palette) {
        Ok(p) => p,
        Err(e) => return fail(&e),
    };
    let written = match placeholder::generate(&a.out, &palette) {
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

fn usage(err: &str) -> ExitCode {
    eprintln!("error: {err}\n");
    eprintln!("usage:");
    eprintln!("  atlas palette     [--palette FILE]");
    eprintln!("  atlas export      [--palette FILE] [--out DIR]");
    eprintln!("  atlas validate    [--assets DIR] [--palette FILE]");
    eprintln!("  atlas placeholder [--out DIR] [--palette FILE]");
    eprintln!("  atlas quantize    --in FILE --out FILE [--downsample N] [--palette FILE]");
    ExitCode::FAILURE
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("palette") => palette_report(&args[1..]),
        Some("export") => export(&args[1..]),
        Some("validate") => validate_all(&args[1..]),
        Some("placeholder") => placeholders(&args[1..]),
        Some("quantize") => quantize_render(&args[1..]),
        Some(other) => usage(&format!("unknown command {other}")),
        None => usage("no command given"),
    }
}
