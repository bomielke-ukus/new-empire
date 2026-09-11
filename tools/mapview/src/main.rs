//! ```text
//! mapview [--seed N] [--size N] [--players N] [--ticks N]
//!         [--zoom 0.5|1|1.5|2] [--width W] [--height H]
//!         [--at X,Y | --start P] [--out frame.png] [--minimap mini.png] [--atlas atlas.png]
//! ```
//!
//! Generates a map, runs it for `--ticks`, and writes a frame rendered by the
//! same code path the game uses, minus the GPU.

use sim::kinds;
use std::process::ExitCode;
use view::camera::ZOOM_LEVELS;
use view::{raster, Atlas, Camera, Ghost, Hud, HudInput, Scene};

struct Args {
    seed: u64,
    size: u16,
    players: u8,
    ticks: u64,
    zoom: usize,
    width: u32,
    height: u32,
    at: Option<(f32, f32)>,
    start: Option<usize>,
    out: String,
    minimap: Option<String>,
    atlas: Option<String>,
    scenario: Option<String>,
    select: usize,
    hud: bool,
    ghost: Option<String>,
    assets: Option<std::path::PathBuf>,
}

fn parse() -> Result<Args, String> {
    let mut a = Args {
        seed: 1,
        size: 128,
        players: 2,
        ticks: 0,
        zoom: 1,
        width: 1280,
        height: 720,
        at: None,
        start: Some(0),
        out: "frame.png".into(),
        minimap: None,
        atlas: None,
        scenario: None,
        select: 0,
        hud: false,
        ghost: None,
        assets: None,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let key = &args[i];
        let val = args
            .get(i + 1)
            .ok_or_else(|| format!("{key} needs a value"))?;
        let num = |v: &str| v.parse::<f32>().map_err(|e| format!("{key}: {e}"));
        match key.as_str() {
            "--seed" => a.seed = val.parse().map_err(|e| format!("{key}: {e}"))?,
            "--size" => a.size = val.parse().map_err(|e| format!("{key}: {e}"))?,
            "--players" => a.players = val.parse().map_err(|e| format!("{key}: {e}"))?,
            "--ticks" => a.ticks = val.parse().map_err(|e| format!("{key}: {e}"))?,
            "--zoom" => {
                let z = num(val)?;
                a.zoom = ZOOM_LEVELS
                    .iter()
                    .position(|&l| (l - z).abs() < 1e-3)
                    .ok_or(format!("--zoom must be one of {ZOOM_LEVELS:?}"))?;
            }
            "--width" => a.width = val.parse().map_err(|e| format!("{key}: {e}"))?,
            "--height" => a.height = val.parse().map_err(|e| format!("{key}: {e}"))?,
            "--at" => {
                let (x, y) = val.split_once(',').ok_or("--at wants X,Y")?;
                a.at = Some((num(x)?, num(y)?));
                a.start = None;
            }
            "--start" => a.start = Some(val.parse().map_err(|e| format!("{key}: {e}"))?),
            "--out" => a.out = val.clone(),
            "--minimap" => a.minimap = Some(val.clone()),
            "--atlas" => a.atlas = Some(val.clone()),
            "--scenario" => a.scenario = Some(val.clone()),
            "--select" => a.select = val.parse().map_err(|e| format!("{key}: {e}"))?,
            "--hud" => a.hud = val == "1" || val == "true",
            "--ghost" => a.ghost = Some(val.clone()),
            "--assets" => a.assets = Some(std::path::PathBuf::from(val)),
            _ => return Err(format!("unknown flag {key}")),
        }
        i += 2;
    }
    Ok(a)
}

fn save(path: &str, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    image::save_buffer(path, rgba, width, height, image::ColorType::Rgba8)
        .map_err(|e| format!("{path}: {e}"))
}

fn run() -> Result<(), String> {
    let a = parse()?;
    let config = sim::SimConfig {
        map: sim::MapSpec {
            kind: sim::MapKind::Inland,
            size: a.size,
            players: a.players,
        },
        ..sim::SimConfig::default()
    };
    let mut sim = sim::Simulation::new(a.seed, config);
    if let Some(name) = &a.scenario {
        scenario(&mut sim, name)?;
    }
    for _ in 0..a.ticks {
        sim.step();
    }
    let map = sim.map();
    let hist = map.terrain_histogram();
    println!(
        "seed {} size {} players {}: {} entities, terrain {:?}, starts {:?}",
        a.seed,
        map.width(),
        a.players,
        sim.world().len(),
        hist,
        sim.starts()
    );

    // Rendered sprite sets replace placeholders wherever they exist.
    let sheet_dir = a.assets.clone().or_else(view::sheets::default_dir);
    let (sheets, errors) = sheet_dir
        .map(|d| view::sheets::load_all(&d))
        .unwrap_or_default();
    for e in &errors {
        eprintln!("warning: {e}");
    }
    let atlas = Atlas::with_sheets(&sheets);
    if !atlas.loaded_sets.is_empty() {
        println!("rendered sprite sets: {}", atlas.loaded_sets.join(", "));
    }
    if let Some(path) = &a.atlas {
        let pal = view::palette::texture();
        let rgba: Vec<u8> = atlas
            .indices
            .iter()
            .flat_map(|&i| pal[256 + i as usize]) // player 0's row so the ramp shows
            .collect();
        save(path, atlas.width, atlas.height, &rgba)?;
        println!(
            "wrote atlas {path} ({}x{}, {} frames)",
            atlas.width,
            atlas.height,
            atlas.frames().len()
        );
    }

    let chunks = view::terrain::build_all(map);
    let selected: Vec<u32> = sim
        .world()
        .slots()
        .filter(|s| {
            sim.world().owner[s.index()] == 0 && sim.world().kind[s.index()] == kinds::VILLAGER
        })
        .take(a.select)
        .map(|s| s.index() as u32)
        .collect();
    let (gx, gy) = sim.starts()[0];
    let ghost = a.ghost.as_deref().map(|g| {
        let kind = if g == "store" {
            kinds::STOREHOUSE
        } else {
            kinds::HOUSE
        };
        Ghost {
            kind,
            x: gx + 4,
            y: gy + 1,
            ok: sim.can_place(0, kind, gx + 4, gy + 1).is_ok(),
            row: 1,
        }
    });
    let mut scene = Scene::build_with(&sim, &atlas, None, 0.0, &selected, ghost);
    let mut cam = Camera::new(map.width(), map.height(), (a.width as f32, a.height as f32));
    cam.set_zoom_index(a.zoom);
    if let Some((x, y)) = a.at {
        cam.look_at_tile(x, y);
    } else if let Some(p) = a.start {
        let (sx, sy) = *sim.starts().get(p).ok_or(format!("no start {p}"))?;
        cam.look_at_tile(sx as f32 + 0.5, sy as f32 + 0.5);
    }
    if a.hud {
        let hud = Hud::build(
            &atlas,
            &HudInput {
                sim: &sim,
                player: 0,
                camera: &cam,
                selected: &selected,
                build_mode: ghost.map(|g| g.kind),
                fps: 60.0,
                paused: false,
                speed: 1.0,
                status: &format!("TICK {}", sim.tick()),
            },
        );
        scene.ui = hud.sprites;
    }
    let mut img = raster::Image::new(a.width, a.height, [12, 10, 14, 255]);
    raster::draw_terrain(&mut img, &cam, &chunks);
    let palette = view::palette::texture();
    raster::draw_sprites(&mut img, &cam, &atlas, &palette, &scene.sprites);
    raster::draw_sprites(&mut img, &cam, &atlas, &palette, &scene.ui);
    save(&a.out, img.width, img.height, &img.to_bytes())?;
    println!(
        "wrote {} ({}x{} at {}x, {} sprites)",
        a.out,
        a.width,
        a.height,
        cam.zoom(),
        scene.sprites.len()
    );

    if let Some(path) = &a.minimap {
        let m = view::minimap::Minimap::render(&sim);
        let scale = 4;
        let mut big = Vec::with_capacity((m.width * m.height * scale * scale * 4) as usize);
        for y in 0..m.height * scale {
            for x in 0..m.width * scale {
                big.extend_from_slice(&m.pixels[((y / scale) * m.width + x / scale) as usize]);
            }
        }
        save(path, m.width * scale, m.height * scale, &big)?;
        println!("wrote minimap {path}");
    }
    Ok(())
}

/// Canned command sets so frames show the economy in motion.
fn scenario(sim: &mut sim::Simulation, name: &str) -> Result<(), String> {
    use sim::{Command, CommandKind};
    let (sx, sy) = sim.starts()[0];
    let tc_pos = sim::nav::centre((sx, sy));
    let world = sim.world();
    let villagers: Vec<_> = world
        .slots()
        .filter(|s| world.owner[s.index()] == 0 && world.kind[s.index()] == kinds::VILLAGER)
        .map(|s| world.id_at(s))
        .collect();
    let nearest = |kind| {
        world
            .slots()
            .filter(|s| world.kind[s.index()] == kind)
            .min_by_key(|s| (tc_pos.distance_sq_raw(world.pos[s.index()]), s.index()))
            .map(|s| world.id_at(s))
    };
    let tc = world
        .slots()
        .find(|s| world.owner[s.index()] == 0 && world.kind[s.index()] == kinds::TOWN_CENTER)
        .map(|s| world.id_at(s))
        .ok_or("no town center")?;
    let cmd = |kind| Command { player: 0, kind };
    match name {
        "gather" => {
            let bush = nearest(kinds::BERRY_BUSH).ok_or("no bush")?;
            let tree = nearest(kinds::TREE).ok_or("no tree")?;
            sim.issue(cmd(CommandKind::Gather {
                ids: villagers[..2].to_vec(),
                node: bush,
            }));
            sim.issue(cmd(CommandKind::Gather {
                ids: villagers[2..].to_vec(),
                node: tree,
            }));
            sim.issue(cmd(CommandKind::SetRally {
                building: tc,
                rally: sim::Rally::Entity(tree),
            }));
            for _ in 0..3 {
                sim.issue(cmd(CommandKind::Train {
                    building: tc,
                    kind: kinds::VILLAGER,
                }));
            }
        }
        "build" => {
            sim.issue(cmd(CommandKind::Build {
                kind: kinds::HOUSE,
                x: sx + 4,
                y: sy - 2,
                ids: villagers[..2].to_vec(),
            }));
            sim.issue(cmd(CommandKind::Build {
                kind: kinds::STOREHOUSE,
                x: sx - 4,
                y: sy + 3,
                ids: villagers[2..].to_vec(),
            }));
        }
        other => return Err(format!("unknown scenario {other}")),
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
