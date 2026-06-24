use std::collections::HashSet;
pub use winit::keyboard::KeyCode;
pub use winit::event::MouseButton;

pub struct InputState {
    keys_pressed: HashSet<KeyCode>,
    mouse_delta: (f64, f64),
    /// Buttons currently held down.
    mouse_held: HashSet<u8>,
    /// Buttons pressed since the last `consume_mouse_click` call (one-shot).
    mouse_just_pressed: HashSet<u8>,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            keys_pressed: HashSet::new(),
            mouse_delta: (0.0, 0.0),
            mouse_held: HashSet::new(),
            mouse_just_pressed: HashSet::new(),
        }
    }

    // ── Keyboard ─────────────────────────────────────────────────────────────

    pub fn key_down(&mut self, keycode: KeyCode) {
        self.keys_pressed.insert(keycode);
    }

    pub fn key_up(&mut self, keycode: KeyCode) {
        self.keys_pressed.remove(&keycode);
    }

    pub fn is_key_pressed(&self, keycode: KeyCode) -> bool {
        self.keys_pressed.contains(&keycode)
    }

    // ── Mouse motion ──────────────────────────────────────────────────────────

    pub fn add_mouse_motion(&mut self, dx: f64, dy: f64) {
        self.mouse_delta.0 += dx;
        self.mouse_delta.1 += dy;
    }

    pub fn take_mouse_delta(&mut self) -> (f64, f64) {
        let delta = self.mouse_delta;
        self.mouse_delta = (0.0, 0.0);
        delta
    }

    // ── Mouse buttons ─────────────────────────────────────────────────────────

    pub fn mouse_button_down(&mut self, button: MouseButton) {
        let b = Self::encode(button);
        self.mouse_held.insert(b);
        self.mouse_just_pressed.insert(b);
    }

    pub fn mouse_button_up(&mut self, button: MouseButton) {
        self.mouse_held.remove(&Self::encode(button));
    }

    /// Returns true once per physical click (clears the flag on read).
    pub fn consume_mouse_click(&mut self, button: MouseButton) -> bool {
        self.mouse_just_pressed.remove(&Self::encode(button))
    }

    /// Discard all pending click events that were not consumed this frame.
    /// Call this at the end of every game-loop tick so that clicks made while
    /// the player is out of reach do not carry over to the next frame.
    pub fn flush_clicks(&mut self) {
        self.mouse_just_pressed.clear();
    }

    #[allow(dead_code)]
    pub fn is_mouse_held(&self, button: MouseButton) -> bool {
        self.mouse_held.contains(&Self::encode(button))
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn encode(button: MouseButton) -> u8 {
        match button {
            MouseButton::Left   => 0,
            MouseButton::Right  => 1,
            MouseButton::Middle => 2,
            _                   => 255,
        }
    }
}
