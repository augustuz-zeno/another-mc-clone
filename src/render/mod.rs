use std::sync::Arc;
use winit::window::Window;
use wgpu::util::DeviceExt;

pub mod camera;
pub mod mesh;
pub mod texture;

use crate::world::Chunk;
use camera::CameraUniform;
use mesh::{Vertex, generate_mesh};

// ══════════════════════════════════════════════════════════════════════════════
// Shader source — two entry-points:
//   vs_main / fs_main  →  chunk geometry (colored + lit)
//   vs_line / fs_line  →  block highlight wireframe (solid black)
// ══════════════════════════════════════════════════════════════════════════════
const SHADER_SRC: &str = r#"
// ── Shared camera uniform ────────────────────────────────────────────────────
struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var t_diffuse: texture_2d_array<f32>;
@group(1) @binding(1)
var s_diffuse: sampler;

// ── Chunk geometry ────────────────────────────────────────────────────────────
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal:   vec3<f32>,
    @location(2) uv:       vec2<f32>,
    @location(3) tex_index: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) tex_index: u32,
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    out.normal = model.normal;
    out.uv = model.uv;
    out.tex_index = model.tex_index;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(t_diffuse, s_diffuse, in.uv, in.tex_index);
    if (tex_color.a < 0.1) {
        discard;
    }

    let light_dir = normalize(vec3<f32>(0.4, 1.0, 0.3));
    let ambient   = 0.35;
    let diffuse   = max(dot(in.normal, light_dir), 0.0) * 0.65;
    let shaded    = tex_color.rgb * (ambient + diffuse);
    return vec4<f32>(shaded, tex_color.a);
}

// ── Block highlight wireframe ─────────────────────────────────────────────────
struct LineVertex {
    @location(0) position: vec3<f32>,
};

@vertex
fn vs_line(model: LineVertex) -> @builtin(position) vec4<f32> {
    return camera.view_proj * vec4<f32>(model.position, 1.0);
}

@fragment
fn fs_line() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0); // solid black edges
}
"#;

// ══════════════════════════════════════════════════════════════════════════════
// LineVertex — minimal format for wireframe lines
// ══════════════════════════════════════════════════════════════════════════════
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LineVertex {
    position: [f32; 3],
}

impl LineVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 1] =
        wgpu::vertex_attr_array![0 => Float32x3];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Depth texture helper
// ══════════════════════════════════════════════════════════════════════════════
pub struct DepthTexture {
    #[allow(dead_code)]
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}

impl DepthTexture {
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    pub fn create(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration, label: &str) -> Self {
        let size = wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self { texture, view }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Per-chunk GPU mesh
// ══════════════════════════════════════════════════════════════════════════════
pub struct ChunkMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
}

// ══════════════════════════════════════════════════════════════════════════════
// Main renderer state
// ══════════════════════════════════════════════════════════════════════════════
pub struct State {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    pub window: Arc<Window>,

    pub render_pipeline: wgpu::RenderPipeline,
    pub line_pipeline: wgpu::RenderPipeline,

    pub chunk_meshes: std::collections::HashMap<glam::IVec2, ChunkMesh>,
    pub depth_texture: DepthTexture,
    pub texture_array: texture::TextureArray,

    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,

    // Block highlight wireframe (12 edges × 2 verts = 24 LineVertex)
    pub highlight_buffer: Option<wgpu::Buffer>,
    pub highlight_vertex_count: u32,

    pub hand_mesh: Option<ChunkMesh>,
    pub hand_block_id: u8,
    pub hand_camera_buffer: wgpu::Buffer,
    pub hand_camera_bind_group: wgpu::BindGroup,
}

impl State {
    pub async fn new(window: Window) -> Self {
        let size = window.inner_size();
        let window = Arc::new(window);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }).await.unwrap();

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Voxel logical device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ).await.unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats.iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Voxel+Line Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(SHADER_SRC)),
        });

        // ── Camera uniform & bind group ──────────────────────────────────────
        let camera_uniform = CameraUniform::new();
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera BGL"),
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

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // ── Hand camera uniform & bind group ──────────────────────────────────
        let hand_camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Hand Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let hand_camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Hand Camera Bind Group"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: hand_camera_buffer.as_entire_binding(),
            }],
        });

        let depth_texture = DepthTexture::create(&device, &config, "Depth Texture");

        let texture_array = texture::TextureArray::new(&device, &queue, &[
            "src/assets/assets/minecraft/textures/block/dirt.png",
            "src/assets/assets/minecraft/textures/block/grass_block_top.png",
            "src/assets/assets/minecraft/textures/block/grass_block_side.png",
            "src/assets/assets/minecraft/textures/block/stone.png",
        ]);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[&camera_bgl, &texture_array.bind_group_layout],
            push_constant_ranges: &[],
        });

        let line_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Line Pipeline Layout"),
            bind_group_layouts: &[&camera_bgl],
            push_constant_ranges: &[],
        });

        // ── Chunk geometry pipeline (TriangleList) ────────────────────────────
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Voxel Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DepthTexture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ── Highlight wireframe pipeline (LineList) ───────────────────────────
        let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Line Render Pipeline"),
            layout: Some(&line_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_line"),
                buffers: &[LineVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_line"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            // Disable depth write so lines don't occlude block faces,
            // but still read depth to avoid drawing lines through terrain.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DepthTexture::DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: -2,  // pull lines slightly towards camera
                    slope_scale: -1.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            surface,
            device,
            queue,
            config,
            size,
            window,
            render_pipeline,
            line_pipeline,
            chunk_meshes: std::collections::HashMap::new(),
            depth_texture,
            texture_array,
            camera_buffer,
            camera_bind_group,
            highlight_buffer: None,
            highlight_vertex_count: 0,
            hand_mesh: None,
            hand_block_id: 0,
            hand_camera_buffer,
            hand_camera_bind_group,
        }
    }

    // ── Chunk mesh management ─────────────────────────────────────────────────

    pub fn add_chunk_mesh(&mut self, coords: glam::IVec2, chunk: &Chunk) {
        let (vertices, indices) = generate_mesh(chunk, coords);
        if vertices.is_empty() {
            self.chunk_meshes.remove(&coords);
            return;
        }
        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Chunk {:?} VB", coords)),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Chunk {:?} IB", coords)),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.chunk_meshes.insert(coords, ChunkMesh {
            vertex_buffer,
            index_buffer,
            num_indices: indices.len() as u32,
        });
    }

    pub fn remove_chunk_mesh(&mut self, coords: glam::IVec2) {
        self.chunk_meshes.remove(&coords);
    }

    // ── Block highlight ───────────────────────────────────────────────────────

    /// Update (or clear) the wireframe highlight around `block_pos`.
    pub fn set_highlight(&mut self, block_pos: Option<glam::IVec3>) {
        match block_pos {
            None => {
                self.highlight_buffer = None;
                self.highlight_vertex_count = 0;
            }
            Some(p) => {
                let verts = Self::build_highlight_verts(p);
                let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Highlight Buffer"),
                    contents: bytemuck::cast_slice(&verts),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                self.highlight_vertex_count = verts.len() as u32;
                self.highlight_buffer = Some(buffer);
            }
        }
    }

    /// Build 24 LineVertex (12 edges) for a slightly expanded cube around `p`.
    fn build_highlight_verts(p: glam::IVec3) -> Vec<LineVertex> {
        const E: f32 = 0.003; // expansion to avoid z-fighting
        let (x0, y0, z0) = (p.x as f32 - E, p.y as f32 - E, p.z as f32 - E);
        let (x1, y1, z1) = (p.x as f32 + 1.0 + E, p.y as f32 + 1.0 + E, p.z as f32 + 1.0 + E);

        // 8 corners of the cube
        let c = [
            [x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1], // bottom 0-3
            [x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1], // top    4-7
        ];

        // 12 edges expressed as pairs of corner indices
        let edges = [
            (0,1),(1,2),(2,3),(3,0), // bottom ring
            (4,5),(5,6),(6,7),(7,4), // top ring
            (0,4),(1,5),(2,6),(3,7), // vertical pillars
        ];

        edges.iter().flat_map(|&(a, b)| {
            [LineVertex { position: c[a] }, LineVertex { position: c[b] }]
        }).collect()
    }

    // ── Camera / resize ───────────────────────────────────────────────────────

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.depth_texture = DepthTexture::create(&self.device, &self.config, "Depth Texture");
        }
    }

    pub fn update_camera(&mut self, camera_uniform: &CameraUniform) {
        self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[*camera_uniform]));
    }

    pub fn update_hand_camera(&mut self, hand_uniform: &CameraUniform) {
        self.queue.write_buffer(&self.hand_camera_buffer, 0, bytemuck::cast_slice(&[*hand_uniform]));
    }

    pub fn set_hand_block(&mut self, block_id: u8) {
        if self.hand_block_id == block_id && self.hand_mesh.is_some() {
            return;
        }
        self.hand_block_id = block_id;
        
        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, block_id);
        let (vertices, indices) = generate_mesh(&chunk, glam::IVec2::ZERO);
        
        if vertices.is_empty() {
            self.hand_mesh = None;
            return;
        }

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Hand VB"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Hand IB"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.hand_mesh = Some(ChunkMesh {
            vertex_buffer,
            index_buffer,
            num_indices: indices.len() as u32,
        });
    }

    // ── Render frame ──────────────────────────────────────────────────────────

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Voxel Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.53, g: 0.81, b: 0.92, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Pass 1 — chunk geometry
            pass.set_pipeline(&self.render_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(1, &self.texture_array.bind_group, &[]);
            for mesh in self.chunk_meshes.values() {
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.num_indices, 0, 0..1);
            }

            // Pass 2 — block highlight wireframe (if any)
            if let Some(buf) = &self.highlight_buffer {
                pass.set_pipeline(&self.line_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..self.highlight_vertex_count, 0..1);
            }

            // Pass 3 — Hand (overlay)
            if let Some(hand) = &self.hand_mesh {
                pass.set_pipeline(&self.render_pipeline);
                pass.set_bind_group(0, &self.hand_camera_bind_group, &[]);
                // We keep the texture bind group the same
                pass.set_vertex_buffer(0, hand.vertex_buffer.slice(..));
                pass.set_index_buffer(hand.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..hand.num_indices, 0, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}
