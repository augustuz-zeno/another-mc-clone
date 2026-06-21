use glam::{Vec3, IVec3};
use crate::input::{InputState, KeyCode};
use crate::render::camera::Camera;
use crate::world::World;

// ── Physics constants ──────────────────────────────────────────────────────────
const GRAVITY: f32 = -25.0;        // m/s²  (negative = downward)
const JUMP_VELOCITY: f32 = 9.0;    // m/s   upward impulse on jump
const MAX_FALL_SPEED: f32 = -50.0; // terminal velocity

// ── Player body dimensions ─────────────────────────────────────────────────────
const PLAYER_HALF_W: f32 = 0.3;    // half-width in X and Z
const PLAYER_HEIGHT: f32 = 1.8;    // total height
const EYE_OFFSET: f32 = 1.6;       // camera eye above feet

pub struct Player {
    pub camera: Camera,
    pub speed: f32,
    pub sensitivity: f32,
    pub velocity: Vec3,
    pub on_ground: bool,
}

impl Player {
    pub fn new(position: Vec3, speed: f32, sensitivity: f32) -> Self {
        Self {
            camera: Camera::new(position, -std::f32::consts::FRAC_PI_2, 0.0),
            speed,
            sensitivity,
            velocity: Vec3::ZERO,
            on_ground: false,
        }
    }

    /// Main update: mouse look → ground check → horizontal input → gravity/jump → collision sweep.
    /// `world` is needed for AABB collision queries.
    pub fn update(&mut self, dt: f32, input: &mut InputState, world: &World) {
        // ── Mouse look ──────────────────────────────────────────────────────────
        let (rx, ry) = input.take_mouse_delta();
        self.camera.yaw   += rx as f32 * self.sensitivity;
        self.camera.pitch -= ry as f32 * self.sensitivity;
        let limit = 89.0_f32.to_radians();
        self.camera.pitch = self.camera.pitch.clamp(-limit, limit);

        // ── Ground check ────────────────────────────────────────────────────────
        self.on_ground = self.is_on_ground(world);

        // ── Horizontal movement (always relative to camera yaw) ─────────────────
        let forward = Vec3::new(
            self.camera.yaw.cos(),
            0.0,
            self.camera.yaw.sin(),
        ).normalize_or_zero();
        let right = Vec3::new(
            -self.camera.yaw.sin(),
            0.0,
            self.camera.yaw.cos(),
        ).normalize_or_zero();

        let mut move_dir = Vec3::ZERO;
        if input.is_key_pressed(KeyCode::KeyW) { move_dir += forward; }
        if input.is_key_pressed(KeyCode::KeyS) { move_dir -= forward; }
        if input.is_key_pressed(KeyCode::KeyD) { move_dir += right; }
        if input.is_key_pressed(KeyCode::KeyA) { move_dir -= right; }

        if move_dir.length_squared() > 0.0 {
            let h = move_dir.normalize() * self.speed;
            self.velocity.x = h.x;
            self.velocity.z = h.z;
        } else {
            // Instant stop on ground; keep momentum for responsiveness in air too
            self.velocity.x = 0.0;
            self.velocity.z = 0.0;
        }

        // ── Vertical: jump & gravity ────────────────────────────────────────────
        if self.on_ground {
            if self.velocity.y < 0.0 {
                self.velocity.y = 0.0; // Clamp downward drift when standing
            }
            if input.is_key_pressed(KeyCode::Space) {
                self.velocity.y = JUMP_VELOCITY;
            }
        } else {
            self.velocity.y = (self.velocity.y + GRAVITY * dt).max(MAX_FALL_SPEED);
        }

        // ── Collision-resolved movement ─────────────────────────────────────────
        let delta = self.velocity * dt;
        self.sweep_move(delta, world);
    }

    /// Returns the normalised look direction of the camera (for ray casting).
    pub fn look_direction(&self) -> Vec3 {
        Vec3::new(
            self.camera.pitch.cos() * self.camera.yaw.cos(),
            self.camera.pitch.sin(),
            self.camera.pitch.cos() * self.camera.yaw.sin(),
        ).normalize()
    }

    // ── Private helpers ────────────────────────────────────────────────────────

    /// Move along each axis independently, resolving collisions per axis.
    fn sweep_move(&mut self, delta: Vec3, world: &World) {
        // X
        self.camera.position.x += delta.x;
        if self.check_aabb(world) {
            self.camera.position.x -= delta.x;
            self.velocity.x = 0.0;
        }
        // Y
        self.camera.position.y += delta.y;
        if self.check_aabb(world) {
            self.camera.position.y -= delta.y;
            self.velocity.y = 0.0;
        }
        // Z
        self.camera.position.z += delta.z;
        if self.check_aabb(world) {
            self.camera.position.z -= delta.z;
            self.velocity.z = 0.0;
        }
    }

    /// True if the player AABB overlaps at least one solid block.
    fn check_aabb(&self, world: &World) -> bool {
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

    /// True if there is a solid block directly below the player's feet.
    fn is_on_ground(&self, world: &World) -> bool {
        let feet_y = self.camera.position.y - EYE_OFFSET;
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

    /// Compute the inclusive block-coordinate range that the player AABB overlaps.
    /// `shrink` reduces the half-width slightly to avoid false positives on edges.
    fn aabb_block_range(&self, shrink: f32) -> (IVec3, IVec3) {
        let feet = self.camera.position.y - EYE_OFFSET;
        let hw = PLAYER_HALF_W - shrink;

        let min_x = self.camera.position.x - hw;
        let max_x = self.camera.position.x + hw;
        let min_y = feet;
        let max_y = feet + PLAYER_HEIGHT;
        let min_z = self.camera.position.z - hw;
        let max_z = self.camera.position.z + hw;

        let min_b = IVec3::new(
            min_x.floor() as i32,
            min_y.floor() as i32,
            min_z.floor() as i32,
        );
        let max_b = IVec3::new(
            (max_x - 0.001).floor() as i32,
            (max_y - 0.001).floor() as i32,
            (max_z - 0.001).floor() as i32,
        );
        (min_b, max_b)
    }
}
