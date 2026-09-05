//! New Empire — the game binary.
//!
//! M0: opens a window, clears it, and drives the simulation at a fixed 20 Hz
//! from a real-time clock. The window title shows the tick and state hash so
//! the loop is visibly alive. Everything else comes in M1+.

mod clock;

use clock::FixedClock;
use sim::{Command, CommandKind, SimConfig, Simulation, Vec2Fx, TICK_MS};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

/// GPU surface state. Created once the window exists.
struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
}

impl Gpu {
    fn new(window: Arc<Window>) -> Gpu {
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
        Gpu {
            surface,
            device,
            queue,
            config,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    /// Clears the frame. Terrain, sprites and UI passes replace this in M1.
    fn render(&mut self) {
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
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // A grassland green, so the first window already looks like a map.
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.16,
                            g: 0.30,
                            b: 0.12,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        self.queue.submit(Some(encoder.finish()));
        frame.present();
    }
}

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    sim: Simulation,
    clock: FixedClock,
    last_title: Instant,
}

impl App {
    fn new() -> App {
        let mut sim = Simulation::new(0x5EED, SimConfig::default());
        // A few wandering units so the hash visibly changes.
        for i in 0..12 {
            sim.issue(Command {
                player: (i % 2) as u8,
                kind: CommandKind::Spawn {
                    kind: 1,
                    pos: Vec2Fx::from_int(60 + i, 60),
                },
            });
        }
        App {
            window: None,
            gpu: None,
            sim,
            clock: FixedClock::new(TICK_MS),
            last_title: Instant::now(),
        }
    }

    fn update_title(&mut self) {
        if let Some(w) = &self.window {
            let paused = if self.clock.paused() { " [paused]" } else { "" };
            w.set_title(&format!(
                "New Empire — tick {} — {:016x} — {} units — {:.1}x{paused}",
                self.sim.tick(),
                self.sim.state_hash(),
                self.sim.world().len(),
                self.clock.speed
            ));
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
        self.gpu = Some(Gpu::new(window.clone()));
        self.window = Some(window);
        event_loop.set_control_flow(ControlFlow::Poll);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => match code {
                KeyCode::Escape => event_loop.exit(),
                KeyCode::Space => {
                    let p = !self.clock.paused();
                    self.clock.set_paused(p);
                }
                KeyCode::Equal | KeyCode::NumpadAdd => {
                    self.clock.speed = (self.clock.speed * 2.0).min(8.0)
                }
                KeyCode::Minus | KeyCode::NumpadSubtract => {
                    self.clock.speed = (self.clock.speed / 2.0).max(0.25)
                }
                _ => {}
            },
            WindowEvent::RedrawRequested => {
                let ticks = self.clock.advance(Instant::now());
                for _ in 0..ticks {
                    self.sim.step();
                }
                if self.last_title.elapsed().as_millis() >= 250 {
                    self.update_title();
                    self.last_title = Instant::now();
                }
                if let Some(gpu) = &mut self.gpu {
                    gpu.render();
                }
            }
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
