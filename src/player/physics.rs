//! Physics helpers for the Player: AABB collision, ground detection, sweep movement.
//!
//! These live in a separate file from `player.rs` so the movement logic can be
//! read and tested independently of the input-handling / animation code.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::similar_names,
    clippy::cast_possible_wrap
)]

use glam::{IVec3, Vec3};
use crate::world::World;
use super::player::{Player, PLAYER_HALF_W, PLAYER_HEIGHT};

impl Player {
    /// Per-frame collision-resolved translation.
    ///
    /// Moves each axis independently and reverts it if it would cause an AABB
    /// intersection, producing smooth wall-sliding (Quake-style sweep).
    /// While sneaking on the ground, horizontal movement that would step the
    /// player off an edge is also reverted.
    pub(super) fn sweep_move(&mut self, delta: Vec3, world: &World) {
        let was_on_ground = self.on_ground;

        // X
        self.camera.position.x += delta.x;
        if self.check_aabb(world)
            || (self.sneaking && was_on_ground && !self.is_on_ground(world))
        {
            self.camera.position.x -= delta.x;
            self.velocity.x = 0.0;
        }

        // Y — sneaking never suppresses vertical movement
        self.camera.position.y += delta.y;
        if self.check_aabb(world) {
            self.camera.position.y -= delta.y;
            self.velocity.y = 0.0;
        }

        // Z
        self.camera.position.z += delta.z;
        if self.check_aabb(world)
            || (self.sneaking && was_on_ground && !self.is_on_ground(world))
        {
            self.camera.position.z -= delta.z;
            self.velocity.z = 0.0;
        }
    }

    /// `true` if the player's AABB overlaps any solid block.
    pub(super) fn check_aabb(&self, world: &World) -> bool {
        let (min_b, max_b) = self.aabb_block_range(0.0);
        for bx in min_b.x..=max_b.x {
            for by in min_b.y..=max_b.y {
                for bz in min_b.z..=max_b.z {
                    if world.get_block_world(bx, by, bz) != 0 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// `true` if there is a solid block directly below the player's feet.
    pub(super) fn is_on_ground(&self, world: &World) -> bool {
        let feet_y = self.camera.position.y - self.eye_offset;
        let check_y = (feet_y - 0.05).floor() as i32;

        let min_bx = (self.camera.position.x - PLAYER_HALF_W + 0.001).floor() as i32;
        let max_bx = (self.camera.position.x + PLAYER_HALF_W - 0.001).floor() as i32;
        let min_bz = (self.camera.position.z - PLAYER_HALF_W + 0.001).floor() as i32;
        let max_bz = (self.camera.position.z + PLAYER_HALF_W - 0.001).floor() as i32;

        for bx in min_bx..=max_bx {
            for bz in min_bz..=max_bz {
                if world.get_block_world(bx, check_y, bz) != 0 {
                    return true;
                }
            }
        }
        false
    }

    /// Returns the min/max block coordinates that the player's AABB (optionally
    /// inset by `shrink` metres) occupies in the world.
    pub(super) fn aabb_block_range(&self, shrink: f32) -> (IVec3, IVec3) {
        let feet = self.camera.position.y - self.eye_offset;
        let hw = PLAYER_HALF_W - shrink;

        let min_b = IVec3::new(
            (self.camera.position.x - hw).floor() as i32,
            feet.floor() as i32,
            (self.camera.position.z - hw).floor() as i32,
        );
        let max_b = IVec3::new(
            (self.camera.position.x + hw - 0.001).floor() as i32,
            (feet + PLAYER_HEIGHT - 0.001).floor() as i32,
            (self.camera.position.z + hw - 0.001).floor() as i32,
        );
        (min_b, max_b)
    }
}
