use std::collections::HashMap;
use glam::{IVec2, Vec3};
use crate::world::chunk::{Chunk, CHUNK_SIZE};
use crate::world::generator::TerrainGenerator;

pub struct World {
    pub chunks: HashMap<IVec2, Chunk>,
    generator: TerrainGenerator,
}

impl World {
    pub fn new(seed: i32) -> Self {
        Self {
            chunks: HashMap::new(),
            generator: TerrainGenerator::new(seed),
        }
    }

    /// Updates the loaded chunks according to the player's current position and render distance.
    /// Returns:
    /// - `Vec<IVec2>` of newly spawned/loaded chunk coordinates (which need mesh generation).
    /// - `Vec<IVec2>` of unloaded chunk coordinates (which need GPU memory cleanup).
    pub fn update(&mut self, player_pos: Vec3, render_distance: i32) -> (Vec<IVec2>, Vec<IVec2>) {
        let player_chunk_x = (player_pos.x / CHUNK_SIZE as f32).floor() as i32;
        let player_chunk_z = (player_pos.z / CHUNK_SIZE as f32).floor() as i32;
        let player_chunk = IVec2::new(player_chunk_x, player_chunk_z);

        let mut newly_loaded = Vec::new();
        let mut active_coords = std::collections::HashSet::new();

        // 1. Generate/find chunks that should be active within render distance
        for dx in -render_distance..=render_distance {
            for dz in -render_distance..=render_distance {
                let coord = player_chunk + IVec2::new(dx, dz);
                active_coords.insert(coord);

                if !self.chunks.contains_key(&coord) {
                    let chunk = self.generator.generate_chunk(coord.x, coord.y);
                    self.chunks.insert(coord, chunk);
                    newly_loaded.push(coord);
                }
            }
        }

        // 2. Filter and unload chunks that are too far.
        // We use (render_distance + 1) for unloading to create a buffer zone (hysteresis),
        // preventing constant loading/unloading when a player moves back and forth on a border.
        let mut unloaded = Vec::new();
        self.chunks.retain(|coord, _| {
            let keep = active_coords.contains(coord) || 
                       ((coord.x - player_chunk.x).abs() <= render_distance + 1 &&
                        (coord.y - player_chunk.y).abs() <= render_distance + 1);
            if !keep {
                unloaded.push(*coord);
            }
            keep
        });

        (newly_loaded, unloaded)
    }
}
