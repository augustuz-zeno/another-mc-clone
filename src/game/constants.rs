/// Game-wide tuning constants — single source of truth.
/// Import these instead of re-declaring magic numbers in `app.rs` or `main.rs`.

/// How many chunks in each direction are loaded around the player.
pub const RENDER_DISTANCE: i32 = 4;

/// Maximum ray-cast range for block interaction (blocks).
pub const RAYCAST_REACH: f32 = 5.0;

/// Initial spawn height (world Y). The player is spawned above terrain
/// and dropped by gravity to the surface.
pub const SPAWN_Y: f32 = 22.0;

/// Mouse sensitivity (radians per pixel of raw mouse delta).
pub const MOUSE_SENSITIVITY: f32 = 0.003;
