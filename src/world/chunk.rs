pub const CHUNK_SIZE: usize = 16;
pub const CHUNK_SIZE_I32: i32 = CHUNK_SIZE as i32;
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE; // 4096

/// Canonical block type enumeration — single source of truth.
/// All block-ID constants elsewhere should use these variants.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockType {
    Air   = 0,
    Dirt  = 1,
    Grass = 2,
    Stone = 3,
}

impl BlockType {
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    #[allow(dead_code)]
    pub fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::Dirt,
            2 => Self::Grass,
            3 => Self::Stone,
            _ => Self::Air,
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
        if !(0..CHUNK_SIZE_I32).contains(&x) ||
           !(0..CHUNK_SIZE_I32).contains(&y) ||
           !(0..CHUNK_SIZE_I32).contains(&z) {
            return 0; // Out of bounds is treated as Air
        }
        // Safety: bounds checked above, values are in [0, CHUNK_SIZE)
        #[allow(clippy::cast_sign_loss)]
        let index = Self::coords_to_index(x as usize, y as usize, z as usize);
        self.blocks[index]
    }

    #[inline]
    pub fn set_block(&mut self, x: usize, y: usize, z: usize, block_id: u8) {
        if x < CHUNK_SIZE && y < CHUNK_SIZE && z < CHUNK_SIZE {
            let index = Self::coords_to_index(x, y, z);
            self.blocks[index] = block_id;
        }
    }

    /// Convert (x, y, z) chunk-local coordinates to a flat array index.
    ///
    /// Layout: `index = y * 256 + z * 16 + x`
    ///
    /// With loop nesting (y outer, z middle, x inner) this guarantees
    /// sequential reads with stride 1, fitting the full 4 KB chunk into L1 cache.
    #[inline]
    pub fn coords_to_index(x: usize, y: usize, z: usize) -> usize {
        y * (CHUNK_SIZE * CHUNK_SIZE) + z * CHUNK_SIZE + x
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coords_to_index() {
        assert_eq!(Chunk::coords_to_index(0, 0, 0), 0);
        assert_eq!(Chunk::coords_to_index(15, 0, 0), 15);
        assert_eq!(Chunk::coords_to_index(0, 0, 1), 16);
        assert_eq!(Chunk::coords_to_index(0, 1, 0), 256);
        assert_eq!(Chunk::coords_to_index(15, 15, 15), 4095);
    }
}
