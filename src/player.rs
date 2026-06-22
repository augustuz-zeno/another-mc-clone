use glam::{Vec3, IVec3};
use crate::input::{InputState, KeyCode};
use crate::render::camera::Camera;
use crate::world::World;

// ── Physics constants ──────────────────────────────────────────────────────────
const GRAVITY: f32 = -32.0;        // m/s²  (negative = downward)
const JUMP_VELOCITY: f32 = 8.4;    // m/s   upward impulse on jump
const MAX_FALL_SPEED: f32 = -50.0; // terminal velocity

const BASE_SPEED: f32 = 4.317;
const SPRINT_MULT: f32 = 1.3;
const SNEAK_MULT: f32 = 0.3;

const GROUND_ACCEL: f32 = 45.0;    // how fast you reach top speed
const AIR_ACCEL: f32 = 5.0;        // less control in the air
const GROUND_FRICTION: f32 = 10.0; // deceleration multiplier
const AIR_FRICTION: f32 = 1.0;     // horizontal drag in air

// ── Player body dimensions ─────────────────────────────────────────────────────
const PLAYER_HALF_W: f32 = 0.3;    // half-width in X and Z
const PLAYER_HEIGHT: f32 = 1.8;    // total height
const EYE_OFFSET_STAND: f32 = 1.6; // camera eye above feet
const EYE_OFFSET_SNEAK: f32 = 1.3; // camera eye when sneaking

pub struct Player {
    pub camera: Camera,
    pub sensitivity: f32,
    pub velocity: Vec3,
    pub on_ground: bool,
    pub sprinting: bool,
    pub sneaking: bool,
    pub fov_multiplier: f32,
    pub distance_walked: f32,
    pub eye_offset: f32,
}

impl Player {
    pub fn new(position: Vec3, sensitivity: f32) -> Self {
        Self {
            camera: Camera::new(position, -std::f32::consts::FRAC_PI_2, 0.0),
            sensitivity,
            velocity: Vec3::ZERO,
            on_ground: false,
            sprinting: false,
            sneaking: false,
            fov_multiplier: 1.0,
            distance_walked: 0.0,
            eye_offset: EYE_OFFSET_STAND,
        }
    }

    pub fn update(&mut self, dt: f32, input: &mut InputState, world: &World) {
        // ── Mouse look ──────────────────────────────────────────────────────────
        let (rx, ry) = input.take_mouse_delta();
        self.camera.yaw   += rx as f32 * self.sensitivity;
        self.camera.pitch -= ry as f32 * self.sensitivity;
        let limit = 89.0_f32.to_radians();
        self.camera.pitch = self.camera.pitch.clamp(-limit, limit);

        // ── Ground check ────────────────────────────────────────────────────────
        self.on_ground = self.is_on_ground(world);

        // ── Sneaking ────────────────────────────────────────────────────────────
        self.sneaking = input.is_key_pressed(KeyCode::ShiftLeft);
        let target_eye = if self.sneaking { EYE_OFFSET_SNEAK } else { EYE_OFFSET_STAND };
        self.eye_offset += (target_eye - self.eye_offset) * 15.0 * dt; // smooth crouch

        // ── Sprinting ───────────────────────────────────────────────────────────
        if !input.is_key_pressed(KeyCode::KeyW) || (self.on_ground && self.velocity.length_squared() < 1.0) || self.sneaking {
            self.sprinting = false;
        }
        if input.is_key_pressed(KeyCode::ControlLeft) && input.is_key_pressed(KeyCode::KeyW) && !self.sneaking {
            self.sprinting = true;
        }

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

        let speed_mult = if self.sneaking { SNEAK_MULT } else if self.sprinting { SPRINT_MULT } else { 1.0 };
        let target_speed = BASE_SPEED * speed_mult;

        // Apply acceleration
        if move_dir.length_squared() > 0.0 {
            let accel_factor = if self.on_ground { GROUND_ACCEL } else { AIR_ACCEL };
            let added_vel = move_dir.normalize() * accel_factor * dt;
            self.velocity.x += added_vel.x;
            self.velocity.z += added_vel.z;
            
            // Cap horizontal speed
            let horiz_vel = glam::Vec2::new(self.velocity.x, self.velocity.z);
            if horiz_vel.length() > target_speed {
                let capped = horiz_vel.normalize() * target_speed;
                self.velocity.x = capped.x;
                self.velocity.z = capped.y;
            }
        }

        // Apply friction
        let friction = if self.on_ground { GROUND_FRICTION } else { AIR_FRICTION };
        let drag = (-friction * dt).exp();
        self.velocity.x *= drag;
        self.velocity.z *= drag;

        // ── Vertical: jump & gravity ────────────────────────────────────────────
        if self.on_ground {
            if self.velocity.y < 0.0 {
                self.velocity.y = 0.0;
            }
            if input.is_key_pressed(KeyCode::Space) && !self.sneaking {
                self.velocity.y = JUMP_VELOCITY;
            }
        } else {
            self.velocity.y = (self.velocity.y + GRAVITY * dt).max(MAX_FALL_SPEED);
        }

        // ── Distance Walked (for bobbing) ───────────────────────────────────────
        let horiz_speed = glam::Vec2::new(self.velocity.x, self.velocity.z).length();
        if self.on_ground {
            // Speed up bobbing when sprinting
            self.distance_walked += horiz_speed * dt * if self.sprinting { 1.2 } else { 1.0 };
        } else {
            // Reset bobbing slowly when jumping
            self.distance_walked = (self.distance_walked / (std::f32::consts::PI * 2.0)).round() * std::f32::consts::PI * 2.0;
        }

        // ── Dynamic FOV ─────────────────────────────────────────────────────────
        let target_fov = if self.sprinting { 1.15 } else if self.sneaking { 0.95 } else { 1.0 };
        self.fov_multiplier += (target_fov - self.fov_multiplier) * 10.0 * dt;

        // ── Collision-resolved movement ─────────────────────────────────────────
        let delta = self.velocity * dt;
        self.sweep_move(delta, world);
    }

    pub fn look_direction(&self) -> Vec3 {
        Vec3::new(
            self.camera.pitch.cos() * self.camera.yaw.cos(),
            self.camera.pitch.sin(),
            self.camera.pitch.cos() * self.camera.yaw.sin(),
        ).normalize()
    }

    // ── Private helpers ────────────────────────────────────────────────────────

    fn sweep_move(&mut self, delta: Vec3, world: &World) {
        // We separate axes to slide along walls.
        // For sneaking, if we are on ground, we prevent falling off edges.
        let was_on_ground = self.on_ground;

        // X
        self.camera.position.x += delta.x;
        if self.check_aabb(world) {
            self.camera.position.x -= delta.x;
            self.velocity.x = 0.0;
        } else if self.sneaking && was_on_ground && !self.is_on_ground(world) {
            // Revert movement if it causes us to fall while sneaking
            self.camera.position.x -= delta.x;
            self.velocity.x = 0.0;
        }

        // Y (Vertical) - sneaking doesn't prevent falling if we jump or fall from above, 
        // but wait, Y movement is safe to apply independently.
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
        } else if self.sneaking && was_on_ground && !self.is_on_ground(world) {
            self.camera.position.z -= delta.z;
            self.velocity.z = 0.0;
        }
    }

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

    fn is_on_ground(&self, world: &World) -> bool {
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

    fn aabb_block_range(&self, shrink: f32) -> (IVec3, IVec3) {
        let feet = self.camera.position.y - self.eye_offset;
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
