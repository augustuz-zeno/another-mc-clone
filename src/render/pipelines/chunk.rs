use crate::render::mesh::Vertex;
pub const SHADER_SRC: &str = r"
// ── Shared camera uniform ────────────────────────────────────────────────────
struct CameraUniform {
    view_proj: mat4x4<f32>,
    world_position: vec4<f32>,
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
    @location(4) world_pos: vec3<f32>,
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    out.normal     = model.normal;
    out.uv         = model.uv;
    out.tex_index  = model.tex_index;
    out.tint       = model.tint;
    out.world_pos  = model.position;
    return out;
}

const GRASS_TINT: vec3<f32> = vec3<f32>(0.365, 0.659, 0.200);

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(t_diffuse, s_diffuse, in.uv, in.tex_index);
    if (tex_color.a < 0.1) { discard; }

    var rgb = tex_color.rgb;
    if (in.tint == 1u) {
        rgb = rgb * GRASS_TINT;
    }

    let light_dir = normalize(vec3<f32>(0.4, 1.0, 0.3));
    let ambient    = 0.35;
    let diffuse    = max(dot(in.normal, light_dir), 0.0) * 0.65;
    var shaded     = rgb * (ambient + diffuse);

    let sky_color = vec3<f32>(fog.sky_r, fog.sky_g, fog.sky_b);
    let dist = distance(in.world_pos, camera.world_position.xyz);
    let fog_factor = clamp((dist - fog.fog_start) / (fog.fog_end - fog.fog_start), 0.0, 1.0);
    shaded = mix(shaded, sky_color, fog_factor);

    return vec4<f32>(shaded, tex_color.a);
}
";

pub fn build_chunk_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    shader: &wgpu::ShaderModule,
    camera_bgl: &wgpu::BindGroupLayout,
    texture_bgl: &wgpu::BindGroupLayout,
    fog_bgl: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Chunk Pipeline Layout"),
        bind_group_layouts: &[camera_bgl, texture_bgl, fog_bgl],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Chunk Render Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[Vertex::desc()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
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
            format: crate::render::DepthTexture::DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}
