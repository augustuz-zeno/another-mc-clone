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
use input::InputState;

struct App {
    state: Option<render::State>,
    player: Option<Player>,
    projection: Option<Projection>,
    camera_uniform: Option<CameraUniform>,
    input_state: InputState,
    world: World,
    last_frame: Option<Instant>,
}

impl App {
    fn new() -> Self {
        Self {
            state: None,
            player: None,
            projection: None,
            camera_uniform: None,
            input_state: InputState::new(),
            world: World::new(42), // seed = 42
            last_frame: None,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        // Create window attributes
        let attributes = winit::window::Window::default_attributes()
            .with_title("Rust Modular Voxel Game (wgpu)")
            .with_inner_size(winit::dpi::PhysicalSize::new(1280, 720));

        let window = event_loop.create_window(attributes).expect("Failed to create winit window");

        // Hide and grab mouse cursor for FPS look controls
        window.set_cursor_visible(false);
        let _ = window.set_cursor_grab(winit::window::CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(winit::window::CursorGrabMode::Confined));

        // State::new is async, so we block on it during initialization using pollster
        let mut state = pollster::block_on(render::State::new(window));

        // Position camera to look at the generated terrain.
        let mut player = Player::new(
            Vec3::new(8.0, 18.0, 24.0),
            12.0, // movement speed (units/sec)
            0.003, // mouse look sensitivity
        );
        // Look down towards the center
        player.camera.pitch = -0.4;
        player.camera.yaw = -std::f32::consts::FRAC_PI_2; // looking down -z

        // Initial chunk generation around player spawn point
        const RENDER_DISTANCE: i32 = 2;
        let (new_chunks, _) = self.world.update(player.camera.position, RENDER_DISTANCE);
        for coords in new_chunks {
            if let Some(chunk) = self.world.chunks.get(&coords) {
                state.add_chunk_mesh(coords, chunk);
            }
        }

        let projection = Projection::new(state.size.width, state.size.height, 60.0f32.to_radians(), 0.1, 100.0);
        let camera_uniform = CameraUniform::new();

        self.state = Some(state);
        self.player = Some(player);
        self.projection = Some(projection);
        self.camera_uniform = Some(camera_uniform);
        self.last_frame = Some(Instant::now());
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        if let winit::event::DeviceEvent::MouseMotion { delta } = event {
            // Accumulate relative mouse movements for looking
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
            winit::event::WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            winit::event::WindowEvent::Resized(physical_size) => {
                if let (Some(state), Some(projection)) = (&mut self.state, &mut self.projection) {
                    state.resize(physical_size);
                    projection.resize(physical_size.width, physical_size.height);
                }
            }
            winit::event::WindowEvent::KeyboardInput { event, .. } => {
                if let winit::event::KeyEvent {
                    physical_key: winit::keyboard::PhysicalKey::Code(keycode),
                    state: element_state,
                    ..
                } = event {
                    match element_state {
                        winit::event::ElementState::Pressed => {
                            if keycode == winit::keyboard::KeyCode::Escape {
                                event_loop.exit();
                            }
                            self.input_state.key_down(keycode);
                        }
                        winit::event::ElementState::Released => {
                            self.input_state.key_up(keycode);
                        }
                    }
                }
            }
            winit::event::WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = now.duration_since(self.last_frame.unwrap_or(now)).as_secs_f32();
                self.last_frame = Some(now);

                // Update player camera position/angle
                if let Some(player) = &mut self.player {
                    player.update(dt, &mut self.input_state);

                    // Update active world chunks around player dynamically
                    const RENDER_DISTANCE: i32 = 2;
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

                // Update uniform buffer and request redraw
                if let (Some(state), Some(player), Some(projection), Some(camera_uniform)) = (
                    &mut self.state,
                    &self.player,
                    &self.projection,
                    &mut self.camera_uniform,
                ) {
                    camera_uniform.update_view_proj(&player.camera, projection);
                    state.update_camera(camera_uniform);

                    match state.render() {
                        Ok(_) => {}
                        Err(wgpu::SurfaceError::Lost) => {
                            state.resize(state.size);
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => {
                            eprintln!("GPU Out of memory! Exiting...");
                            event_loop.exit();
                        }
                        Err(e) => {
                            eprintln!("Render error: {:?}", e);
                        }
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
        // Drop GPU resources explicitly before the window is destroyed
        self.state = None;
    }
}

fn main() {
    env_logger::init();
    
    let event_loop = EventLoop::new().expect("Failed to create EventLoop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new();
    event_loop.run_app(&mut app).expect("Error running Application Event Loop");
}
