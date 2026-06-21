use std::collections::HashSet;
pub use winit::keyboard::KeyCode;

pub struct InputState {
    keys_pressed: HashSet<KeyCode>,
    mouse_delta: (f64, f64),
}

impl InputState {
    pub fn new() -> Self {
        Self {
            keys_pressed: HashSet::new(),
            mouse_delta: (0.0, 0.0),
        }
    }

    pub fn key_down(&mut self, keycode: KeyCode) {
        self.keys_pressed.insert(keycode);
    }

    pub fn key_up(&mut self, keycode: KeyCode) {
        self.keys_pressed.remove(&keycode);
    }

    pub fn is_key_pressed(&self, keycode: KeyCode) -> bool {
        self.keys_pressed.contains(&keycode)
    }

    pub fn add_mouse_motion(&mut self, dx: f64, dy: f64) {
        self.mouse_delta.0 += dx;
        self.mouse_delta.1 += dy;
    }

    pub fn take_mouse_delta(&mut self) -> (f64, f64) {
        let delta = self.mouse_delta;
        self.mouse_delta = (0.0, 0.0);
        delta
    }
}
