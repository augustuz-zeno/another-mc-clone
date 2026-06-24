#![allow(clippy::cast_precision_loss)]
use glam::{Mat4, Vec3};

pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,   // in radians
    pub pitch: f32, // in radians
}

impl Camera {
    pub fn new(position: Vec3, yaw: f32, pitch: f32) -> Self {
        Self {
            position,
            yaw,
            pitch,
        }
    }

    pub fn get_view_matrix(&self) -> Mat4 {
        // Forward vector computed in right-handed coordinate space
        let forward = Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        ).normalize();

        Mat4::look_at_rh(self.position, self.position + forward, Vec3::Y)
    }
}

pub struct Projection {
    pub aspect: f32,
    pub fovy: f32,
    pub znear: f32,
    pub zfar: f32,
}

impl Projection {
    pub fn new(width: u32, height: u32, fovy: f32, znear: f32, zfar: f32) -> Self {
        // u32→f32 for pixel dimensions: precision loss only beyond ~16 million pixels,
        // well outside any practical resolution.
        #[allow(clippy::cast_precision_loss)]
        Self {
            aspect: width as f32 / height as f32,
            fovy,
            znear,
            zfar,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        #[allow(clippy::cast_precision_loss)]
        { self.aspect = width as f32 / height as f32; }
    }

    pub fn get_projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(self.fovy, self.aspect, self.znear, self.zfar)
    }
}

/// Bytemuck-compatible uniform struct to upload to a wgpu uniform buffer.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
    pub world_position: [f32; 4], // padded to 16 bytes for WGSL alignment
}

impl CameraUniform {
    pub fn new() -> Self {
        Self {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
            world_position: [0.0; 4],
        }
    }

    pub fn update_view_proj(&mut self, camera: &Camera, projection: &Projection) {
        let view = camera.get_view_matrix();
        let proj = projection.get_projection_matrix();
        self.view_proj = (proj * view).to_cols_array_2d();
        self.world_position = [camera.position.x, camera.position.y, camera.position.z, 1.0];
    }
}

impl Default for CameraUniform {
    fn default() -> Self {
        Self::new()
    }
}
