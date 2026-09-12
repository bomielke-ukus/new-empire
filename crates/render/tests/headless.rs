//! Renders one frame on whatever adapter wgpu can find.
//!
//! The shader test proves the WGSL parses; this proves the pipelines,
//! bind groups, textures and buffers are accepted by a real wgpu device and
//! that a frame comes out the other end. On a machine with no GPU it runs
//! on a software Vulkan driver (lavapipe) if one is installed, and skips
//! with a message if there is no adapter at all — so a machine that cannot
//! run it does not turn CI red, but one that can, exercises the GPU path.

use render::Renderer;
use sim::{SimConfig, Simulation};
use view::minimap::{Minimap, MinimapRect};
use view::{Atlas, Camera, Scene};

const W: u32 = 640;
const H: u32 = 360;

#[test]
fn a_frame_renders_on_a_real_device() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let Some(adapter) =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
    else {
        eprintln!("no wgpu adapter on this machine; skipping the headless render");
        return;
    };
    let info = adapter.get_info();
    eprintln!(
        "rendering on {} ({:?}, {:?})",
        info.name, info.backend, info.device_type
    );
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("headless"),
            required_features: wgpu::Features::empty(),
            // The same limits the app asks for, so a limit the app would
            // trip is tripped here first.
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .expect("request device");
    device.on_uncaptured_error(Box::new(|e| panic!("wgpu validation error: {e}")));

    let sim = Simulation::new(1, SimConfig::default());
    let atlas = Atlas::placeholder();
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut renderer = Renderer::new(&device, &queue, format, &atlas);
    renderer.upload_terrain(&device, &view::terrain::build_all(sim.map()));
    renderer.upload_minimap(&device, &queue, &Minimap::render(&sim));

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("frame"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut camera = Camera::new(sim.map().width(), sim.map().height(), (W as f32, H as f32));
    let (sx, sy) = sim.starts()[0];
    camera.look_at_tile(sx as f32 + 0.5, sy as f32 + 0.5);
    let scene = Scene::build(&sim, &atlas, None, 0.0);
    let rect = MinimapRect::bottom_right((W as f32, H as f32), 128.0, 8.0);
    renderer.render(&device, &queue, &view, &camera, &scene, Some(rect));

    // Read the frame back and make sure something was drawn.
    let bytes_per_row = (W * 4).div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (bytes_per_row * H) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &readback,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(H),
            },
        },
        wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::Maintain::Wait);
    rx.recv().unwrap().expect("map readback");
    let data = slice.get_mapped_range();
    let mut distinct = std::collections::BTreeSet::new();
    for y in 0..H as usize {
        let row = &data[y * bytes_per_row as usize..][..(W * 4) as usize];
        // `as_chunks::<4>().0` rather than `chunks_exact(4)`: same pixels,
        // and the form stable clippy asks for at a constant width.
        for px in row.as_chunks::<4>().0 {
            distinct.insert([px[0], px[1], px[2]]);
        }
    }
    assert!(
        distinct.len() > 16,
        "a rendered start position has terrain, trees and buildings in it; \
         only {} distinct colours came back",
        distinct.len()
    );
}
