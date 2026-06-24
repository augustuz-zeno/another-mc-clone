#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::similar_names,
    clippy::cast_possible_wrap
)]
use glam::Vec3;
use crate::input::{InputState, KeyCode};
use crate::render::camera::Camera;
use crate::world::World;

// ── Physics constants ─────────────────────────────────────────────────────────
const GRAVITY: f32        = -32.0;  // m/s² (downward)
const JUMP_VELOCITY: f32  =   8.4;  // m/s
const MAX_FALL_SPEED: f32 = -50.0;  // terminal velocity

const BASE_SPEED: f32    = 4.317;
const SPRINT_MULT: f32   = 1.3;
const SNEAK_MULT: f32    = 0.3;

const GROUND_ACCEL: f32   = 45.0;
const AIR_ACCEL: f32      =  5.0;
const GROUND_FRICTION: f32 = 10.0;
const AIR_FRICTION: f32    =  1.0;

// ── Player body dimensions ────────────────────────────────────────────────────
/// Half-width of the player's AABB in X and Z. Published so `physics.rs` can use it.
pub(super) const PLAYER_HALF_W: f32 = 0.3;
/// Total height of the player's AABB. Published so `physics.rs` can use it.
pub(super) const PLAYER_HEIGHT: f32 = 1.8;

const EYE_OFFSET_STAND: f32 = 1.6; // camera Y above feet while standing
const EYE_OFFSET_SNEAK: f32 = 1.3; // camera Y above feet while sneaking

pub struct Player {
    pub camera: Camera,
    pub sensitivity: f32,
    pub velocity: Vec3,
    pub on_ground: bool,
    pub sprinting: bool,
    pub sneaking: bool,
    pub fov_multiplier: f32,
    pub distance_walked: f32,
    /// Current eye height above feet — smoothly interpolated between stand/sneak.
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

    /// Full per-frame update: mouse look, physics, animation, collision.
    pub fn update(&mut self, dt: f32, input: &mut InputState, world: &World) {
        // ── Mouse look ───────────────────────────────────────────────────────
        let (rx, ry) = input.take_mouse_delta();
        self.camera.yaw   += rx as f32 * self.sensitivity;
        self.camera.pitch -= ry as f32 * self.sensitivity;
        let limit = 89.0_f32.to_radians();
        self.camera.pitch = self.camera.pitch.clamp(-limit, limit);

        // ── Ground check ─────────────────────────────────────────────────────
        self.on_ground = self.is_on_ground(world);

        // ── Sneaking ─────────────────────────────────────────────────────────
        self.sneaking = input.is_key_pressed(KeyCode::ShiftLeft);
        let target_eye = if self.sneaking { EYE_OFFSET_SNEAK } else { EYE_OFFSET_STAND };
        let old_eye = self.eye_offset;
        self.eye_offset += (target_eye - self.eye_offset) * 15.0 * dt;
        // Compensate so the feet (and AABB bottom) stay in place
        self.camera.position.y += self.eye_offset - old_eye;

        // ── Sprinting ────────────────────────────────────────────────────────
        if !input.is_key_pressed(KeyCode::KeyW)
            || (self.on_ground && self.velocity.length_squared() < 1.0)
            || self.sneaking
        {
            self.sprinting = false;
        }
        if input.is_key_pressed(KeyCode::ControlLeft)
            && input.is_key_pressed(KeyCode::KeyW)
            && !self.sneaking
        {
            self.sprinting = true;
        }

        // ── Horizontal movement (always camera-yaw-relative) ─────────────────
        let forward = Vec3::new(self.camera.yaw.cos(), 0.0, self.camera.yaw.sin()).normalize_or_zero();
        let right   = Vec3::new(-self.camera.yaw.sin(), 0.0, self.camera.yaw.cos()).normalize_or_zero();

        let mut move_dir = Vec3::ZERO;
        if input.is_key_pressed(KeyCode::KeyW) { move_dir += forward; }
        if input.is_key_pressed(KeyCode::KeyS) { move_dir -= forward; }
        if input.is_key_pressed(KeyCode::KeyD) { move_dir += right; }
        if input.is_key_pressed(KeyCode::KeyA) { move_dir -= right; }

        let speed_mult   = if self.sneaking { SNEAK_MULT } else if self.sprinting { SPRINT_MULT } else { 1.0 };
        let target_speed = BASE_SPEED * speed_mult;

        if move_dir.length_squared() > 0.0 {
            let accel_factor = if self.on_ground { GROUND_ACCEL } else { AIR_ACCEL };
            let added = move_dir.normalize() * accel_factor * dt;
            self.velocity.x += added.x;
            self.velocity.z += added.z;

            let horiz = glam::Vec2::new(self.velocity.x, self.velocity.z);
            if horiz.length() > target_speed {
                let capped = horiz.normalize() * target_speed;
                self.velocity.x = capped.x;
                self.velocity.z = capped.y;
            }
        }

        // ── Friction / drag ───────────────────────────────────────────────────
        let friction = if self.on_ground { GROUND_FRICTION } else { AIR_FRICTION };
        let drag = (-friction * dt).exp();
        self.velocity.x *= drag;
        self.velocity.z *= drag;

        // ── Vertical: jump & gravity ──────────────────────────────────────────
        if self.on_ground {
            if self.velocity.y < 0.0 { self.velocity.y = 0.0; }
            if input.is_key_pressed(KeyCode::Space) && !self.sneaking {
                self.velocity.y = JUMP_VELOCITY;
            }
        } else {
            self.velocity.y = (self.velocity.y + GRAVITY * dt).max(MAX_FALL_SPEED);
        }

        // ── Head bob (walk distance for animation) ────────────────────────────
        let horiz_speed = glam::Vec2::new(self.velocity.x, self.velocity.z).length();
        if self.on_ground {
            self.distance_walked += horiz_speed * dt * if self.sprinting { 1.2 } else { 1.0 };
        } else {
            // Snap to nearest full cycle to avoid pop when landing
            self.distance_walked = (self.distance_walked / (std::f32::consts::TAU)).round()
                * std::f32::consts::TAU;
        }

        // ── Dynamic FOV ───────────────────────────────────────────────────────
        let target_fov = if self.sprinting { 1.15 } else if self.sneaking { 0.95 } else { 1.0 };
        self.fov_multiplier += (target_fov - self.fov_multiplier) * 10.0 * dt;

        // ── Collision-resolved movement ───────────────────────────────────────
        let delta = self.velocity * dt;
        self.sweep_move(delta, world);
    }

    /// Normalised look direction (unit vector in world space).
    pub fn look_direction(&self) -> Vec3 {
        Vec3::new(
            self.camera.pitch.cos() * self.camera.yaw.cos(),
            self.camera.pitch.sin(),
            self.camera.pitch.cos() * self.camera.yaw.sin(),
        )
        .normalize()
    }
}
