mod world;
mod render;
mod player;
mod input;

use std::time::Instant;
use glam::Vec3;
use winit::application::ApplicationHandler;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

use world::World;
use render::camera::{Projection, CameraUniform};
use player::Player;
use input::{InputState, MouseButton};

// ── Block type selection ───────────────────────────────────────────────────────
// Maps keyboard digit keys to block IDs from chunk::BlockType
const BLOCK_DIRT:  u8 = 1;
const BLOCK_GRASS: u8 = 2;
const BLOCK_STONE: u8 = 3;

const RENDER_DISTANCE: i32 = 4;
const RAYCAST_REACH:   f32 = 5.0; // blocks

struct App {
    state:          Option<render::State>,
    player:         Option<Player>,
    projection:     Option<Projection>,
    camera_uniform: Option<CameraUniform>,
    input_state:    InputState,
    world:          World,
    last_frame:     Option<Instant>,
    /// Block type the player places on right-click.
    selected_block: u8,
}

impl App {
    fn new() -> Self {
        Self {
            state:          None,
            player:         None,
            projection:     None,
            camera_uniform: None,
            input_state:    InputState::new(),
            world:          World::new(42),
            last_frame:     None,
            selected_block: BLOCK_DIRT,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let attributes = winit::window::Window::default_attributes()
            .with_title("Rust Voxel Game")
            .with_inner_size(winit::dpi::PhysicalSize::new(1280, 720));

        let window = event_loop.create_window(attributes).expect("Failed to create window");
        window.set_cursor_visible(false);
        let _ = window.set_cursor_grab(winit::window::CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(winit::window::CursorGrabMode::Confined));

        let mut state = pollster::block_on(render::State::new(window));

        // Spawn player above terrain so gravity drops them onto the surface
        let player = Player::new(
            Vec3::new(8.0, 22.0, 8.0),
            5.0,   // walk speed (m/s)
            0.003, // mouse sensitivity
        );

        // Initial chunk load around spawn
        let (new_chunks, _) = self.world.update(player.camera.position, RENDER_DISTANCE);
        for coords in new_chunks {
            if let Some(chunk) = self.world.chunks.get(&coords) {
                state.add_chunk_mesh(coords, chunk);
            }
        }

        let projection    = Projection::new(state.size.width, state.size.height, 60.0_f32.to_radians(), 0.1, 500.0);
        let camera_uniform = CameraUniform::new();

        self.state          = Some(state);
        self.player         = Some(player);
        self.projection     = Some(projection);
        self.camera_uniform = Some(camera_uniform);
        self.last_frame     = Some(Instant::now());
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        if let winit::event::DeviceEvent::MouseMotion { delta } = event {
            self.input_state.add_mouse_motion(delta.0, delta.1);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: winit::event::WindowEvent,
    ) {
        match event {
            // ── Quit ──────────────────────────────────────────────────────────
            winit::event::WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            // ── Resize ────────────────────────────────────────────────────────
            winit::event::WindowEvent::Resized(physical_size) => {
                if let (Some(state), Some(proj)) = (&mut self.state, &mut self.projection) {
                    state.resize(physical_size);
                    proj.resize(physical_size.width, physical_size.height);
                }
            }

            // ── Keyboard ──────────────────────────────────────────────────────
            winit::event::WindowEvent::KeyboardInput { event, .. } => {
                if let winit::event::KeyEvent {
                    physical_key: winit::keyboard::PhysicalKey::Code(keycode),
                    state: element_state,
                    ..
                } = event {
                    match element_state {
                        winit::event::ElementState::Pressed => {
                            use winit::keyboard::KeyCode;
                            match keycode {
                                KeyCode::Escape => event_loop.exit(),
                                // Block selector
                                KeyCode::Digit1 => self.selected_block = BLOCK_DIRT,
                                KeyCode::Digit2 => self.selected_block = BLOCK_GRASS,
                                KeyCode::Digit3 => self.selected_block = BLOCK_STONE,
                                _ => {}
                            }
                            self.input_state.key_down(keycode);
                        }
                        winit::event::ElementState::Released => {
                            self.input_state.key_up(keycode);
                        }
                    }
                }
            }

            // ── Mouse buttons ─────────────────────────────────────────────────
            winit::event::WindowEvent::MouseInput { state: btn_state, button, .. } => {
                match btn_state {
                    winit::event::ElementState::Pressed  => self.input_state.mouse_button_down(button),
                    winit::event::ElementState::Released => self.input_state.mouse_button_up(button),
                }
            }

            // ── Main game loop tick (every frame) ─────────────────────────────
            winit::event::WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt  = now.duration_since(self.last_frame.unwrap_or(now)).as_secs_f32();
                self.last_frame = Some(now);

                // Cap dt to avoid huge physics steps after a long freeze
                let dt = dt.min(0.05);

                // ── Player update (physics + input) ───────────────────────────
                // Use .take() so we can hold &mut player while borrowing &self.world
                if let Some(mut player) = self.player.take() {
                    player.update(dt, &mut self.input_state, &self.world);

                    // ── Ray cast: find looked-at block ────────────────────────
                    let hit = self.world.raycast(
                        player.camera.position,
                        player.look_direction(),
                        RAYCAST_REACH,
                    );

                    // ── Block interaction (LMB / RMB) ─────────────────────────
                    if let Some(ref hit) = hit {
                        // Left click → break block
                        if self.input_state.consume_mouse_click(MouseButton::Left) {
                            let p = hit.block_pos;
                            if let Some(chunk_coord) = self.world.set_block_world(p.x, p.y, p.z, 0) {
                                // Rebuild GPU mesh for the modified chunk
                                if let (Some(state), Some(chunk)) = (
                                    &mut self.state,
                                    self.world.chunks.get(&chunk_coord),
                                ) {
                                    state.add_chunk_mesh(chunk_coord, chunk);
                                }
                            }
                        }

                        // Right click → place block on adjacent face
                        if self.input_state.consume_mouse_click(MouseButton::Right) {
                            let p = hit.block_pos + hit.normal;
                            // Don't place inside the player's body
                            if !player_overlaps_block(&player, p) {
                                if let Some(chunk_coord) = self.world.set_block_world(
                                    p.x, p.y, p.z, self.selected_block,
                                ) {
                                    if let (Some(state), Some(chunk)) = (
                                        &mut self.state,
                                        self.world.chunks.get(&chunk_coord),
                                    ) {
                                        state.add_chunk_mesh(chunk_coord, chunk);
                                    }
                                }
                            }
                        }
                    }

                    // ── Update highlight wireframe ─────────────────────────────
                    if let Some(state) = &mut self.state {
                        state.set_highlight(hit.as_ref().map(|h| h.block_pos));
                    }

                    // Put player back
                    self.player = Some(player);
                }

                // ── Dynamic chunk streaming ────────────────────────────────────
                if let Some(player) = &self.player {
                    let (newly_loaded, unloaded) = self.world.update(player.camera.position, RENDER_DISTANCE);
                    if let Some(state) = &mut self.state {
                        for coords in newly_loaded {
                            if let Some(chunk) = self.world.chunks.get(&coords) {
                                state.add_chunk_mesh(coords, chunk);
                            }
                        }
                        for coords in unloaded {
                            state.remove_chunk_mesh(coords);
                        }
                    }
                }

                // ── Camera uniform & render ────────────────────────────────────
                if let (Some(state), Some(player), Some(proj), Some(cam_uni)) = (
                    &mut self.state,
                    &self.player,
                    &self.projection,
                    &mut self.camera_uniform,
                ) {
                    cam_uni.update_view_proj(&player.camera, proj);
                    state.update_camera(cam_uni);

                    match state.render() {
                        Ok(_) => {}
                        Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                        Err(wgpu::SurfaceError::OutOfMemory) => {
                            eprintln!("GPU Out of memory! Exiting…");
                            event_loop.exit();
                        }
                        Err(e) => eprintln!("Render error: {:?}", e),
                    }
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.state = None; // Drop GPU resources before the window closes
    }
}

/// Check whether placing a block at world position `block` would intersect
/// the player's AABB (to prevent placing inside yourself).
fn player_overlaps_block(player: &Player, block: glam::IVec3) -> bool {
    use glam::Vec3;
    const HALF_W: f32 = 0.3;
    const EYE:    f32 = 1.6;
    const HEIGHT: f32 = 1.8;

    let feet = player.camera.position.y - EYE;
    let p_min = Vec3::new(player.camera.position.x - HALF_W, feet,          player.camera.position.z - HALF_W);
    let p_max = Vec3::new(player.camera.position.x + HALF_W, feet + HEIGHT, player.camera.position.z + HALF_W);

    let b_min = Vec3::new(block.x as f32,       block.y as f32,       block.z as f32);
    let b_max = Vec3::new(block.x as f32 + 1.0, block.y as f32 + 1.0, block.z as f32 + 1.0);

    p_min.x < b_max.x && p_max.x > b_min.x &&
    p_min.y < b_max.y && p_max.y > b_min.y &&
    p_min.z < b_max.z && p_max.z > b_min.z
}

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().expect("Failed to create EventLoop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new();
    event_loop.run_app(&mut app).expect("Error running Application Event Loop");
}
