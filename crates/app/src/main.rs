//! New Empire — the game binary.
//!
//! The shell (`docs/06` M6) opens on a title screen; a skirmish is set up
//! on the next one and played against computer opponents that think on
//! their own view of the match every tick. In the match: select with
//! click, drag, double-click and control groups; right-click to move,
//! gather, build or attack; the HUD shows resources, population, idle
//! villagers and the selection; Escape opens the pause menu, and the
//! results come up when the match is decided.

mod clock;
mod input;
mod selection;

#[cfg(test)]
mod tests;

use ai::Opponent;
use clock::FixedClock;
use fogged::FoggedView;
use input::Input;
use selection::Selection;
use sim::kinds;
use sim::tech;
use sim::{Age, Command, CommandKind, EntityId, Rally, Simulation, Source, Vec2Fx, TICK_MS};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use view::hud::{Action, BOTTOM_PANEL, TOP_BAR};
use view::minimap::{Minimap, MinimapRect};
use view::shell::{self, Results, Side};
use view::{
    Atlas, Camera, FogLights, Ghost, Hud, HudInput, LoadRow, Scene, SceneOptions, Screen, Setup,
    ShellAction, ShellInput, Sweep, SWEEP_MS,
};

/// How long "SAVED ..." stays up, in ms.
const SAVED_NOTE_MS: u128 = 4000;

/// How long the age banner stays up, in ms.
const BANNER_MS: u128 = 4000;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{CursorIcon, Window, WindowId};

/// The human player.
const ME: u8 = 0;
/// Drags shorter than this are clicks.
const DRAG_THRESHOLD: f32 = 4.0;
/// Two clicks within this are a double-click.
const DOUBLE_CLICK: Duration = Duration::from_millis(350);

/// GPU surface state. Created once the window exists.
struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: render::Renderer,
}

impl Gpu {
    fn new(window: Arc<Window>, atlas: &Atlas) -> Gpu {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("no compatible GPU adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("new-empire"),
                required_features: wgpu::Features::empty(),
                // The atlas is 2048 wide and grows past 2048 tall once
                // rendered sets load; `default()` allows 8192.
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("request device");

        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let renderer = render::Renderer::new(&device, &queue, format, atlas);
        Gpu {
            surface,
            device,
            queue,
            config,
            renderer,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    fn render(&mut self, camera: &Camera, scene: &Scene, minimap: Option<MinimapRect>) {
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            Err(wgpu::SurfaceError::OutOfMemory) => panic!("GPU out of memory"),
            Err(wgpu::SurfaceError::Timeout) => return,
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.renderer
            .render(&self.device, &self.queue, &view, camera, scene, minimap);
        frame.present();
    }
}

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    atlas: Atlas,
    sim: Simulation,
    prev_pos: Vec<Vec2Fx>,
    feedback: view::feedback::CombatFeedback,
    clock: FixedClock,
    camera: Camera,
    input: Input,
    selection: Selection,
    /// Building being placed.
    build_mode: Option<sim::entity::KindId>,
    /// Picking a point for an attack-move or a patrol.
    targeting: Option<Targeting>,
    /// The defences page is open on the command grid.
    defences: bool,
    /// A wall is being dragged from this tile (`UX-PLACE-03`).
    wall_from: Option<(i32, i32)>,
    /// When the player's side was last told it was under attack.
    alarm_at: Option<Instant>,
    /// The player's HUD magnification on top of the display scale: 1, 1.5
    /// or 2. `F2` cycles it.
    ui_scale_user: f32,
    /// The controls overlay is open (`F1` or `?`).
    show_help: bool,
    /// The age the player was in last frame, to notice an advance.
    last_age: Age,
    /// When the last advance completed, and to what, for the celebration.
    age_up: Option<(Instant, Age)>,
    /// Last built scene, for picking.
    scene: Scene,
    /// Last built HUD, for button hit-testing.
    hud: Hud,
    modifiers: ModifiersState,
    last_click: Option<(Instant, EntityId)>,
    last_frame: Instant,
    last_title: Instant,
    last_minimap: Instant,
    /// The tick whose fog lights the GPU has.
    last_fog_tick: Option<u64>,
    frames: u32,
    fps: f32,
    /// Which screen the game is on.
    shell: Shell,
    /// The skirmish being set up, and the one the match was started from.
    setup: Setup,
    /// The computer opponents in the match, one per non-human player.
    opponents: Vec<Opponent>,
    /// The pause menu is open.
    menu: bool,
    /// Whether the clock was paused before the menu paused it.
    paused_before_menu: bool,
    /// A menu button that ends the match, awaiting its second click.
    confirm: Option<ShellAction>,
    /// Where the results screen is in its life.
    results: ResultsState,
    /// What the engine's check refused on the setup screen, if anything.
    setup_error: Option<String>,
    /// The last built shell screen or overlay, for button hit-testing.
    screen: Screen,
    /// The setup preview's minimap needs uploading.
    minimap_dirty: bool,
    /// The player asked to close the game.
    quit: bool,
    /// Where saves are written and read.
    saves_dir: PathBuf,
    /// The saves listed on the load screen, newest first.
    saves: Vec<save::Entry>,
    /// Why the last load was refused, shown on the load screen.
    load_error: Option<String>,
    /// The last save's outcome and when, for the note that follows it.
    saved: Option<(Instant, String)>,
}

/// Which screen the game is on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shell {
    /// The title and the main menu.
    Title,
    /// The skirmish setup.
    Setup,
    /// The saves, to load one.
    Load,
    /// A match, with the pause menu or the results over it or not.
    Match,
}

/// Where saves live: `NEW_EMPIRE_SAVES` if set, else the platform's data
/// directory for the game, else `saves` under the working directory.
fn saves_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("NEW_EMPIRE_SAVES") {
        return PathBuf::from(dir);
    }
    let home = |var: &str| std::env::var_os(var).map(PathBuf::from);
    let base = if cfg!(target_os = "windows") {
        home("APPDATA")
    } else if cfg!(target_os = "macos") {
        home("HOME").map(|h| h.join("Library").join("Application Support"))
    } else {
        home("XDG_DATA_HOME").or_else(|| home("HOME").map(|h| h.join(".local").join("share")))
    };
    base.map_or_else(
        || PathBuf::from("saves"),
        |b| b.join("new-empire").join("saves"),
    )
}

/// The results screen comes up once, when the match is decided, and
/// stays away once put away.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ResultsState {
    Pending,
    Shown,
    Dismissed,
}

/// A seed nobody chose: the clock's low digits, short enough to read off
/// the setup screen and type back.
fn random_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64);
    nanos % 899_999 + 1
}

impl App {
    fn new() -> App {
        // `new-empire [SEED]` pre-fills the setup screen's seed; without
        // one the clock picks.
        let seed = std::env::args()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(random_seed);
        let setup = Setup::new(seed);
        let sim = Simulation::new(seed, setup.config());
        let map = sim.map();
        let mut camera = Camera::new(map.width(), map.height(), (1280.0, 720.0));
        let (sx, sy) = sim.starts()[ME as usize];
        camera.look_at_tile(sx as f32 + 0.5, sy as f32 + 0.5);
        let prev_pos = sim.world().pos.clone();
        // Rendered sprite sets replace placeholders wherever they exist.
        let (sheets, errors) = view::sheets::default_dir()
            .map(|d| view::sheets::load_all(&d))
            .unwrap_or_default();
        for e in &errors {
            eprintln!("warning: {e}");
        }
        let atlas = Atlas::with_sheets(&sheets);
        if !atlas.loaded_sets.is_empty() {
            eprintln!("rendered sprite sets: {}", atlas.loaded_sets.join(", "));
        }
        App {
            window: None,
            gpu: None,
            atlas,
            sim,
            prev_pos,
            feedback: view::feedback::CombatFeedback::default(),
            clock: FixedClock::new(TICK_MS),
            camera,
            input: Input::new(),
            selection: Selection::new(),
            build_mode: None,
            targeting: None,
            defences: false,
            wall_from: None,
            alarm_at: None,
            ui_scale_user: 1.0,
            show_help: false,
            last_age: Age::Stone,
            age_up: None,
            scene: Scene::default(),
            hud: Hud::default(),
            modifiers: ModifiersState::empty(),
            last_click: None,
            last_frame: Instant::now(),
            last_title: Instant::now(),
            last_minimap: Instant::now() - Duration::from_secs(10),
            last_fog_tick: None,
            frames: 0,
            fps: 0.0,
            shell: Shell::Title,
            setup,
            opponents: Vec::new(),
            menu: false,
            paused_before_menu: false,
            confirm: None,
            results: ResultsState::Pending,
            setup_error: None,
            screen: Screen::default(),
            minimap_dirty: true,
            quit: false,
            saves_dir: saves_dir(),
            saves: Vec::new(),
            load_error: None,
            saved: None,
        }
    }

    /// Device pixels per HUD pixel.
    fn ui_scale(&self) -> f32 {
        self.camera.dpi * self.ui_scale_user
    }

    fn minimap_rect(&self) -> MinimapRect {
        let s = self.ui_scale();
        MinimapRect::bottom_right(self.camera.viewport, 256.0 * s, 16.0 * s)
    }

    /// Zooms by whole wheel steps about the cursor, or the centre without one.
    fn wheel(&mut self, lines: Option<f32>, pixels: Option<f32>) {
        if self.shell != Shell::Match || self.overlay() {
            return;
        }
        let steps = self.input.wheel_steps(lines, pixels);
        if steps == 0 {
            return;
        }
        let (cx, cy) = self
            .input
            .cursor
            .filter(|&(px, py)| !self.over_hud(px, py))
            .unwrap_or((self.camera.viewport.0 * 0.5, self.camera.viewport.1 * 0.5));
        self.camera.zoom_step_at(steps, cx, cy);
    }

    fn map_size(&self) -> (i32, i32) {
        (self.sim.map().width(), self.sim.map().height())
    }

    fn issue(&mut self, kind: CommandKind) {
        self.sim.issue(Command { player: ME, kind });
    }

    /// True if a window point is over the HUD rather than the world.
    fn over_hud(&self, _px: f32, py: f32) -> bool {
        let s = self.ui_scale();
        py < TOP_BAR * s || py > self.camera.viewport.1 - BOTTOM_PANEL * s
    }

    /// The world point under a window point, in tiles.
    fn world_point(&self, px: f32, py: f32) -> Vec2Fx {
        let (wx, wy) = self.camera.window_to_world(px, py);
        Vec2Fx::new(
            sim::Fx::from_ratio((wx * 1000.0) as i32, 1000),
            sim::Fx::from_ratio((wy * 1000.0) as i32, 1000),
        )
    }

    /// Tile under the cursor, for placement.
    fn hover_tile(&self) -> Option<(i32, i32)> {
        let (px, py) = self.input.cursor?;
        let (wx, wy) = self.camera.window_to_world(px, py);
        Some((wx.floor() as i32, wy.floor() as i32))
    }

    /// Whether the player can put `kind` on a tile: the simulation's rules,
    /// and only on ground they have seen (`GD-FOG-01`).
    fn placeable(&self, kind: sim::KindId, x: i32, y: i32) -> bool {
        self.sim.fog(ME).is_some_and(|f| f.explored(x, y))
            && self.sim.can_place(ME, kind, x, y).is_ok()
    }

    fn ghost(&self) -> Option<Ghost> {
        let kind = self.build_mode?;
        let (x, y) = self.hover_tile()?;
        Some(Ghost {
            kind,
            x,
            y,
            ok: self.placeable(kind, x, y),
            row: view::palette::row_for_owner(ME),
            player: ME,
            age: self.sim.player(ME).map_or(0, |p| p.age.index() as u8),
            run: self.wall_from,
        })
    }

    fn update_title(&mut self) {
        if let Some(w) = &self.window {
            let state = match self.shell {
                Shell::Title => "title".to_string(),
                Shell::Load => "load".to_string(),
                Shell::Setup => format!("setup — seed {}", self.setup.seed),
                Shell::Match => {
                    let paused = if self.clock.paused() { " [paused]" } else { "" };
                    format!(
                        "seed {} — tick {} — {} entities — {:.1}x{paused}",
                        self.sim.seed(),
                        self.sim.tick(),
                        self.sim.world().len(),
                        self.clock.speed
                    )
                }
            };
            w.set_title(&format!("New Empire — {state} — {:.0} fps", self.fps));
        }
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        match self.shell {
            Shell::Match => self.frame_match(now, dt),
            Shell::Title | Shell::Setup | Shell::Load => self.frame_shell(now),
        }
    }

    /// One tick of the match: every opponent thinks on its own view of it
    /// and issues as the AI (`GD-AI-01`), then the world moves.
    fn tick_once(&mut self, now: Instant) {
        self.prev_pos.clone_from(&self.sim.world().pos);
        for bot in &mut self.opponents {
            let commands = {
                let view = FoggedView::new(&self.sim, bot.player());
                bot.think(&view)
            };
            for c in commands {
                self.sim.issue_from(c, Source::Ai);
            }
        }
        self.sim.step();
        self.feedback.observe(&self.sim);
        if self
            .sim
            .events()
            .iter()
            .any(|e| matches!(e, sim::Event::Alarm { player, .. } if *player == ME))
        {
            self.alarm_at = Some(now);
        }
    }

    fn frame_match(&mut self, now: Instant, dt: f32) {
        if !self.overlay() {
            self.input.update_camera(&mut self.camera, dt);
        }
        let ticks = self.clock.advance(now);
        for _ in 0..ticks {
            self.tick_once(now);
        }
        self.selection.prune(&self.sim);

        // An age completing is the moment the presentation celebrates
        // ([GD-AGE-02]): the sweep over the settlement and the banner.
        let age = self.sim.player(ME).map_or(Age::Stone, |p| p.age);
        if age != self.last_age {
            self.age_up = Some((now, age));
            self.last_age = age;
        }
        let since = self.age_up.map(|(t, _)| t.elapsed().as_millis());
        let sweep = since.filter(|&ms| ms < SWEEP_MS as u128).map(|ms| Sweep {
            player: ME,
            elapsed_ms: ms as u32,
        });
        // The match decided outranks everything else ([GD-WIN-01]), and
        // stays up.
        let decided = match self.sim.winner() {
            Some(w) if w == ME => Some(view::hud::Banner::Victory),
            Some(_) => Some(view::hud::Banner::Defeat),
            None if !self.sim.standing(ME) => Some(view::hud::Banner::Defeat),
            None => None,
        };
        let banner = decided.or(match (self.age_up, since) {
            (Some((_, a)), Some(ms)) if ms < BANNER_MS => Some(view::hud::Banner::AgeUp(a)),
            _ => match self.alarm_at {
                // The villagers' alarm ([GD-STANCE-02]): the side is told.
                Some(t) if t.elapsed().as_millis() < ALARM_BANNER_MS => {
                    Some(view::hud::Banner::UnderAttack)
                }
                _ => None,
            },
        });

        let selected = self.selection.slots(&self.sim);
        let mut scene = Scene::build_full(
            &self.sim,
            &self.atlas,
            Some(&self.prev_pos),
            self.clock.alpha(),
            &SceneOptions {
                selected: &selected,
                ghost: self.ghost(),
                sweep,
                viewer: Some(ME),
            },
        );
        self.feedback
            .decorate(&mut scene, &self.sim, &self.atlas, Some(ME));
        // Band-box outline.
        if let (Some(from), Some(to)) = (self.selection.drag_from, self.input.cursor) {
            let thr = DRAG_THRESHOLD * self.camera.dpi;
            if (from.0 - to.0).abs() > thr || (from.1 - to.1).abs() > thr {
                let mut p = view::hud::Painter::new(&self.atlas);
                let (x, y) = (from.0.min(to.0), from.1.min(to.1));
                let (w, h) = ((from.0 - to.0).abs(), (from.1 - to.1).abs());
                p.outline(x, y, w, h, view::palette::WHITE);
                scene.ui.extend(p.out);
            }
        }
        let run = self.run_status();
        let saved = if self.saved_note().is_some() {
            "SAVED "
        } else {
            ""
        };
        let status = format!(
            "{saved}{run}SEED {} TICK {}",
            self.sim.seed(),
            self.sim.tick()
        );
        let hud = Hud::build(
            &self.atlas,
            &HudInput {
                sim: &self.sim,
                player: ME,
                camera: &self.camera,
                selected: &selected,
                build_mode: self.build_mode,
                fps: self.fps,
                paused: self.clock.paused(),
                speed: self.clock.speed,
                status: &status,
                hover: self.input.cursor,
                banner,
                targeting: self.targeting.is_some(),
                defences: self.defences,
                ui_scale: self.ui_scale(),
                help: self.show_help,
            },
        );
        scene.ui.extend(hud.sprites.iter().cloned());
        self.hud = hud;

        // The match decided brings the results up once ([GD-WIN-01]);
        // the pause menu and the results are the shell's, over the HUD.
        if self.results == ResultsState::Pending && self.decided() {
            self.results = ResultsState::Shown;
        }
        let input = self.shell_input();
        let overlay = if self.menu {
            Some(shell::pause_menu(
                &self.atlas,
                &input,
                self.decided(),
                self.confirm,
                self.saved_note().as_deref(),
            ))
        } else if self.results == ResultsState::Shown {
            Some(shell::results(&self.atlas, &input, &self.results_now()))
        } else {
            None
        };
        self.screen = match overlay {
            Some(o) => {
                scene.ui.extend(o.sprites.iter().cloned());
                o
            }
            None => Screen::default(),
        };

        self.count_frame(now);
        self.update_cursor();
        let rect = self.minimap_rect();
        if let Some(gpu) = &mut self.gpu {
            if self.last_minimap.elapsed().as_millis() >= 500 {
                let m = Minimap::render_for(&self.sim, Some(ME));
                gpu.renderer.upload_minimap(&gpu.device, &gpu.queue, &m);
                self.last_minimap = now;
            }
            // The fog changes only with the tick.
            if self.last_fog_tick != Some(self.sim.tick()) {
                if let Some(fog) = self.sim.fog(ME) {
                    let lights = FogLights::from_fog(fog);
                    gpu.renderer.upload_fog(&gpu.device, &gpu.queue, &lights);
                    self.last_fog_tick = Some(self.sim.tick());
                }
            }
            gpu.render(&self.camera, &scene, Some(rect));
        }
        self.scene = scene;
    }

    /// The title and the setup screen: no world, a backdrop and buttons,
    /// and on the setup screen the seed's map as the minimap will show it.
    fn frame_shell(&mut self, now: Instant) {
        let input = self.shell_input();
        let screen = match self.shell {
            Shell::Title => shell::title(&self.atlas, &input),
            Shell::Load => shell::load_screen(
                &self.atlas,
                &input,
                &self.load_rows(),
                self.load_error.as_deref(),
            ),
            _ => shell::setup(
                &self.atlas,
                &input,
                &self.setup,
                self.setup_error.as_deref(),
            ),
        };
        let scene = Scene {
            sprites: Vec::new(),
            ui: screen.sprites.clone(),
        };
        self.hud = Hud::default();
        self.count_frame(now);
        self.update_cursor();
        if let Some(gpu) = &mut self.gpu {
            if self.minimap_dirty {
                let m = Minimap::render(&self.sim);
                gpu.renderer.upload_minimap(&gpu.device, &gpu.queue, &m);
                self.minimap_dirty = false;
            }
            gpu.render(&self.camera, &scene, screen.preview);
        }
        self.screen = screen;
        self.scene = scene;
    }

    /// The frame counter and the window title, twice a second.
    fn count_frame(&mut self, now: Instant) {
        self.frames += 1;
        if self.last_title.elapsed().as_secs_f32() >= 0.5 {
            self.fps = self.frames as f32 / self.last_title.elapsed().as_secs_f32();
            self.frames = 0;
            self.last_title = now;
            self.update_title();
        }
    }

    /// "SAVED ..." while it is fresh.
    fn saved_note(&self) -> Option<String> {
        self.saved
            .as_ref()
            .filter(|(at, _)| at.elapsed().as_millis() < SAVED_NOTE_MS)
            .map(|(_, note)| note.clone())
    }

    /// The saves as the load screen lists them.
    fn load_rows(&self) -> Vec<LoadRow> {
        self.saves
            .iter()
            .map(|e| LoadRow {
                title: format!("SEED {} AT {}", e.summary.seed, save::clock(e.summary.tick)),
                detail: format!(
                    "{} - {} PLAYERS",
                    save::stamp(e.summary.saved_at),
                    e.summary.players
                ),
            })
            .collect()
    }

    /// Writes the match as it stands to the saves directory and notes
    /// the outcome for the menu and the status line.
    fn save_game(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let view = save::View {
            focus: self.camera.focus,
            zoom_index: self.camera.zoom_index,
        };
        let file = save::Save::new(&self.sim, &self.opponents, view, now);
        let note = match save::write(&self.saves_dir, &file) {
            Ok(path) => format!(
                "SAVED {}",
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("?")
                    .to_uppercase()
            ),
            Err(e) => format!("SAVE FAILED: {e}").to_uppercase(),
        };
        self.saved = Some((Instant::now(), note));
    }

    /// Opens the load screen on what the saves directory holds.
    fn open_load(&mut self) {
        self.saves = save::list(&self.saves_dir);
        self.load_error = None;
        self.shell = Shell::Load;
    }

    /// Loads the save on a row of the load screen, or says why not.
    fn load_game(&mut self, row: usize) {
        let Some(entry) = self.saves.get(row) else {
            return;
        };
        match save::read(&entry.path) {
            Ok(file) => self.resume(file),
            Err(e) => self.load_error = Some(e.to_string()),
        }
    }

    /// Resumes a loaded match: its world, its opponents mid-thought, and
    /// the camera where it was.
    fn resume(&mut self, file: save::Save) {
        self.sim = file.sim;
        self.opponents = file.opponents;
        self.enter_match();
        self.camera.focus = file.view.focus;
        self.camera.set_zoom_index(file.view.zoom_index);
        self.camera.clamp();
    }

    /// Whether the shell has something over the world that takes the
    /// input: the pause menu or the results.
    fn overlay(&self) -> bool {
        self.menu || self.results == ResultsState::Shown
    }

    /// The match is decided for the player: won, lost, or resigned.
    fn decided(&self) -> bool {
        self.sim.over() || !self.sim.standing(ME)
    }

    fn shell_input(&self) -> ShellInput {
        ShellInput {
            viewport: self.camera.viewport,
            ui_scale: self.ui_scale(),
            hover: self.input.cursor,
        }
    }

    /// The results as they stand: who won, why, and every side's score.
    fn results_now(&self) -> Results {
        let won = self.sim.winner() == Some(ME);
        let resigned = self.sim.player(ME).is_some_and(|p| p.resigned);
        let why = if won {
            "EVERY OTHER SIDE IS OUT"
        } else if resigned {
            "YOU RESIGNED"
        } else if !self.sim.standing(ME) {
            "NOTHING LEFT TO FIGHT WITH"
        } else {
            "ANOTHER SIDE WON"
        };
        let sides = (0..self.sim.players().len() as u8)
            .map(|p| Side {
                player: p,
                name: if p == ME {
                    "YOU".to_string()
                } else {
                    self.opponents
                        .iter()
                        .find(|o| o.player() == p)
                        .map_or("PLAYER".to_string(), |o| {
                            o.difficulty().name().to_uppercase()
                        })
                },
                score: self.sim.score(p),
                standing: self.sim.standing(p),
            })
            .collect();
        Results {
            won,
            why: why.to_string(),
            sides,
        }
    }

    /// A click on a shell screen or overlay: the enabled button under it.
    fn shell_click(&mut self, px: f32, py: f32) {
        if let Some(b) = self
            .screen
            .buttons
            .iter()
            .find(|b| b.contains(px, py))
            .cloned()
        {
            if b.enabled {
                self.shell_action(b.action);
            }
        }
    }

    fn shell_action(&mut self, action: ShellAction) {
        match action {
            ShellAction::NewGame => {
                self.shell = Shell::Setup;
                self.preview();
            }
            ShellAction::LoadGame => self.open_load(),
            ShellAction::Load(row) => self.load_game(row),
            ShellAction::Save => self.save_game(),
            ShellAction::WatchReplay | ShellAction::Settings => {}
            ShellAction::Quit => self.quit = true,
            ShellAction::Adjust(field, delta) => {
                self.setup.adjust(field, delta);
                self.preview();
            }
            ShellAction::Shuffle => {
                self.setup.seed = random_seed();
                self.preview();
            }
            ShellAction::Start => self.start_match(),
            ShellAction::Back => self.shell = Shell::Title,
            ShellAction::Resume => self.close_menu(),
            // Ending a live match takes two clicks: the first arms the
            // button, the second is the deed.
            ShellAction::Resign => {
                if self.confirm == Some(ShellAction::Resign) {
                    self.issue(CommandKind::Resign);
                    self.close_menu();
                } else {
                    self.confirm = Some(ShellAction::Resign);
                }
            }
            ShellAction::QuitToTitle => {
                if self.decided() || self.confirm == Some(ShellAction::QuitToTitle) {
                    self.quit_to_title();
                } else {
                    self.confirm = Some(ShellAction::QuitToTitle);
                }
            }
            ShellAction::KeepWatching => self.results = ResultsState::Dismissed,
        }
    }

    /// Regenerates the setup screen's preview: the map the seed gives, as
    /// the minimap will show it. The engine's check runs here too, so
    /// START greys the moment a setup is refused.
    fn preview(&mut self) {
        self.setup_error = self.setup.validate().err().map(|e| e.to_string());
        self.sim = Simulation::new(self.setup.seed, self.setup.config());
        self.minimap_dirty = true;
    }

    /// Starts the match the setup describes: a fresh world, an opponent
    /// per non-human player seeded from the match, the camera on the
    /// player's start, and everything of the last match cleared.
    fn start_match(&mut self) {
        if let Err(e) = self.setup.validate() {
            self.setup_error = Some(e.to_string());
            return;
        }
        let seed = self.setup.seed;
        self.sim = Simulation::new(seed, self.setup.config());
        self.opponents = self
            .setup
            .opponents
            .iter()
            .enumerate()
            .map(|(i, d)| Opponent::new(i as u8 + 1, *d, seed))
            .collect();
        self.enter_match();
    }

    /// Enters the match `self.sim` holds, new or loaded: the camera over
    /// the player's start, the clock fresh, and everything of the last
    /// match cleared.
    fn enter_match(&mut self) {
        let (viewport, dpi) = (self.camera.viewport, self.camera.dpi);
        let map = self.sim.map();
        self.camera = Camera::new(map.width(), map.height(), viewport);
        self.camera.dpi = dpi;
        if let Some(&(sx, sy)) = self.sim.starts().get(ME as usize) {
            self.camera.look_at_tile(sx as f32 + 0.5, sy as f32 + 0.5);
        }
        self.prev_pos = self.sim.world().pos.clone();
        self.feedback = view::feedback::CombatFeedback::default();
        self.clock = FixedClock::new(TICK_MS);
        self.selection = Selection::new();
        self.build_mode = None;
        self.targeting = None;
        self.defences = false;
        self.wall_from = None;
        self.alarm_at = None;
        self.show_help = false;
        // A loaded match is in the age it was left in: no celebration.
        self.last_age = self.sim.player(ME).map_or(Age::Stone, |p| p.age);
        self.age_up = None;
        self.last_click = None;
        self.last_fog_tick = None;
        self.last_minimap = Instant::now() - Duration::from_secs(10);
        self.menu = false;
        self.confirm = None;
        self.results = ResultsState::Pending;
        self.shell = Shell::Match;
        if let Some(gpu) = &mut self.gpu {
            let chunks = view::terrain::build_all(self.sim.map());
            gpu.renderer.upload_terrain(&gpu.device, &chunks);
        }
    }

    /// Leaves the match for the title. The world stays until the next
    /// setup replaces it.
    fn quit_to_title(&mut self) {
        self.shell = Shell::Title;
        self.opponents.clear();
        self.menu = false;
        self.confirm = None;
        self.build_mode = None;
        self.targeting = None;
        self.defences = false;
        self.wall_from = None;
    }

    /// Opens the pause menu. Menus pause (`docs/03` §1); a pause the
    /// player set before stays set after.
    fn open_menu(&mut self) {
        self.paused_before_menu = self.clock.paused();
        self.clock.set_paused(true);
        self.menu = true;
        self.confirm = None;
    }

    fn close_menu(&mut self) {
        self.menu = false;
        self.confirm = None;
        self.clock.set_paused(self.paused_before_menu);
    }

    /// The cursor tells you what a right-click will do.
    fn update_cursor(&self) {
        let Some(w) = &self.window else {
            return;
        };
        if self.shell != Shell::Match || self.overlay() {
            w.set_cursor(CursorIcon::Default);
            return;
        }
        let icon = match (self.build_mode, self.input.cursor) {
            (Some(_), _) => CursorIcon::Cell,
            (None, _) if self.targeting.is_some() => CursorIcon::Crosshair,
            (None, Some((px, py))) if !self.over_hud(px, py) => match self.hovered_target(px, py) {
                Some(Target::Gather) => CursorIcon::Grab,
                Some(Target::Assist) => CursorIcon::Pointer,
                Some(Target::Attack) => CursorIcon::Crosshair,
                Some(Target::Garrison) => CursorIcon::Copy,
                _ => CursorIcon::Default,
            },
            _ => CursorIcon::Default,
        };
        w.set_cursor(icon);
    }

    /// What a right-click at a point would do with the current selection.
    fn hovered_target(&self, px: f32, py: f32) -> Option<Target> {
        let id = selection::pick(&self.scene, &self.atlas, &self.camera, &self.sim, px, py)?;
        let slot = self.sim.world().slot(id)?;
        let i = slot.index();
        let world = self.sim.world();
        let villagers = !self.selection.own_villagers(&self.sim, ME).is_empty();
        if villagers && gatherable_by_me(&self.sim, i) {
            return Some(Target::Gather);
        }
        if villagers && world.owner[i] == ME && world.construction[i].is_some() {
            return Some(Target::Assist);
        }
        if enemy_of_me(&self.sim, i) && !self.selection.own_fighters(&self.sim, ME).is_empty() {
            return Some(Target::Attack);
        }
        if shelter_of_me(&self.sim, i) && !self.selection.own_mobile(&self.sim, ME).is_empty() {
            return Some(Target::Garrison);
        }
        Some(Target::Other(id))
    }

    /// "12 PALISADE WALL 60 WOOD " while a run is being dragged, else nothing.
    fn run_status(&self) -> String {
        let (Some(kind), Some(from), Some(to)) =
            (self.build_mode, self.wall_from, self.hover_tile())
        else {
            return String::new();
        };
        let info = kinds::info(kind);
        let n = sim::nav::line_tiles(from, to)
            .into_iter()
            .filter(|(x, y)| self.placeable(kind, *x, *y))
            .count() as i32;
        let cost: Vec<String> = info
            .cost
            .iter()
            .enumerate()
            .filter(|(_, c)| **c > 0)
            .map(|(r, c)| {
                format!(
                    "{} {}",
                    c * n,
                    kinds::Resource::from_index(r).name().to_uppercase()
                )
            })
            .collect();
        format!("{n} {} {} ", info.name.to_uppercase(), cost.join(" "))
    }

    /// The wall run dragged from `from` to the cursor is placed: every
    /// tile the simulation accepts, the villagers sent to the first so they
    /// carry on along it (`UX-PLACE-03`).
    fn place_run(&mut self, kind: sim::KindId, from: (i32, i32)) {
        let Some(to) = self.hover_tile() else {
            return;
        };
        let mut ids = self.selection.own_villagers(&self.sim, ME);
        for (x, y) in sim::nav::line_tiles(from, to) {
            if self.placeable(kind, x, y) {
                self.issue(CommandKind::Build {
                    kind,
                    x,
                    y,
                    ids: std::mem::take(&mut ids),
                });
            }
        }
    }

    fn left_press(&mut self, px: f32, py: f32) {
        // A shell screen or overlay takes the click; the world gets none.
        if self.shell != Shell::Match || self.overlay() {
            self.shell_click(px, py);
            return;
        }
        // The minimap is drawn above the HUD. Its whole diamond must also
        // win hit-testing, including while placing a building.
        let (mm, map) = (self.minimap_rect(), self.map_size());
        if self.input.left_pressed(&mut self.camera, mm, map, px, py) {
            return;
        }
        // HUD buttons first.
        if self.over_hud(px, py) {
            if let Some(b) = self
                .hud
                .buttons
                .iter()
                .find(|b| b.contains(px, py))
                .cloned()
            {
                if b.enabled {
                    self.do_action(b.action);
                }
            }
            return;
        }
        if let Some(kind) = self.build_mode {
            if let Some((x, y)) = self.hover_tile() {
                if kinds::is_wall(kind) {
                    // A wall is dragged: the run is placed on release.
                    self.wall_from = Some((x, y));
                } else if self.placeable(kind, x, y) {
                    let ids = self.selection.own_villagers(&self.sim, ME);
                    self.issue(CommandKind::Build { kind, x, y, ids });
                    if !self.modifiers.shift_key() {
                        self.build_mode = None;
                    }
                }
            }
            return;
        }
        if let Some(what) = self.targeting {
            let target = self.world_point(px, py);
            let ids = self.selection.own_mobile(&self.sim, ME);
            if !ids.is_empty() {
                self.issue(match what {
                    Targeting::AttackMove => CommandKind::AttackMove { ids, target },
                    Targeting::Patrol => CommandKind::Patrol { ids, target },
                });
            }
            if !self.modifiers.shift_key() {
                self.targeting = None;
            }
            return;
        }
        self.selection.drag_from = Some((px, py));
    }

    fn left_release(&mut self, px: f32, py: f32) {
        self.input.scrubbing = false;
        if self.shell != Shell::Match || self.overlay() {
            self.selection.drag_from = None;
            return;
        }
        if let (Some(kind), Some(from)) = (self.build_mode, self.wall_from.take()) {
            self.place_run(kind, from);
            if !self.modifiers.shift_key() {
                self.build_mode = None;
            }
            return;
        }
        let Some(from) = self.selection.drag_from.take() else {
            return;
        };
        let shift = self.modifiers.shift_key();
        let thr = DRAG_THRESHOLD * self.camera.dpi;
        if (from.0 - px).abs() > thr || (from.1 - py).abs() > thr {
            let ids = selection::band_box(&self.sim, &self.camera, ME, from, (px, py));
            if shift {
                self.selection.extend(ids);
            } else {
                self.selection.set(ids);
            }
            return;
        }
        // A click.
        let picked = selection::pick(&self.scene, &self.atlas, &self.camera, &self.sim, px, py);
        match picked {
            Some(id) => {
                let now = Instant::now();
                let double = matches!(self.last_click, Some((t, last)) if last == id && now.duration_since(t) < DOUBLE_CLICK);
                self.last_click = Some((now, id));
                if double {
                    let slot = self.sim.world().slot(id).unwrap();
                    let i = slot.index();
                    if self.sim.world().owner[i] == ME {
                        let kind = self.sim.world().kind[i];
                        let same =
                            selection::same_kind_on_screen(&self.sim, &self.camera, ME, kind);
                        self.selection.set(same);
                        return;
                    }
                }
                if shift {
                    self.selection.toggle(id);
                } else {
                    self.selection.set(vec![id]);
                }
            }
            None => {
                if !shift {
                    self.selection.set(vec![]);
                }
            }
        }
    }

    fn right_press(&mut self, px: f32, py: f32) {
        if self.shell != Shell::Match || self.overlay() {
            return;
        }
        if self.build_mode.is_some() || self.targeting.is_some() || self.defences {
            self.build_mode = None;
            self.wall_from = None;
            self.targeting = None;
            self.defences = false;
            return;
        }
        let minimap_uv = self.minimap_rect().to_uv(px, py);
        if minimap_uv.is_none() && self.over_hud(px, py) {
            return;
        }
        // Minimap right-click: move there.
        let target_world = match minimap_uv {
            Some((u, v)) => {
                let (w, h) = self.map_size();
                (u * w as f32, v * h as f32)
            }
            None => self.camera.window_to_world(px, py),
        };
        let target = Vec2Fx::new(
            sim::Fx::from_ratio((target_world.0 * 1000.0) as i32, 1000),
            sim::Fx::from_ratio((target_world.1 * 1000.0) as i32, 1000),
        );
        let mobile = self.selection.own_mobile(&self.sim, ME);
        let villagers = self.selection.own_villagers(&self.sim, ME);
        let trainer = self.selection.own_trainer(&self.sim, ME);

        // Contextual: something under the cursor?
        let picked = if minimap_uv.is_some() {
            None
        } else {
            selection::pick(&self.scene, &self.atlas, &self.camera, &self.sim, px, py)
        };
        if let Some(id) = picked {
            let slot = self.sim.world().slot(id).unwrap();
            let i = slot.index();
            let world = self.sim.world();
            let gatherable = gatherable_by_me(&self.sim, i);
            let site = world.owner[i] == ME && world.construction[i].is_some();
            let fighters = self.selection.own_fighters(&self.sim, ME);
            if !fighters.is_empty() && enemy_of_me(&self.sim, i) {
                self.issue(CommandKind::Attack {
                    ids: fighters,
                    target: id,
                });
                return;
            }
            if !mobile.is_empty() && shelter_of_me(&self.sim, i) {
                self.issue(CommandKind::Garrison {
                    ids: mobile,
                    building: id,
                });
                return;
            }
            if !villagers.is_empty() && gatherable {
                self.issue(CommandKind::Gather {
                    ids: villagers,
                    node: id,
                });
                return;
            }
            if !villagers.is_empty() && site {
                self.issue(CommandKind::Assist {
                    ids: villagers,
                    site: id,
                });
                return;
            }
            if mobile.is_empty() {
                if let Some(b) = trainer {
                    let rally = if gatherable || site {
                        Rally::Entity(id)
                    } else {
                        Rally::Point(target)
                    };
                    self.issue(CommandKind::SetRally { building: b, rally });
                }
                return;
            }
        }
        if !mobile.is_empty() {
            self.issue(CommandKind::Move {
                ids: mobile,
                target,
            });
        } else if let Some(b) = trainer {
            self.issue(CommandKind::SetRally {
                building: b,
                rally: Rally::Point(target),
            });
        }
    }

    fn do_action(&mut self, action: Action) {
        match action {
            Action::Build(kind) => {
                if !self.selection.own_villagers(&self.sim, ME).is_empty() {
                    self.build_mode = Some(kind);
                    self.defences = false;
                }
            }
            Action::Defences => self.defences = true,
            Action::Ungarrison(slot) => {
                let building = {
                    let world = self.sim.world();
                    world
                        .slots()
                        .find(|s| s.index() as u32 == slot)
                        .map(|s| world.id_at(s))
                };
                if let Some(building) = building {
                    self.issue(CommandKind::Ungarrison { building });
                }
            }
            Action::Train(kind) => {
                if let Some(b) = self.selection.own_trainer_of(&self.sim, ME, kind) {
                    self.issue(CommandKind::Train { building: b, kind });
                }
            }
            Action::CancelTrain(building) => {
                self.issue(CommandKind::CancelTrain { building });
            }
            Action::Stop => {
                let ids = self.selection.own_mobile(&self.sim, ME);
                if !ids.is_empty() {
                    self.issue(CommandKind::Stop { ids });
                }
            }
            Action::Cancel => {
                self.build_mode = None;
                self.wall_from = None;
                self.targeting = None;
                self.defences = false;
            }
            Action::AttackMove => self.targeting = Some(Targeting::AttackMove),
            Action::Patrol => self.targeting = Some(Targeting::Patrol),
            Action::Stance(stance) => {
                let ids = self.selection.own_mobile(&self.sim, ME);
                if !ids.is_empty() {
                    self.issue(CommandKind::SetStance { ids, stance });
                }
            }
            Action::Formation(formation) => {
                let ids = self.selection.own_mobile(&self.sim, ME);
                if !ids.is_empty() {
                    self.issue(CommandKind::SetFormation { ids, formation });
                }
            }
            Action::Research(t) => {
                let Some(info) = tech::info(t) else {
                    return;
                };
                if let Some(b) = self.selection.own_building(&self.sim, ME, info.building) {
                    self.issue(CommandKind::Research {
                        building: b,
                        tech: t,
                    });
                }
            }
            Action::ToggleReseed => {
                let on = self.sim.player(ME).is_some_and(|p| p.auto_reseed);
                self.issue(CommandKind::SetAutoReseed { enabled: !on });
            }
        }
    }

    /// Presses the enabled command button carrying this hotkey, if any. The
    /// HUD decides which keys mean what for the current selection, so the
    /// app does not keep a second copy of that table.
    fn hotkey(&mut self, ch: char) -> bool {
        let Some(b) = self
            .hud
            .buttons
            .iter()
            .find(|b| b.hotkey == ch && b.enabled)
            .cloned()
        else {
            return false;
        };
        self.do_action(b.action);
        true
    }

    /// Shared by the native event handler and headless input tests.
    /// Returns true only when the caller should close the window.
    fn keyboard_input(&mut self, code: KeyCode, state: ElementState, repeat: bool) -> bool {
        match state {
            ElementState::Pressed => {
                self.input.held.insert(code);
                if !repeat {
                    return self.key(code);
                }
            }
            ElementState::Released => {
                self.input.held.remove(&code);
            }
        }
        false
    }

    fn key(&mut self, code: KeyCode) -> bool {
        // The shell's screens take every key; the world gets none.
        match self.shell {
            Shell::Title => {
                match code {
                    KeyCode::Enter | KeyCode::NumpadEnter => {
                        self.shell_action(ShellAction::NewGame)
                    }
                    KeyCode::Escape => self.shell_action(ShellAction::Quit),
                    _ => {}
                }
                return self.quit;
            }
            Shell::Setup => {
                match code {
                    KeyCode::Enter | KeyCode::NumpadEnter => self.shell_action(ShellAction::Start),
                    KeyCode::Escape => self.shell_action(ShellAction::Back),
                    _ => {}
                }
                return false;
            }
            Shell::Load => {
                match code {
                    KeyCode::Enter | KeyCode::NumpadEnter => {
                        self.shell_action(ShellAction::Load(0))
                    }
                    KeyCode::Escape => self.shell_action(ShellAction::Back),
                    _ => {}
                }
                return false;
            }
            Shell::Match => {}
        }
        if self.menu {
            if code == KeyCode::Escape {
                self.close_menu();
            }
            return false;
        }
        if self.results == ResultsState::Shown {
            if code == KeyCode::Escape {
                self.results = ResultsState::Dismissed;
            }
            return false;
        }
        // Camera movement is handled through held keys, regardless of the
        // selection. Never let WASD also dispatch a command.
        if matches!(
            code,
            KeyCode::KeyW | KeyCode::KeyA | KeyCode::KeyS | KeyCode::KeyD
        ) {
            return false;
        }
        let ctrl = self.modifiers.control_key();
        let digit = match code {
            KeyCode::Digit0 => Some(0),
            KeyCode::Digit1 => Some(1),
            KeyCode::Digit2 => Some(2),
            KeyCode::Digit3 => Some(3),
            KeyCode::Digit4 => Some(4),
            KeyCode::Digit5 => Some(5),
            KeyCode::Digit6 => Some(6),
            KeyCode::Digit7 => Some(7),
            KeyCode::Digit8 => Some(8),
            KeyCode::Digit9 => Some(9),
            _ => None,
        };
        if let Some(d) = digit {
            if ctrl {
                self.selection.groups[d] = self.selection.ids.clone();
            } else if !self.selection.groups[d].is_empty() {
                let g = self.selection.groups[d].clone();
                self.selection.set(g);
            }
            return false;
        }
        match code {
            KeyCode::F1 | KeyCode::Slash => self.show_help = !self.show_help,
            KeyCode::F5 => self.save_game(),
            KeyCode::Escape => {
                if self.show_help {
                    self.show_help = false;
                } else if self.build_mode.is_some() || self.targeting.is_some() || self.defences {
                    self.build_mode = None;
                    self.wall_from = None;
                    self.targeting = None;
                    self.defences = false;
                } else if !self.selection.ids.is_empty() {
                    self.selection.set(vec![]);
                } else {
                    self.open_menu();
                }
            }
            KeyCode::Space => {
                let p = !self.clock.paused();
                self.clock.set_paused(p);
            }
            KeyCode::BracketRight => self.clock.speed = (self.clock.speed * 2.0).min(8.0),
            KeyCode::BracketLeft => self.clock.speed = (self.clock.speed / 2.0).max(0.25),
            KeyCode::Equal | KeyCode::NumpadAdd => self.camera.zoom_step(1),
            KeyCode::Minus | KeyCode::NumpadSubtract => self.camera.zoom_step(-1),
            KeyCode::F2 => {
                self.ui_scale_user = match self.ui_scale_user {
                    x if x < 1.25 => 1.5,
                    x if x < 1.75 => 2.0,
                    _ => 1.0,
                };
            }
            KeyCode::KeyH if self.selection.own_villagers(&self.sim, ME).is_empty() => {
                let (sx, sy) = self.sim.starts()[ME as usize];
                self.camera.look_at_tile(sx as f32 + 0.5, sy as f32 + 0.5);
            }
            KeyCode::KeyE if self.modifiers.shift_key() => {
                self.input.edge_scroll = !self.input.edge_scroll;
            }
            KeyCode::Period => {
                if let Some(id) = self.selection.next_idle(&self.sim, ME) {
                    let i = self.sim.world().slot(id).unwrap().index();
                    let p = self.sim.world().pos[i];
                    self.camera
                        .look_at_tile(view::fx_to_f32(p.x), view::fx_to_f32(p.y));
                }
            }
            KeyCode::Delete => {
                let ids = self.selection.own_mobile(&self.sim, ME);
                let sites: Vec<_> = self
                    .selection
                    .ids
                    .iter()
                    .copied()
                    .filter(|id| {
                        self.sim.world().slot(*id).is_some_and(|s| {
                            self.sim.world().owner[s.index()] == ME
                                && self.sim.world().construction[s.index()].is_some()
                        })
                    })
                    .collect();
                for id in ids.into_iter().chain(sites) {
                    self.issue(CommandKind::Despawn { id });
                }
            }
            code => {
                if let Some(ch) = letter(code) {
                    self.hotkey(ch);
                }
            }
        }
        false
    }
}

/// What a right-click would target.
enum Target {
    Gather,
    Assist,
    Attack,
    Garrison,
    #[allow(dead_code)]
    Other(EntityId),
}

/// What the next click on the ground orders (`UX-CMD-02`, `UX-CMD-03`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Targeting {
    AttackMove,
    Patrol,
}

/// How long the under-attack banner stays up, in ms.
const ALARM_BANNER_MS: u128 = 3000;

/// True if entity `i` is another player's and can be fought: not ours, not
/// nature's, not a corpse.
fn enemy_of_me(sim: &Simulation, i: usize) -> bool {
    let world = sim.world();
    let owner = world.owner[i];
    owner != ME
        && owner != kinds::GAIA
        && world.dying[i] == 0
        && kinds::info(world.kind[i]).class != kinds::Class::Other
}

/// True if entity `i` is a finished building of ours that units can
/// shelter in (`UX-CMD-09`).
fn shelter_of_me(sim: &Simulation, i: usize) -> bool {
    let world = sim.world();
    world.owner[i] == ME
        && world.dying[i] == 0
        && world.construction[i].is_none()
        && kinds::garrisons(world.kind[i])
}

/// True if the player's villagers may gather from entity `i`: a node with
/// something in it, and — for a farm — one of ours.
fn gatherable_by_me(sim: &Simulation, i: usize) -> bool {
    let world = sim.world();
    let kind = world.kind[i];
    kinds::gatherable(kind)
        && world.resource[i] > 0
        && world.construction[i].is_none()
        && (kind != kinds::FARM || world.owner[i] == ME)
}

/// The letter a key carries, for command hotkeys.
fn letter(code: KeyCode) -> Option<char> {
    Some(match code {
        KeyCode::KeyA => 'A',
        KeyCode::KeyB => 'B',
        KeyCode::KeyC => 'C',
        KeyCode::KeyD => 'D',
        KeyCode::KeyE => 'E',
        KeyCode::KeyF => 'F',
        KeyCode::KeyG => 'G',
        KeyCode::KeyH => 'H',
        KeyCode::KeyI => 'I',
        KeyCode::KeyJ => 'J',
        KeyCode::KeyK => 'K',
        KeyCode::KeyL => 'L',
        KeyCode::KeyM => 'M',
        KeyCode::KeyN => 'N',
        KeyCode::KeyO => 'O',
        KeyCode::KeyP => 'P',
        KeyCode::KeyQ => 'Q',
        KeyCode::KeyR => 'R',
        KeyCode::KeyS => 'S',
        KeyCode::KeyT => 'T',
        KeyCode::KeyU => 'U',
        KeyCode::KeyV => 'V',
        KeyCode::KeyW => 'W',
        KeyCode::KeyX => 'X',
        KeyCode::KeyY => 'Y',
        KeyCode::KeyZ => 'Z',
        _ => return None,
    })
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("New Empire")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let mut gpu = Gpu::new(window.clone(), &self.atlas);
        let chunks = view::terrain::build_all(self.sim.map());
        gpu.renderer.upload_terrain(&gpu.device, &chunks);
        let size = window.inner_size();
        self.camera.viewport = (size.width as f32, size.height as f32);
        // A Retina display reports twice the device pixels for the same
        // window; without this the world and the HUD draw at half size.
        self.camera.dpi = window.scale_factor() as f32;
        self.gpu = Some(gpu);
        self.window = Some(window);
        event_loop.set_control_flow(ControlFlow::Poll);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.camera.viewport = (size.width.max(1) as f32, size.height.max(1) as f32);
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(size.width, size.height);
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // Dragged to a display with a different density; the
                // matching `Resized` follows and fixes the viewport.
                self.camera.dpi = scale_factor as f32;
            }
            WindowEvent::Focused(f) => {
                self.input.focused = f;
                if !f {
                    self.input.held.clear();
                    self.input.dragging = None;
                    self.input.scrubbing = false;
                    self.selection.drag_from = None;
                }
            }
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state,
                        repeat,
                        ..
                    },
                ..
            } => {
                if self.keyboard_input(code, state, repeat) {
                    event_loop.exit();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (px, py) = (position.x as f32, position.y as f32);
                let (mm, map) = (self.minimap_rect(), self.map_size());
                self.input.cursor_moved(&mut self.camera, mm, map, px, py);
            }
            WindowEvent::CursorLeft { .. } => self.input.cursor = None,
            WindowEvent::MouseInput { state, button, .. } => {
                let Some((px, py)) = self.input.cursor else {
                    return;
                };
                match (button, state) {
                    (MouseButton::Middle, ElementState::Pressed) => {
                        self.input.dragging = Some((px, py))
                    }
                    (MouseButton::Middle, ElementState::Released) => self.input.dragging = None,
                    (MouseButton::Left, ElementState::Pressed) => self.left_press(px, py),
                    (MouseButton::Left, ElementState::Released) => self.left_release(px, py),
                    (MouseButton::Right, ElementState::Pressed) => self.right_press(px, py),
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => match delta {
                MouseScrollDelta::LineDelta(_, y) => self.wheel(Some(y), None),
                MouseScrollDelta::PixelDelta(p) => self.wheel(None, Some(p.y as f32)),
            },
            WindowEvent::RedrawRequested => self.frame(),
            _ => {}
        }
        if self.quit {
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

fn main() {
    // A panic inside an AppKit or Win32 callback cannot unwind, so the
    // process aborts and the message is easy to lose. Keep a copy on disk
    // beside the usual stderr line, so a crash report can be paired with
    // the cause.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_hook(info);
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("new-empire-panic.log")
        {
            let _ = writeln!(f, "{info}");
            eprintln!("(panic recorded in new-empire-panic.log)");
        }
    }));
    let event_loop = EventLoop::new().expect("event loop");
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("event loop failed");
}
