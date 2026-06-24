pub const SHADER_SRC: &str = r"
struct CrosshairOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct CrosshairParams {
    half_w: f32,
    half_h: f32,
    _pad0:  f32,
    _pad1:  f32,
};

@group(1) @binding(0) var<uniform> params: CrosshairParams;

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> CrosshairOutput {
    var x = f32(0.0);
    var y = f32(0.0);
    if (in_vertex_index == 1u || in_vertex_index == 4u || in_vertex_index == 5u) { x = 1.0; } else { x = -1.0; }
    if (in_vertex_index == 2u || in_vertex_index == 3u || in_vertex_index == 5u) { y = 1.0; } else { y = -1.0; }

    var out: CrosshairOutput;
    out.clip_position = vec4<f32>(x * params.half_w, y * params.half_h, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@group(0) @binding(0) var t_crosshair: texture_2d<f32>;
@group(0) @binding(1) var s_crosshair: sampler;

@fragment
fn fs_main(in: CrosshairOutput) -> @location(0) vec4<f32> {
    let color = textureSample(t_crosshair, s_crosshair, in.uv);
    if (color.a < 0.1) { discard; }
    // Invert the pixels under the crosshair to ensure visibility against any background.
    // The texture itself is white (#FFFFFF). Using OneMinusDst for blending with a white source
    // achieves `1.0 - dst`, effectively inverting it.
    return color;
}
";

pub fn build_crosshair_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    shader: &wgpu::ShaderModule,
    texture_bgl: &wgpu::BindGroupLayout,
    params_bgl: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Crosshair Pipeline Layout"),
        bind_group_layouts: &[texture_bgl, params_bgl],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Crosshair Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[], // generated from vertex_index
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::OneMinusDst,
                        dst_factor: wgpu::BlendFactor::Zero,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent::REPLACE,
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::render::DepthTexture::DEPTH_FORMAT,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}
