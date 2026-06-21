use fastnoise_lite::{FastNoiseLite, NoiseType};
use crate::world::chunk::{Chunk, CHUNK_SIZE};

pub struct TerrainGenerator {
    noise: FastNoiseLite,
}

impl TerrainGenerator {
    pub fn new(seed: i32) -> Self {
        let mut noise = FastNoiseLite::new();
        noise.set_seed(Some(seed));
        noise.set_noise_type(Some(NoiseType::OpenSimplex2));
        noise.set_frequency(Some(0.04)); // smooth hills
        Self { noise }
    }

    pub fn generate_chunk(&self, chunk_x: i32, chunk_z: i32) -> Chunk {
        let mut chunk = Chunk::new();

        // Loop order: outer y, middle z, inner x.
        // Since coords_to_index is y * 256 + z * 16 + x, x changes fastest,
        // which corresponds to sequential memory access (stride 1).
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let world_x = chunk_x * CHUNK_SIZE as i32 + x as i32;
                    let world_z = chunk_z * CHUNK_SIZE as i32 + z as i32;
                    
                    // Sample 2D noise: value is between -1.0 and 1.0
                    let noise_val = self.noise.get_noise_2d(world_x as f32, world_z as f32);
                    
                    // Map to height in range [2, CHUNK_SIZE - 2] so the surface is fully visible
                    let normalized = (noise_val * 0.5 + 0.5).clamp(0.0, 1.0);
                    let height = (normalized * (CHUNK_SIZE - 4) as f32).round() as i32 + 2;

                    let y_i32 = y as i32;
                    let block_id = if y_i32 < height - 1 {
                        1 // Dirt
                    } else if y_i32 == height - 1 {
                        2 // Grass
                    } else {
                        0 // Air
                    };

                    chunk.set_block(x, y, z, block_id);
                }
            }
        }

        chunk
    }
}
