use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use crate::game::{RENDER_DISTANCE, RAYCAST_REACH, SPAWN_Y};
use crate::game::{handle_break, handle_place};
use crate::player::Player;
use crate::input::{InputState, MouseButton, KeyCode};
use crate::world::{World, BlockType};
use crate::render::{self, camera::{Projection, CameraUniform}};

pub struct App {
    state:          Option<render::State>,
    player:         Option<Player>,
    projection:     Option<Projection>,
    camera_uniform: Option<CameraUniform>,
    hand_uniform:   CameraUniform,
    input_state:    InputState,
    world:          World,
    last_frame:     Option<Instant>,
    selected_block: BlockType,
}

impl App {
    pub fn new() -> Self {
        Self {
            state:          None,
            player:         None,
            projection:     None,
            camera_uniform: None,
            hand_uniform:   CameraUniform::new(),
            input_state:    InputState::new(),
            world:          World::new(42),
            last_frame:     None,
            selected_block: BlockType::Dirt,
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

        let player = Player::new(
            glam::Vec3::new(8.0, SPAWN_Y, 8.0),
            crate::game::MOUSE_SENSITIVITY,
        );

        let (new_chunks, _) = self.world.update(player.camera.position, RENDER_DISTANCE);
        for coords in new_chunks {
            if let Some(chunk) = self.world.chunks.get(&coords) {
                state.add_chunk_mesh(coords, chunk);
            }
        }

        let projection = Projection::new(state.size.width, state.size.height, 110.0_f32.to_radians(), 0.1, 500.0);
        let camera_uniform = CameraUniform::new();

        self.state          = Some(state);
        self.player         = Some(player);
        self.projection     = Some(projection);
        self.camera_uniform = Some(camera_uniform);
        self.last_frame     = Some(Instant::now());
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: winit::event::DeviceId, event: winit::event::DeviceEvent) {
        if let winit::event::DeviceEvent::MouseMotion { delta } = event {
            self.input_state.add_mouse_motion(delta.0, delta.1);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: winit::event::WindowEvent) {
        match event {
            winit::event::WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            winit::event::WindowEvent::Resized(physical_size) => {
                if let (Some(state), Some(proj)) = (&mut self.state, &mut self.projection) {
                    state.resize(physical_size);
                    proj.resize(physical_size.width, physical_size.height);
                }
            }
            winit::event::WindowEvent::KeyboardInput { event: winit::event::KeyEvent { physical_key: winit::keyboard::PhysicalKey::Code(keycode), state: element_state, .. }, .. } => {
                match element_state {
                    winit::event::ElementState::Pressed => {
                        match keycode {
                            KeyCode::Escape => event_loop.exit(),
                            KeyCode::Digit1 => self.selected_block = BlockType::Dirt,
                            KeyCode::Digit2 => self.selected_block = BlockType::Grass,
                            KeyCode::Digit3 => self.selected_block = BlockType::Stone,
                            _ => {}
                        }
                        self.input_state.key_down(keycode);
                    }
                    winit::event::ElementState::Released => {
                        self.input_state.key_up(keycode);
                    }
                }
            }
            winit::event::WindowEvent::MouseInput { state: btn_state, button, .. } => {
                match btn_state {
                    winit::event::ElementState::Pressed  => self.input_state.mouse_button_down(button),
                    winit::event::ElementState::Released => self.input_state.mouse_button_up(button),
                }
            }
            winit::event::WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt  = now.duration_since(self.last_frame.unwrap_or(now)).as_secs_f32().min(0.05);
                self.last_frame = Some(now);

                if let Some(mut player) = self.player.take() {
                    player.update(dt, &mut self.input_state, &self.world);

                    let hit = self.world.raycast(player.camera.position, player.look_direction(), RAYCAST_REACH);

                    if let Some(ref hit) = hit {
                        if self.input_state.consume_mouse_click(MouseButton::Left)
                            && let Some(chunk_coord) = handle_break(&mut self.world, hit.block_pos)
                                && let (Some(state), Some(chunk)) = (&mut self.state, self.world.chunks.get(&chunk_coord)) {
                                    state.add_chunk_mesh(chunk_coord, chunk);
                                }
                        if self.input_state.consume_mouse_click(MouseButton::Right)
                            && let Some(chunk_coord) = handle_place(&mut self.world, hit.block_pos, hit.normal, self.selected_block.to_u8(), &player)
                                && let (Some(state), Some(chunk)) = (&mut self.state, self.world.chunks.get(&chunk_coord)) {
                                    state.add_chunk_mesh(chunk_coord, chunk);
                                }
                    }

                    self.input_state.flush_clicks();

                    if let Some(state) = &mut self.state {
                        state.set_highlight(hit.as_ref().map(|h| h.block_pos));
                    }

                    self.player = Some(player);
                }

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

                if let (Some(state), Some(player), Some(proj), Some(cam_uni)) = (
                    &mut self.state, &self.player, &mut self.projection, &mut self.camera_uniform
                ) {
                    proj.fovy = (110.0_f32 * player.fov_multiplier).to_radians();
                    cam_uni.update_view_proj(&player.camera, proj);
                    state.update_camera(cam_uni);
                    state.update_clouds(player.camera.position, dt);

                    state.set_hand_block(self.selected_block.to_u8());
                    let bob_walked = player.distance_walked;
                    let bob_y = (bob_walked * 15.0).sin() * 0.05;
                    let bob_x = (bob_walked * 7.5).cos() * 0.05;
                    let hand_view = glam::Mat4::look_at_rh(glam::Vec3::ZERO, glam::Vec3::new(0.0, 0.0, -1.0), glam::Vec3::Y);
                    let hand_proj_matrix = glam::Mat4::perspective_rh(60.0_f32.to_radians(), proj.aspect, 0.01, 10.0);
                    let hand_model = glam::Mat4::from_translation(glam::Vec3::new(0.6 + bob_x, -0.6 + bob_y, -1.2))
                        * glam::Mat4::from_rotation_y(-0.5) * glam::Mat4::from_rotation_x(0.3)
                        * glam::Mat4::from_scale(glam::Vec3::splat(0.3))
                        * glam::Mat4::from_translation(glam::Vec3::new(-0.5, -0.5, -0.5));
                    
                    self.hand_uniform.view_proj = (hand_proj_matrix * hand_view * hand_model).to_cols_array_2d();
                    state.update_hand_camera(&self.hand_uniform);

                    match state.render() {
                        Ok(()) => {}
                        Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                        Err(wgpu::SurfaceError::OutOfMemory) => {
                            eprintln!("GPU Out of memory! Exiting…");
                            event_loop.exit();
                        }
                        Err(e) => eprintln!("Render error: {e:?}"),
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
        self.state = None;
    }
}
