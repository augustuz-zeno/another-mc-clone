#[allow(clippy::module_inception)]
pub mod world;
pub mod chunk;
pub mod generator;

pub use chunk::{Chunk, BlockType};
pub use world::World;
