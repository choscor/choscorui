//! Safe Windows backend surface and platform-neutral Win32 message translation.

use choscorui_core::{Key, KeyEvent, Point, PointerButton, PointerEvent, Size, WheelUnit};

mod render;
pub use render::{
    DeviceEvent, DeviceResources, FrameResult, RenderOp, RenderPlan, RenderPlanError,
};

#[cfg(target_os = "windows")]
mod native;
#[cfg(target_os = "windows")]
pub use native::{Application, Error, Poster, Window, WindowOptions};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub system: bool,
    pub caps_lock: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WindowEvent {
    Resized {
        size: Size,
        scale: f64,
    },
    ScaleChanged {
        scale: f64,
    },
    Focused(bool),
    Closing,
    Pointer {
        event: PointerEvent,
        modifiers: Modifiers,
    },
    Key {
        event: KeyEvent,
        modifiers: Modifiers,
    },
    Text {
        text: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl NativeRect {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Result<Self, TranslateError> {
        if right <= left || bottom <= top {
            return Err(TranslateError::InvalidRectangle);
        }
        Ok(Self {
            left,
            top,
            right,
            bottom,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NativeMessage {
    Resized {
        width: u32,
        height: u32,
    },
    DpiChanged {
        dpi: u32,
        suggested_frame: NativeRect,
        client_width: u32,
        client_height: u32,
    },
    Focused(bool),
    CloseRequested,
    PointerDown {
        x: i32,
        y: i32,
        button: usize,
        modifiers: Modifiers,
    },
    PointerUp {
        x: i32,
        y: i32,
        button: usize,
        modifiers: Modifiers,
    },
    PointerMoved {
        x: i32,
        y: i32,
        modifiers: Modifiers,
    },
    Wheel {
        x: i32,
        y: i32,
        delta_x: f64,
        delta_y: f64,
        precise: bool,
        modifiers: Modifiers,
    },
    KeyDown {
        virtual_key: u32,
        repeat: bool,
        modifiers: Modifiers,
    },
    KeyUp {
        virtual_key: u32,
        modifiers: Modifiers,
    },
    Character(u16),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TranslateError {
    InvalidDpi,
    InvalidRectangle,
}

#[derive(Clone, Copy, Debug)]
pub struct DpiState {
    dpi: u32,
    suggested_frame: Option<NativeRect>,
}

impl DpiState {
    pub fn new(dpi: u32) -> Result<Self, TranslateError> {
        if dpi == 0 {
            return Err(TranslateError::InvalidDpi);
        }
        Ok(Self {
            dpi,
            suggested_frame: None,
        })
    }

    pub fn scale(self) -> f64 {
        f64::from(self.dpi) / 96.0
    }

    pub fn logical_size(self, width: u32, height: u32) -> Size {
        Size::new(
            f64::from(width) / self.scale(),
            f64::from(height) / self.scale(),
        )
    }

    pub fn translate(
        &mut self,
        message: NativeMessage,
    ) -> Result<Vec<WindowEvent>, TranslateError> {
        match message {
            NativeMessage::DpiChanged {
                dpi,
                suggested_frame,
                client_width,
                client_height,
            } => {
                if dpi == 0 {
                    return Err(TranslateError::InvalidDpi);
                }
                self.dpi = dpi;
                self.suggested_frame = Some(suggested_frame);
                let scale = self.scale();
                Ok(vec![
                    WindowEvent::ScaleChanged { scale },
                    WindowEvent::Resized {
                        size: self.logical_size(client_width, client_height),
                        scale,
                    },
                ])
            }
            NativeMessage::Resized { width, height } => Ok(vec![WindowEvent::Resized {
                size: self.logical_size(width, height),
                scale: self.scale(),
            }]),
            _ => Ok(Vec::new()),
        }
    }

    pub fn take_suggested_frame(&mut self) -> Option<NativeRect> {
        self.suggested_frame.take()
    }
}

#[derive(Debug)]
pub struct InputTranslator {
    dpi: DpiState,
    pending_high_surrogate: Option<u16>,
}

impl InputTranslator {
    pub fn new(dpi: u32) -> Result<Self, TranslateError> {
        Ok(Self {
            dpi: DpiState::new(dpi)?,
            pending_high_surrogate: None,
        })
    }

    pub fn translate(&mut self, message: NativeMessage) -> Vec<WindowEvent> {
        match message {
            NativeMessage::Resized { width, height } => vec![WindowEvent::Resized {
                size: self.dpi.logical_size(width, height),
                scale: self.dpi.scale(),
            }],
            NativeMessage::DpiChanged { .. } => self.dpi.translate(message).unwrap_or_default(),
            NativeMessage::Focused(focused) => {
                if !focused {
                    self.pending_high_surrogate = None;
                }
                vec![WindowEvent::Focused(focused)]
            }
            NativeMessage::CloseRequested => vec![WindowEvent::Closing],
            NativeMessage::PointerDown {
                x,
                y,
                button,
                modifiers,
            } => vec![WindowEvent::Pointer {
                event: PointerEvent::Down {
                    position: self.logical_point(x, y),
                    button: pointer_button(button),
                },
                modifiers,
            }],
            NativeMessage::PointerUp {
                x,
                y,
                button,
                modifiers,
            } => vec![WindowEvent::Pointer {
                event: PointerEvent::Up {
                    position: self.logical_point(x, y),
                    button: pointer_button(button),
                },
                modifiers,
            }],
            NativeMessage::PointerMoved { x, y, modifiers } => vec![WindowEvent::Pointer {
                event: PointerEvent::Move {
                    position: self.logical_point(x, y),
                },
                modifiers,
            }],
            NativeMessage::Wheel {
                x,
                y,
                delta_x,
                delta_y,
                precise,
                modifiers,
            } => {
                let divisor = if precise { self.dpi.scale() } else { 120.0 };
                vec![WindowEvent::Pointer {
                    event: PointerEvent::Wheel {
                        position: self.logical_point(x, y),
                        delta: Point::new(delta_x / divisor, delta_y / divisor),
                        unit: if precise {
                            WheelUnit::Points
                        } else {
                            WheelUnit::Lines
                        },
                    },
                    modifiers,
                }]
            }
            NativeMessage::KeyDown {
                virtual_key,
                repeat,
                modifiers,
            } => vec![WindowEvent::Key {
                event: KeyEvent::Down {
                    key: key(virtual_key),
                    repeat,
                },
                modifiers,
            }],
            NativeMessage::KeyUp {
                virtual_key,
                modifiers,
            } => vec![WindowEvent::Key {
                event: KeyEvent::Up {
                    key: key(virtual_key),
                },
                modifiers,
            }],
            NativeMessage::Character(unit) => self.character(unit),
        }
    }

    fn logical_point(&self, x: i32, y: i32) -> Point {
        Point::new(
            f64::from(x) / self.dpi.scale(),
            f64::from(y) / self.dpi.scale(),
        )
    }

    fn character(&mut self, unit: u16) -> Vec<WindowEvent> {
        if (0xd800..=0xdbff).contains(&unit) {
            self.pending_high_surrogate = Some(unit);
            return Vec::new();
        }
        let units = self.pending_high_surrogate.take().map_or_else(
            || vec![unit],
            |high| {
                if (0xdc00..=0xdfff).contains(&unit) {
                    vec![high, unit]
                } else {
                    vec![unit]
                }
            },
        );
        String::from_utf16(&units)
            .ok()
            .map(|text| vec![WindowEvent::Text { text }])
            .unwrap_or_default()
    }
}

fn pointer_button(button: usize) -> PointerButton {
    match button {
        1 => PointerButton::Primary,
        2 => PointerButton::Secondary,
        3 => PointerButton::Middle,
        other => PointerButton::Other(other),
    }
}

fn key(virtual_key: u32) -> Key {
    match virtual_key {
        0x09 => Key::Tab,
        0x0d => Key::Enter,
        0x20 => Key::Space,
        0x1b => Key::Escape,
        0x25 => Key::ArrowLeft,
        0x26 => Key::ArrowUp,
        0x27 => Key::ArrowRight,
        0x28 => Key::ArrowDown,
        0x24 => Key::Home,
        0x23 => Key::End,
        0x08 => Key::Backspace,
        0x2e => Key::Delete,
        // VK_F1 to VK_F24.
        0x70..=0x87 => Key::F((virtual_key - 0x6f) as u8),
        // VK_APPS: the Menu key.
        0x5d => Key::ContextMenu,
        other => Key::Other(other),
    }
}
