use std::collections::HashMap;
use glam::{IVec2, IVec3, Vec3};
use crate::world::chunk::{Chunk, CHUNK_SIZE};
use crate::world::generator::TerrainGenerator;

/// Result of a ray-voxel intersection query.
pub struct RaycastHit {
    /// Block coordinate in world space.
    pub block_pos: IVec3,
    /// Outward face normal of the hit face (one of ±X, ±Y, ±Z unit vectors).
    pub normal: IVec3,
}

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

    /// Read any block by world-space coordinates.
    /// Returns 0 (Air) for unloaded chunks or out-of-vertical-bounds positions.
    pub fn get_block_world(&self, x: i32, y: i32, z: i32) -> u8 {
        if y < 0 || y >= CHUNK_SIZE as i32 {
            return 0;
        }
        let cx = x.div_euclid(CHUNK_SIZE as i32);
        let cz = z.div_euclid(CHUNK_SIZE as i32);
        let lx = x.rem_euclid(CHUNK_SIZE as i32);
        let lz = z.rem_euclid(CHUNK_SIZE as i32);
        if let Some(chunk) = self.chunks.get(&IVec2::new(cx, cz)) {
            chunk.get_block(lx, y, lz)
        } else {
            0
        }
    }

    /// Write a block by world-space coordinates.
    /// Returns the chunk coordinate that was modified (for mesh rebuild), or None
    /// if the target chunk is not loaded or the position is out of bounds.
    pub fn set_block_world(&mut self, x: i32, y: i32, z: i32, block_id: u8) -> Option<IVec2> {
        if y < 0 || y >= CHUNK_SIZE as i32 {
            return None;
        }
        let cx = x.div_euclid(CHUNK_SIZE as i32);
        let cz = z.div_euclid(CHUNK_SIZE as i32);
        let lx = x.rem_euclid(CHUNK_SIZE as i32);
        let lz = z.rem_euclid(CHUNK_SIZE as i32);
        let coord = IVec2::new(cx, cz);
        if let Some(chunk) = self.chunks.get_mut(&coord) {
            chunk.set_block(lx as usize, y as usize, lz as usize, block_id);
            Some(coord)
        } else {
            None
        }
    }

    /// Cast a ray through the voxel grid using the DDA algorithm.
    /// Returns the first solid block hit within `max_distance`, along with the
    /// face normal pointing back towards the origin (useful for block placement).
    pub fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<RaycastHit> {
        let dir = direction.normalize();
        if dir.length_squared() < 1e-6 {
            return None;
        }

        // Current voxel containing the ray origin.
        let mut pos = IVec3::new(
            origin.x.floor() as i32,
            origin.y.floor() as i32,
            origin.z.floor() as i32,
        );

        // Step direction per axis (+1 or -1).
        let step = IVec3::new(
            if dir.x >= 0.0 { 1 } else { -1 },
            if dir.y >= 0.0 { 1 } else { -1 },
            if dir.z >= 0.0 { 1 } else { -1 },
        );

        // t_delta: ray length to cross one voxel in each axis.
        let t_delta = Vec3::new(
            if dir.x.abs() > 1e-9 { (1.0 / dir.x).abs() } else { f32::INFINITY },
            if dir.y.abs() > 1e-9 { (1.0 / dir.y).abs() } else { f32::INFINITY },
            if dir.z.abs() > 1e-9 { (1.0 / dir.z).abs() } else { f32::INFINITY },
        );

        // t_max: ray length to reach the first voxel boundary in each axis.
        let mut t_max = Vec3::new(
            if dir.x >= 0.0 {
                (pos.x as f32 + 1.0 - origin.x) / dir.x.abs().max(1e-9)
            } else {
                (origin.x - pos.x as f32) / dir.x.abs().max(1e-9)
            },
            if dir.y >= 0.0 {
                (pos.y as f32 + 1.0 - origin.y) / dir.y.abs().max(1e-9)
            } else {
                (origin.y - pos.y as f32) / dir.y.abs().max(1e-9)
            },
            if dir.z >= 0.0 {
                (pos.z as f32 + 1.0 - origin.z) / dir.z.abs().max(1e-9)
            } else {
                (origin.z - pos.z as f32) / dir.z.abs().max(1e-9)
            },
        );

        // Clamp infinities from zero-direction components.
        if dir.x.abs() <= 1e-9 { t_max.x = f32::INFINITY; }
        if dir.y.abs() <= 1e-9 { t_max.y = f32::INFINITY; }
        if dir.z.abs() <= 1e-9 { t_max.z = f32::INFINITY; }

        // Track current travel distance and face normal.
        let mut t = 0.0_f32;
        let mut normal = IVec3::ZERO;

        loop {
            if t > max_distance {
                return None;
            }

            if self.get_block_world(pos.x, pos.y, pos.z) != 0 {
                return Some(RaycastHit { block_pos: pos, normal });
            }

            // Advance to the nearest voxel boundary.
            if t_max.x < t_max.y {
                if t_max.x < t_max.z {
                    t = t_max.x;
                    t_max.x += t_delta.x;
                    pos.x += step.x;
                    normal = IVec3::new(-step.x, 0, 0);
                } else {
                    t = t_max.z;
                    t_max.z += t_delta.z;
                    pos.z += step.z;
                    normal = IVec3::new(0, 0, -step.z);
                }
            } else if t_max.y < t_max.z {
                t = t_max.y;
                t_max.y += t_delta.y;
                pos.y += step.y;
                normal = IVec3::new(0, -step.y, 0);
            } else {
                t = t_max.z;
                t_max.z += t_delta.z;
                pos.z += step.z;
                normal = IVec3::new(0, 0, -step.z);
            }
        }
    }
}
