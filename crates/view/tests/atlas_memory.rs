//! What the sprite textures cost with the real art, for the memory budget
//! (`docs/look/colour-sets.md`, "In the atlas"). Not a check: a report,
//! run by hand (it reads every set under `assets`):
//!
//! ```sh
//! cargo test --release -p view --test atlas_memory -- --ignored --nocapture
//! ```

use std::time::Instant;

use sim::Civ;
use view::sheets;
use view::sprites::{Atlas, Loadout};

fn mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn report(label: &str, new_art: bool, civs: Option<&[Civ]>) {
    let sprites = sheets::default_dir();
    let look = new_art.then(sheets::look_dir).flatten();
    let archs = civs.map_or(Loadout::FIRST, |c| {
        Loadout::of_civs(&c.iter().map(|&c| Some(c)).collect::<Vec<_>>())
    });
    // The median of three loads' times.
    let mut runs = Vec::new();
    let mut atlas = None;
    for _ in 0..3 {
        let started = Instant::now();
        let (sheets, errors) =
            sheets::load_art_for(sprites.as_deref(), look.as_deref(), new_art, archs);
        assert!(errors.is_empty(), "{errors:?}");
        let decoded = started.elapsed();
        let built = Atlas::with_sheets(&sheets);
        runs.push((decoded, started.elapsed() - decoded));
        atlas = Some(built);
    }
    let atlas = atlas.unwrap();
    runs.sort_by_key(|r| r.0 + r.1);
    let (decoded, packed) = runs[1];
    let indexed = atlas.indices.len();
    let colour = atlas.colour.len() + atlas.mask.len();
    let used: usize = atlas
        .frames()
        .iter()
        .filter(|f| f.page & view::sprites::COLOUR_PAGE != 0)
        .map(|f| f.w as usize * f.h as usize * 5)
        .sum();
    println!(
        "| {label:<26} | {}x{}x{} {:>6.1} | {}x{}x{} {:>6.1} (frames {:>5.1}) | {:>6.1} | {:>4} ms | {:>4} ms |",
        atlas.width,
        atlas.height,
        atlas.pages,
        mib(indexed),
        atlas.colour_width,
        atlas.colour_height,
        atlas.colour_pages,
        mib(colour),
        mib(used),
        mib(indexed + colour),
        decoded.as_millis(),
        packed.as_millis(),
    );
    if std::env::var_os("ATLAS_BREAKDOWN").is_some() {
        // Where the colour frames' area goes: units by animation, the rest.
        let mut by: std::collections::BTreeMap<String, usize> = Default::default();
        for f in atlas.frames() {
            if f.page & view::sprites::COLOUR_PAGE == 0 {
                continue;
            }
            let unit = sim::kinds::all().iter().any(|k| k.mobile && k.id == f.kind);
            let key = if unit {
                format!("unit {:?}", f.anim)
            } else {
                "other".into()
            };
            *by.entry(key).or_default() += f.w as usize * f.h as usize * 5;
        }
        let masked: usize = atlas
            .frames()
            .iter()
            .filter(|f| f.page & view::sprites::COLOUR_PAGE != 0)
            .filter(|f| {
                (0..f.h as u32).any(|y| {
                    (0..f.w as u32)
                        .any(|x| atlas.mask_at(f.page, f.x as u32 + x, f.y as u32 + y) != 0)
                })
            })
            .map(|f| f.w as usize * f.h as usize * 5)
            .sum();
        println!("    frames with any mask    {:>6.1} MiB", mib(masked));
        for (k, v) in by {
            println!("    {k:<24} {:>6.1} MiB", mib(v));
        }
    }
}

#[test]
#[ignore = "a report on the real art; run by hand"]
fn atlas_memory_report() {
    println!("| load | indexed pages MiB | colour pages MiB | total MiB | decode | pack |");
    report("new art off", false, None);
    report("title / diorama", true, None);
    report("Greeks only", true, Some(&[Civ::Greeks]));
    report(
        "Greeks v Egyptians",
        true,
        Some(&[Civ::Greeks, Civ::Egyptians]),
    );
    report(
        "Egyptians v Shang",
        true,
        Some(&[Civ::Egyptians, Civ::Shang]),
    );
    report(
        "four architectures",
        true,
        Some(&[Civ::Greeks, Civ::Egyptians, Civ::Assyrians, Civ::Shang]),
    );
}
