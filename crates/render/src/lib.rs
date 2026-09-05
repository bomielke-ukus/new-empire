//! The wgpu renderer.
//!
//! Three pipelines, in draw order: terrain (Gouraud diamonds, chunked and
//! culled), sprites (one instanced draw, palette-indexed, depth-sorted on the
//! CPU by `view::Scene`), and UI quads (the minimap). All coordinate maths
//! comes from `view`, so what this draws is what `view::raster` draws.

use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use view::minimap::{Minimap, MinimapRect};
use view::{Atlas, Camera, ChunkMesh, Scene, TerrainVertex};
use wgpu::util::DeviceExt;

const COMMON: &str = include_str!("shaders/common.wgsl");
const TERRAIN: &str = include_str!("shaders/terrain.wgsl");
const SPRITES: &str = include_str!("shaders/sprites.wgsl");
const UI: &str = include_str!("shaders/ui.wgsl");

/// Full WGSL source for each pipeline, with the shared prelude.
pub fn shader_sources() -> [(&'static str, String); 3] {
    [
        ("terrain", format!("{COMMON}\n{TERRAIN}")),
        ("sprites", format!("{COMMON}\n{SPRITES}")),
        ("ui", format!("{COMMON}\n{UI}")),
    ]
}

/// One sprite instance as the GPU sees it.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SpriteGpu {
    rect: [f32; 4],
    uv: [f32; 4],
    misc: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct UiVertex {
    pos: [f32; 2],
    uv: [f32; 2],
}

struct ChunkGpu {
    bounds: (f32, f32, f32, f32),
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
}

/// Everything needed to draw frames.
pub struct Renderer {
    camera_buf: wgpu::Buffer,
    camera_bg: wgpu::BindGroup,
    terrain_pipeline: wgpu::RenderPipeline,
    sprite_pipeline: wgpu::RenderPipeline,
    sprite_bg: wgpu::BindGroup,
    ui_pipeline: wgpu::RenderPipeline,
    ui_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    chunks: HashMap<(i32, i32), ChunkGpu>,
    instances: wgpu::Buffer,
    instance_capacity: usize,
    ui_vertices: wgpu::Buffer,
    minimap: Option<(wgpu::Texture, wgpu::BindGroup, u32, u32)>,
    /// Clear colour.
    pub clear: wgpu::Color,
}

impl Renderer {
    /// Creates pipelines and uploads the atlas and palette.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        atlas: &Atlas,
    ) -> Renderer {
        let [terrain_src, sprites_src, ui_src] = shader_sources();
        let module = |label: &str, src: &str| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(src.into()),
            })
        };
        let terrain_mod = module("terrain", &terrain_src.1);
        let sprites_mod = module("sprites", &sprites_src.1);
        let ui_mod = module("ui", &ui_src.1);

        // Camera uniform: group 0 everywhere.
        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });

        // Atlas (R8Uint) and palette (RGBA8) for sprites: group 1.
        let atlas_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas"),
            size: wgpu::Extent3d {
                width: atlas.width,
                height: atlas.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        write_texture(
            queue,
            &atlas_tex,
            &atlas.indices,
            atlas.width,
            atlas.height,
            1,
        );
        let pal = view::palette::texture();
        let pal_bytes: Vec<u8> = pal.iter().flatten().copied().collect();
        let palette_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("palette"),
            size: wgpu::Extent3d {
                width: 256,
                height: view::palette::ROWS as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        write_texture(
            queue,
            &palette_tex,
            &pal_bytes,
            256,
            view::palette::ROWS as u32,
            4,
        );
        let sprite_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprites"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let sprite_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprites"),
            layout: &sprite_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &atlas_tex.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &palette_tex.create_view(&Default::default()),
                    ),
                },
            ],
        });

        // UI textures: group 1 for the ui pipeline.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let ui_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });

        let blend = Some(wgpu::BlendState::ALPHA_BLENDING);
        let target = |blend| {
            [Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })]
        };
        let primitive = wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        };

        let terrain_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain"),
            bind_group_layouts: &[&camera_bgl],
            push_constant_ranges: &[],
        });
        let terrain_targets = target(None);
        let terrain_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain"),
            layout: Some(&terrain_layout),
            vertex: wgpu::VertexState {
                module: &terrain_mod,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<TerrainVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Unorm8x4],
                }],
                compilation_options: Default::default(),
            },
            primitive,
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &terrain_mod,
                entry_point: Some("fs_main"),
                targets: &terrain_targets,
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        let sprite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprites"),
            bind_group_layouts: &[&camera_bgl, &sprite_bgl],
            push_constant_ranges: &[],
        });
        let sprite_targets = target(blend);
        let sprite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sprites"),
            layout: Some(&sprite_layout),
            vertex: wgpu::VertexState {
                module: &sprites_mod,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<SpriteGpu>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Uint32x4],
                }],
                compilation_options: Default::default(),
            },
            primitive,
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &sprites_mod,
                entry_point: Some("fs_main"),
                targets: &sprite_targets,
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        let ui_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui"),
            bind_group_layouts: &[&camera_bgl, &ui_bgl],
            push_constant_ranges: &[],
        });
        let ui_targets = target(blend);
        let ui_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&ui_layout),
            vertex: wgpu::VertexState {
                module: &ui_mod,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<UiVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
                }],
                compilation_options: Default::default(),
            },
            primitive,
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &ui_mod,
                entry_point: Some("fs_main"),
                targets: &ui_targets,
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        let instance_capacity = 4096;
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite instances"),
            size: (instance_capacity * std::mem::size_of::<SpriteGpu>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ui_vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui vertices"),
            size: (6 * std::mem::size_of::<UiVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Renderer {
            camera_buf,
            camera_bg,
            terrain_pipeline,
            sprite_pipeline,
            sprite_bg,
            ui_pipeline,
            ui_bgl,
            sampler,
            chunks: HashMap::new(),
            instances,
            instance_capacity,
            ui_vertices,
            minimap: None,
            clear: wgpu::Color {
                r: 0.05,
                g: 0.04,
                b: 0.06,
                a: 1.0,
            },
        }
    }

    /// Uploads (or replaces) terrain chunk geometry.
    pub fn upload_terrain(&mut self, device: &wgpu::Device, chunks: &[ChunkMesh]) {
        for c in chunks {
            let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("terrain vertices"),
                contents: bytemuck::cast_slice(&c.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("terrain indices"),
                contents: bytemuck::cast_slice(&c.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            self.chunks.insert(
                (c.cx, c.cy),
                ChunkGpu {
                    bounds: c.bounds,
                    vertices,
                    indices,
                    index_count: c.indices.len() as u32,
                },
            );
        }
    }

    /// Uploads a fresh minimap image.
    pub fn upload_minimap(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, m: &Minimap) {
        let bytes: Vec<u8> = m.pixels.iter().flatten().copied().collect();
        let needs_new =
            !matches!(&self.minimap, Some((_, _, w, h)) if *w == m.width && *h == m.height);
        if needs_new {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("minimap"),
                size: wgpu::Extent3d {
                    width: m.width,
                    height: m.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("minimap"),
                layout: &self.ui_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &tex.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.minimap = Some((tex, bg, m.width, m.height));
        }
        if let Some((tex, _, _, _)) = &self.minimap {
            write_texture(queue, tex, &bytes, m.width, m.height, 4);
        }
    }

    /// Draws one frame: terrain, sprites, then the minimap.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        camera: &Camera,
        scene: &Scene,
        minimap_rect: Option<MinimapRect>,
    ) {
        let u = camera.uniform();
        let cam = [u[0], u[1], u[2], 0.0, u[3], u[4], 0.0, 0.0];
        queue.write_buffer(&self.camera_buf, 0, bytemuck::cast_slice(&cam));

        // Sprites: cull to the view, then upload.
        let visible = camera.visible_rect();
        let gpu: Vec<SpriteGpu> = scene
            .sprites
            .iter()
            .filter(|s| {
                !(s.x + s.w < visible.0
                    || s.x > visible.2
                    || s.y + s.h < visible.1
                    || s.y > visible.3)
            })
            .map(|s| SpriteGpu {
                rect: [s.x, s.y, s.w, s.h],
                uv: [s.u as f32, s.v as f32, s.uw as f32, s.vh as f32],
                misc: [s.row as u32, s.flip as u32, 0, 0],
            })
            .collect();
        if gpu.len() > self.instance_capacity {
            self.instance_capacity = gpu.len().next_power_of_two();
            self.instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sprite instances"),
                size: (self.instance_capacity * std::mem::size_of::<SpriteGpu>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !gpu.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&gpu));
        }

        // Minimap quad.
        let draw_minimap = match (minimap_rect, &self.minimap) {
            (Some(rect), Some(_)) => {
                let c = rect.corners();
                let v = |i: usize| UiVertex {
                    pos: [c[i].0 .0, c[i].0 .1],
                    uv: [c[i].1 .0, c[i].1 .1],
                };
                let quad = [v(0), v(1), v(2), v(0), v(2), v(3)];
                queue.write_buffer(&self.ui_vertices, 0, bytemuck::cast_slice(&quad));
                true
            }
            _ => false,
        };

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.camera_bg, &[]);

            pass.set_pipeline(&self.terrain_pipeline);
            for chunk in self.chunks.values() {
                let (l, t, r, b) = chunk.bounds;
                if r < visible.0 || l > visible.2 || b < visible.1 || t > visible.3 {
                    continue;
                }
                pass.set_vertex_buffer(0, chunk.vertices.slice(..));
                pass.set_index_buffer(chunk.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..chunk.index_count, 0, 0..1);
            }

            if !gpu.is_empty() {
                pass.set_pipeline(&self.sprite_pipeline);
                pass.set_bind_group(1, &self.sprite_bg, &[]);
                pass.set_vertex_buffer(0, self.instances.slice(..));
                pass.draw(0..6, 0..gpu.len() as u32);
            }

            if draw_minimap {
                if let Some((_, bg, _, _)) = &self.minimap {
                    pass.set_pipeline(&self.ui_pipeline);
                    pass.set_bind_group(1, bg, &[]);
                    pass.set_vertex_buffer(0, self.ui_vertices.slice(..));
                    pass.draw(0..6, 0..1);
                }
            }
        }
        queue.submit(Some(encoder.finish()));
    }
}

fn write_texture(
    queue: &wgpu::Queue,
    tex: &wgpu::Texture,
    data: &[u8],
    width: u32,
    height: u32,
    bpp: u32,
) {
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(width * bpp),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shaders must parse and validate without a GPU. This is the only
    /// check the renderer gets in CI, so it is strict: every pipeline's
    /// source, entry points included.
    #[test]
    fn shaders_validate() {
        for (name, src) in shader_sources() {
            let module = naga::front::wgsl::parse_str(&src)
                .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&src)));
            let mut validator = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::default(),
            );
            validator
                .validate(&module)
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
            let entries: Vec<_> = module
                .entry_points
                .iter()
                .map(|e| e.name.as_str())
                .collect();
            assert!(
                entries.contains(&"vs_main") && entries.contains(&"fs_main"),
                "{name}: {entries:?}"
            );
        }
    }

    #[test]
    fn gpu_structs_match_vertex_layouts() {
        assert_eq!(std::mem::size_of::<SpriteGpu>(), 48);
        assert_eq!(std::mem::size_of::<UiVertex>(), 16);
        assert_eq!(std::mem::size_of::<TerrainVertex>(), 12);
    }
}
