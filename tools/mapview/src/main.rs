//! ```text
//! mapview [--seed N] [--size N] [--players N] [--ticks N] [--stockpile N]
//!         [--zoom 0.5|1|1.5|2] [--width W] [--height H]
//!         [--at X,Y | --start P] [--out frame.png] [--minimap mini.png] [--atlas atlas.png]
//!         [--scenario gather|build|ages|army|battle|siege|scout] [--select N] [--select-tc 1] [--select-kind NAME] [--hud 1]
//!         [--ghost house|store|<kind>] [--sweep MS] [--hover X,Y] [--assets DIR]
//!         [--replay FILE] (render at --ticks, or at the end if omitted)
//!         [--dpi N] [--ui-scale N] [--controls 1] [--fog 0]
//!         [--screen title|setup|load|settings] [--difficulty easy,standard,hard,hardest]
//! ```
//!
//! Generates a map, runs it for `--ticks`, and writes a frame rendered by the
//! same code path the game uses, minus the GPU. The frame is player 0's:
//! their fog applies (`GD-FOG-01`) unless `--fog 0` shows the whole map.
//! `--screen` renders one of the shell's screens instead, as the game
//! shows it: the setup screen for `--seed`, `--size` and one opponent per
//! `--difficulty` entry.

use sim::kinds;
use std::process::ExitCode;
use view::camera::ZOOM_LEVELS;
use view::minimap::{Minimap, MinimapRect};
use view::shell::{self, Difficulty, MapSize, ShellInput};
use view::{
    raster, Atlas, Camera, FogLights, Ghost, Hud, HudInput, Scene, SceneOptions, Settings, Setup,
    Sweep,
};

struct Args {
    seed: u64,
    size: u16,
    players: u8,
    ticks: u64,
    replay: Option<String>,
    ticks_set: bool,
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
    stockpile: Option<i32>,
    select_tc: bool,
    select_kind: Option<String>,
    sweep: Option<u32>,
    hover: Option<(f32, f32)>,
    /// Hover the middle of the button with this label, for a tooltip.
    hover_button: Option<String>,
    /// Show the first-time hint the moment would bring up.
    hint: bool,
    /// Show the performance readout with fixed sample numbers.
    perf: bool,
    dpi: f32,
    ui_scale: f32,
    controls: bool,
    fog: bool,
    screen: Option<String>,
    difficulty: Option<String>,
}

fn parse() -> Result<Args, String> {
    let mut a = Args {
        seed: 1,
        size: 128,
        players: 2,
        ticks: 0,
        replay: None,
        ticks_set: false,
        zoom: view::camera::DEFAULT_ZOOM_INDEX,
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
        stockpile: None,
        select_tc: false,
        select_kind: None,
        sweep: None,
        hover: None,
        hover_button: None,
        hint: false,
        perf: false,
        dpi: 1.0,
        ui_scale: 1.0,
        controls: false,
        fog: true,
        screen: None,
        difficulty: None,
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
            "--ticks" => {
                a.ticks = val.parse().map_err(|e| format!("{key}: {e}"))?;
                a.ticks_set = true;
            }
            "--replay" => a.replay = Some(val.clone()),
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
            "--stockpile" => a.stockpile = Some(val.parse().map_err(|e| format!("{key}: {e}"))?),
            "--select-tc" => a.select_tc = val == "1" || val == "true",
            "--select-kind" => a.select_kind = Some(val.clone()),
            "--sweep" => a.sweep = Some(val.parse().map_err(|e| format!("{key}: {e}"))?),
            "--hover-button" => a.hover_button = Some(val.clone()),
            "--hint" => a.hint = val == "1" || val == "true",
            "--perf" => a.perf = val == "1" || val == "true",
            "--hover" => {
                let (x, y) = val.split_once(',').ok_or("--hover wants X,Y")?;
                a.hover = Some((num(x)?, num(y)?));
            }
            // A Retina window: `--width`/`--height` stay device pixels, and
            // the world and HUD draw at this many device pixels per pixel.
            "--dpi" => a.dpi = num(val)?,
            "--ui-scale" => a.ui_scale = num(val)?,
            "--controls" => a.controls = val == "1" || val == "true",
            "--fog" => a.fog = val == "1" || val == "true",
            "--screen" => a.screen = Some(val.clone()),
            "--difficulty" => a.difficulty = Some(val.clone()),
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

/// Renders a shell screen (`--screen`) as the game shows it, with no
/// match behind it: the title, or the setup screen with its preview.
fn render_screen(a: &Args, name: &str) -> Result<(), String> {
    let atlas = Atlas::placeholder();
    let input = ShellInput {
        viewport: (a.width as f32, a.height as f32),
        ui_scale: a.dpi * a.ui_scale,
        hover: a.hover,
    };
    let (screen, preview) = match name {
        "title" => (shell::title(&atlas, &input), None),
        "load" => {
            // Two saves as the app would list them, for the golden image.
            let rows = [
                shell::LoadRow {
                    title: "SEED 42 AT 18:30".into(),
                    detail: "2026-09-19 19:05 UTC - 3 PLAYERS".into(),
                },
                shell::LoadRow {
                    title: "SEED 1 AT 2:15".into(),
                    detail: "2026-09-18 22:40 UTC - 2 PLAYERS".into(),
                },
            ];
            (shell::load_screen(&atlas, &input, &rows, None, false), None)
        }
        "setup" => {
            let mut setup = Setup::new(a.seed);
            setup.size = MapSize::from_tiles(a.size)
                .ok_or_else(|| format!("--size {}: not a size the setup screen offers", a.size))?;
            if let Some(list) = &a.difficulty {
                setup.opponents = list
                    .split(',')
                    .map(|n| {
                        Difficulty::from_name(n)
                            .ok_or_else(|| format!("--difficulty: no level called {n}"))
                    })
                    .collect::<Result<_, _>>()?;
            }
            let error = setup.validate().err().map(|e| e.to_string());
            let sim = sim::Simulation::new(setup.seed, setup.config());
            (
                shell::setup(&atlas, &input, &setup, error.as_deref()),
                Some(Minimap::render(&sim)),
            )
        }
        "settings" => (
            shell::settings_screen(
                &atlas,
                &input,
                &Settings::default(),
                shell::SettingsPage::Keys,
                None,
                None,
            ),
            None,
        ),
        "settings-letters" => (
            shell::settings_screen(
                &atlas,
                &input,
                &Settings::default(),
                shell::SettingsPage::Letters,
                None,
                None,
            ),
            None,
        ),
        other => return Err(format!("--screen {other}: title, setup, load or settings")),
    };
    let mut img = raster::Image::new(a.width, a.height, [12, 10, 14, 255]);
    let cam = Camera::new(1, 1, (a.width as f32, a.height as f32));
    let palette = view::palette::texture();
    raster::draw_sprites(&mut img, &cam, &atlas, &palette, &screen.sprites);
    if let (Some(m), Some(rect)) = (preview, screen.preview) {
        raster::draw_minimap(&mut img, &m, rect);
    }
    save(&a.out, img.width, img.height, &img.to_bytes())?;
    println!(
        "wrote {} ({}x{}, the {name} screen, {} buttons)",
        a.out,
        a.width,
        a.height,
        screen.buttons.len()
    );
    Ok(())
}

fn run() -> Result<(), String> {
    let a = parse()?;
    if let Some(name) = &a.screen {
        return render_screen(&a, name);
    }
    let config = sim::SimConfig {
        map: sim::MapSpec {
            kind: sim::MapKind::Inland,
            size: a.size,
            players: a.players,
        },
        starting_stockpile: a.stockpile.map_or(sim::DEFAULT_STOCKPILE, |n| [n; 4]),
        ..sim::SimConfig::default()
    };
    // This is the front door a setup screen would use, so it runs the
    // setup screen's check.
    config.validate().map_err(|e| format!("match setup: {e}"))?;
    let mut feedback = view::feedback::CombatFeedback::default();
    let (sim, seed) = if let Some(path) = &a.replay {
        if a.scenario.is_some() {
            return Err("--replay and --scenario are mutually exclusive".into());
        }
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        let mut replay: sim::Replay = ron::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
        replay.validate().map_err(|e| format!("{path}: {e}"))?;
        replay
            .config
            .validate()
            .map_err(|e| format!("{path}: {e}"))?;
        if a.ticks_set {
            if a.ticks > replay.ticks {
                return Err("--ticks exceeds the replay's end".into());
            }
            replay.ticks = a.ticks;
            replay.commands.retain(|(tick, _)| *tick <= a.ticks);
        }
        let seed = replay.seed;
        (feedback.replay(&replay).map_err(|e| e.to_string())?, seed)
    } else {
        let mut sim = sim::Simulation::new(a.seed, config);
        if let Some(name) = &a.scenario {
            scenario(&mut sim, name)?;
        }
        for _ in 0..a.ticks {
            sim.step();
            feedback.observe(&sim);
            // A note for whoever is picking a golden's tick.
            if sim
                .events()
                .iter()
                .any(|e| matches!(e, sim::Event::Felled { .. }))
            {
                eprintln!("felled at tick {}", sim.tick());
            }
        }
        (sim, a.seed)
    };
    let map = sim.map();
    let hist = map.terrain_histogram();
    println!(
        "seed {} size {} players {}: {} entities, terrain {:?}, starts {:?}",
        seed,
        map.width(),
        sim.players().len(),
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
    // What to select: the first own building of `--select-kind`
    // (`barracks`, `light_cavalry`, ...), the Town Center, or the first
    // `--select` villagers.
    let (want, take) = match (&a.select_kind, a.select_tc) {
        (Some(name), _) => {
            let kind = kinds::all()
                .iter()
                .find(|k| k.name.to_lowercase().replace(' ', "_") == *name)
                .map(|k| k.id)
                .ok_or_else(|| format!("--select-kind: no kind called {name}"))?;
            (kind, 1)
        }
        (None, true) => (kinds::TOWN_CENTER, 1),
        (None, false) => (kinds::VILLAGER, a.select),
    };
    let selected: Vec<u32> = sim
        .world()
        .slots()
        .filter(|s| sim.world().owner[s.index()] == 0 && sim.world().kind[s.index()] == want)
        .take(take)
        .map(|s| s.index() as u32)
        .collect();
    let (gx, gy) = sim
        .starts()
        .first()
        .copied()
        .unwrap_or((map.width() / 2, map.height() / 2));
    let age = sim.player(0).map_or(0, |p| p.age.index() as u8);
    let ghost = match a.ghost.as_deref() {
        None => None,
        Some(g) => {
            let kind = match g {
                "store" => kinds::STOREHOUSE,
                "house" => kinds::HOUSE,
                name => kinds::all()
                    .iter()
                    .find(|k| k.name.to_lowercase().replace(' ', "_") == name)
                    .map(|k| k.id)
                    .ok_or(format!("no such kind {name}"))?,
            };
            Some(Ghost {
                kind,
                x: gx + 4,
                y: gy + 1,
                ok: sim.can_place(0, kind, gx + 4, gy + 1).is_ok(),
                row: 1,
                player: 0,
                age,
                run: None,
            })
        }
    };
    let sweep = a.sweep.map(|elapsed_ms| Sweep {
        player: 0,
        elapsed_ms,
    });
    let banner = a
        .sweep
        .and_then(|_| sim.player(0))
        .map(|p| view::hud::Banner::AgeUp(p.age));
    let viewer = a.fog.then_some(0);
    let mut scene = Scene::build_full(
        &sim,
        &atlas,
        None,
        0.0,
        &SceneOptions {
            selected: &selected,
            ghost,
            sweep,
            viewer,
        },
    );
    feedback.decorate(&mut scene, &sim, &atlas, viewer);
    let mut cam = Camera::new(map.width(), map.height(), (a.width as f32, a.height as f32));
    cam.set_zoom_index(a.zoom);
    cam.dpi = a.dpi;
    if let Some((x, y)) = a.at {
        cam.look_at_tile(x, y);
    } else if let Some(p) = a.start {
        let (sx, sy) = if sim.starts().is_empty() && p == 0 {
            (map.width() / 2, map.height() / 2)
        } else {
            *sim.starts().get(p).ok_or(format!("no start {p}"))?
        };
        cam.look_at_tile(sx as f32 + 0.5, sy as f32 + 0.5);
    }
    if a.hud {
        let settings = Settings::default();
        // The hint the moment would bring up, given the idle wait.
        let hint = a.hint.then(|| {
            let villager_selected = selected.iter().any(|s| {
                let w = sim.world();
                w.owner[*s as usize] == 0 && w.kind[*s as usize] == kinds::VILLAGER
            });
            let c = view::hints::conditions(&sim, 0, villager_selected, false);
            let mut hints = view::Hints::new(true, &std::collections::BTreeMap::new());
            let last = view::hints::IDLE_TICKS + 1;
            for t in 0..=last {
                hints.update(&c, t);
            }
            hints
                .showing(last)
                .map(|h| h.text(&view::hints::Keys::default()))
        });
        let hint = hint.flatten();
        let perf = a.perf.then(view::Readout::sample);
        let status = format!("TICK {}", sim.tick());
        let input = |hover: Option<(f32, f32)>| HudInput {
            sim: &sim,
            player: 0,
            camera: &cam,
            selected: &selected,
            build_mode: ghost.map(|g| g.kind),
            fps: 60.0,
            paused: false,
            speed: 1.0,
            status: &status,
            hover,
            banner,
            ui_scale: a.dpi * a.ui_scale,
            help: a.controls,
            targeting: false,
            defences: false,
            settings: &settings,
            notices: &[],
            hint: hint.as_deref(),
            flash: [false; 4],
            perf: perf.as_ref(),
        };
        let mut hover = a.hover;
        if let Some(label) = &a.hover_button {
            // Build once to find the button, then hover its middle.
            let first = Hud::build(&atlas, &input(None));
            let b = first
                .buttons
                .iter()
                .find(|b| &b.label == label)
                .ok_or_else(|| format!("no button labelled {label}"))?;
            let s = a.dpi * a.ui_scale;
            hover = Some(((b.x + b.w * 0.5) * s, (b.y + b.h * 0.5) * s));
        }
        let hud = Hud::build(&atlas, &input(hover));
        scene.ui = hud.sprites;
        // The marks at the screen's edge for an attack out of view.
        scene
            .ui
            .extend(feedback.edge_indicators(&sim, &atlas, &cam, viewer, a.dpi * a.ui_scale));
    }
    let mut img = raster::Image::new(a.width, a.height, [12, 10, 14, 255]);
    let lights = viewer.and_then(|p| sim.fog(p)).map(FogLights::from_fog);
    raster::draw_terrain(&mut img, &cam, &chunks, lights.as_ref());
    let palette = view::palette::texture();
    raster::draw_sprites(&mut img, &cam, &atlas, &palette, &scene.sprites);
    raster::draw_sprites(&mut img, &cam, &atlas, &palette, &scene.ui);
    if a.hud {
        // In the panel's reserved corner, as the game draws it.
        let s = a.dpi * a.ui_scale;
        let rect =
            MinimapRect::bottom_right((a.width as f32, a.height as f32), 256.0 * s, 16.0 * s);
        let marks = feedback.minimap_marks(&sim, viewer);
        raster::draw_minimap(
            &mut img,
            &Minimap::render_marked(&sim, viewer, &marks),
            rect,
        );
    }
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
        let m = Minimap::render_for(&sim, viewer);
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
    // A note for whoever is picking a golden's camera.
    eprintln!("start ({sx}, {sy})");
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
        "waypoints" => {
            // One villager with three jobs queued (`UX-CMD-04`): a walk, a
            // bush to gather, a house to build; the last two as waypoints.
            let bush = nearest(kinds::BERRY_BUSH).ok_or("no bush")?;
            let v = villagers[0];
            sim.issue(cmd(CommandKind::Move {
                ids: vec![v],
                target: sim::Vec2Fx::from_int(sx + 5, sy - 2),
            }));
            sim.issue(cmd(CommandKind::Queued(Box::new(CommandKind::Gather {
                ids: vec![v],
                node: bush,
            }))));
            sim.issue(cmd(CommandKind::Queued(Box::new(CommandKind::Build {
                kind: kinds::HOUSE,
                x: sx + 5,
                y: sy + 3,
                ids: vec![v],
            }))));
            eprintln!("waypoints: villager slot {}", v.index());
        }
        "fell" => {
            // Nodes being used up, beside the Town Center: eight villagers on
            // one tree, three on a berry bush, three on a gold vein. The tree
            // falls; the bush and the vein are drawn smaller as they go.
            let node = |sim: &mut sim::Simulation, kind, x, y| {
                sim.issue(Command {
                    player: 0,
                    kind: CommandKind::Spawn {
                        kind,
                        pos: sim::nav::centre((x, y)),
                    },
                });
            };
            node(sim, kinds::TREE, sx + 4, sy - 3);
            node(sim, kinds::BERRY_BUSH, sx - 4, sy + 3);
            node(sim, kinds::GOLD_MINE, sx + 4, sy + 4);
            for k in 0..11 {
                node(sim, kinds::VILLAGER, sx - 5 + k % 6, sy - 6 + k / 6);
            }
            for _ in 0..3 {
                sim.step();
            }
            let world = sim.world();
            // The node spawned on that tile, not the map's own elsewhere.
            let find = |kind, x, y| {
                let at = sim::nav::centre((x, y));
                world
                    .slots()
                    .filter(|s| world.kind[s.index()] == kind && world.pos[s.index()] == at)
                    .map(|s| world.id_at(s))
                    .last()
                    .expect("spawned")
            };
            let mut new_villagers: Vec<_> = world
                .slots()
                .filter(|s| world.owner[s.index()] == 0 && world.kind[s.index()] == kinds::VILLAGER)
                .map(|s| world.id_at(s))
                .filter(|id| !villagers.contains(id))
                .collect();
            let (tree, bush, vein) = (
                find(kinds::TREE, sx + 4, sy - 3),
                find(kinds::BERRY_BUSH, sx - 4, sy + 3),
                find(kinds::GOLD_MINE, sx + 4, sy + 4),
            );
            let gang: Vec<_> = new_villagers.drain(..8).collect();
            let foragers = std::mem::take(&mut new_villagers);
            sim.issue(cmd(CommandKind::Gather {
                ids: gang,
                node: tree,
            }));
            sim.issue(cmd(CommandKind::Gather {
                ids: foragers,
                node: bush,
            }));
            sim.issue(cmd(CommandKind::Gather {
                ids: villagers.clone(),
                node: vein,
            }));
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
        "ages" => {
            // Two Stone Age buildings appear finished, the Tool Age is
            // researched, and once it lands two Tool Age buildings and a
            // farm appear and the Bronze Age is queued. `--ticks` then runs
            // on from there: at 100 more the Tool Age settlement; at 1900
            // more the Bronze Age has just arrived. Needs `--stockpile`
            // high enough to pay for it all.
            let spot = |sim: &sim::Simulation, kind| {
                let (sx, sy) = sim.starts()[0];
                for dy in [-4, 4, -8, 8, 0] {
                    for dx in (4..=24).chain((-24..=-4).rev()) {
                        if sim.can_place(0, kind, sx + dx, sy + dy).is_ok() {
                            return Ok((sx + dx, sy + dy));
                        }
                    }
                }
                Err(format!("no room for {}", kinds::info(kind).name))
            };
            let tree = nearest(kinds::TREE).ok_or("no tree")?;
            let place = |sim: &mut sim::Simulation, kind| -> Result<(), String> {
                let (x, y) = spot(sim, kind)?;
                let fp = kinds::info(kind).footprint as i32;
                sim.issue(cmd(CommandKind::Spawn {
                    kind,
                    pos: sim::nav::building_centre(x, y, fp),
                }));
                for _ in 0..3 {
                    sim.step();
                }
                Ok(())
            };
            place(sim, kinds::BARRACKS)?;
            place(sim, kinds::STOREHOUSE)?;
            sim.issue(cmd(CommandKind::Research {
                building: tc,
                tech: sim::tech::AGE_TOOL,
            }));
            sim.issue(cmd(CommandKind::Gather {
                ids: villagers.clone(),
                node: tree,
            }));
            let tool = sim::tech::info(sim::tech::AGE_TOOL).unwrap().ticks();
            for _ in 0..tool + 5 {
                sim.step();
            }
            if sim.player(0).map(|p| p.age) != Some(sim::Age::Tool) {
                return Err("the Tool Age did not arrive; is --stockpile high enough?".into());
            }
            place(sim, kinds::MARKET)?;
            place(sim, kinds::ARCHERY_RANGE)?;
            place(sim, kinds::FARM)?;
            sim.issue(cmd(CommandKind::Research {
                building: tc,
                tech: sim::tech::AGE_BRONZE,
            }));
            sim.issue(cmd(CommandKind::Train {
                building: tc,
                kind: kinds::VILLAGER,
            }));
        }
        "army" => {
            // A Tool Age garrison: the three training buildings finished
            // beside the Town Center, a line of every soldier the slice
            // has in front of them, the Barracks selected so the panel
            // shows its roster. Needs `--stockpile` high enough for the
            // Tool Age and `--select-kind barracks`.
            let spot = |sim: &sim::Simulation, kind| {
                let (sx, sy) = sim.starts()[0];
                for dy in [-4, 4, -8, 8, 0] {
                    for dx in (4..=24).chain((-24..=-4).rev()) {
                        if sim.can_place(0, kind, sx + dx, sy + dy).is_ok() {
                            return Ok((sx + dx, sy + dy));
                        }
                    }
                }
                Err(format!("no room for {}", kinds::info(kind).name))
            };
            let place = |sim: &mut sim::Simulation, kind| -> Result<(), String> {
                let (x, y) = spot(sim, kind)?;
                let fp = kinds::info(kind).footprint as i32;
                sim.issue(cmd(CommandKind::Spawn {
                    kind,
                    pos: sim::nav::building_centre(x, y, fp),
                }));
                for _ in 0..3 {
                    sim.step();
                }
                Ok(())
            };
            place(sim, kinds::BARRACKS)?;
            place(sim, kinds::STOREHOUSE)?;
            sim.issue(cmd(CommandKind::Research {
                building: tc,
                tech: sim::tech::AGE_TOOL,
            }));
            let tool = sim::tech::info(sim::tech::AGE_TOOL).unwrap().ticks();
            for _ in 0..tool + 5 {
                sim.step();
            }
            if sim.player(0).map(|p| p.age) != Some(sim::Age::Tool) {
                return Err("the Tool Age did not arrive; is --stockpile high enough?".into());
            }
            place(sim, kinds::ARCHERY_RANGE)?;
            place(sim, kinds::STABLE)?;
            let line = [
                kinds::CLUBMAN,
                kinds::AXEMAN,
                kinds::SPEARMAN,
                kinds::SLINGER,
                kinds::BOWMAN,
                kinds::LIGHT_CAVALRY,
                kinds::SCOUT,
            ];
            for (n, kind) in line.iter().enumerate() {
                sim.issue(cmd(CommandKind::Spawn {
                    kind: *kind,
                    pos: sim::nav::centre((sx - 3 + n as i32, sy + 4)),
                }));
            }
            for _ in 0..3 {
                sim.step();
            }
            let barracks = sim
                .world()
                .slots()
                .find(|s| {
                    sim.world().owner[s.index()] == 0
                        && sim.world().kind[s.index()] == kinds::BARRACKS
                })
                .map(|s| sim.world().id_at(s))
                .ok_or("no barracks")?;
            for kind in [kinds::SPEARMAN, kinds::CLUBMAN] {
                sim.issue(cmd(CommandKind::Train {
                    building: barracks,
                    kind,
                }));
            }
        }
        "battle" => {
            // Two lines meet in front of the Town Center: our clubmen and
            // bowmen against their axemen and slingers, both attack-moving
            // through each other. Run `--ticks` on from here: at 140 the
            // lines have closed, arrows are in the air and the first
            // bodies are down. `--select-kind bowman` shows the soldier's
            // panel.
            let row = |sim: &mut sim::Simulation, player: u8, kinds: [sim::KindId; 2], x: i32| {
                for k in 0..5 {
                    for (n, kind) in kinds.iter().enumerate() {
                        sim.issue(Command {
                            player,
                            kind: CommandKind::Spawn {
                                kind: *kind,
                                pos: sim::nav::centre((x + n as i32 * 2, sy + 5 + k)),
                            },
                        });
                    }
                }
            };
            row(sim, 0, [kinds::BOWMAN, kinds::CLUBMAN], sx - 8);
            row(sim, 1, [kinds::AXEMAN, kinds::SLINGER], sx + 6);
            for _ in 0..3 {
                sim.step();
            }
            let side = |sim: &sim::Simulation, player: u8| -> Vec<sim::EntityId> {
                sim.world()
                    .slots()
                    .filter(|s| {
                        sim.world().owner[s.index()] == player
                            && kinds::info(sim.world().kind[s.index()]).combat.attack > 0
                            && sim.world().kind[s.index()] != kinds::VILLAGER
                            && sim.world().kind[s.index()] != kinds::SCOUT
                    })
                    .map(|s| sim.world().id_at(s))
                    .collect()
            };
            let (mine, theirs) = (side(sim, 0), side(sim, 1));
            sim.issue(Command {
                player: 0,
                kind: CommandKind::AttackMove {
                    ids: mine,
                    target: sim::nav::centre((sx + 8, sy + 7)),
                },
            });
            sim.issue(Command {
                player: 1,
                kind: CommandKind::AttackMove {
                    ids: theirs,
                    target: sim::nav::centre((sx - 8, sy + 7)),
                },
            });
        }
        "siege" => {
            // A palisade with a gate sealing the map in front of the
            // settlement, a Watch Tower behind it with two bowmen inside,
            // and their axemen and slingers attack-moving at the Town
            // Center: they find the gate shut, set about the wall, and
            // take the tower's arrows. Run `--ticks 260` from here;
            // `--select-kind watch_tower` shows the tower's panel with its
            // garrison.
            let x = sx + 7;
            for y in 0..sim.map().height() {
                let kind = if y == sy {
                    kinds::GATE
                } else {
                    kinds::PALISADE_WALL
                };
                sim.issue(cmd(CommandKind::Spawn {
                    kind,
                    pos: sim::nav::centre((x, y)),
                }));
            }
            sim.issue(cmd(CommandKind::Spawn {
                kind: kinds::WATCH_TOWER,
                pos: sim::nav::centre((x - 3, sy + 4)),
            }));
            for k in 0..2 {
                sim.issue(cmd(CommandKind::Spawn {
                    kind: kinds::BOWMAN,
                    pos: sim::nav::centre((x - 4, sy + 5 + k)),
                }));
            }
            for k in 0..4 {
                for (n, kind) in [kinds::AXEMAN, kinds::SLINGER].iter().enumerate() {
                    sim.issue(Command {
                        player: 1,
                        kind: CommandKind::Spawn {
                            kind: *kind,
                            pos: sim::nav::centre((x + 6 + n as i32 * 2, sy - 3 + k * 2)),
                        },
                    });
                }
            }
            for _ in 0..3 {
                sim.step();
            }
            let tower = sim
                .world()
                .slots()
                .find(|s| sim.world().kind[s.index()] == kinds::WATCH_TOWER)
                .map(|s| sim.world().id_at(s))
                .ok_or("no tower")?;
            let bows: Vec<sim::EntityId> = sim
                .world()
                .slots()
                .filter(|s| {
                    sim.world().owner[s.index()] == 0
                        && sim.world().kind[s.index()] == kinds::BOWMAN
                })
                .map(|s| sim.world().id_at(s))
                .collect();
            sim.issue(cmd(CommandKind::Garrison {
                ids: bows,
                building: tower,
            }));
            let theirs: Vec<sim::EntityId> = sim
                .world()
                .slots()
                .filter(|s| {
                    sim.world().owner[s.index()] == 1
                        && kinds::info(sim.world().kind[s.index()]).mobile
                })
                .map(|s| sim.world().id_at(s))
                .collect();
            sim.issue(Command {
                player: 1,
                kind: CommandKind::AttackMove {
                    ids: theirs,
                    target: tc_pos,
                },
            });
            let _ = tc;
        }
        "scout" => {
            // The scout rides away from the settlement past a house of
            // theirs into ground nobody has seen (`GD-FOG-01`): behind it
            // the trail is seen once and the house is remembered, dimmed;
            // around it the ground is in sight; ahead of it nothing is
            // known. Run `--ticks 400` from here and look at it zoomed out
            // from between the settlement and the scout.
            let dir = if sx < sim.map().width() / 2 { 1 } else { -1 };
            let house = kinds::HOUSE;
            let spot = (12..=20)
                .flat_map(|dx| [0, -2, 2, -4, 4].map(|dy| (sx + dir * dx, sy + dy)))
                .find(|&(x, y)| sim.can_place(0, house, x, y).is_ok())
                .ok_or("no room for their house")?;
            let fp = kinds::info(house).footprint as i32;
            sim.issue(Command {
                player: 1,
                kind: CommandKind::Spawn {
                    kind: house,
                    pos: sim::nav::building_centre(spot.0, spot.1, fp),
                },
            });
            sim.issue(cmd(CommandKind::Spawn {
                kind: kinds::SCOUT,
                pos: sim::nav::centre((sx + dir * 2, sy)),
            }));
            for _ in 0..3 {
                sim.step();
            }
            let scout = sim
                .world()
                .slots()
                .find(|s| {
                    sim.world().owner[s.index()] == 0 && sim.world().kind[s.index()] == kinds::SCOUT
                })
                .map(|s| sim.world().id_at(s))
                .ok_or("no scout")?;
            sim.issue(cmd(CommandKind::Move {
                ids: vec![scout],
                target: sim::nav::centre((sx + dir * 28, sy)),
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
