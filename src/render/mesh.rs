use crate::world::chunk::{Chunk, CHUNK_SIZE};

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub tex_index: u32,
    /// 1 = apply grass-green colormap tint (for grayscale grass textures), 0 = no tint
    pub tint: u32,
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x3, // position
        1 => Float32x3, // normal
        2 => Float32x2, // uv
        3 => Uint32,    // tex_index
        4 => Uint32,    // tint
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub const TEX_DIRT:       u32 = 0;
pub const TEX_GRASS_TOP:  u32 = 1;
pub const TEX_GRASS_SIDE: u32 = 2;
pub const TEX_STONE:      u32 = 3;

/// Helper to create a Vertex. `tint=1` means apply grass-green colormap tint.
#[inline]
fn v(position: [f32; 3], normal: [f32; 3], uv: [f32; 2], tex_index: u32, tint: u32) -> Vertex {
    Vertex { position, normal, uv, tex_index, tint }
}

/// Generate a greedy-ish mesh for a chunk.
///
/// All `usize → i32` and `usize → f32` casts in this function are safe:
/// - `CHUNK_SIZE = 16`, so `x`, `y`, `z` are always in `[0, 15]`.
/// - `vertices.len()` ≤ 6 faces × 16³ blocks × 4 verts = 24576 << `u32::MAX`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
pub fn generate_mesh(chunk: &Chunk, chunk_coords: glam::IVec2) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices:  Vec<u32>   = Vec::new();

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

                // (tex_top, tex_side, tex_bottom, tint_top, tint_side)
                let (tex_top, tex_side, tex_bottom, tint_top, tint_side) = match block_id {
                    2 => (TEX_GRASS_TOP, TEX_GRASS_SIDE, TEX_DIRT, 1, 1),
                    3 => (TEX_STONE,     TEX_STONE,       TEX_STONE, 0, 0),
                    _ => (TEX_DIRT,      TEX_DIRT,        TEX_DIRT, 0, 0),
                };

                // 1. Front face (+z)
                if chunk.get_block(x as i32, y as i32, z as i32 + 1) == 0 {
                    let (t, tn) = (tex_side, tint_side);
                    let base_idx = vertices.len() as u32;
                    vertices.push(v([px, py, pz + 1.0],             [0.0, 0.0, 1.0], [0.0, 1.0], t, tn));
                    vertices.push(v([px + 1.0, py, pz + 1.0],       [0.0, 0.0, 1.0], [1.0, 1.0], t, tn));
                    vertices.push(v([px + 1.0, py + 1.0, pz + 1.0], [0.0, 0.0, 1.0], [1.0, 0.0], t, tn));
                    vertices.push(v([px, py + 1.0, pz + 1.0],       [0.0, 0.0, 1.0], [0.0, 0.0], t, tn));
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                }

                // 2. Back face (-z)
                if chunk.get_block(x as i32, y as i32, z as i32 - 1) == 0 {
                    let (t, tn) = (tex_side, tint_side);
                    let base_idx = vertices.len() as u32;
                    vertices.push(v([px + 1.0, py, pz],             [0.0, 0.0, -1.0], [0.0, 1.0], t, tn));
                    vertices.push(v([px, py, pz],                   [0.0, 0.0, -1.0], [1.0, 1.0], t, tn));
                    vertices.push(v([px, py + 1.0, pz],             [0.0, 0.0, -1.0], [1.0, 0.0], t, tn));
                    vertices.push(v([px + 1.0, py + 1.0, pz],       [0.0, 0.0, -1.0], [0.0, 0.0], t, tn));
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                }

                // 3. Left face (-x)
                if chunk.get_block(x as i32 - 1, y as i32, z as i32) == 0 {
                    let (t, tn) = (tex_side, tint_side);
                    let base_idx = vertices.len() as u32;
                    vertices.push(v([px, py, pz],               [-1.0, 0.0, 0.0], [0.0, 1.0], t, tn));
                    vertices.push(v([px, py, pz + 1.0],         [-1.0, 0.0, 0.0], [1.0, 1.0], t, tn));
                    vertices.push(v([px, py + 1.0, pz + 1.0],   [-1.0, 0.0, 0.0], [1.0, 0.0], t, tn));
                    vertices.push(v([px, py + 1.0, pz],         [-1.0, 0.0, 0.0], [0.0, 0.0], t, tn));
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                }

                // 4. Right face (+x)
                if chunk.get_block(x as i32 + 1, y as i32, z as i32) == 0 {
                    let (t, tn) = (tex_side, tint_side);
                    let base_idx = vertices.len() as u32;
                    vertices.push(v([px + 1.0, py, pz + 1.0],       [1.0, 0.0, 0.0], [0.0, 1.0], t, tn));
                    vertices.push(v([px + 1.0, py, pz],             [1.0, 0.0, 0.0], [1.0, 1.0], t, tn));
                    vertices.push(v([px + 1.0, py + 1.0, pz],       [1.0, 0.0, 0.0], [1.0, 0.0], t, tn));
                    vertices.push(v([px + 1.0, py + 1.0, pz + 1.0], [1.0, 0.0, 0.0], [0.0, 0.0], t, tn));
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                }

                // 5. Top face (+y)
                if chunk.get_block(x as i32, y as i32 + 1, z as i32) == 0 {
                    let (t, tn) = (tex_top, tint_top);
                    let base_idx = vertices.len() as u32;
                    vertices.push(v([px, py + 1.0, pz],             [0.0, 1.0, 0.0], [0.0, 1.0], t, tn));
                    vertices.push(v([px, py + 1.0, pz + 1.0],       [0.0, 1.0, 0.0], [0.0, 0.0], t, tn));
                    vertices.push(v([px + 1.0, py + 1.0, pz + 1.0], [0.0, 1.0, 0.0], [1.0, 0.0], t, tn));
                    vertices.push(v([px + 1.0, py + 1.0, pz],       [0.0, 1.0, 0.0], [1.0, 1.0], t, tn));
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                }

                // 6. Bottom face (-y)
                if chunk.get_block(x as i32, y as i32 - 1, z as i32) == 0 {
                    let (t, tn) = (tex_bottom, 0);
                    let base_idx = vertices.len() as u32;
                    vertices.push(v([px, py, pz],             [0.0, -1.0, 0.0], [0.0, 0.0], t, tn));
                    vertices.push(v([px + 1.0, py, pz],       [0.0, -1.0, 0.0], [1.0, 0.0], t, tn));
                    vertices.push(v([px + 1.0, py, pz + 1.0], [0.0, -1.0, 0.0], [1.0, 1.0], t, tn));
                    vertices.push(v([px, py, pz + 1.0],       [0.0, -1.0, 0.0], [0.0, 1.0], t, tn));
                    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2, base_idx, base_idx + 2, base_idx + 3]);
                }
            }
        }
    }

    (vertices, indices)
}
