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

// ── Fog uniform ──────────────────────────────────────────────────────────────
struct FogUniform {
    fog_start:  f32,
    fog_end:    f32,
    sky_r:      f32,
    sky_g:      f32,
    sky_b:      f32,
    _pad:       f32,
};
@group(2) @binding(0)
var<uniform> fog: FogUniform;

// ── Chunk geometry ────────────────────────────────────────────────────────────
struct VertexInput {
    @location(0) position:  vec3<f32>,
    @location(1) normal:    vec3<f32>,
    @location(2) uv:        vec2<f32>,
    @location(3) tex_index: u32,
    @location(4) tint:      u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal:    vec3<f32>,
    @location(1) uv:        vec2<f32>,
    @location(2) @interpolate(flat) tex_index: u32,
    @location(3) @interpolate(flat) tint:      u32,
    @location(4) view_dist: f32,
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    out.normal     = model.normal;
    out.uv         = model.uv;
    out.tex_index  = model.tex_index;
    out.tint       = model.tint;
    // For fog: use clip-space W as an approximation of view-space depth
    out.view_dist  = out.clip_position.w;
    return out;
}

// Grass colormap colour (MC temperate biome): #5DA833 = (93/255, 168/255, 51/255)
const GRASS_TINT: vec3<f32> = vec3<f32>(0.365, 0.659, 0.200);

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(t_diffuse, s_diffuse, in.uv, in.tex_index);
    if (tex_color.a < 0.1) {
        discard;
    }

    // Apply grass-green colormap tint to grayscale grass textures
    var rgb = tex_color.rgb;
    if (in.tint == 1u) {
        rgb = rgb * GRASS_TINT;
    }

    // Simple directional + ambient lighting
    let light_dir = normalize(vec3<f32>(0.4, 1.0, 0.3));
    let ambient    = 0.35;
    let diffuse    = max(dot(in.normal, light_dir), 0.0) * 0.65;
    var shaded     = rgb * (ambient + diffuse);

    // Distance fog  (linear)
    let sky_color = vec3<f32>(fog.sky_r, fog.sky_g, fog.sky_b);
    let fog_factor = clamp((in.view_dist - fog.fog_start) / (fog.fog_end - fog.fog_start), 0.0, 1.0);
    shaded = mix(shaded, sky_color, fog_factor);

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

// Cloud shader is separate because fog is at group(1), not group(2)
// (cloud pipeline has: group 0 = camera, group 1 = fog — no texture array)
const CLOUD_SHADER_SRC: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct FogUniform {
    fog_start:  f32,
    fog_end:    f32,
    sky_r:      f32,
    sky_g:      f32,
    sky_b:      f32,
    _pad:       f32,
};
@group(1) @binding(0)
var<uniform> fog: FogUniform;

struct CloudVertex {
    @location(0) position: vec3<f32>,
};

struct CloudOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) view_dist: f32,
};

@vertex
fn vs_cloud(model: CloudVertex) -> CloudOutput {
    var out: CloudOutput;
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    out.view_dist     = out.clip_position.w;
    return out;
}

@fragment
fn fs_cloud(in: CloudOutput) -> @location(0) vec4<f32> {
    let sky_color  = vec3<f32>(fog.sky_r, fog.sky_g, fog.sky_b);
    let fog_factor = clamp((in.view_dist - fog.fog_start) / (fog.fog_end - fog.fog_start), 0.0, 1.0);
    let cloud_color = mix(vec3<f32>(1.0, 1.0, 1.0), sky_color, fog_factor);
    return vec4<f32>(cloud_color, 0.85);
}
"#;

// ══════════════════════════════════════════════════════════════════════════════
// CloudVertex — position-only for cloud geometry
// ══════════════════════════════════════════════════════════════════════════════
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CloudVertex {
    position: [f32; 3],
}

impl CloudVertex {
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
// FogUniform — sent to GPU each frame
// ══════════════════════════════════════════════════════════════════════════════
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FogUniform {
    fog_start: f32,
    fog_end:   f32,
    sky_r:     f32,
    sky_g:     f32,
    sky_b:     f32,
    _pad:      f32,
}

impl FogUniform {
    fn new(fog_start: f32, fog_end: f32) -> Self {
        Self {
            fog_start,
            fog_end,
            sky_r: 0.53,
            sky_g: 0.81,
            sky_b: 0.92,
            _pad:  0.0,
        }
    }
}

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
    pub cloud_pipeline: wgpu::RenderPipeline,

    pub chunk_meshes: std::collections::HashMap<glam::IVec2, ChunkMesh>,
    pub depth_texture: DepthTexture,
    pub texture_array: texture::TextureArray,

    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,

    // Fog uniform (used via GPU bind group, not read from Rust directly)
    #[allow(dead_code)]
    pub fog_buffer: wgpu::Buffer,
    pub fog_bind_group: wgpu::BindGroup,
    #[allow(dead_code)]
    pub fog_start: f32,
    #[allow(dead_code)]
    pub fog_end: f32,

    // Cloud state
    pub cloud_vertex_buffer: Option<wgpu::Buffer>,
    pub cloud_vertex_count: u32,
    pub cloud_offset: f32,     // drifts slowly over time (world units)

    // Block highlight wireframe (12 edges × 2 verts = 24 LineVertex)
    pub highlight_buffer: Option<wgpu::Buffer>,
    pub highlight_vertex_count: u32,

    pub hand_mesh: Option<ChunkMesh>,
    pub hand_block_id: u8,
    pub hand_camera_buffer: wgpu::Buffer,
    pub hand_camera_bind_group: wgpu::BindGroup,

    pub crosshair_pipeline: wgpu::RenderPipeline,
    pub crosshair_texture: texture::SingleTexture,
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
            "src/assets/textures/block/dirt.png",
            "src/assets/textures/block/grass_block_top.png",
            "src/assets/textures/block/grass_block_side.png",
            "src/assets/textures/block/stone.png",
        ]);

        // ── Fog uniform & bind group ──────────────────────────────────────────
        const RENDER_DIST_BLOCKS: f32 = 4.0 * 16.0; // render_distance * chunk_size
        let fog_start = RENDER_DIST_BLOCKS * 0.45;
        let fog_end   = RENDER_DIST_BLOCKS * 0.85;
        let fog_data  = FogUniform::new(fog_start, fog_end);
        let fog_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Fog Buffer"),
            contents: bytemuck::cast_slice(&[fog_data]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let fog_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Fog BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let fog_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Fog Bind Group"),
            layout: &fog_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: fog_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[&camera_bgl, &texture_array.bind_group_layout, &fog_bgl],
            push_constant_ranges: &[],
        });

        let line_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Line Pipeline Layout"),
            bind_group_layouts: &[&camera_bgl],
            push_constant_ranges: &[],
        });

        let cloud_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Cloud Pipeline Layout"),
            bind_group_layouts: &[&camera_bgl, &fog_bgl],
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

        // ── Cloud pipeline (semi-transparent TriangleList) ────────────────────
        let cloud_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Cloud Pipeline"),
            layout: Some(&cloud_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_cloud"),
                buffers: &[CloudVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_cloud"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,  // clouds visible from below
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DepthTexture::DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ── Crosshair pipeline ────────────────────────────────────────────────
        let crosshair_texture = texture::SingleTexture::new(&device, &queue, "src/assets/textures/gui/sprites/hud/crosshair.png");
        let crosshair_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Crosshair Shader"),
            source: wgpu::ShaderSource::Wgsl(CROSSHAIR_SHADER.into()),
        });
        let crosshair_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Crosshair Pipeline Layout"),
            bind_group_layouts: &[&crosshair_texture.bind_group_layout],
            push_constant_ranges: &[],
        });
        let crosshair_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Crosshair Pipeline"),
            layout: Some(&crosshair_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &crosshair_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &crosshair_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DepthTexture::DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
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
            cloud_pipeline,
            chunk_meshes: std::collections::HashMap::new(),
            depth_texture,
            texture_array,
            camera_buffer,
            camera_bind_group,
            fog_buffer,
            fog_bind_group,
            fog_start,
            fog_end,
            cloud_vertex_buffer: None,
            cloud_vertex_count: 0,
            cloud_offset: 0.0,
            highlight_buffer: None,
            highlight_vertex_count: 0,
            hand_mesh: None,
            hand_block_id: 0,
            hand_camera_buffer,
            hand_camera_bind_group,
            crosshair_pipeline,
            crosshair_texture,
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
            pass.set_bind_group(2, &self.fog_bind_group, &[]);
            for mesh in self.chunk_meshes.values() {
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.num_indices, 0, 0..1);
            }

            // Pass 2 — clouds (before highlight so they're affected by depth)
            if let Some(cloud_buf) = &self.cloud_vertex_buffer {
                pass.set_pipeline(&self.cloud_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_bind_group(1, &self.fog_bind_group, &[]);
                pass.set_vertex_buffer(0, cloud_buf.slice(..));
                pass.draw(0..self.cloud_vertex_count, 0..1);
            }

            // Pass 3 — block highlight wireframe (if any)
            if let Some(buf) = &self.highlight_buffer {
                pass.set_pipeline(&self.line_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..self.highlight_vertex_count, 0..1);
            }
        }

        // Pass 3 — Hand (overlay in a separate render pass to clear depth)
        {
            let mut pass2 = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Hand Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Keep the rendered world
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0), // Clear depth so hand renders on top
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if let Some(hand) = &self.hand_mesh {
                pass2.set_pipeline(&self.render_pipeline);
                pass2.set_bind_group(0, &self.hand_camera_bind_group, &[]);
                pass2.set_bind_group(1, &self.texture_array.bind_group, &[]);
                pass2.set_bind_group(2, &self.fog_bind_group, &[]);
                pass2.set_vertex_buffer(0, hand.vertex_buffer.slice(..));
                pass2.set_index_buffer(hand.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass2.draw_indexed(0..hand.num_indices, 0, 0..1);
            }

            // Crosshair
            pass2.set_pipeline(&self.crosshair_pipeline);
            pass2.set_bind_group(0, &self.crosshair_texture.bind_group, &[]);
            pass2.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }

    // ── Cloud update ───────────────────────────────────────────────────────────────

    /// Rebuild cloud geometry centered around `player_pos`.
    /// `dt` advances the slow cloud-drift animation.
    pub fn update_clouds(&mut self, player_pos: glam::Vec3, dt: f32) {
        // Drift clouds slowly in +X direction
        self.cloud_offset += dt * 2.0; // 2 blocks per second

        let cloud_y: f32 = 128.0; // cloud layer height
        // Cloud pattern: a 2D grid of patches — roughly like MC Beta
        // Each "patch" is one flat box (width=12, height=4, depth=12)
        const PATCH_W: f32 = 12.0;
        const PATCH_D: f32 = 12.0;
        const CLOUD_H: f32 = 4.0;
        const SPACING: f32 = 18.0; // gap between cloud centres
        const RADIUS:  i32  = 6;   // patches in each direction

        // Deterministic pattern using a simple hash
        let hash = |ix: i32, iz: i32| -> bool {
            let v = (ix.wrapping_mul(73856093)) ^ (iz.wrapping_mul(19349663));
            (v & 3) != 0  // 75% density
        };

        let mut verts: Vec<CloudVertex> = Vec::new();

        let cx = ((player_pos.x + self.cloud_offset) / SPACING).floor() as i32;
        let cz = (player_pos.z / SPACING).floor() as i32;

        for di in -RADIUS..=RADIUS {
            for dk in -RADIUS..=RADIUS {
                let pi = cx + di;
                let pk = cz + dk;
                if !hash(pi, pk) { continue; }

                let wx = pi as f32 * SPACING - self.cloud_offset;
                let wz = pk as f32 * SPACING;

                let x0 = wx - PATCH_W * 0.5;
                let x1 = wx + PATCH_W * 0.5;
                let z0 = wz - PATCH_D * 0.5;
                let z1 = wz + PATCH_D * 0.5;
                let y0 = cloud_y;
                let y1 = cloud_y + CLOUD_H;

                // Top face (two triangles)
                macro_rules! push_quad {
                    ($a:expr, $b:expr, $c:expr, $d:expr) => {{
                        verts.push(CloudVertex { position: $a });
                        verts.push(CloudVertex { position: $b });
                        verts.push(CloudVertex { position: $c });
                        verts.push(CloudVertex { position: $a });
                        verts.push(CloudVertex { position: $c });
                        verts.push(CloudVertex { position: $d });
                    }}
                }
                // Top
                push_quad!([x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]);
                // Bottom
                push_quad!([x0, y0, z1], [x0, y0, z0], [x1, y0, z0], [x1, y0, z1]);
                // Front (+z)
                push_quad!([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]);
                // Back (-z)
                push_quad!([x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]);
                // Left (-x)
                push_quad!([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]);
                // Right (+x)
                push_quad!([x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]);
            }
        }

        if verts.is_empty() {
            self.cloud_vertex_buffer = None;
            self.cloud_vertex_count = 0;
            return;
        }

        self.cloud_vertex_count = verts.len() as u32;
        self.cloud_vertex_buffer = Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cloud VB"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        }));
    }
}

const CROSSHAIR_SHADER: &str = r#"
struct CrosshairOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> CrosshairOutput {
    var x = f32(0.0);
    var y = f32(0.0);
    if (in_vertex_index == 1u || in_vertex_index == 4u || in_vertex_index == 5u) { x = 1.0; } else { x = -1.0; }
    if (in_vertex_index == 2u || in_vertex_index == 3u || in_vertex_index == 5u) { y = 1.0; } else { y = -1.0; }
    
    var out: CrosshairOutput;
    // Scale for crosshair (approx 16x16 on a 1280x720 screen)
    // 16 / 1280 = 0.0125
    // 16 / 720 = 0.0222
    out.clip_position = vec4<f32>(x * 0.0125, y * 0.0222, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@group(0) @binding(0) var t_crosshair: texture_2d<f32>;
@group(0) @binding(1) var s_crosshair: sampler;

@fragment
fn fs_main(in: CrosshairOutput) -> @location(0) vec4<f32> {
    let color = textureSample(t_crosshair, s_crosshair, in.uv);
    if (color.a < 0.1) { discard; }
    return color;
}
"#;
