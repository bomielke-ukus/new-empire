//! ```text
//! mapview [--seed N] [--size N] [--players N] [--ticks N]
//!         [--zoom 0.5|1|1.5|2] [--width W] [--height H]
//!         [--at X,Y | --start P] [--out frame.png] [--minimap mini.png] [--atlas atlas.png]
//! ```
//!
//! Generates a map, runs it for `--ticks`, and writes a frame rendered by the
//! same code path the game uses, minus the GPU.

use std::process::ExitCode;
use view::camera::ZOOM_LEVELS;
use view::{raster, Atlas, Camera, Scene};

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

    let atlas = Atlas::placeholder();
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
    let scene = Scene::build(&sim, &atlas, None, 0.0);
    let mut cam = Camera::new(map.width(), map.height(), (a.width as f32, a.height as f32));
    cam.set_zoom_index(a.zoom);
    if let Some((x, y)) = a.at {
        cam.look_at_tile(x, y);
    } else if let Some(p) = a.start {
        let (sx, sy) = *sim.starts().get(p).ok_or(format!("no start {p}"))?;
        cam.look_at_tile(sx as f32 + 0.5, sy as f32 + 0.5);
    }
    let mut img = raster::Image::new(a.width, a.height, [12, 10, 14, 255]);
    raster::draw_terrain(&mut img, &cam, &chunks);
    raster::draw_sprites(
        &mut img,
        &cam,
        &atlas,
        &view::palette::texture(),
        &scene.sprites,
    );
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

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
