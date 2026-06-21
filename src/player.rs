use glam::Vec3;
use crate::input::{InputState, KeyCode};
use crate::render::camera::Camera;

pub struct Player {
    pub camera: Camera,
    pub speed: f32,
    pub sensitivity: f32,
}

impl Player {
    pub fn new(position: Vec3, speed: f32, sensitivity: f32) -> Self {
        Self {
            camera: Camera::new(position, -std::f32::consts::FRAC_PI_2, 0.0), // default view looking down -z
            speed,
            sensitivity,
        }
    }

    pub fn update(&mut self, dt: f32, input: &mut InputState) {
        // Process mouse movement for look controls
        let (rx, ry) = input.take_mouse_delta();
        
        self.camera.yaw += rx as f32 * self.sensitivity;
        self.camera.pitch -= ry as f32 * self.sensitivity;

        // Clamp pitch to prevent the camera from flipping upside down
        let limit = 89.0f32.to_radians();
        self.camera.pitch = self.camera.pitch.clamp(-limit, limit);

        // Compute forward and right direction vectors in XZ plane
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

        if input.is_key_pressed(KeyCode::KeyW) {
            move_dir += forward;
        }
        if input.is_key_pressed(KeyCode::KeyS) {
            move_dir -= forward;
        }
        if input.is_key_pressed(KeyCode::KeyD) {
            move_dir += right;
        }
        if input.is_key_pressed(KeyCode::KeyA) {
            move_dir -= right;
        }
        if input.is_key_pressed(KeyCode::Space) {
            move_dir += Vec3::Y;
        }
        if input.is_key_pressed(KeyCode::ShiftLeft) {
            move_dir -= Vec3::Y;
        }

        if move_dir.length_squared() > 0.0 {
            self.camera.position += move_dir.normalize() * self.speed * dt;
        }
    }
}
