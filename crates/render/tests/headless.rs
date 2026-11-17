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
    // mirrored and not; and the fourth faded nine twentieths, as a corpse
    // decaying from its death's last frame is (`docs/look/units.md`,
    // "Memory").
    let (w, h) = (208u32, 64u32);
    let mut scene = Scene::default();
    for (i, (row, light, flip, fade)) in [
        (1, 255, false, 0),
        (2, 255, true, 0),
        (5, 128, false, 0),
        (3, 255, false, 9),
    ]
    .into_iter()
    .enumerate()
    {
        let mut s = view::SpriteInstance {
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
        };
        s.fade(fade);
        assert_eq!(s.palette_row(), row);
        scene.ui.push(s);
    }
    assert!((scene.ui[3].opacity() - 0.55).abs() < 1e-6);
    // Unorm, not sRGB, so the GPU writes and blends the bytes as the
    // rasteriser does. Twice: with the atlas the renderer was made with,
    // and with it uploaded in place of another, as a match that brings
    // other architectures' buildings does (`docs/look/buildings.md`,
    // "Memory").
    let format = wgpu::TextureFormat::Rgba8Unorm;
    for swapped in [false, true] {
        let mut renderer = if swapped {
            let mut r = Renderer::new(&device, &queue, format, &Atlas::placeholder());
            r.upload_atlas(&device, &queue, &atlas);
            r
        } else {
            Renderer::new(&device, &queue, format, &atlas)
        };
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
        // Compare the middle of every texel's 8 x 8 block, a step or two
        // of float rounding allowed.
        for i in 0..4 {
            for ty in 0..2 {
                for tx in 0..4 {
                    let (x, y) = (8 + i * 48 + tx * 8 + 4, 8 + ty * 8 + 4);
                    let g = gpu[(y * w + x) as usize];
                    let c = cpu.get(x as i32, y as i32);
                    assert!(
                        (0..3).all(|k| (g[k] as i32 - c[k] as i32).abs() <= 2),
                        "sprite {i} texel ({tx}, {ty}), atlas swapped {swapped}: \
                         GPU {g:?}, rasteriser {c:?}"
                    );
                }
            }
        }
        // And the owners differ where the mask is set.
        assert_ne!(cpu.get(12, 12), cpu.get(60 + 24, 12));
        // The faded one lets the ground through: its opaque cloth (texel
        // (0, 0)) is 55% itself, 45% the clear colour behind.
        let mut unfaded = scene.ui[3];
        unfaded.fade(0);
        let mut solid = view::raster::Image::new(w, h, clear);
        view::raster::draw_sprites(
            &mut solid,
            &camera,
            &atlas,
            &view::palette::texture(),
            &[unfaded],
        );
        let (x, y) = (8 + 3 * 48 + 4, 12);
        let (faded, solid) = (cpu.get(x, y), solid.get(x, y));
        for k in 0..3 {
            let want = clear[k] as f32 + (solid[k] as f32 - clear[k] as f32) * 0.55;
            assert!(
                (faded[k] as f32 - want).abs() <= 2.0,
                "{faded:?}, {solid:?} unfaded"
            );
        }
        assert_ne!(faded, solid);
    }
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

/// The new look's skinned UI (`docs/look/ui.md`) drawn by the GPU is what
/// the software rasteriser draws: the parchment panels, their bronze
/// frames and soft shadows, the baked type in its inks, the key caps and
/// a tooltip, all colour frames blended over what is behind. Over a
/// bright ground, so the shadows' and the glyphs' edges blend with
/// something; at a UI scale of 1.5, so the skin's device-pixel placement
/// (whole device pixels under a fractional scale) is held on both paths.
#[test]
fn the_skinned_hud_draws_as_the_rasteriser_draws_it() {
    let Some((device, queue)) = device() else {
        return;
    };
    let atlas = Atlas::with_sheets_skinned(&[], view::sprites::ATLAS_PAGE_HEIGHT, true);
    assert!(atlas.has_skin());
    let sim = Simulation::new(3, SimConfig::default());
    let selected: Vec<u32> = sim
        .world()
        .slots()
        .filter(|s| {
            sim.world().owner[s.index()] == 0
                && sim.world().kind[s.index()] == sim::kinds::TOWN_CENTER
        })
        .map(|s| s.index() as u32)
        .collect();
    assert_eq!(selected.len(), 1);
    let (w, h) = (960u32, 540u32);
    let s = 1.5;
    let camera = Camera::new(sim.map().width(), sim.map().height(), (w as f32, h as f32));
    let settings = view::Settings::default();
    let notices = [view::Notice {
        kind: view::NoticeKind::Attack,
        text: "YOUR TOWN IS UNDER ATTACK".into(),
        tile: None,
        tick: sim.tick(),
    }];
    let input = |hover: Option<(f32, f32)>| view::HudInput {
        sim: &sim,
        player: 0,
        camera: &camera,
        selected: &selected,
        build_mode: None,
        fps: 60.0,
        paused: false,
        speed: 1.0,
        status: "TICK 0",
        hover,
        banner: None,
        ui_scale: s,
        help: false,
        targeting: false,
        defences: false,
        settings: &settings,
        notices: &notices,
        hint: Some("VILLAGERS ARE IDLE"),
        narration: None,
        flash: [true, false, false, false],
        perf: None,
        fresh: &[],
    };
    // Hover the first button, for its tooltip and the hover style.
    let b = view::Hud::build(&atlas, &input(None)).buttons[0].clone();
    let hud = view::Hud::build(
        &atlas,
        &input(Some(((b.x + b.w * 0.5) * s, (b.y + b.h * 0.5) * s))),
    );
    let scene = Scene {
        ui: hud.sprites,
        ..Scene::default()
    };
    assert!(scene.ui.len() > 200, "{} sprites", scene.ui.len());

    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = Renderer::new(&device, &queue, format, &atlas);
    let clear = [204u8, 230, 153, 255];
    renderer.clear = wgpu::Color {
        r: 0.8,
        g: 0.9,
        b: 0.6,
        a: 1.0,
    };
    let target = target(&device, format, w, h);
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
    // Texel for texel, every edge on the same pixel: what is left is the
    // rounding of each blend, a step for each layer the skin stacks
    // (a shadow, a face, its sheen, the parchment's grain), so no pixel
    // more than three steps out, and most none.
    let (mut far, mut worst, mut drawn, mut sum) = (0usize, 0u32, 0usize, 0u64);
    for y in 0..h {
        for x in 0..w {
            let g = gpu[(y * w + x) as usize];
            let c = cpu.get(x as i32, y as i32);
            let d = (0..3)
                .map(|k| (g[k] as i32 - c[k] as i32).unsigned_abs())
                .max()
                .unwrap();
            worst = worst.max(d);
            sum += d as u64;
            far += usize::from(d > 3);
            drawn += usize::from(c[..3] != clear[..3]);
        }
    }
    assert!(
        drawn > (w * h / 5) as usize,
        "the HUD covers {drawn} pixels"
    );
    let mean = sum as f64 / (w * h) as f64;
    eprintln!("skinned HUD: mean channel delta {mean:.2}, worst {worst}");
    assert_eq!(far, 0, "{far} pixels differ by more than 3 (worst {worst})");
    assert!(mean < 1.0, "mean channel delta {mean:.2}");
}

/// The atmosphere (`docs/look/atmosphere.md`) drawn by the GPU looks as
/// the software rasteriser draws it: the backdrop past the map's edge, the
/// haze over ground never seen with its feathered edge, the memory of
/// ground seen once, the cloud shadows half a minute in, the glints on the
/// deep water and the grade, on the ground and on world sprites lit and
/// remembered; and a UI sprite over it all is drawn as it is without the
/// atmosphere, ungraded.
#[test]
fn the_atmosphere_draws_as_the_rasteriser_draws_it() {
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
                _ => sim::Terrain::ForestFloor,
            };
            map.set_terrain(x, y, t);
        }
    }
    for (x, y) in [(10, 6), (11, 6), (10, 7), (11, 7)] {
        map.set_corner(x, y, 1);
    }
    // Never seen past column 12; seen once from 6 to 11; in sight now
    // west of 6, the water and the map's western edges.
    let mut fog = sim::Fog::new(16, 16);
    for y in 0..16 {
        for x in 0..12 {
            fog.see(x, y);
        }
    }
    fog.clear_visible();
    for y in 0..16 {
        for x in 0..6 {
            fog.see(x, y);
        }
    }
    let lights = FogLights::from_fog(&fog);
    // Two colour frames, one in sight and one remembered, and the same
    // frame on the screen as UI.
    let dir = std::env::temp_dir().join(format!("ne-headless-atmo-{}", std::process::id()));
    let set = dir.join("house");
    std::fs::create_dir_all(&set).unwrap();
    let px: [[u8; 4]; 4] = [
        [230, 225, 215, 255],
        [180, 60, 40, 255],
        [40, 40, 60, 255],
        [90, 140, 200, 255],
    ];
    let png = |name: &str, colour: png::ColorType, data: &[u8]| {
        let file = std::fs::File::create(set.join(name)).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), 2, 2);
        enc.set_color(colour);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(data).unwrap();
    };
    png("house.png", png::ColorType::Rgba, px.as_flattened());
    png("house.mask.png", png::ColorType::Grayscale, &[0, 0, 0, 255]);
    std::fs::write(
        set.join("house.ron"),
        r#"SpriteSet(name: "house", class: SmallBuilding, scale: 1, palette: "rgba",
            sheet: "house.png", anchor: (1, 2),
            animations: [(name: "idle", frames: 1, frame_ms: 100, loops: true, impact: None)],
            colour: true, mask: Some("house.mask.png"), frame: Some((2, 2)))"#,
    )
    .unwrap();
    let sheet = view::sheets::Sheet::load(&set.join("house.ron")).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let atlas = Atlas::with_sheets(&[sheet]);
    let (f, _) = atlas.frame(sim::kinds::HOUSE, 0).unwrap();

    let (w, h) = (360u32, 240u32);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = Renderer::new(&device, &queue, format, &atlas);
    renderer.upload_ground_art(&device, &queue, &art);
    renderer.upload_map(&device, &queue, &map);
    renderer.upload_fog(&device, &queue, &lights);
    renderer.set_atmosphere(true);
    assert!(renderer.atmosphere());
    let time = 31.5;
    renderer.set_ground_time(time);
    let mut camera = Camera::new(16, 16, (w as f32, h as f32));
    camera.set_zoom_index(1);
    camera.look_at_tile(6.0, 6.0);
    let sprite = |x: f32, y: f32, light: u8, screen: bool| view::SpriteInstance {
        x,
        y,
        w: 24.0,
        h: 24.0,
        page: f.page,
        u: f.x,
        v: f.y,
        uw: f.w,
        vh: f.h,
        row: 1,
        flip: false,
        depth: 0.0,
        slot: u32::MAX,
        screen,
        light,
    };
    let (ax, ay) = view::iso::project(3.0, 6.0, 0.0);
    let (bx, by) = view::iso::project(8.0, 6.0, 0.0);
    let scene = Scene {
        sprites: vec![
            sprite(ax - 12.0, ay - 24.0, 255, false),
            sprite(bx - 12.0, by - 24.0, view::fog::EXPLORED, false),
        ],
        ui: vec![sprite(8.0, 8.0, 255, true)],
    };
    let gpu = {
        let target = target(&device, format, w, h);
        renderer.render(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            &camera,
            &scene,
            None,
        );
        read_back(&device, &queue, &target, w, h)
    };
    let types = view::ground::TypeMap::from_map(&map);
    let ground = view::ground::Ground {
        art: &art,
        types: &types,
    };
    let mut cpu = view::raster::Image::new(w, h, [0, 0, 0, 255]);
    view::raster::draw_backdrop(&mut cpu, &camera, Some(&lights), ground, time);
    view::raster::draw_ground_with(
        &mut cpu,
        &camera,
        &view::terrain::build_all_textured(&map),
        Some(&lights),
        ground,
        time,
        true,
    );
    let pal = view::palette::texture();
    let atmo = view::raster::Atmosphere {
        noise: &art.noise,
        time,
    };
    view::raster::draw_sprites_with(&mut cpu, &camera, &atlas, &pal, &scene.sprites, Some(atmo));
    view::raster::draw_sprites(&mut cpu, &camera, &atlas, &pal, &scene.ui);
    let (mut far, mut sum) = (0usize, 0u64);
    for y in 0..h {
        for x in 0..w {
            let g = gpu[(y * w + x) as usize];
            let c = cpu.get(x as i32, y as i32);
            let d = (0..3)
                .map(|k| (g[k] as i32 - c[k] as i32).unsigned_abs())
                .max()
                .unwrap();
            sum += d as u64;
            far += usize::from(d > 6);
        }
    }
    let total = (w * h) as usize;
    // For a look: `NE_HEADLESS_OUT=dir` writes both frames there.
    if let Some(out) = std::env::var_os("NE_HEADLESS_OUT") {
        let out = std::path::PathBuf::from(out);
        for (name, px) in [
            ("atmosphere_gpu.png", gpu.clone()),
            ("atmosphere_cpu.png", cpu.pixels.clone()),
        ] {
            let file = std::fs::File::create(out.join(name)).unwrap();
            let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header()
                .unwrap()
                .write_image_data(px.as_flattened())
                .unwrap();
        }
    }
    let mean = sum as f64 / total as f64;
    eprintln!("atmosphere: mean channel delta {mean:.2}, {far} of {total} over 6");
    assert!(mean < 1.5, "mean channel delta {mean:.2}");
    assert!(
        far * 100 < total,
        "{far} of {total} pixels differ by more than 6"
    );
    // The picture has all of it: haze (never seen), the backdrop past the
    // western edge, memory and sight.
    let at = |x: u32, y: u32| gpu[(y * w + x) as usize];
    let (sx, sy) = view::iso::project(12.5, 8.5, 0.0);
    let (hx, hy) = camera.to_window(sx, sy);
    let haze = at(hx as u32, hy as u32);
    assert!(haze[2] > haze[1] && haze[0] < 70, "haze: {haze:?}");
    // The UI sprite, texel for texel as without the atmosphere.
    let mut plain = view::raster::Image::new(w, h, [0, 0, 0, 255]);
    view::raster::draw_sprites(&mut plain, &camera, &atlas, &pal, &scene.ui);
    for (x, y) in [(14, 14), (26, 14), (14, 26), (26, 26)] {
        let (g, p) = (at(x, y), plain.get(x as i32, y as i32));
        assert!(
            (0..3).all(|k| (g[k] as i32 - p[k] as i32).abs() <= 1),
            "UI at {x},{y}: {g:?} vs {p:?}"
        );
    }
    // And off, the same renderer draws the ground as it did.
    renderer.set_atmosphere(false);
    let off = {
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
    let mut plain = view::raster::Image::new(w, h, [0, 0, 0, 255]);
    view::raster::draw_ground(
        &mut plain,
        &camera,
        &view::terrain::build_all_textured(&map),
        Some(&lights),
        ground,
        time,
    );
    let (hx, hy) = (hx as i32, hy as i32);
    assert_eq!(
        plain.get(hx, hy),
        [0, 0, 0, 255],
        "never seen is black without it"
    );
    assert!(off[(hy as u32 * w + hx as u32) as usize][..3] == [0, 0, 0]);
}
