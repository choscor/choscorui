#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputEvent {
    Key { code: u32, pressed: bool },
    PointerMotion { x: f64, y: f64 },
    PointerButton { button: u32, pressed: bool },
    Touch { id: i32, x: f64, y: f64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputState {
    KeyboardUnavailable,
    PointerUnavailable,
    TouchUnavailable,
}

#[derive(Debug, Default)]
pub struct LinuxInput {
    keyboard: bool,
    pointer: bool,
    touch: bool,
    pressed_keys: Vec<u32>,
}

impl LinuxInput {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            keyboard: false,
            pointer: false,
            touch: false,
            pressed_keys: Vec::new(),
        }
    }

    pub fn update_seat(&mut self, keyboard: bool, pointer: bool, touch: bool) {
        self.keyboard = keyboard;
        self.pointer = pointer;
        self.touch = touch;
        if !keyboard {
            self.pressed_keys.clear();
        }
    }

    pub fn dispatch(&mut self, event: InputEvent) -> Result<(), InputState> {
        match event {
            InputEvent::Key { code, pressed } => {
                if !self.keyboard {
                    return Err(InputState::KeyboardUnavailable);
                }
                if pressed && !self.pressed_keys.contains(&code) {
                    self.pressed_keys.push(code);
                }
                if !pressed {
                    self.pressed_keys.retain(|key| *key != code);
                }
            }
            InputEvent::PointerMotion { .. } | InputEvent::PointerButton { .. } => {
                if !self.pointer {
                    return Err(InputState::PointerUnavailable);
                }
            }
            InputEvent::Touch { .. } => {
                if !self.touch {
                    return Err(InputState::TouchUnavailable);
                }
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn pressed_keys(&self) -> &[u32] {
        &self.pressed_keys
    }
}
