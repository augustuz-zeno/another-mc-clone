//! Block highlight wireframe state.
//!
//! Manages a pre-allocated GPU vertex buffer for the 12-edge selection cube.
//! Callers call `set()` each frame — zero GPU allocations, only `write_buffer`.

/// Minimal vertex used for wireframe lines.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineVertex {
    pub position: [f32; 3],
}

impl LineVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Owns the GPU buffer for the block-selection wireframe.
pub struct HighlightState {
    /// Pre-allocated buffer sized for 24 `LineVertex` (12 edges of a cube).
    pub buffer: wgpu::Buffer,
    /// Number of vertices to draw this frame (0 = nothing highlighted).
    pub vertex_count: u32,
}

impl HighlightState {
    /// Allocate the buffer. No vertices are drawn until `set()` is called.
    pub fn new(device: &wgpu::Device) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Highlight Buffer"),
            size: (24 * std::mem::size_of::<LineVertex>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { buffer, vertex_count: 0 }
    }

    /// Update the wireframe for `block_pos`, or clear it when `None`.
    pub fn set(&mut self, queue: &wgpu::Queue, block_pos: Option<glam::IVec3>) {
        match block_pos {
            None => {
                self.vertex_count = 0;
            }
            Some(p) => {
                let verts = Self::build_verts(p);
                queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&verts));
                #[allow(clippy::cast_possible_truncation)]
                { self.vertex_count = verts.len() as u32; }
            }
        }
    }

    /// Build 24 `LineVertex` (12 edges) for a slightly expanded cube around `p`.
    fn build_verts(p: glam::IVec3) -> Vec<LineVertex> {
        const E: f32 = 0.003; // expansion to avoid z-fighting
        #[allow(clippy::cast_precision_loss)]
        let (x0, y0, z0) = (p.x as f32 - E, p.y as f32 - E, p.z as f32 - E);
        #[allow(clippy::cast_precision_loss)]
        let (x1, y1, z1) = (
            p.x as f32 + 1.0 + E,
            p.y as f32 + 1.0 + E,
            p.z as f32 + 1.0 + E,
        );

        // 8 corners
        let c = [
            [x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1], // bottom
            [x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1], // top
        ];
        // 12 edges as index pairs
        let edges = [
            (0,1),(1,2),(2,3),(3,0), // bottom ring
            (4,5),(5,6),(6,7),(7,4), // top ring
            (0,4),(1,5),(2,6),(3,7), // pillars
        ];
        edges
            .iter()
            .flat_map(|&(a, b)| [LineVertex { position: c[a] }, LineVertex { position: c[b] }])
            .collect()
    }
}
