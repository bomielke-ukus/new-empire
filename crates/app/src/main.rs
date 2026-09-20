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
mod keys;
mod selection;
mod sound;

#[cfg(test)]
mod tests;

use ai::Opponent;
use audio::{Bus, Cue};
use clock::FixedClock;
use fogged::FoggedView;
use input::Input;
use selection::Selection;
use sim::kinds;
use sim::tech;
use sim::Class;
use sim::{
    Age, Command, CommandKind, EntityId, Rally, Replay, Simulation, Source, Vec2Fx, TICK_MS,
};
use sound::Speaker;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use view::hud::{Action, BOTTOM_PANEL, TOP_BAR};
use view::minimap::{Minimap, MinimapRect};
use view::shell::{self, Results, Side};
use view::{
    Atlas, Camera, Control, FogLights, Ghost, Hud, HudInput, LoadRow, Notice, NoticeKind, Notices,
    Scene, SceneOptions, Screen, Settings, Setup, ShellAction, ShellInput, Sweep, SWEEP_MS,
};

/// How long "SAVED ..." stays up, in ms.
const SAVED_NOTE_MS: u128 = 4000;

/// How long the age banner stays up, in ms.
const BANNER_MS: u128 = 4000;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{CursorIcon, Fullscreen, Window, WindowId};

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
    /// Where recordings of matches are written and read.
    replays_dir: PathBuf,
    /// The recordings listed on the replay screen, newest first.
    replays: Vec<save::Entry>,
    /// The recording being watched, if the match is a replay.
    playback: Option<Playback>,
    /// Whose eyes the world is seen through: a player's, or nobody's for
    /// the whole map, which only a replay allows.
    viewer: Option<u8>,
    /// When the match was entered, seconds since the epoch, for the
    /// recording's name.
    match_started: u64,
    /// The recording written for this match so far, replaced as it goes.
    recording: Option<PathBuf>,
    /// The player's settings, as applied.
    settings: Settings,
    /// Where they are kept.
    settings_path: PathBuf,
    /// The control waiting for its new key on the settings screen.
    capturing: Option<Control>,
    /// Why the last key was refused, or the file could not be written.
    settings_error: Option<String>,
    /// The notification stack (`docs/03` §6.3).
    notices: Notices,
    /// How many technologies the viewer had last frame, to notice a new
    /// one.
    last_researched: usize,
    /// What plays: the mixer decides (`docs/04` §8).
    mixer: audio::Mixer,
    /// Where it plays: the device, a recorder in tests, or nowhere.
    speaker: Speaker,
    /// When the app started: the mixer's clock, wall time.
    started: Instant,
}

/// A world position as a tile for the camera and the notices.
fn tile_of(pos: Vec2Fx) -> (f32, f32) {
    (view::fx_to_f32(pos.x), view::fx_to_f32(pos.y))
}

/// A recording being watched: the log, and where playback is in it.
struct Playback {
    replay: Replay,
    /// The next command to issue.
    next: usize,
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
    /// The recordings, to watch one.
    Replays,
    /// The settings.
    Settings,
    /// A match, with the pause menu or the results over it or not.
    Match,
}

/// Where the game keeps a kind of file: `env` if set, else `name` under
/// the platform's data directory for the game, else under the working
/// directory.
fn data_dir(env: &str, name: &str) -> PathBuf {
    if let Some(dir) = std::env::var_os(env) {
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
    base.map_or_else(|| PathBuf::from(name), |b| b.join("new-empire").join(name))
}

/// Saves or recordings as the list screens show them.
fn rows_for(entries: &[save::Entry]) -> Vec<LoadRow> {
    entries
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

/// Seconds since the epoch, or 0 if the clock is before it.
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
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
        // Recordings under assets/sounds replace the placeholder sounds by
        // cue name, as rendered sprite sets replace placeholder art.
        let mut library = audio::placeholder::library();
        let recorded = view::sheets::default_dir()
            .and_then(|d| d.parent().map(|p| p.join("sounds")))
            .map(|dir| sound::recordings(&dir, &mut library))
            .unwrap_or_default();
        if !recorded.is_empty() {
            eprintln!("recorded sounds: {}", recorded.join(", "));
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
            saves_dir: data_dir("NEW_EMPIRE_SAVES", "saves"),
            saves: Vec::new(),
            load_error: None,
            saved: None,
            replays_dir: data_dir("NEW_EMPIRE_REPLAYS", "replays"),
            replays: Vec::new(),
            playback: None,
            viewer: Some(ME),
            match_started: 0,
            recording: None,
            settings: Settings::default(),
            settings_path: data_dir("NEW_EMPIRE_SETTINGS", "settings.ron"),
            capturing: None,
            settings_error: None,
            notices: Notices::default(),
            last_researched: 0,
            mixer: audio::Mixer::new(library),
            speaker: Speaker::Silent,
            started: Instant::now(),
        }
    }

    /// Reads the settings file, if there is one, and applies it. A file
    /// that does not parse is left alone and reported; the defaults
    /// stand in.
    fn load_settings(&mut self) {
        match std::fs::read_to_string(&self.settings_path) {
            Ok(text) => match Settings::from_ron(&text) {
                Ok(s) => self.settings = s,
                Err(e) => {
                    eprintln!(
                        "warning: {}: {e}; the default settings are in use",
                        self.settings_path.display()
                    );
                    self.settings_error = Some("THE SETTINGS FILE COULD NOT BE READ".to_string());
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => eprintln!("warning: {}: {e}", self.settings_path.display()),
        }
        self.apply_settings();
    }

    /// Puts the settings into effect: the HUD size, edge scrolling, the
    /// pan keys the camera reads while held, and the window mode.
    fn apply_settings(&mut self) {
        self.ui_scale_user = self.settings.ui_scale;
        for bus in Bus::ALL {
            let volume = f32::from(self.settings.volume(bus)) / 100.0;
            self.mixer.set_volume(bus, volume);
            self.speaker.set_volume(bus, volume);
        }
        self.input.edge_scroll = self.settings.edge_scroll;
        self.input.pan = [
            Control::PanUp,
            Control::PanDown,
            Control::PanLeft,
            Control::PanRight,
        ]
        .map(|c| keys::code(self.settings.key(c)));
        if let Some(w) = &self.window {
            let mode = self
                .settings
                .fullscreen
                .then_some(Fullscreen::Borderless(None));
            if w.fullscreen().is_some() != self.settings.fullscreen {
                w.set_fullscreen(mode);
            }
        }
    }

    /// Writes the settings file, making its directory; a failure is
    /// shown on the settings screen and does not undo the change.
    fn save_settings(&mut self) {
        let written = self.settings.to_ron().and_then(|text| {
            if let Some(dir) = self.settings_path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            std::fs::write(&self.settings_path, text).map_err(|e| e.to_string())
        });
        if let Err(e) = written {
            eprintln!("warning: {}: {e}", self.settings_path.display());
            self.settings_error = Some("THE SETTINGS COULD NOT BE WRITTEN".to_string());
        }
    }

    /// A change on the settings screen or from a key: in effect and on
    /// disk at once.
    fn apply_and_save_settings(&mut self) {
        self.apply_settings();
        self.save_settings();
    }

    /// The settings screen's new key for the control being rebound: a
    /// pan key must be one the camera can read while held.
    fn capture_key(&mut self, control: Control, code: KeyCode) {
        let name = keys::name(code);
        let bound = if control.pans() && keys::code(&name).is_none() {
            Err(format!("{} CANNOT PAN", view::settings::pretty(&name)))
        } else {
            self.settings.bind(control, &name)
        };
        self.capturing = None;
        match bound {
            Ok(()) => {
                self.settings_error = None;
                self.apply_and_save_settings();
            }
            Err(e) => self.settings_error = Some(e),
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
        // A replay is watched: the log is the only source of commands.
        if self.playback.is_some() {
            return;
        }
        // The units answer the moment they are told (`UX-AUDIO-01`): the
        // bark is the command's, not the tick's, which is two ticks off.
        let voice = match &kind {
            CommandKind::Move { ids, .. }
            | CommandKind::Stop { ids }
            | CommandKind::Gather { ids, .. }
            | CommandKind::Build { ids, .. }
            | CommandKind::Assist { ids, .. }
            | CommandKind::Attack { ids, .. }
            | CommandKind::AttackMove { ids, .. }
            | CommandKind::Patrol { ids, .. }
            | CommandKind::Garrison { ids, .. } => self.voice_of(ids),
            _ => None,
        };
        if let Some(class) = voice {
            self.cue(Cue::Ack(class), None);
        }
        self.sim.issue(Command { player: ME, kind });
    }

    /// Milliseconds since the app started: the mixer's clock. Wall time,
    /// so a voice is busy for its clip's length whatever the match does.
    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// Asks for a sound, from a tile or from nowhere in particular; the
    /// mixer decides whether and how it plays, and the speaker plays it.
    fn cue(&mut self, cue: Cue, at: Option<(f32, f32)>) {
        let at = at.map(|(x, y)| view::iso::project(x, y, 0.0));
        let now = self.now_ms();
        if let Some(play) = self.mixer.cue(cue, at, now) {
            if let Some(clip) = self.mixer.library().clip(play.cue, play.variant) {
                self.speaker.play(&play, clip);
            }
        }
    }

    /// The class that answers for some of the player's units: the first
    /// mobile one's.
    fn voice_of(&self, ids: &[EntityId]) -> Option<Class> {
        let world = self.sim.world();
        ids.iter().find_map(|id| {
            let i = world.slot(*id)?.index();
            let info = kinds::info(world.kind[i]);
            (world.owner[i] == ME && info.mobile).then_some(info.class)
        })
    }

    /// The selection answers: the first of the player's units in it.
    fn selection_sound(&mut self) {
        if let Some(class) = self.voice_of(&self.selection.ids) {
            self.cue(Cue::Select(class), None);
        }
    }

    /// Opens the audio device; without one the game is silent and says
    /// so once.
    fn open_speaker(&mut self) {
        match sound::Device::open(self.mixer.library()) {
            Ok(device) => self.speaker = Speaker::Device(Box::new(device)),
            Err(e) => eprintln!("warning: no audio device: {e}"),
        }
        self.apply_settings();
    }

    /// The player whose panel the HUD shows.
    fn hud_player(&self) -> u8 {
        self.viewer.unwrap_or(ME)
    }

    /// The fastest the clock goes: a replay may be hurried more.
    fn max_speed(&self) -> f32 {
        if self.playback.is_some() {
            16.0
        } else {
            8.0
        }
    }

    /// A replay has reached the end of its recording.
    fn playback_over(&self) -> bool {
        self.playback
            .as_ref()
            .is_some_and(|p| self.sim.tick() >= p.replay.ticks)
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
                Shell::Settings => "settings".to_string(),
                Shell::Replays => "replays".to_string(),
                Shell::Setup => format!("setup — seed {}", self.setup.seed),
                Shell::Match => {
                    let paused = if self.clock.paused() { " [paused]" } else { "" };
                    let mode = if self.playback.is_some() {
                        "replay "
                    } else {
                        ""
                    };
                    format!(
                        "{mode}seed {} — tick {} — {} entities — {:.1}x{paused}",
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
            Shell::Title | Shell::Setup | Shell::Load | Shell::Replays | Shell::Settings => {
                self.frame_shell(now)
            }
        }
    }

    /// One tick of the match: every opponent thinks on its own view of it
    /// and issues as the AI (`GD-AI-01`), then the world moves. In a
    /// replay the recording issues instead, as `Replay::run` does.
    fn tick_once(&mut self, now: Instant) {
        self.prev_pos.clone_from(&self.sim.world().pos);
        if let Some(p) = &mut self.playback {
            while let Some((tick, command)) = p.replay.commands.get(p.next) {
                if *tick != self.sim.tick() {
                    break;
                }
                let via = p.replay.sources.get(p.next).copied().unwrap_or_default();
                self.sim.issue_from(command.clone(), via);
                p.next += 1;
            }
        }
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
        // What the tick sounded like, through the viewer's fog
        // (`TA-AUDIO-02`).
        let viewer = self.viewer;
        for (cue, at) in audio::events::cues(&self.sim, viewer) {
            self.cue(cue, at);
        }
        // What happened to the side goes on the stack (`docs/03` §6.3);
        // an attack also raises the banner.
        let me = self.hud_player();
        let tick = self.sim.tick();
        for e in self.sim.events() {
            match *e {
                sim::Event::Alarm { player, pos } if player == me => {
                    self.alarm_at = Some(now);
                    self.notices.push(Notice {
                        kind: NoticeKind::Attack,
                        text: "UNDER ATTACK".to_string(),
                        tile: Some(tile_of(pos)),
                        tick,
                    });
                }
                sim::Event::Death { kind, owner, pos } if owner == me => {
                    let info = kinds::info(kind);
                    let what = if info.mobile { "LOST" } else { "DESTROYED" };
                    self.notices.push(Notice {
                        kind: NoticeKind::Loss,
                        text: format!("{} {what}", info.name.to_uppercase()),
                        tile: Some(tile_of(pos)),
                        tick,
                    });
                }
                _ => {}
            }
        }
        self.notices.expire(tick);
    }

    fn frame_match(&mut self, now: Instant, dt: f32) {
        if !self.overlay() {
            self.input.update_camera(&mut self.camera, dt);
        }
        // The listener is the camera: the centre of the view, and how far
        // the view reaches from it (`docs/03` §6.1).
        let (l, t, r, b) = self.camera.visible_rect();
        self.mixer.set_listener(audio::Listener {
            focus: self.camera.focus,
            half: ((r - l) * 0.5, (b - t) * 0.5),
        });
        let ticks = self.clock.advance(now);
        for _ in 0..ticks {
            if self.playback_over() {
                break;
            }
            self.tick_once(now);
        }
        self.selection.prune(&self.sim);

        // An age completing is the moment the presentation celebrates
        // ([GD-AGE-02]): the sweep over the settlement and the banner.
        let me = self.hud_player();
        let age = self.sim.player(me).map_or(Age::Stone, |p| p.age);
        if age != self.last_age {
            self.age_up = Some((now, age));
            self.last_age = age;
            let world = self.sim.world();
            let town = world
                .slots()
                .find(|s| {
                    world.owner[s.index()] == me
                        && world.kind[s.index()] == kinds::TOWN_CENTER
                        && world.construction[s.index()].is_none()
                })
                .map(|s| tile_of(world.pos[s.index()]));
            self.notices.push(Notice {
                kind: NoticeKind::Age,
                text: age.name().to_uppercase(),
                tile: town,
                tick: self.sim.tick(),
            });
        }
        // A technology finishing is a notice; an age is announced above.
        let researched: Vec<sim::TechId> = self
            .sim
            .player(me)
            .map_or_else(Vec::new, |p| p.researched.clone());
        for id in researched.iter().skip(self.last_researched) {
            if let Some(t) = tech::info(*id).filter(|t| t.advances_age().is_none()) {
                self.notices.push(Notice {
                    kind: NoticeKind::Research,
                    text: format!("{} RESEARCHED", t.name.to_uppercase()),
                    tile: None,
                    tick: self.sim.tick(),
                });
            }
        }
        self.last_researched = researched.len();
        let since = self.age_up.map(|(t, _)| t.elapsed().as_millis());
        let sweep = since.filter(|&ms| ms < SWEEP_MS as u128).map(|ms| Sweep {
            player: ME,
            elapsed_ms: ms as u32,
        });
        // The match decided outranks everything else ([GD-WIN-01]), and
        // stays up.
        let decided = match self.sim.winner() {
            Some(w) if w == me => Some(view::hud::Banner::Victory),
            Some(_) => Some(view::hud::Banner::Defeat),
            None if !self.sim.standing(me) => Some(view::hud::Banner::Defeat),
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
                viewer: self.viewer,
            },
        );
        self.feedback
            .decorate(&mut scene, &self.sim, &self.atlas, self.viewer);
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
        // A replay says where it is and whose eyes it is seen through.
        let watching = match &self.playback {
            Some(p) => format!(
                "REPLAY {}/{} {} ",
                save::clock(self.sim.tick()),
                save::clock(p.replay.ticks),
                match self.viewer {
                    Some(v) => format!("P{}", v + 1),
                    None => "ALL".to_string(),
                }
            ),
            None => String::new(),
        };
        let status = format!(
            "{saved}{watching}{run}SEED {} TICK {}",
            self.sim.seed(),
            self.sim.tick()
        );
        let hud = Hud::build(
            &self.atlas,
            &HudInput {
                sim: &self.sim,
                player: me,
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
                settings: &self.settings,
                notices: self.notices.shown(),
            },
        );
        scene.ui.extend(hud.sprites.iter().cloned());
        self.hud = hud;

        // The match decided brings the results up once ([GD-WIN-01]), and
        // a replay's end does the same; the match is recorded then, so a
        // window closed on the results loses nothing. The pause menu and
        // the results are the shell's, over the HUD.
        let ended = if self.playback.is_some() {
            self.playback_over()
        } else {
            self.decided()
        };
        if self.results == ResultsState::Pending && ended {
            self.results = ResultsState::Shown;
            self.record_replay();
        }
        let input = self.shell_input();
        let overlay = if self.menu {
            Some(shell::pause_menu(
                &self.atlas,
                &input,
                self.decided(),
                self.confirm,
                self.saved_note().as_deref(),
                self.playback.is_some(),
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
                let m = Minimap::render_for(&self.sim, self.viewer);
                gpu.renderer.upload_minimap(&gpu.device, &gpu.queue, &m);
                self.last_minimap = now;
            }
            // The fog changes only with the tick; nobody's eyes see it all.
            if self.last_fog_tick != Some(self.sim.tick()) {
                let map = self.sim.map();
                let lights = match self.viewer {
                    Some(p) => self.sim.fog(p).map(FogLights::from_fog),
                    None => Some(FogLights::lit(map.width(), map.height())),
                };
                if let Some(lights) = lights {
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
                &rows_for(&self.saves),
                self.load_error.as_deref(),
                false,
            ),
            Shell::Replays => shell::load_screen(
                &self.atlas,
                &input,
                &rows_for(&self.replays),
                self.load_error.as_deref(),
                true,
            ),
            Shell::Settings => shell::settings_screen(
                &self.atlas,
                &input,
                &self.settings,
                self.capturing,
                self.settings_error.as_deref(),
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

    /// Writes the match as it stands to the saves directory and notes
    /// the outcome for the menu and the status line. A replay is not
    /// saved: it is a recording already.
    fn save_game(&mut self) {
        if self.playback.is_some() {
            return;
        }
        let now = now_secs();
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
        self.playback = None;
        self.enter_match();
        self.camera.focus = file.view.focus;
        self.camera.set_zoom_index(file.view.zoom_index);
        self.camera.clamp();
    }

    /// Opens the replay screen on what the recordings directory holds.
    fn open_replays(&mut self) {
        self.replays = save::replays::list(&self.replays_dir);
        self.load_error = None;
        self.shell = Shell::Replays;
    }

    /// Watches the recording on a row of the replay screen, from tick 0
    /// through the player's own eyes, or says why not.
    fn watch_replay(&mut self, row: usize) {
        let Some(entry) = self.replays.get(row) else {
            return;
        };
        match save::replays::read(&entry.path) {
            Ok(replay) => {
                self.sim = Simulation::new(replay.seed, replay.config.clone());
                self.opponents.clear();
                self.playback = Some(Playback { replay, next: 0 });
                self.enter_match();
            }
            Err(e) => self.load_error = Some(e.to_string()),
        }
    }

    /// Records the match so far (`TA-DET-05`): the log the simulation
    /// carries, under a name for when the match started and how far it
    /// got, replacing this match's earlier recording. A replay being
    /// watched is not recorded again.
    fn record_replay(&mut self) {
        if self.playback.is_some() || self.sim.tick() == 0 {
            return;
        }
        let replay = self.sim.replay();
        match save::replays::write(&self.replays_dir, &replay, self.match_started) {
            Ok(path) => {
                if let Some(old) = self.recording.replace(path.clone()) {
                    if old != path {
                        let _ = std::fs::remove_file(old);
                    }
                }
            }
            Err(e) => eprintln!("warning: the match was not recorded: {e}"),
        }
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

    /// The results as they stand: who won, why, and every side's score;
    /// or where a replay's recording ends.
    fn results_now(&self) -> Results {
        let won = self.sim.winner() == Some(ME);
        let resigned = self.sim.player(ME).is_some_and(|p| p.resigned);
        let (heading, why) = match &self.playback {
            Some(p) => (
                "REPLAY OVER",
                format!("THE RECORDING ENDS AT {}", save::clock(p.replay.ticks)),
            ),
            None if won => ("VICTORY", "EVERY OTHER SIDE IS OUT".to_string()),
            None if resigned => ("DEFEAT", "YOU RESIGNED".to_string()),
            None if !self.sim.standing(ME) => ("DEFEAT", "NOTHING LEFT TO FIGHT WITH".to_string()),
            None => ("DEFEAT", "ANOTHER SIDE WON".to_string()),
        };
        let sides = (0..self.sim.players().len() as u8)
            .map(|p| Side {
                player: p,
                name: if self.playback.is_some() {
                    format!("PLAYER {}", p + 1)
                } else if p == ME {
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
            heading: heading.to_string(),
            why,
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
                self.cue(Cue::Click, None);
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
            ShellAction::WatchReplay => self.open_replays(),
            ShellAction::Watch(row) => self.watch_replay(row),
            ShellAction::Settings => {
                self.shell = Shell::Settings;
                self.capturing = None;
                self.settings_error = None;
            }
            ShellAction::SettingScale(delta) => {
                self.settings.cycle_scale(delta);
                self.apply_and_save_settings();
            }
            ShellAction::Volume(bus, steps) => {
                self.settings.step_volume(bus, steps);
                self.apply_and_save_settings();
            }
            ShellAction::ToggleEdgeScroll => {
                self.settings.edge_scroll = !self.settings.edge_scroll;
                self.apply_and_save_settings();
            }
            ShellAction::ToggleFullscreen => {
                self.settings.fullscreen = !self.settings.fullscreen;
                self.apply_and_save_settings();
            }
            ShellAction::Rebind(control) => {
                self.capturing = Some(control);
                self.settings_error = None;
            }
            ShellAction::ResetSettings => {
                self.settings.reset();
                self.capturing = None;
                self.settings_error = None;
                self.apply_and_save_settings();
            }
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
            ShellAction::Resign if self.playback.is_some() => {}
            ShellAction::Resign => {
                if self.confirm == Some(ShellAction::Resign) {
                    self.issue(CommandKind::Resign);
                    self.close_menu();
                } else {
                    self.confirm = Some(ShellAction::Resign);
                }
            }
            ShellAction::QuitToTitle => {
                if self.decided()
                    || self.playback.is_some()
                    || self.confirm == Some(ShellAction::QuitToTitle)
                {
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
        self.playback = None;
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
        self.viewer = Some(ME);
        self.notices.clear();
        self.last_researched = self.sim.player(ME).map_or(0, |p| p.researched.len());
        self.match_started = now_secs();
        self.recording = None;
        self.shell = Shell::Match;
        if let Some(gpu) = &mut self.gpu {
            let chunks = view::terrain::build_all(self.sim.map());
            gpu.renderer.upload_terrain(&gpu.device, &chunks);
        }
    }

    /// Leaves the match for the title. The world stays until the next
    /// setup replaces it.
    fn quit_to_title(&mut self) {
        self.record_replay();
        self.shell = Shell::Title;
        self.opponents.clear();
        self.playback = None;
        self.viewer = Some(ME);
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
        // A notice on the stack is clicked to look where it points.
        if let Some(b) = self
            .hud
            .buttons
            .iter()
            .find(|b| matches!(b.action, Action::Jump(_)) && b.contains(px, py))
            .cloned()
        {
            self.do_action(b.action);
            return;
        }
        // HUD buttons first. A replay's panels are looked at, not used.
        if self.over_hud(px, py) {
            if let Some(b) = self
                .hud
                .buttons
                .iter()
                .find(|b| b.contains(px, py))
                .cloned()
            {
                if b.enabled && self.playback.is_none() {
                    self.do_action(b.action);
                } else if self.playback.is_none() {
                    // A greyed button buzzes: the refusal is heard as well
                    // as read (`docs/03` §8).
                    self.cue(Cue::Invalid, None);
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
            self.selection_sound();
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
                        self.selection_sound();
                        return;
                    }
                }
                if shift {
                    self.selection.toggle(id);
                } else {
                    self.selection.set(vec![id]);
                }
                self.selection_sound();
            }
            None => {
                if !shift {
                    self.selection.set(vec![]);
                }
            }
        }
    }

    fn right_press(&mut self, px: f32, py: f32) {
        if self.shell != Shell::Match || self.overlay() || self.playback.is_some() {
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
        // A button answers as it is pressed (`UX-AUDIO-01`), from the
        // panel or its key alike.
        self.cue(Cue::Click, None);
        match action {
            Action::Jump(row) => {
                if let Some((x, y)) = self.notices.shown().get(row).and_then(|n| n.tile) {
                    self.camera.look_at_tile(x, y);
                }
            }
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
            Shell::Replays => {
                match code {
                    KeyCode::Enter | KeyCode::NumpadEnter => {
                        self.shell_action(ShellAction::Watch(0))
                    }
                    KeyCode::Escape => self.shell_action(ShellAction::Back),
                    _ => {}
                }
                return false;
            }
            Shell::Settings => {
                match (self.capturing, code) {
                    (Some(_), KeyCode::Escape) => self.capturing = None,
                    (Some(control), code) => self.capture_key(control, code),
                    (None, KeyCode::Escape) => self.shell_action(ShellAction::Back),
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
        // selection. Never let a pan key also dispatch a command.
        if self.input.is_pan_key(code) {
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
                self.selection_sound();
            }
            return false;
        }
        // The general keys are the player's bindings (`GD-A11Y-02`); a few
        // fixed aliases keep the keypad and `?` working.
        let control = self.settings.control(&keys::name(code)).or(match code {
            KeyCode::Slash => Some(Control::Help),
            KeyCode::NumpadAdd => Some(Control::ZoomIn),
            KeyCode::NumpadSubtract => Some(Control::ZoomOut),
            _ => None,
        });
        match control {
            Some(Control::Help) => self.show_help = !self.show_help,
            Some(Control::QuickSave) => self.save_game(),
            Some(Control::Pause) => {
                let p = !self.clock.paused();
                self.clock.set_paused(p);
            }
            Some(Control::Faster) => {
                self.clock.speed = (self.clock.speed * 2.0).min(self.max_speed())
            }
            Some(Control::Slower) => self.clock.speed = (self.clock.speed / 2.0).max(0.25),
            Some(Control::ZoomIn) => self.camera.zoom_step(1),
            Some(Control::ZoomOut) => self.camera.zoom_step(-1),
            Some(Control::HudSize) => {
                self.settings.cycle_scale(1);
                self.apply_and_save_settings();
            }
            Some(Control::EdgeScroll) => {
                self.settings.edge_scroll = !self.settings.edge_scroll;
                self.apply_and_save_settings();
            }
            Some(Control::Home) => {
                if let Some(&(sx, sy)) = self.sim.starts().get(ME as usize) {
                    self.camera.look_at_tile(sx as f32 + 0.5, sy as f32 + 0.5);
                }
            }
            Some(Control::NextIdle) => {
                if let Some(id) = self.selection.next_idle(&self.sim, ME) {
                    let i = self.sim.world().slot(id).unwrap().index();
                    let p = self.sim.world().pos[i];
                    self.camera
                        .look_at_tile(view::fx_to_f32(p.x), view::fx_to_f32(p.y));
                    self.selection_sound();
                }
            }
            Some(Control::Eyes) if self.playback.is_some() => {
                let players = self.sim.players().len() as u8;
                self.viewer = match self.viewer {
                    Some(p) if p + 1 < players => Some(p + 1),
                    Some(_) => None,
                    None => Some(ME),
                };
                // The new side's past is not news.
                self.last_researched = self
                    .sim
                    .player(self.hud_player())
                    .map_or(0, |p| p.researched.len());
                self.notices.clear();
                self.last_fog_tick = None;
                self.last_minimap = Instant::now() - Duration::from_secs(10);
            }
            Some(Control::Dismiss) if self.playback.is_none() => {
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
            Some(
                Control::Eyes
                | Control::Dismiss
                | Control::PanUp
                | Control::PanDown
                | Control::PanLeft
                | Control::PanRight,
            ) => {}
            None => match code {
                KeyCode::Escape => {
                    if self.show_help {
                        self.show_help = false;
                    } else if self.build_mode.is_some() || self.targeting.is_some() || self.defences
                    {
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
                code => {
                    if let Some(ch) = letter(code) {
                        if self.playback.is_none() {
                            self.hotkey(ch);
                        }
                    }
                }
            },
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
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0))
            .with_fullscreen(
                self.settings
                    .fullscreen
                    .then_some(Fullscreen::Borderless(None)),
            );
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
            WindowEvent::CloseRequested => {
                // A match closed on is still recorded.
                if self.shell == Shell::Match {
                    self.record_replay();
                }
                event_loop.exit()
            }
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
            if self.shell == Shell::Match {
                self.record_replay();
            }
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
    app.load_settings();
    app.open_speaker();
    event_loop.run_app(&mut app).expect("event loop failed");
}
