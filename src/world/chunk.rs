pub const CHUNK_SIZE: usize = 16;
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE; // 4096

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockType {
    Air = 0,
    Dirt = 1,
    Grass = 2,
    Stone = 3,
}

impl BlockType {
    #[allow(dead_code)]
    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => BlockType::Dirt,
            2 => BlockType::Grass,
            3 => BlockType::Stone,
            _ => BlockType::Air,
        }
    }
}

pub struct Chunk {
    pub blocks: [u8; CHUNK_VOLUME],
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            blocks: [0; CHUNK_VOLUME],
        }
    }

    #[inline]
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> u8 {
        if x < 0 || x >= CHUNK_SIZE as i32 ||
           y < 0 || y >= CHUNK_SIZE as i32 ||
           z < 0 || z >= CHUNK_SIZE as i32 {
            return 0; // Out of bounds is treated as Air
        }
        let index = self.coords_to_index(x as usize, y as usize, z as usize);
        self.blocks[index]
    }

    #[inline]
    pub fn set_block(&mut self, x: usize, y: usize, z: usize, block_id: u8) {
        if x < CHUNK_SIZE && y < CHUNK_SIZE && z < CHUNK_SIZE {
            let index = self.coords_to_index(x, y, z);
            self.blocks[index] = block_id;
        }
    }

    #[inline]
    fn coords_to_index(&self, x: usize, y: usize, z: usize) -> usize {
        // Laying out memory sequentially: index = y * 256 + z * 16 + x.
        // With loop nesting (y outer, z middle, x inner), this guarantees
        // contiguous sequential reads with a stride of 1 byte, which leverages
        // the CPU prefetcher and fits fully into L1 cache (4 KB).
        y * (CHUNK_SIZE * CHUNK_SIZE) + z * CHUNK_SIZE + x
    }
}
