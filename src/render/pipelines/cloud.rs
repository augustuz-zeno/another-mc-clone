use crate::render::cloud::CloudVertex;

pub const SHADER_SRC: &str = r"
struct CameraUniform {
    view_proj: mat4x4<f32>,
    world_pos: vec4<f32>,
};
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct FogUniform {
    fog_start: f32,
    fog_end:   f32,
    sky_r:     f32,
    sky_g:     f32,
    sky_b:     f32,
    _pad:      f32,
};
@group(1) @binding(0)
var<uniform> fog: FogUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color:    vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) world_pos: vec3<f32>,
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    out.color = model.color;
    out.world_pos = model.position;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let sky_color = vec3<f32>(fog.sky_r, fog.sky_g, fog.sky_b);
    let dist = distance(in.world_pos, camera.world_pos.xyz);
    let fog_factor = clamp((dist - fog.fog_start) / (fog.fog_end - fog.fog_start), 0.0, 1.0);

    let final_color = mix(in.color, sky_color, fog_factor);
    return vec4<f32>(final_color, 0.85); // slight transparency
}
";

pub fn build_cloud_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    shader: &wgpu::ShaderModule,
    camera_bgl: &wgpu::BindGroupLayout,
    fog_bgl: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Cloud Pipeline Layout"),
        bind_group_layouts: &[camera_bgl, fog_bgl],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Cloud Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[CloudVertex::desc()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
            depth_write_enabled: false, // Transparent, don't write depth
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}
