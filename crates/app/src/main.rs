//! New Empire — the game binary.
//!
//! M2: villagers and the economy. Select with click, drag, double-click and
//! control groups; right-click to move, gather or help build; place houses
//! and storehouses; train villagers and set rally points. The HUD shows
//! resources, population, idle villagers and the selection.

mod clock;
mod input;
mod selection;

use clock::FixedClock;
use input::Input;
use selection::Selection;
use sim::kinds;
use sim::{Command, CommandKind, EntityId, Rally, SimConfig, Simulation, Vec2Fx, TICK_MS};
use std::sync::Arc;
use std::time::{Duration, Instant};
use view::hud::{Action, BOTTOM_PANEL, TOP_BAR};
use view::minimap::{Minimap, MinimapRect};
use view::{Atlas, Camera, Ghost, Hud, HudInput, Scene};
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

    fn render(&mut self, camera: &Camera, scene: &Scene, minimap: MinimapRect) {
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
        self.renderer.render(
            &self.device,
            &self.queue,
            &view,
            camera,
            scene,
            Some(minimap),
        );
        frame.present();
    }
}

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    atlas: Atlas,
    sim: Simulation,
    prev_pos: Vec<Vec2Fx>,
    clock: FixedClock,
    camera: Camera,
    input: Input,
    selection: Selection,
    /// Building being placed.
    build_mode: Option<sim::entity::KindId>,
    /// Last built scene, for picking.
    scene: Scene,
    /// Last built HUD, for button hit-testing.
    hud: Hud,
    modifiers: ModifiersState,
    last_click: Option<(Instant, EntityId)>,
    last_frame: Instant,
    last_title: Instant,
    last_minimap: Instant,
    frames: u32,
    fps: f32,
}

impl App {
    fn new() -> App {
        let seed = std::env::args()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        let sim = Simulation::new(seed, SimConfig::default());
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
            clock: FixedClock::new(TICK_MS),
            camera,
            input: Input::new(),
            selection: Selection::new(),
            build_mode: None,
            scene: Scene::default(),
            hud: Hud::default(),
            modifiers: ModifiersState::empty(),
            last_click: None,
            last_frame: Instant::now(),
            last_title: Instant::now(),
            last_minimap: Instant::now() - Duration::from_secs(10),
            frames: 0,
            fps: 0.0,
        }
    }

    fn minimap_rect(&self) -> MinimapRect {
        MinimapRect::bottom_right(self.camera.viewport, 256.0, 16.0)
    }

    fn map_size(&self) -> (i32, i32) {
        (self.sim.map().width(), self.sim.map().height())
    }

    fn issue(&mut self, kind: CommandKind) {
        self.sim.issue(Command { player: ME, kind });
    }

    /// True if a window point is over the HUD rather than the world.
    fn over_hud(&self, _px: f32, py: f32) -> bool {
        py < TOP_BAR || py > self.camera.viewport.1 - BOTTOM_PANEL
    }

    /// Tile under the cursor, for placement.
    fn hover_tile(&self) -> Option<(i32, i32)> {
        let (px, py) = self.input.cursor?;
        let (wx, wy) = self.camera.window_to_world(px, py);
        Some((wx.floor() as i32, wy.floor() as i32))
    }

    fn ghost(&self) -> Option<Ghost> {
        let kind = self.build_mode?;
        let (x, y) = self.hover_tile()?;
        Some(Ghost {
            kind,
            x,
            y,
            ok: self.sim.can_place(ME, kind, x, y).is_ok(),
            row: view::palette::row_for_owner(ME),
        })
    }

    fn update_title(&mut self) {
        if let Some(w) = &self.window {
            let paused = if self.clock.paused() { " [paused]" } else { "" };
            w.set_title(&format!(
                "New Empire — seed {} — tick {} — {} entities — {:.0} fps — {:.1}x{paused}",
                self.sim.seed(),
                self.sim.tick(),
                self.sim.world().len(),
                self.fps,
                self.clock.speed
            ));
        }
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;

        self.input.update_camera(&mut self.camera, dt);

        let ticks = self.clock.advance(now);
        for _ in 0..ticks {
            self.prev_pos.clone_from(&self.sim.world().pos);
            self.sim.step();
        }
        self.selection.prune(&self.sim);

        let selected = self.selection.slots(&self.sim);
        let mut scene = Scene::build_with(
            &self.sim,
            &self.atlas,
            Some(&self.prev_pos),
            self.clock.alpha(),
            &selected,
            self.ghost(),
        );
        // Band-box outline.
        if let (Some(from), Some(to)) = (self.selection.drag_from, self.input.cursor) {
            if (from.0 - to.0).abs() > DRAG_THRESHOLD || (from.1 - to.1).abs() > DRAG_THRESHOLD {
                let mut p = view::hud::Painter::new(&self.atlas);
                let (x, y) = (from.0.min(to.0), from.1.min(to.1));
                let (w, h) = ((from.0 - to.0).abs(), (from.1 - to.1).abs());
                p.outline(x, y, w, h, view::palette::WHITE);
                scene.ui.extend(p.out);
            }
        }
        let status = format!("SEED {} TICK {}", self.sim.seed(), self.sim.tick());
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
            },
        );
        scene.ui.extend(hud.sprites.iter().cloned());
        self.hud = hud;

        self.frames += 1;
        if self.last_title.elapsed().as_secs_f32() >= 0.5 {
            self.fps = self.frames as f32 / self.last_title.elapsed().as_secs_f32();
            self.frames = 0;
            self.last_title = now;
            self.update_title();
        }
        self.update_cursor();
        if let Some(gpu) = &mut self.gpu {
            if self.last_minimap.elapsed().as_millis() >= 500 {
                let m = Minimap::render(&self.sim);
                gpu.renderer.upload_minimap(&gpu.device, &gpu.queue, &m);
                self.last_minimap = now;
            }
            let rect = MinimapRect::bottom_right(self.camera.viewport, 256.0, 16.0);
            gpu.render(&self.camera, &scene, rect);
        }
        self.scene = scene;
    }

    /// The cursor tells you what a right-click will do.
    fn update_cursor(&self) {
        let Some(w) = &self.window else {
            return;
        };
        let icon = match (self.build_mode, self.input.cursor) {
            (Some(_), _) => CursorIcon::Cell,
            (None, Some((px, py))) if !self.over_hud(px, py) => match self.hovered_target(px, py) {
                Some(Target::Gather) => CursorIcon::Grab,
                Some(Target::Assist) => CursorIcon::Pointer,
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
        if villagers && kinds::gatherable(world.kind[i]) && world.resource[i] > 0 {
            return Some(Target::Gather);
        }
        if villagers && world.owner[i] == ME && world.construction[i].is_some() {
            return Some(Target::Assist);
        }
        Some(Target::Other(id))
    }

    fn left_press(&mut self, px: f32, py: f32) {
        // HUD buttons first.
        if self.over_hud(px, py) {
            if let Some(b) = self
                .hud
                .buttons
                .iter()
                .find(|b| b.contains(px, py))
                .cloned()
            {
                self.do_action(b.action);
            }
            return;
        }
        if let Some(kind) = self.build_mode {
            if let Some((x, y)) = self.hover_tile() {
                if self.sim.can_place(ME, kind, x, y).is_ok() {
                    let ids = self.selection.own_villagers(&self.sim, ME);
                    self.issue(CommandKind::Build { kind, x, y, ids });
                    if !self.modifiers.shift_key() {
                        self.build_mode = None;
                    }
                }
            }
            return;
        }
        let (mm, map) = (self.minimap_rect(), self.map_size());
        if self.input.left_pressed(&mut self.camera, mm, map, px, py) {
            return;
        }
        self.selection.drag_from = Some((px, py));
    }

    fn left_release(&mut self, px: f32, py: f32) {
        self.input.scrubbing = false;
        let Some(from) = self.selection.drag_from.take() else {
            return;
        };
        let shift = self.modifiers.shift_key();
        if (from.0 - px).abs() > DRAG_THRESHOLD || (from.1 - py).abs() > DRAG_THRESHOLD {
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
        if self.build_mode.is_some() {
            self.build_mode = None;
            return;
        }
        if self.over_hud(px, py) {
            return;
        }
        // Minimap right-click: move there.
        let target_world = match self.minimap_rect().to_uv(px, py) {
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
        let picked = if self.minimap_rect().to_uv(px, py).is_some() {
            None
        } else {
            selection::pick(&self.scene, &self.atlas, &self.camera, &self.sim, px, py)
        };
        if let Some(id) = picked {
            let slot = self.sim.world().slot(id).unwrap();
            let i = slot.index();
            let world = self.sim.world();
            let gatherable = kinds::gatherable(world.kind[i]) && world.resource[i] > 0;
            let site = world.owner[i] == ME && world.construction[i].is_some();
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
                }
            }
            Action::Train => {
                if let Some(b) = self.selection.own_trainer(&self.sim, ME) {
                    self.issue(CommandKind::Train {
                        building: b,
                        kind: kinds::VILLAGER,
                    });
                }
            }
            Action::CancelTrain => {
                if let Some(b) = self.selection.own_trainer(&self.sim, ME) {
                    self.issue(CommandKind::CancelTrain { building: b });
                }
            }
            Action::Stop => {
                let ids = self.selection.own_mobile(&self.sim, ME);
                if !ids.is_empty() {
                    self.issue(CommandKind::Stop { ids });
                }
            }
            Action::Cancel => self.build_mode = None,
        }
    }

    fn key(&mut self, event_loop: &ActiveEventLoop, code: KeyCode) {
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
            return;
        }
        match code {
            KeyCode::Escape => {
                if self.build_mode.is_some() {
                    self.build_mode = None;
                } else if !self.selection.ids.is_empty() {
                    self.selection.set(vec![]);
                } else {
                    event_loop.exit();
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
            KeyCode::KeyH if self.selection.own_villagers(&self.sim, ME).is_empty() => {
                let (sx, sy) = self.sim.starts()[ME as usize];
                self.camera.look_at_tile(sx as f32 + 0.5, sy as f32 + 0.5);
            }
            KeyCode::KeyH => self.do_action(Action::Build(kinds::HOUSE)),
            KeyCode::KeyB => self.do_action(Action::Build(kinds::STOREHOUSE)),
            KeyCode::KeyV => self.do_action(Action::Train),
            KeyCode::KeyT => self.do_action(Action::Stop),
            KeyCode::KeyX => {
                if self.build_mode.is_some() {
                    self.do_action(Action::Cancel);
                } else {
                    self.do_action(Action::CancelTrain);
                }
            }
            KeyCode::Period => {
                if let Some(id) = self.selection.next_idle(&self.sim, ME) {
                    let i = self.sim.world().slot(id).unwrap().index();
                    let p = self.sim.world().pos[i];
                    self.camera
                        .look_at_tile(view::fx_to_f32(p.x), view::fx_to_f32(p.y));
                }
            }
            KeyCode::KeyE => self.input.edge_scroll = !self.input.edge_scroll,
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
            _ => {}
        }
    }
}

/// What a right-click would target.
enum Target {
    Gather,
    Assist,
    #[allow(dead_code)]
    Other(EntityId),
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
            } => match state {
                ElementState::Pressed => {
                    self.input.held.insert(code);
                    if !repeat {
                        self.key(event_loop, code);
                    }
                }
                ElementState::Released => {
                    self.input.held.remove(&code);
                }
            },
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
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                if steps > 0.0 {
                    self.camera.zoom_step(1);
                } else if steps < 0.0 {
                    self.camera.zoom_step(-1);
                }
            }
            WindowEvent::RedrawRequested => self.frame(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("event loop failed");
}
