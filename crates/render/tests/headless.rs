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
use view::{Atlas, Camera, FogLights, Scene, SceneOptions};

const W: u32 = 640;
const H: u32 = 360;

/// A device on whatever adapter wgpu can find, with the limits the app
/// asks for, or `None` (and a message) on a machine with none.
fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let Some(adapter) =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
    else {
        eprintln!("no wgpu adapter on this machine; skipping the headless render");
        return None;
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
    Some((device, queue))
}

/// A `w` x `h` render target of `format`.
fn target(device: &wgpu::Device, format: wgpu::TextureFormat, w: u32, h: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("frame"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Reads a `w` x `h` RGBA8 target back, row after row.
fn read_back(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &wgpu::Texture,
    w: u32,
    h: u32,
) -> Vec<[u8; 4]> {
    let bytes_per_row = (w * 4).div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (bytes_per_row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &readback,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
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
    let mut out = Vec::with_capacity((w * h) as usize);
    for y in 0..h as usize {
        let row = &data[y * bytes_per_row as usize..][..(w * 4) as usize];
        // `as_chunks::<4>().0` rather than `chunks_exact(4)`: same pixels,
        // and the form stable clippy asks for at a constant width.
        out.extend(row.as_chunks::<4>().0.iter().copied());
    }
    out
}

#[test]
fn a_frame_renders_on_a_real_device() {
    let Some((device, queue)) = device() else {
        return;
    };
    let sim = Simulation::new(1, SimConfig::default());
    let atlas = Atlas::placeholder();
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut renderer = Renderer::new(&device, &queue, format, &atlas);
    renderer.upload_terrain(&device, &view::terrain::build_all(sim.map()));
    renderer.upload_minimap(&device, &queue, &Minimap::render_for(&sim, Some(0)));
    renderer.upload_fog(&device, &queue, &FogLights::from_fog(sim.fog(0).unwrap()));

    let target = target(&device, format, W, H);
    let view = target.create_view(&Default::default());
    let mut camera = Camera::new(sim.map().width(), sim.map().height(), (W as f32, H as f32));
    let (sx, sy) = sim.starts()[0];
    camera.look_at_tile(sx as f32 + 0.5, sy as f32 + 0.5);
    let scene = Scene::build_full(
        &sim,
        &atlas,
        None,
        0.0,
        &SceneOptions {
            viewer: Some(0),
            ..SceneOptions::default()
        },
    );
    let rect = MinimapRect::bottom_right((W as f32, H as f32), 128.0, 8.0);
    renderer.render(&device, &queue, &view, &camera, &scene, Some(rect));

    // Read the frame back and make sure something was drawn.
    let distinct: std::collections::BTreeSet<[u8; 3]> = read_back(&device, &queue, &target, W, H)
        .iter()
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    assert!(
        distinct.len() > 16,
        "a rendered start position has terrain, trees and buildings in it; \
         only {} distinct colours came back",
        distinct.len()
    );
}

/// A colour set's frame (`docs/look/colour-sets.md`) drawn by the GPU is
/// what the software rasteriser draws: the owner's colour through the
/// mask, alpha over the clear colour, the fog's light, mirrored and not.
#[test]
fn a_colour_frame_draws_as_the_rasteriser_draws_it() {
    let Some((device, queue)) = device() else {
        return;
    };
    // A 4 x 2 house: cloth, red, a shadow and half-masked grey over a row
    // of green and cloth.
    let dir = std::env::temp_dir().join(format!("ne-headless-look-{}", std::process::id()));
    let set = dir.join("house");
    std::fs::create_dir_all(&set).unwrap();
    let px: [[u8; 4]; 8] = [
        [128, 128, 128, 255],
        [200, 30, 20, 255],
        [0, 0, 0, 100],
        [200, 200, 200, 255],
        [20, 160, 40, 255],
        [60, 60, 60, 255],
        [250, 250, 250, 255],
        [10, 20, 200, 160],
    ];
    let mask = [255u8, 0, 0, 128, 0, 255, 255, 0];
    let png = |name: &str, colour: png::ColorType, data: &[u8]| {
        let file = std::fs::File::create(set.join(name)).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), 4, 2);
        enc.set_color(colour);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(data).unwrap();
    };
    png("house.png", png::ColorType::Rgba, px.as_flattened());
    png("house.mask.png", png::ColorType::Grayscale, &mask);
    std::fs::write(
        set.join("house.ron"),
        r#"SpriteSet(name: "house", class: SmallBuilding, scale: 1, palette: "rgba",
            sheet: "house.png", anchor: (2, 2),
            animations: [(name: "idle", frames: 1, frame_ms: 100, loops: true, impact: None)],
            colour: true, mask: Some("house.mask.png"), frame: Some((4, 2)))"#,
    )
    .unwrap();
    let sheet = view::sheets::Sheet::load(&set.join("house.ron")).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let atlas = Atlas::with_sheets(&[sheet]);
    let (f, _) = atlas.frame(sim::kinds::HOUSE, 0).unwrap();
    assert_eq!(f.page, view::sprites::COLOUR_PAGE);

    // Each texel 8 x 8 on screen, for three owners, lit and dimmed,
    // mirrored and not.
    let (w, h) = (160u32, 64u32);
    let mut scene = Scene::default();
    for (i, (row, light, flip)) in [(1, 255, false), (2, 255, true), (5, 128, false)]
        .into_iter()
        .enumerate()
    {
        scene.ui.push(view::SpriteInstance {
            x: 8.0 + i as f32 * 48.0,
            y: 8.0,
            w: 32.0,
            h: 16.0,
            page: f.page,
            u: f.x,
            v: f.y,
            uw: f.w,
            vh: f.h,
            row,
            flip,
            depth: 0.0,
            slot: u32::MAX,
            screen: true,
            light,
        });
    }
    // Unorm, not sRGB, so the GPU writes and blends the bytes as the
    // rasteriser does.
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = Renderer::new(&device, &queue, format, &atlas);
    let clear = [51u8, 77, 102, 255];
    renderer.clear = wgpu::Color {
        r: 0.2,
        g: 0.3,
        b: 0.4,
        a: 1.0,
    };
    let target = target(&device, format, w, h);
    let camera = Camera::new(16, 16, (w as f32, h as f32));
    renderer.render(
        &device,
        &queue,
        &target.create_view(&Default::default()),
        &camera,
        &scene,
        None,
    );
    let gpu = read_back(&device, &queue, &target, w, h);
    let mut cpu = view::raster::Image::new(w, h, clear);
    view::raster::draw_sprites(
        &mut cpu,
        &camera,
        &atlas,
        &view::palette::texture(),
        &scene.ui,
    );
    // Compare the middle of every texel's 8 x 8 block, a step or two of
    // float rounding allowed.
    for i in 0..3 {
        for ty in 0..2 {
            for tx in 0..4 {
                let (x, y) = (8 + i * 48 + tx * 8 + 4, 8 + ty * 8 + 4);
                let g = gpu[(y * w + x) as usize];
                let c = cpu.get(x as i32, y as i32);
                assert!(
                    (0..3).all(|k| (g[k] as i32 - c[k] as i32).abs() <= 2),
                    "sprite {i} texel ({tx}, {ty}): GPU {g:?}, rasteriser {c:?}"
                );
            }
        }
    }
    // And the owners differ where the mask is set.
    assert_ne!(cpu.get(12, 12), cpu.get(60 + 24, 12));
}

/// The ground painted from its textures (`docs/look/ground.md`) by the GPU
/// looks as the software rasteriser paints it: a patch of meadow, beach,
/// shallows and sea on a slope, drifting and foaming a few seconds in,
/// half of it seen once and dimmed. Not bit for bit (the GPU's filtering
/// rounds its own way) but within a few steps on almost every pixel.
#[test]
fn the_textured_ground_draws_as_the_rasteriser_draws_it() {
    let Some((device, queue)) = device() else {
        return;
    };
    let art = view::ground::GroundArt::load(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/look/terrain"),
    )
    .unwrap();
    let mut map = sim::TileMap::new(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            let t = match x + y / 3 {
                0..=4 => sim::Terrain::DeepWater,
                5..=6 => sim::Terrain::ShallowWater,
                7..=8 => sim::Terrain::Sand,
                9..=11 => sim::Terrain::Grass,
                12 => sim::Terrain::Dirt,
                _ => sim::Terrain::ForestFloor,
            };
            map.set_terrain(x, y, t);
        }
    }
    // A hill on the meadow, lit on one side and shaded on the other.
    for (x, y) in [(10, 6), (11, 6), (10, 7), (11, 7)] {
        map.set_corner(x, y, 1);
    }
    let mut fog = sim::Fog::new(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            fog.see(x, y);
        }
    }
    fog.clear_visible();
    for y in 0..16 {
        for x in 0..8 {
            fog.see(x, y);
        }
    }
    let lights = FogLights::from_fog(&fog);
    let (w, h) = (320u32, 200u32);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = Renderer::new(&device, &queue, format, &Atlas::placeholder());
    renderer.upload_ground_art(&device, &queue, &art);
    assert!(renderer.textured_ground());
    renderer.upload_map(&device, &queue, &map);
    renderer.upload_fog(&device, &queue, &lights);
    let time = 3.25;
    renderer.set_ground_time(time);
    // At 2x, a whole level of the textures; at 0.75x, between two.
    for zoom in [4, 1] {
        let mut camera = Camera::new(16, 16, (w as f32, h as f32));
        camera.set_zoom_index(zoom);
        camera.look_at_tile(8.0, 7.0);
        let gpu = {
            let target = target(&device, format, w, h);
            renderer.render(
                &device,
                &queue,
                &target.create_view(&Default::default()),
                &camera,
                &Scene::default(),
                None,
            );
            read_back(&device, &queue, &target, w, h)
        };
        let mut cpu = view::raster::Image::new(w, h, [0, 0, 0, 255]);
        let types = view::ground::TypeMap::from_map(&map);
        view::raster::draw_ground(
            &mut cpu,
            &camera,
            &view::terrain::build_all_textured(&map),
            Some(&lights),
            view::ground::Ground {
                art: &art,
                types: &types,
            },
            time,
        );
        let (mut far, mut total, mut sum) = (0usize, 0usize, 0u64);
        for y in 0..h {
            for x in 0..w {
                let g = gpu[(y * w + x) as usize];
                let c = cpu.get(x as i32, y as i32);
                let d = (0..3)
                    .map(|k| (g[k] as i32 - c[k] as i32).unsigned_abs())
                    .max()
                    .unwrap();
                total += 1;
                sum += d as u64;
                if d > 6 {
                    far += 1;
                }
            }
        }
        let mean = sum as f64 / total as f64;
        eprintln!("ground at zoom {zoom}: mean channel delta {mean:.2}, {far} of {total} over 6");
        assert!(mean < 1.5, "zoom {zoom}: mean channel delta {mean:.2}");
        assert!(
            far * 100 < total,
            "zoom {zoom}: {far} of {total} pixels differ by more than 6"
        );
        // And it is the ground: many colours.
        let distinct: std::collections::BTreeSet<[u8; 4]> = gpu.iter().copied().collect();
        assert!(distinct.len() > 1000, "{} colours", distinct.len());
    }
}
