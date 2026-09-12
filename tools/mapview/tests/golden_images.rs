//! Golden-image tests: render fixed scenes and compare against committed PNGs.
//!
//! These drive the `mapview` binary itself rather than calling the rendering
//! library, because the command line is what CI invokes and what a developer
//! types — so the thing under test is the whole pipeline, including argument
//! handling, not a parallel path that happens to share some code.
//!
//! `mapview` renders through the software rasteriser in `crates/view`, which
//! is a deliberate advantage here over a GPU or even a software Vulkan
//! driver: there is no driver to vary between runners, so a pixel difference
//! means the renderer changed rather than that the machine did.
//!
//! **Tolerance.** The rasteriser uses floating point (it is presentation, not
//! simulation), so exact equality is not promised across architectures. The
//! comparison allows a small per-channel difference on a small fraction of
//! pixels — tight enough to catch a real change, loose enough that a test
//! failing on a different CPU trains nobody to ignore it.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A pixel may differ by this much per channel without counting as different.
const CHANNEL_TOLERANCE: u8 = 2;
/// And at most this fraction of pixels may differ at all.
const MAX_DIFFERING_FRACTION: f64 = 0.002;

/// The scenes pinned here. Small enough to commit and diff by eye, large
/// enough that a layout regression is visible.
struct Scene {
    name: &'static str,
    args: &'static [&'static str],
}

const SCENES: &[Scene] = &[
    Scene {
        // Terrain, elevation shading and scenery, with no units acting.
        name: "inland-start",
        args: &[
            "--seed",
            "1",
            "--size",
            "96",
            "--players",
            "2",
            "--ticks",
            "0",
            "--start",
            "0",
            "--width",
            "960",
            "--height",
            "540",
        ],
    },
    Scene {
        // The scenario CI already renders: villagers gathering, a rally
        // point, production queued, the HUD drawn over it.
        name: "gather-hud",
        args: &[
            "--seed",
            "1",
            "--scenario",
            "gather",
            "--ticks",
            "600",
            "--select",
            "3",
            "--hud",
            "1",
            "--width",
            "960",
            "--height",
            "540",
        ],
    },
    Scene {
        // A placement ghost, which is its own sprite path.
        name: "build-ghost",
        args: &[
            "--seed",
            "2",
            "--scenario",
            "build",
            "--ticks",
            "200",
            "--ghost",
            "house",
            "--hud",
            "1",
            "--width",
            "960",
            "--height",
            "540",
        ],
    },
    Scene {
        // Zoomed out: a different terrain chunk path and sprite scale.
        name: "zoomed-out",
        args: &[
            "--seed",
            "1",
            "--size",
            "96",
            "--players",
            "4",
            "--ticks",
            "300",
            "--zoom",
            "0.5",
            "--start",
            "1",
            "--width",
            "960",
            "--height",
            "540",
        ],
    },
    Scene {
        // The Tool Age settlement: mudbrick buildings, the Town Center
        // selected with the Bronze Age button and its gate on the grid, a
        // technology queued, the farm in the field.
        name: "ages-tool-hud",
        args: &[
            "--seed",
            "1",
            "--scenario",
            "ages",
            "--stockpile",
            "5000",
            "--ticks",
            "100",
            "--select-tc",
            "1",
            "--hud",
            "1",
            "--width",
            "960",
            "--height",
            "540",
        ],
    },
    Scene {
        // The Bronze Age has just landed: limestone buildings, the light
        // sweep part-way across the settlement, the banner up.
        name: "ages-bronze-sweep",
        args: &[
            "--seed",
            "1",
            "--scenario",
            "ages",
            "--stockpile",
            "5000",
            "--ticks",
            "1900",
            "--sweep",
            "700",
            "--hud",
            "1",
            "--width",
            "960",
            "--height",
            "540",
        ],
    },
    Scene {
        // The Tool Age garrison: every soldier of the slice in a line, the
        // Barracks selected with its roster on the grid (the Axeman greyed
        // for the Axe), two units queued.
        name: "army-hud",
        args: &[
            "--seed",
            "1",
            "--scenario",
            "army",
            "--stockpile",
            "5000",
            "--ticks",
            "40",
            "--select-kind",
            "barracks",
            "--hud",
            "1",
            "--width",
            "960",
            "--height",
            "540",
        ],
    },
    Scene {
        // The F1 controls overlay over a fresh match, with the resource bar
        // still carrying its first-minute hint.
        name: "controls-overlay",
        args: &[
            "--seed",
            "1",
            "--ticks",
            "0",
            "--hud",
            "1",
            "--controls",
            "1",
            "--width",
            "960",
            "--height",
            "540",
        ],
    },
    Scene {
        // The gather scene on a 2x display: the same window in device
        // pixels is twice as large, and the world and HUD must come out at
        // the same apparent size, not at half.
        name: "retina-hud",
        args: &[
            "--seed",
            "1",
            "--scenario",
            "gather",
            "--ticks",
            "600",
            "--select",
            "3",
            "--hud",
            "1",
            "--width",
            "1280",
            "--height",
            "720",
            "--dpi",
            "2",
        ],
    },
    Scene {
        // Once a characterisation scene for a defect: below roughly 960px the
        // resource bar used to run `GOLD` into `POP` and both into the
        // status text. `docs/03` §1 says the layout reflows, and since M3 it
        // does: the worker counts go first, then the text shrinks, then the
        // status. The name is kept so the history of the image reads as one
        // scene.
        name: "narrow-hud-overlap",
        args: &[
            "--seed",
            "1",
            "--scenario",
            "gather",
            "--ticks",
            "600",
            "--hud",
            "1",
            "--width",
            "640",
            "--height",
            "360",
        ],
    },
];

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden-images")
}

/// Where failure artefacts go, so CI can upload them.
fn artefact_dir() -> PathBuf {
    std::env::var_os("MAPVIEW_ARTEFACT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("new-empire-golden-failures"))
}

fn updating() -> bool {
    std::env::var("UPDATE_GOLDEN").is_ok_and(|v| v == "1")
}

fn render(scene: &Scene, out: &Path) {
    let status = Command::new(env!("CARGO_BIN_EXE_mapview"))
        .args(scene.args)
        .arg("--out")
        .arg(out)
        .output()
        .unwrap_or_else(|e| panic!("could not run mapview for `{}`: {e}", scene.name));
    assert!(
        status.status.success(),
        "mapview failed for `{}`:\n{}\n{}",
        scene.name,
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr),
    );
}

fn read_rgba(path: &Path) -> (u32, u32, Vec<u8>) {
    let img = image::open(path)
        .unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()))
        .to_rgba8();
    (img.width(), img.height(), img.into_raw())
}

struct Difference {
    differing: usize,
    total: usize,
    worst_channel: u8,
    first_at: Option<(u32, u32)>,
}

fn compare(width: u32, got: &[u8], want: &[u8]) -> Difference {
    let mut d = Difference {
        differing: 0,
        total: got.len() / 4,
        worst_channel: 0,
        first_at: None,
    };
    for (i, (g, w)) in got
        .as_chunks::<4>()
        .0
        .iter()
        .zip(want.as_chunks::<4>().0)
        .enumerate()
    {
        let delta = g
            .iter()
            .zip(w)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        d.worst_channel = d.worst_channel.max(delta);
        if delta > CHANNEL_TOLERANCE {
            d.differing += 1;
            if d.first_at.is_none() {
                d.first_at = Some((i as u32 % width, i as u32 / width));
            }
        }
    }
    d
}

/// Writes what was rendered and an amplified difference, so a CI failure
/// comes with something a person can look at.
fn save_artefacts(name: &str, rendered: &Path, got: &[u8], want: &[u8], w: u32, h: u32) -> PathBuf {
    let dir = artefact_dir();
    std::fs::create_dir_all(&dir).expect("could not create the artefact directory");
    let _ = std::fs::copy(rendered, dir.join(format!("{name}.actual.png")));

    let mut diff = Vec::with_capacity(got.len());
    for (g, wp) in got.as_chunks::<4>().0.iter().zip(want.as_chunks::<4>().0) {
        let delta = g
            .iter()
            .zip(wp)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        // Amplified, so a one-value difference is visible rather than a
        // shade of black nobody notices.
        let v = delta.saturating_mul(32);
        diff.extend_from_slice(&[v, 0, 0, 255]);
    }
    let buf: image::RgbaImage = image::ImageBuffer::from_raw(w, h, diff).expect("diff buffer");
    let _ = buf.save(dir.join(format!("{name}.diff.png")));
    dir
}

#[test]
fn scenes_match_their_golden_images() {
    let tmp = std::env::temp_dir().join("new-empire-mapview-golden");
    std::fs::create_dir_all(&tmp).expect("scratch directory");
    let mut failures = Vec::new();

    for scene in SCENES {
        let rendered = tmp.join(format!("{}.png", scene.name));
        render(scene, &rendered);

        let golden = golden_dir().join(format!("{}.png", scene.name));
        if updating() || !golden.exists() {
            std::fs::create_dir_all(golden_dir()).expect("golden directory");
            std::fs::copy(&rendered, &golden).expect("could not write the golden");
            eprintln!("wrote golden {}", golden.display());
            continue;
        }

        let (gw, gh, got) = read_rgba(&rendered);
        let (ww, wh, want) = read_rgba(&golden);
        if (gw, gh) != (ww, wh) {
            failures.push(format!(
                "{}: golden is {ww}x{wh}, the render is {gw}x{gh}",
                scene.name
            ));
            continue;
        }

        let d = compare(gw, &got, &want);
        let fraction = d.differing as f64 / d.total as f64;
        if fraction > MAX_DIFFERING_FRACTION {
            let dir = save_artefacts(scene.name, &rendered, &got, &want, gw, gh);
            failures.push(format!(
                "{}: {} of {} pixels differ ({:.3}%, budget {:.3}%), worst channel \
                 delta {}, first at {:?}. Images in {}",
                scene.name,
                d.differing,
                d.total,
                fraction * 100.0,
                MAX_DIFFERING_FRACTION * 100.0,
                d.worst_channel,
                d.first_at,
                dir.display(),
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "the renderer no longer produces the committed images:\n  {}\n\n\
         If the change was intended, re-record with\n    \
         UPDATE_GOLDEN=1 cargo test -p mapview --test golden_images\n\
         and put the new images in the same commit so they are reviewed.",
        failures.join("\n  ")
    );
}

/// The renderer must be deterministic run to run, or a golden test is a coin
/// toss. Cheaper to assert than to discover from a flaky CI job.
#[test]
fn rendering_is_reproducible() {
    let tmp = std::env::temp_dir().join("new-empire-mapview-repro");
    std::fs::create_dir_all(&tmp).expect("scratch directory");
    let scene = &SCENES[1];
    let a = tmp.join("a.png");
    let b = tmp.join("b.png");
    render(scene, &a);
    render(scene, &b);
    assert_eq!(
        std::fs::read(&a).unwrap(),
        std::fs::read(&b).unwrap(),
        "two renders of `{}` differ byte for byte",
        scene.name
    );
}

/// The comparison must be able to fail. A golden test that passes against any
/// input is the most expensive kind of no test at all.
#[test]
fn the_comparison_rejects_a_different_image() {
    let a = vec![10u8, 20, 30, 255, 10, 20, 30, 255];
    let b = vec![10u8, 20, 30, 255, 200, 20, 30, 255];
    let d = compare(2, &a, &b);
    assert_eq!(d.differing, 1);
    assert_eq!(d.first_at, Some((1, 0)));
    assert_eq!(d.worst_channel, 190);

    // And must tolerate rasteriser noise inside the budget.
    let c = vec![10u8, 20, 30, 255, 11, 21, 31, 255];
    let d = compare(2, &a, &c);
    assert_eq!(d.differing, 0, "a one-value difference is not a regression");
}

/// A golden file with no scene behind it is dead weight nobody notices.
#[test]
fn every_golden_file_belongs_to_a_scene() {
    let dir = golden_dir();
    if !dir.exists() {
        return;
    }
    let names: Vec<&str> = SCENES.iter().map(|s| s.name).collect();
    for entry in std::fs::read_dir(&dir).expect("unreadable golden directory") {
        let path = entry.expect("unreadable entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("png") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        assert!(
            names.contains(&stem),
            "{} has no matching scene; delete it or restore the scene",
            path.display()
        );
    }
}
