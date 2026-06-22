use crate::world::chunk::{Chunk, CHUNK_SIZE};

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub tex_index: u32,
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        0 => Float32x3, // position
        1 => Float32x3, // normal
        2 => Float32x2, // uv
        3 => Uint32,    // tex_index
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub const TEX_DIRT: u32 = 0;
pub const TEX_GRASS_TOP: u32 = 1;
pub const TEX_GRASS_SIDE: u32 = 2;
pub const TEX_STONE: u32 = 3;

pub fn generate_mesh(chunk: &Chunk, chunk_coords: glam::IVec2) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let offset_x = (chunk_coords.x * CHUNK_SIZE as i32) as f32;
    let offset_z = (chunk_coords.y * CHUNK_SIZE as i32) as f32;

    // Traverse the chunk.
    for y in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let block_id = chunk.get_block(x as i32, y as i32, z as i32);
                if block_id == 0 {
                    continue; // Skip air
                }

                let px = x as f32 + offset_x;
                let py = y as f32;
                let pz = z as f32 + offset_z;

                let (tex_top, tex_side, tex_bottom) = match block_id {
                    1 => (TEX_DIRT, TEX_DIRT, TEX_DIRT),
                    2 => (TEX_GRASS_TOP, TEX_GRASS_SIDE, TEX_DIRT),
                    3 => (TEX_STONE, TEX_STONE, TEX_STONE),
                    _ => (TEX_DIRT, TEX_DIRT, TEX_DIRT), // fallback
                };

                // 1. Front face (+z)
                if chunk.get_block(x as i32, y as i32, z as i32 + 1) == 0 {
                    let t = tex_side;
                    let base_idx = vertices.len() as u32;
                    vertices.push(Vertex { position: [px, py, pz + 1.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 1.0], tex_index: t });
                    vertices.push(Vertex { position: [px + 1.0, py, pz + 1.0], normal: [0.0, 0.0, 1.0], uv: [1.0, 1.0], tex_index: t });
                    vertices.push(Vertex { position: [px + 1.0, py + 1.0, pz + 1.0], normal: [0.0, 0.0, 1.0], uv: [1.0, 0.0], tex_index: t });
                    vertices.push(Vertex { position: [px, py + 1.0, pz + 1.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 0.0], tex_index: t });
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                }

                // 2. Back face (-z)
                if chunk.get_block(x as i32, y as i32, z as i32 - 1) == 0 {
                    let t = tex_side;
                    let base_idx = vertices.len() as u32;
                    vertices.push(Vertex { position: [px + 1.0, py, pz], normal: [0.0, 0.0, -1.0], uv: [0.0, 1.0], tex_index: t });
                    vertices.push(Vertex { position: [px, py, pz], normal: [0.0, 0.0, -1.0], uv: [1.0, 1.0], tex_index: t });
                    vertices.push(Vertex { position: [px, py + 1.0, pz], normal: [0.0, 0.0, -1.0], uv: [1.0, 0.0], tex_index: t });
                    vertices.push(Vertex { position: [px + 1.0, py + 1.0, pz], normal: [0.0, 0.0, -1.0], uv: [0.0, 0.0], tex_index: t });
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                }

                // 3. Left face (-x)
                if chunk.get_block(x as i32 - 1, y as i32, z as i32) == 0 {
                    let t = tex_side;
                    let base_idx = vertices.len() as u32;
                    vertices.push(Vertex { position: [px, py, pz], normal: [-1.0, 0.0, 0.0], uv: [0.0, 1.0], tex_index: t });
                    vertices.push(Vertex { position: [px, py, pz + 1.0], normal: [-1.0, 0.0, 0.0], uv: [1.0, 1.0], tex_index: t });
                    vertices.push(Vertex { position: [px, py + 1.0, pz + 1.0], normal: [-1.0, 0.0, 0.0], uv: [1.0, 0.0], tex_index: t });
                    vertices.push(Vertex { position: [px, py + 1.0, pz], normal: [-1.0, 0.0, 0.0], uv: [0.0, 0.0], tex_index: t });
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                }

                // 4. Right face (+x)
                if chunk.get_block(x as i32 + 1, y as i32, z as i32) == 0 {
                    let t = tex_side;
                    let base_idx = vertices.len() as u32;
                    vertices.push(Vertex { position: [px + 1.0, py, pz + 1.0], normal: [1.0, 0.0, 0.0], uv: [0.0, 1.0], tex_index: t });
                    vertices.push(Vertex { position: [px + 1.0, py, pz], normal: [1.0, 0.0, 0.0], uv: [1.0, 1.0], tex_index: t });
                    vertices.push(Vertex { position: [px + 1.0, py + 1.0, pz], normal: [1.0, 0.0, 0.0], uv: [1.0, 0.0], tex_index: t });
                    vertices.push(Vertex { position: [px + 1.0, py + 1.0, pz + 1.0], normal: [1.0, 0.0, 0.0], uv: [0.0, 0.0], tex_index: t });
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                }

                // 5. Top face (+y)
                if chunk.get_block(x as i32, y as i32 + 1, z as i32) == 0 {
                    let t = tex_top;
                    let base_idx = vertices.len() as u32;
                    vertices.push(Vertex { position: [px, py + 1.0, pz], normal: [0.0, 1.0, 0.0], uv: [0.0, 1.0], tex_index: t });
                    vertices.push(Vertex { position: [px, py + 1.0, pz + 1.0], normal: [0.0, 1.0, 0.0], uv: [0.0, 0.0], tex_index: t });
                    vertices.push(Vertex { position: [px + 1.0, py + 1.0, pz + 1.0], normal: [0.0, 1.0, 0.0], uv: [1.0, 0.0], tex_index: t });
                    vertices.push(Vertex { position: [px + 1.0, py + 1.0, pz], normal: [0.0, 1.0, 0.0], uv: [1.0, 1.0], tex_index: t });
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                }

                // 6. Bottom face (-y)
                if chunk.get_block(x as i32, y as i32 - 1, z as i32) == 0 {
                    let t = tex_bottom;
                    let base_idx = vertices.len() as u32;
                    vertices.push(Vertex { position: [px, py, pz], normal: [0.0, -1.0, 0.0], uv: [0.0, 0.0], tex_index: t });
                    vertices.push(Vertex { position: [px + 1.0, py, pz], normal: [0.0, -1.0, 0.0], uv: [1.0, 0.0], tex_index: t });
                    vertices.push(Vertex { position: [px + 1.0, py, pz + 1.0], normal: [0.0, -1.0, 0.0], uv: [1.0, 1.0], tex_index: t });
                    vertices.push(Vertex { position: [px, py, pz + 1.0], normal: [0.0, -1.0, 0.0], uv: [0.0, 1.0], tex_index: t });
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                }
            }
        }
    }

    (vertices, indices)
}
