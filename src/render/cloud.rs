//! Cloud rendering state.
//!
//! Manages a pre-allocated GPU vertex buffer for clouds.
//! Geometry is generated dynamically based on the cloud map and player position,
//! but uses `write_buffer` instead of reallocation each frame.

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CloudVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

impl CloudVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x3
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

// Maximum number of CloudVertex that can fit in the pre-allocated cloud buffer.
// RADIUS=12 → up to 25×25 = 625 patches, each up to 6 faces × 6 verts = 36 verts.
const MAX_CLOUD_VERTS: usize = 625 * 36;

pub struct CloudState {
    pub vertex_buffer: wgpu::Buffer,
    pub vertex_count: u32,
    pub offset: f32, // drifts slowly over time (world units)
    map: Vec<bool>,
    map_size: (usize, usize),
}

impl CloudState {
    pub fn new(device: &wgpu::Device) -> Self {
        let mut map = Vec::new();
        let mut map_size = (0, 0);
        // We use the same path resolution technique here or we can just expect it to work if run from the right directory.
        // Actually, we'll fix the asset path robustness in `render/mod.rs` or here using a helper.
        // For now we'll use the helper if we pass it, but let's use `current_exe` logic directly or wait.
        // The implementation plan says "use helper asset_path()". Let's assume there's a helper or we just do it inline here.
        // Let's use `include_bytes!` for the cloud map to make it super robust, or just read the file robustly.
        // To be safe, we'll try to read it dynamically but fall back to hash-based generation.
        // Wait, the plan said "asset_path() helper". I'll just use a relative path here, and fix it in `asset_path` helper in `render/mod.rs` and pass the path, or just use `std::env::current_exe`.
        // Let's implement robust path resolution inline for now.
        
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
            .unwrap_or_else(|| std::path::PathBuf::from("."));
            
        // Search in a few possible locations (cargo run vs compiled binary)
        let possible_paths = [
            std::path::PathBuf::from("src/assets/textures/environment/clouds.png"),
            exe_dir.join("assets/textures/environment/clouds.png"),
            exe_dir.join("../../src/assets/textures/environment/clouds.png"),
        ];

        let mut loaded_img = None;
        for path in &possible_paths {
            if let Ok(img) = image::open(path) {
                loaded_img = Some(img);
                break;
            }
        }

        if let Some(img) = loaded_img {
            let rgba = img.to_rgba8();
            map_size = (rgba.width() as usize, rgba.height() as usize);
            map.reserve(map_size.0 * map_size.1);
            for pixel in rgba.pixels() {
                map.push(pixel[3] > 10 && pixel[0] > 10);
            }
        }

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cloud VB"),
            size: (MAX_CLOUD_VERTS * std::mem::size_of::<CloudVertex>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            vertex_buffer,
            vertex_count: 0,
            offset: 0.0,
            map,
            map_size,
        }
    }

    pub fn update(&mut self, queue: &wgpu::Queue, player_pos: glam::Vec3, dt: f32) {
        self.offset += dt * 2.0;

        let cloud_y: f32 = 128.0;
        const PATCH_W: f32 = 12.0;
        const PATCH_D: f32 = 12.0;
        const CLOUD_H: f32 = 4.0;
        const SPACING: f32 = 12.0;
        const RADIUS: i32 = 12;

        let is_cloud = |ix: i32, iz: i32| -> bool {
            if self.map.is_empty() {
                let v = (ix.wrapping_mul(73_856_093)) ^ (iz.wrapping_mul(19_349_663));
                (v & 3) != 0
            } else {
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                let w = self.map_size.0 as i32;
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                let h = self.map_size.1 as i32;
                let u = ix.rem_euclid(w) as usize;
                let v = iz.rem_euclid(h) as usize;
                self.map[v * self.map_size.0 + u]
            }
        };

        let mut verts: Vec<CloudVertex> = Vec::with_capacity(MAX_CLOUD_VERTS);

        #[allow(clippy::cast_possible_truncation)]
        let cx = ((player_pos.x + self.offset) / SPACING).floor() as i32;
        #[allow(clippy::cast_possible_truncation)]
        let cz = (player_pos.z / SPACING).floor() as i32;

        for di in -RADIUS..=RADIUS {
            for dk in -RADIUS..=RADIUS {
                let pi = cx + di;
                let pk = cz + dk;
                if !is_cloud(pi, pk) { continue; }

                #[allow(clippy::cast_precision_loss)]
                let wx = pi as f32 * SPACING - self.offset;
                #[allow(clippy::cast_precision_loss)]
                let wz = pk as f32 * SPACING;

                let x0 = wx - PATCH_W * 0.5;
                let x1 = wx + PATCH_W * 0.5;
                let z0 = wz - PATCH_D * 0.5;
                let z1 = wz + PATCH_D * 0.5;
                let y0 = cloud_y;
                let y1 = cloud_y + CLOUD_H;

                macro_rules! push_quad {
                    ($a:expr, $b:expr, $c:expr, $d:expr, $col:expr) => {{
                        verts.push(CloudVertex { position: $a, color: $col });
                        verts.push(CloudVertex { position: $b, color: $col });
                        verts.push(CloudVertex { position: $c, color: $col });
                        verts.push(CloudVertex { position: $a, color: $col });
                        verts.push(CloudVertex { position: $c, color: $col });
                        verts.push(CloudVertex { position: $d, color: $col });
                    }}
                }

                // Top
                push_quad!([x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [1.0, 1.0, 1.0]);
                // Bottom
                push_quad!([x0, y0, z1], [x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [0.6, 0.6, 0.6]);

                if !is_cloud(pi, pk + 1) { push_quad!([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1], [0.8, 0.8, 0.8]); }
                if !is_cloud(pi, pk - 1) { push_quad!([x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [0.8, 0.8, 0.8]); }
                if !is_cloud(pi - 1, pk) { push_quad!([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0], [0.8, 0.8, 0.8]); }
                if !is_cloud(pi + 1, pk) { push_quad!([x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [0.8, 0.8, 0.8]); }

                if verts.len() + 36 > MAX_CLOUD_VERTS { break; }
            }
        }

        if verts.is_empty() {
            self.vertex_count = 0;
            return;
        }

        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        #[allow(clippy::cast_possible_truncation)]
        { self.vertex_count = verts.len() as u32; }
    }
}
