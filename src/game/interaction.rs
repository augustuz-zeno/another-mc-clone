#![allow(clippy::cast_precision_loss)]

use glam::{IVec3, Vec3};
use crate::player::Player;
use crate::world::World;

/// Try to break (set to Air) the block at the raycast hit position.
/// Returns the chunk coordinate that changed (for mesh rebuild), or `None`.
pub fn handle_break(world: &mut World, block_pos: IVec3) -> Option<glam::IVec2> {
    world.set_block_world(block_pos.x, block_pos.y, block_pos.z, 0)
}

/// Try to place `block_id` on the face adjacent to the raycast hit.
/// Silently does nothing if the placement would intersect the player body.
/// Returns the chunk coordinate that changed, or `None`.
pub fn handle_place(
    world: &mut World,
    block_pos: IVec3,
    normal: IVec3,
    block_id: u8,
    player: &Player,
) -> Option<glam::IVec2> {
    let target = block_pos + normal;
    if player_overlaps_block(player, target) {
        return None;
    }
    world.set_block_world(target.x, target.y, target.z, block_id)
}

/// Returns `true` if an axis-aligned unit block at `block` (integer coords)
/// would overlap the player's AABB.
///
/// Player AABB: half-width 0.3 in X/Z, height 1.8, camera is 1.6 above feet.
pub fn player_overlaps_block(player: &Player, block: IVec3) -> bool {
    const HALF_W: f32 = 0.3;
    const EYE_H: f32  = 1.6;
    const HEIGHT: f32 = 1.8;

    let feet = player.camera.position.y - EYE_H;
    let p_min = Vec3::new(
        player.camera.position.x - HALF_W,
        feet,
        player.camera.position.z - HALF_W,
    );
    let p_max = Vec3::new(
        player.camera.position.x + HALF_W,
        feet + HEIGHT,
        player.camera.position.z + HALF_W,
    );

    let b_min = Vec3::new(block.x as f32,       block.y as f32,       block.z as f32);
    let b_max = Vec3::new(block.x as f32 + 1.0, block.y as f32 + 1.0, block.z as f32 + 1.0);

    p_min.x < b_max.x && p_max.x > b_min.x
        && p_min.y < b_max.y && p_max.y > b_min.y
        && p_min.z < b_max.z && p_max.z > b_min.z
}
