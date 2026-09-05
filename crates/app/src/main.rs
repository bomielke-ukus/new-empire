//! New Empire — the game binary.
//!
//! M1: a generated Inland map you can look at. The window shows terrain,
//! placeholder sprites for everything on the map, and a minimap; the camera
//! scrolls (edges, WASD/arrows, middle-drag), zooms (wheel, +/-) and jumps
//! (minimap click, H for home). The simulation ticks underneath at 20 Hz;
//! Space pauses, [ and ] change speed.

mod clock;
mod input;

use clock::FixedClock;
use input::Input;
use sim::{SimConfig, Simulation, Vec2Fx, TICK_MS};
use std::sync::Arc;
use std::time::Instant;
use view::minimap::{Minimap, MinimapRect};
use view::{Atlas, Camera, Scene};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

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
                required_limits: wgpu::Limits::downlevel_defaults(),
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
        let (sx, sy) = sim.starts()[0];
        camera.look_at_tile(sx as f32 + 0.5, sy as f32 + 0.5);
        let prev_pos = sim.world().pos.clone();
        App {
            window: None,
            gpu: None,
            atlas: Atlas::placeholder(),
            sim,
            prev_pos,
            clock: FixedClock::new(TICK_MS),
            camera,
            input: Input::new(),
            last_frame: Instant::now(),
            last_title: Instant::now(),
            last_minimap: Instant::now() - std::time::Duration::from_secs(10),
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

    fn update_title(&mut self) {
        if let Some(w) = &self.window {
            let paused = if self.clock.paused() { " [paused]" } else { "" };
            w.set_title(&format!(
                "New Empire — seed {} — tick {} — {} entities — {:.0} fps — {:.1}x — zoom {}x{paused}",
                self.sim.seed(),
                self.sim.tick(),
                self.sim.world().len(),
                self.fps,
                self.clock.speed,
                self.camera.zoom()
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
        let scene = Scene::build(
            &self.sim,
            &self.atlas,
            Some(&self.prev_pos),
            self.clock.alpha(),
        );

        self.frames += 1;
        if self.last_title.elapsed().as_secs_f32() >= 0.5 {
            self.fps = self.frames as f32 / self.last_title.elapsed().as_secs_f32();
            self.frames = 0;
            self.last_title = now;
            self.update_title();
        }
        if let Some(gpu) = &mut self.gpu {
            if self.last_minimap.elapsed().as_millis() >= 500 {
                let m = Minimap::render(&self.sim);
                gpu.renderer.upload_minimap(&gpu.device, &gpu.queue, &m);
                self.last_minimap = now;
            }
            let rect = MinimapRect::bottom_right(self.camera.viewport, 256.0, 16.0);
            gpu.render(&self.camera, &scene, rect);
        }
    }

    fn key(&mut self, event_loop: &ActiveEventLoop, code: KeyCode) {
        match code {
            KeyCode::Escape => event_loop.exit(),
            KeyCode::Space => {
                let p = !self.clock.paused();
                self.clock.set_paused(p);
            }
            KeyCode::BracketRight => self.clock.speed = (self.clock.speed * 2.0).min(8.0),
            KeyCode::BracketLeft => self.clock.speed = (self.clock.speed / 2.0).max(0.25),
            KeyCode::Equal | KeyCode::NumpadAdd => self.camera.zoom_step(1),
            KeyCode::Minus | KeyCode::NumpadSubtract => self.camera.zoom_step(-1),
            KeyCode::KeyH => {
                let (sx, sy) = self.sim.starts()[0];
                self.camera.look_at_tile(sx as f32 + 0.5, sy as f32 + 0.5);
            }
            KeyCode::KeyE => self.input.edge_scroll = !self.input.edge_scroll,
            _ => {}
        }
    }
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
                }
            }
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
                let cursor = self.input.cursor;
                match (button, state) {
                    (MouseButton::Middle, ElementState::Pressed) => self.input.dragging = cursor,
                    (MouseButton::Middle, ElementState::Released) => self.input.dragging = None,
                    (MouseButton::Left, ElementState::Pressed) => {
                        if let Some((px, py)) = cursor {
                            let (mm, map) = (self.minimap_rect(), self.map_size());
                            self.input.left_pressed(&mut self.camera, mm, map, px, py);
                        }
                    }
                    (MouseButton::Left, ElementState::Released) => self.input.scrubbing = false,
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
