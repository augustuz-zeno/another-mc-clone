use crate::world::chunk::{Chunk, CHUNK_SIZE};

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub normal: [f32; 3],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x3, // position
        1 => Float32x3, // color
        2 => Float32x3, // normal
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub fn generate_mesh(chunk: &Chunk, chunk_coords: glam::IVec2) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let offset_x = (chunk_coords.x * CHUNK_SIZE as i32) as f32;
    let offset_z = (chunk_coords.y * CHUNK_SIZE as i32) as f32;

    let dirt_color = [0.45, 0.3, 0.15];
    let grass_color = [0.18, 0.62, 0.18];

    // Traverse the chunk.
    // Iterating y outer, z middle, x inner aligns with index layout coords_to_index.
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

                let is_grass = block_id == 2;

                // For each of the 6 faces, check the neighbor block.
                // If the neighbor is air (or off-chunk), generate the face.

                // 1. Front face (+z)
                if chunk.get_block(x as i32, y as i32, z as i32 + 1) == 0 {
                    let color = dirt_color; // Side of grass is dirt
                    let base_idx = vertices.len() as u32;
                    vertices.push(Vertex { position: [px, py, pz + 1.0], color, normal: [0.0, 0.0, 1.0] });
                    vertices.push(Vertex { position: [px + 1.0, py, pz + 1.0], color, normal: [0.0, 0.0, 1.0] });
                    vertices.push(Vertex { position: [px + 1.0, py + 1.0, pz + 1.0], color, normal: [0.0, 0.0, 1.0] });
                    vertices.push(Vertex { position: [px, py + 1.0, pz + 1.0], color, normal: [0.0, 0.0, 1.0] });
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                }

                // 2. Back face (-z)
                if chunk.get_block(x as i32, y as i32, z as i32 - 1) == 0 {
                    let color = dirt_color;
                    let base_idx = vertices.len() as u32;
                    vertices.push(Vertex { position: [px + 1.0, py, pz], color, normal: [0.0, 0.0, -1.0] });
                    vertices.push(Vertex { position: [px, py, pz], color, normal: [0.0, 0.0, -1.0] });
                    vertices.push(Vertex { position: [px, py + 1.0, pz], color, normal: [0.0, 0.0, -1.0] });
                    vertices.push(Vertex { position: [px + 1.0, py + 1.0, pz], color, normal: [0.0, 0.0, -1.0] });
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                }

                // 3. Left face (-x)
                if chunk.get_block(x as i32 - 1, y as i32, z as i32) == 0 {
                    let color = dirt_color;
                    let base_idx = vertices.len() as u32;
                    vertices.push(Vertex { position: [px, py, pz], color, normal: [-1.0, 0.0, 0.0] });
                    vertices.push(Vertex { position: [px, py, pz + 1.0], color, normal: [-1.0, 0.0, 0.0] });
                    vertices.push(Vertex { position: [px, py + 1.0, pz + 1.0], color, normal: [-1.0, 0.0, 0.0] });
                    vertices.push(Vertex { position: [px, py + 1.0, pz], color, normal: [-1.0, 0.0, 0.0] });
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                }

                // 4. Right face (+x)
                if chunk.get_block(x as i32 + 1, y as i32, z as i32) == 0 {
                    let color = dirt_color;
                    let base_idx = vertices.len() as u32;
                    vertices.push(Vertex { position: [px + 1.0, py, pz + 1.0], color, normal: [1.0, 0.0, 0.0] });
                    vertices.push(Vertex { position: [px + 1.0, py, pz], color, normal: [1.0, 0.0, 0.0] });
                    vertices.push(Vertex { position: [px + 1.0, py + 1.0, pz], color, normal: [1.0, 0.0, 0.0] });
                    vertices.push(Vertex { position: [px + 1.0, py + 1.0, pz + 1.0], color, normal: [1.0, 0.0, 0.0] });
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                }

                // 5. Top face (+y)
                if chunk.get_block(x as i32, y as i32 + 1, z as i32) == 0 {
                    let color = if is_grass { grass_color } else { dirt_color };
                    let base_idx = vertices.len() as u32;
                    vertices.push(Vertex { position: [px, py + 1.0, pz], color, normal: [0.0, 1.0, 0.0] });
                    vertices.push(Vertex { position: [px, py + 1.0, pz + 1.0], color, normal: [0.0, 1.0, 0.0] });
                    vertices.push(Vertex { position: [px + 1.0, py + 1.0, pz + 1.0], color, normal: [0.0, 1.0, 0.0] });
                    vertices.push(Vertex { position: [px + 1.0, py + 1.0, pz], color, normal: [0.0, 1.0, 0.0] });
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                }

                // 6. Bottom face (-y)
                if chunk.get_block(x as i32, y as i32 - 1, z as i32) == 0 {
                    let color = dirt_color;
                    let base_idx = vertices.len() as u32;
                    vertices.push(Vertex { position: [px, py, pz], color, normal: [0.0, -1.0, 0.0] });
                    vertices.push(Vertex { position: [px + 1.0, py, pz], color, normal: [0.0, -1.0, 0.0] });
                    vertices.push(Vertex { position: [px + 1.0, py, pz + 1.0], color, normal: [0.0, -1.0, 0.0] });
                    vertices.push(Vertex { position: [px, py, pz + 1.0], color, normal: [0.0, -1.0, 0.0] });
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                }
            }
        }
    }

    (vertices, indices)
}
