use std::cmp::Ordering;

use fastnoise_lite::{FastNoiseLite, NoiseType};
use crate::world::chunk::{BlockType, Chunk, CHUNK_SIZE, CHUNK_SIZE_I32};

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

        // Loop order: outer z, inner x — noise is 2D (only depends on x,z).
        // By hoisting get_noise_2d above the y-loop we call it 256 times
        // per chunk instead of 4096, a 16× reduction in noise samples.
        // Since coords_to_index is y * 256 + z * 16 + x, the inner y loop
        // still fills memory sequentially (stride 1).
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                let world_x = chunk_x * CHUNK_SIZE_I32 + x as i32;
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                let world_z = chunk_z * CHUNK_SIZE_I32 + z as i32;

                // Sample 2D noise: value is between -1.0 and 1.0.
                // The i32→f32 cast loses precision only beyond 2^23 world
                // coordinates (~8 million blocks), acceptable for a game world.
                #[allow(clippy::cast_precision_loss)]
                let noise_val = self.noise.get_noise_2d(world_x as f32, world_z as f32);

                // Map to height in [2, CHUNK_SIZE - 2] so the surface is visible.
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                let normalized = (noise_val * 0.5 + 0.5).clamp(0.0, 1.0);
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                let height = (normalized * (CHUNK_SIZE - 4) as f32).round() as i32 + 2;

                for y in 0..CHUNK_SIZE {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                    let y_i32 = y as i32;

                    let block_id = match y_i32.cmp(&(height - 1)) {
                        Ordering::Less    => BlockType::Dirt.to_u8(),
                        Ordering::Equal   => BlockType::Grass.to_u8(),
                        Ordering::Greater => BlockType::Air.to_u8(),
                    };

                    chunk.set_block(x, y, z, block_id);
                }
            }
        }

        chunk
    }
}
