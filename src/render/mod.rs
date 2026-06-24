#![allow(clippy::struct_field_names, clippy::too_many_lines, clippy::items_after_statements)]

use std::sync::Arc;
use winit::window::Window;
use wgpu::util::DeviceExt;

pub mod camera;
pub mod cloud;
pub mod highlight;
pub mod mesh;
pub mod pipelines;
pub mod texture;

use crate::world::Chunk;
use camera::CameraUniform;
use cloud::CloudState;
use highlight::HighlightState;
use mesh::generate_mesh;
use pipelines::{
    chunk::build_chunk_pipeline, cloud::build_cloud_pipeline,
    crosshair::build_crosshair_pipeline, line::build_line_pipeline,
};

// ══════════════════════════════════════════════════════════════════════════════
// Helpers for resource location
// ══════════════════════════════════════════════════════════════════════════════

/// Resolves an asset path robustly, trying `src/assets/...` and relative paths
/// so that running via `cargo run` or double-clicking the compiled `.exe` works.
pub fn asset_path(rel: &str) -> String {
    let rel_stripped = rel.trim_start_matches("src/");
    
    // 1. Try exact relative (works with `cargo run` if cwd is project root)
    let p1 = std::path::PathBuf::from(rel);
    if p1.exists() {
        return p1.to_string_lossy().into_owned();
    }
    
    // 2. Try relative to the executable (for double-clicking .exe)
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent() {
            // e.g. target/release/another-mc-clone.exe -> try target/release/assets/...
            let p2 = exe_dir.join(rel_stripped);
            if p2.exists() {
                return p2.to_string_lossy().into_owned();
            }
            
            // 3. Try climbing up to the project root
            if let Some(target_dir) = exe_dir.parent()
                && let Some(proj_dir) = target_dir.parent() {
                    let p3 = proj_dir.join(rel);
                    if p3.exists() {
                        return p3.to_string_lossy().into_owned();
                    }
                }
        }
    
    // Fallback: just return the original and let the panic show the path
    rel.to_string()
}

// ══════════════════════════════════════════════════════════════════════════════
// Internal uniforms & textures
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

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CrosshairParams {
    half_w: f32,
    half_h: f32,
    _pad0:  f32,
    _pad1:  f32,
}

impl CrosshairParams {
    const CROSSHAIR_PX: f32 = 16.0;

    #[allow(clippy::cast_precision_loss)]
    fn from_size(width: u32, height: u32) -> Self {
        Self {
            half_w: Self::CROSSHAIR_PX / width as f32,
            half_h: Self::CROSSHAIR_PX / height as f32,
            _pad0: 0.0,
            _pad1: 0.0,
        }
    }
}

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

    pub fog_bind_group: wgpu::BindGroup,
    _fog_buffer: wgpu::Buffer,

    pub cloud_state: CloudState,
    pub highlight_state: HighlightState,

    pub hand_mesh: Option<ChunkMesh>,
    pub hand_block_id: u8,
    pub hand_camera_buffer: wgpu::Buffer,
    pub hand_camera_bind_group: wgpu::BindGroup,

    pub crosshair_pipeline: wgpu::RenderPipeline,
    pub crosshair_texture: texture::SingleTexture,
    crosshair_params_buffer: wgpu::Buffer,
    pub crosshair_params_bind_group: wgpu::BindGroup,
}

impl State {
    pub async fn new(window: Window) -> Self {
        let size = window.inner_size();
        let window = Arc::new(window);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
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
            .copied().find(wgpu::TextureFormat::is_srgb).unwrap_or(surface_caps.formats[0]);

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

        // ── Shaders ───────────────────────────────────────────────────────────
        let chunk_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Chunk Shader"),
            source: wgpu::ShaderSource::Wgsl(pipelines::chunk::SHADER_SRC.into()),
        });
        let line_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Line Shader"),
            source: wgpu::ShaderSource::Wgsl(pipelines::line::SHADER_SRC.into()),
        });
        let cloud_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Cloud Shader"),
            source: wgpu::ShaderSource::Wgsl(pipelines::cloud::SHADER_SRC.into()),
        });
        let crosshair_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Crosshair Shader"),
            source: wgpu::ShaderSource::Wgsl(pipelines::crosshair::SHADER_SRC.into()),
        });

        // ── Camera uniform ────────────────────────────────────────────────────
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
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"), layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: camera_buffer.as_entire_binding() }],
        });

        // ── Hand camera ───────────────────────────────────────────────────────
        let hand_camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Hand Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let hand_camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Hand Camera Bind Group"), layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: hand_camera_buffer.as_entire_binding() }],
        });

        // ── Textures & Fog ────────────────────────────────────────────────────
        let depth_texture = DepthTexture::create(&device, &config, "Depth Texture");
        let texture_array = texture::TextureArray::new(&device, &queue, &[
            &asset_path("src/assets/textures/block/dirt.png"),
            &asset_path("src/assets/textures/block/grass_block_top.png"),
            &asset_path("src/assets/textures/block/grass_block_side.png"),
            &asset_path("src/assets/textures/block/stone.png"),
        ]);

        const RENDER_DIST_BLOCKS: f32 = 4.0 * 16.0;
        let fog_data = FogUniform::new(RENDER_DIST_BLOCKS * 0.45, RENDER_DIST_BLOCKS * 0.85);
        let fog_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Fog Buffer"), contents: bytemuck::cast_slice(&[fog_data]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let fog_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Fog BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0, visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let fog_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Fog Bind Group"), layout: &fog_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: fog_buffer.as_entire_binding() }],
        });

        // ── Crosshair ─────────────────────────────────────────────────────────
        let crosshair_texture = texture::SingleTexture::new(&device, &queue, &asset_path("src/assets/textures/gui/sprites/hud/crosshair.png"));
        let crosshair_params = CrosshairParams::from_size(size.width, size.height);
        let crosshair_params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Crosshair Params Buffer"), contents: bytemuck::cast_slice(&[crosshair_params]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let crosshair_params_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Crosshair Params BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0, visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let crosshair_params_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Crosshair Params BG"), layout: &crosshair_params_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: crosshair_params_buffer.as_entire_binding() }],
        });

        // ── Pipelines ─────────────────────────────────────────────────────────
        let render_pipeline    = build_chunk_pipeline(&device, config.format, &chunk_shader, &camera_bgl, &texture_array.bind_group_layout, &fog_bgl);
        let line_pipeline      = build_line_pipeline(&device, config.format, &line_shader, &camera_bgl);
        let cloud_pipeline     = build_cloud_pipeline(&device, config.format, &cloud_shader, &camera_bgl, &fog_bgl);
        let crosshair_pipeline = build_crosshair_pipeline(&device, config.format, &crosshair_shader, &crosshair_texture.bind_group_layout, &crosshair_params_bgl);

        // ── States ────────────────────────────────────────────────────────────
        let cloud_state = CloudState::new(&device);
        let highlight_state = HighlightState::new(&device);

        Self {
            surface, device, queue, config, size, window,
            render_pipeline, line_pipeline, cloud_pipeline,
            chunk_meshes: std::collections::HashMap::new(),
            depth_texture, texture_array,
            camera_buffer, camera_bind_group,
            fog_bind_group, _fog_buffer: fog_buffer,
            cloud_state, highlight_state,
            hand_mesh: None, hand_block_id: 0,
            hand_camera_buffer, hand_camera_bind_group,
            crosshair_pipeline, crosshair_texture,
            crosshair_params_buffer, crosshair_params_bind_group,
        }
    }

    // ── Mesh & Resize Management ──────────────────────────────────────────────

    pub fn add_chunk_mesh(&mut self, coords: glam::IVec2, chunk: &Chunk) {
        let (vertices, indices) = generate_mesh(chunk, coords);
        if vertices.is_empty() {
            self.chunk_meshes.remove(&coords);
            return;
        }
        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Chunk {coords:?} VB")), contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Chunk {coords:?} IB")), contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        #[allow(clippy::cast_possible_truncation)]
        self.chunk_meshes.insert(coords, ChunkMesh { vertex_buffer, index_buffer, num_indices: indices.len() as u32 });
    }

    pub fn remove_chunk_mesh(&mut self, coords: glam::IVec2) {
        self.chunk_meshes.remove(&coords);
    }

    pub fn set_highlight(&mut self, block_pos: Option<glam::IVec3>) {
        self.highlight_state.set(&self.queue, block_pos);
    }

    pub fn update_clouds(&mut self, player_pos: glam::Vec3, dt: f32) {
        self.cloud_state.update(&self.queue, player_pos, dt);
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.depth_texture = DepthTexture::create(&self.device, &self.config, "Depth Texture");

            let params = CrosshairParams::from_size(new_size.width, new_size.height);
            self.queue.write_buffer(&self.crosshair_params_buffer, 0, bytemuck::cast_slice(&[params]));
        }
    }

    pub fn update_camera(&mut self, camera_uniform: &CameraUniform) {
        self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[*camera_uniform]));
    }

    pub fn update_hand_camera(&mut self, hand_uniform: &CameraUniform) {
        self.queue.write_buffer(&self.hand_camera_buffer, 0, bytemuck::cast_slice(&[*hand_uniform]));
    }

    pub fn set_hand_block(&mut self, block_id: u8) {
        if self.hand_block_id == block_id && self.hand_mesh.is_some() { return; }
        self.hand_block_id = block_id;

        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, block_id);
        let (vertices, indices) = generate_mesh(&chunk, glam::IVec2::ZERO);

        if vertices.is_empty() {
            self.hand_mesh = None;
            return;
        }

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Hand VB"), contents: bytemuck::cast_slice(&vertices), usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Hand IB"), contents: bytemuck::cast_slice(&indices), usage: wgpu::BufferUsages::INDEX,
        });
        #[allow(clippy::cast_possible_truncation)]
        { self.hand_mesh = Some(ChunkMesh { vertex_buffer, index_buffer, num_indices: indices.len() as u32 }); }
    }

    // ── Render loop ───────────────────────────────────────────────────────────

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") });

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
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
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

            // Pass 2 — clouds
            if self.cloud_state.vertex_count > 0 {
                pass.set_pipeline(&self.cloud_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_bind_group(1, &self.fog_bind_group, &[]);
                pass.set_vertex_buffer(0, self.cloud_state.vertex_buffer.slice(..));
                pass.draw(0..self.cloud_state.vertex_count, 0..1);
            }

            // Pass 3 — block highlight
            if self.highlight_state.vertex_count > 0 {
                pass.set_pipeline(&self.line_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.highlight_state.buffer.slice(..));
                pass.draw(0..self.highlight_state.vertex_count, 0..1);
            }
        }

        // Pass 4 — Hand (clears depth so it renders over everything)
        {
            let mut pass2 = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Hand Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
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
            pass2.set_bind_group(1, &self.crosshair_params_bind_group, &[]);
            pass2.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}
