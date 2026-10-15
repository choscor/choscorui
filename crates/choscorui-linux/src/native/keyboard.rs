//! Keyboard state on xkbcommon, for both display servers.
//!
//! The display-server code passes key codes (evdev codes plus 8) and
//! modifier masks. `Keyboard` turns them into the `Key` events of core
//! and the committed text, applies compose sequences from the locale,
//! and keeps the auto-repeat deadline of the held key.

use super::types::{ImeEvent, Modifiers, WindowEvent};
use crate::ffi::xkb::{self, Compose, ComposeStatus, Context, Keymap, State};
use choscorui_core::{Key, KeyEvent};
use std::time::{Duration, Instant};

/// The repeat rate and delay of a keyboard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RepeatInfo {
    /// Repeats per second, or 0 for no repeat.
    pub(crate) rate: u32,
    pub(crate) delay: Duration,
}

impl Default for RepeatInfo {
    fn default() -> Self {
        Self {
            rate: 25,
            delay: Duration::from_millis(600),
        }
    }
}

struct Held {
    code: u32,
    next: Instant,
}

/// The keyboard of a seat.
pub(crate) struct Keyboard {
    context: Context,
    state: Option<State>,
    compose: Option<Compose>,
    repeat: RepeatInfo,
    held: Option<Held>,
}

/// The locale of the compose table: the first of `LC_ALL`, `LC_CTYPE`,
/// and `LANG` that is set, else `C`.
fn locale() -> String {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
        .unwrap_or_else(|| "C".to_owned())
}

/// The key of a keysym that has a name in core.
fn named_key(sym: u32) -> Option<Key> {
    use xkb::*;
    Some(match sym {
        KEY_TAB | KEY_ISO_LEFT_TAB => Key::Tab,
        KEY_RETURN | KEY_KP_ENTER => Key::Enter,
        KEY_SPACE | KEY_KP_SPACE => Key::Space,
        KEY_ESCAPE => Key::Escape,
        KEY_UP | KEY_KP_UP => Key::ArrowUp,
        KEY_DOWN | KEY_KP_DOWN => Key::ArrowDown,
        KEY_LEFT | KEY_KP_LEFT => Key::ArrowLeft,
        KEY_RIGHT | KEY_KP_RIGHT => Key::ArrowRight,
        KEY_HOME | KEY_KP_HOME => Key::Home,
        KEY_END | KEY_KP_END => Key::End,
        KEY_PAGE_UP | KEY_KP_PAGE_UP => Key::PageUp,
        KEY_PAGE_DOWN | KEY_KP_PAGE_DOWN => Key::PageDown,
        KEY_BACKSPACE => Key::Backspace,
        KEY_DELETE | KEY_KP_DELETE => Key::Delete,
        KEY_MENU => Key::ContextMenu,
        KEY_F1..=KEY_F20 => Key::F(u8::try_from(sym - KEY_F1 + 1).ok()?),
        _ => return None,
    })
}

/// True for a character that a key event types as text.
fn is_text_character(character: &char) -> bool {
    !character.is_control()
}

impl Keyboard {
    pub(crate) fn new() -> Result<Self, crate::LoadError> {
        let context = Context::new()?;
        let compose = context.compose_from_locale(&locale());
        Ok(Self {
            context,
            state: None,
            compose,
            repeat: RepeatInfo::default(),
            held: None,
        })
    }

    pub(crate) fn context(&self) -> &Context {
        &self.context
    }

    /// Replaces the keymap with a text keymap. A keymap that does not
    /// compile leaves the keyboard without one, so keys are ignored.
    pub(crate) fn set_keymap_text(&mut self, text: &[u8]) {
        self.state = self.context.keymap_from_text(text).and_then(State::new);
        self.held = None;
    }

    /// Replaces the keymap and state, from the X11 functions.
    pub(crate) fn set_state(&mut self, state: Option<State>) {
        self.state = state;
        self.held = None;
    }

    pub(crate) fn keymap(&self) -> Option<&Keymap> {
        self.state.as_ref().map(State::keymap)
    }

    pub(crate) fn set_repeat(&mut self, repeat: RepeatInfo) {
        self.repeat = repeat;
        if repeat.rate == 0 {
            self.held = None;
        }
    }

    pub(crate) fn update_modifiers(
        &mut self,
        depressed: u32,
        latched: u32,
        locked: u32,
        group: u32,
    ) {
        if let Some(state) = &mut self.state {
            state.update_mask(depressed, latched, locked, group);
        }
    }

    pub(crate) fn modifiers(&self) -> Modifiers {
        let Some(state) = &self.state else {
            return Modifiers::default();
        };
        Modifiers {
            shift: state.modifier(c"Shift"),
            control: state.modifier(c"Control"),
            option: state.modifier(c"Mod1"),
            command: state.modifier(c"Mod4"),
            caps_lock: state.modifier(c"Lock"),
        }
    }

    /// The core key of `code` with the current state.
    fn key(&self, code: u32) -> Option<Key> {
        let state = self.state.as_ref()?;
        let sym = state.sym(code);
        if let Some(key) = named_key(sym) {
            return Some(key);
        }
        // A character key is its base character without modifiers,
        // lowercased, as `charactersIgnoringModifiers` is on macOS.
        let base = state
            .keymap()
            .base_sym(code, state.layout(code))
            .and_then(xkb::keysym_char)
            .filter(is_text_character);
        Some(match base {
            Some(character) => Key::Character(character.to_lowercase().next().unwrap_or(character)),
            None => Key::Other(code),
        })
    }

    /// The events of a key press or release.
    pub(crate) fn key_event(&mut self, code: u32, pressed: bool, now: Instant) -> Vec<WindowEvent> {
        let Some(key) = self.key(code) else {
            return Vec::new();
        };
        if !pressed {
            if self.held.as_ref().is_some_and(|held| held.code == code) {
                self.held = None;
            }
            return vec![WindowEvent::Key {
                event: KeyEvent::Up { key },
                modifiers: self.modifiers(),
            }];
        }
        let repeats =
            self.repeat.rate > 0 && self.keymap().is_some_and(|keymap| keymap.repeats(code));
        self.held = repeats.then(|| Held {
            code,
            next: now + self.repeat.delay,
        });
        self.press(code, key, false)
    }

    fn press(&mut self, code: u32, key: Key, repeat: bool) -> Vec<WindowEvent> {
        let modifiers = self.modifiers();
        let mut events = vec![WindowEvent::Key {
            event: KeyEvent::Down { key, repeat },
            modifiers,
        }];
        if modifiers.control || modifiers.option || modifiers.command {
            return events;
        }
        if let Some(text) = self.text(code).filter(|text| !text.is_empty()) {
            events.push(WindowEvent::Ime(ImeEvent::CommitText {
                text,
                replacement_utf16: None,
            }));
        }
        events
    }

    /// The text that `code` types, through the compose sequence.
    fn text(&mut self, code: u32) -> Option<String> {
        let state = self.state.as_ref()?;
        let sym = state.sym(code);
        let plain = || -> String { state.utf8(code).chars().filter(is_text_character).collect() };
        let Some(compose) = &mut self.compose else {
            return Some(plain());
        };
        if !compose.feed(sym) {
            return Some(plain());
        }
        match compose.status() {
            ComposeStatus::Composing => None,
            ComposeStatus::Composed => {
                let text = compose.utf8();
                compose.reset();
                Some(text)
            }
            ComposeStatus::Cancelled => {
                compose.reset();
                None
            }
            ComposeStatus::Nothing => Some(plain()),
        }
    }

    /// Stops the repeat, when the window loses the keyboard.
    pub(crate) fn release_all(&mut self) {
        self.held = None;
        if let Some(compose) = &mut self.compose {
            compose.reset();
        }
    }

    /// The deadline of the next repeat.
    pub(crate) fn next_repeat(&self) -> Option<Instant> {
        self.held.as_ref().map(|held| held.next)
    }

    /// The events of the repeats that are due.
    pub(crate) fn repeat_due(&mut self, now: Instant) -> Vec<WindowEvent> {
        let Some(held) = &mut self.held else {
            return Vec::new();
        };
        if held.next > now || self.repeat.rate == 0 {
            return Vec::new();
        }
        let interval = Duration::from_secs(1) / self.repeat.rate;
        // A late wakeup repeats once, not once for each missed interval.
        while held.next <= now {
            held.next += interval;
        }
        let code = held.code;
        match self.key(code) {
            Some(key) => self.press(code, key, true),
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEYMAP: &str = r#"xkb_keymap {
        xkb_keycodes { include "evdev" };
        xkb_types { include "complete" };
        xkb_compat { include "complete" };
        xkb_symbols { include "pc+us" };
    };"#;

    fn keyboard() -> Keyboard {
        let mut keyboard = Keyboard::new().unwrap();
        keyboard.compose = None;
        keyboard.set_keymap_text(KEYMAP.as_bytes());
        keyboard
    }

    const A: u32 = 30 + 8;
    const LEFT: u32 = 105 + 8;
    const F5: u32 = 63 + 8;

    #[test]
    fn letters_type_text_and_name_their_lowercase_key() {
        let mut keyboard = keyboard();
        keyboard.update_modifiers(1, 0, 0, 0);
        let events = keyboard.key_event(A, true, Instant::now());
        assert_eq!(
            events,
            vec![
                WindowEvent::Key {
                    event: KeyEvent::Down {
                        key: Key::Character('a'),
                        repeat: false
                    },
                    modifiers: Modifiers {
                        shift: true,
                        ..Modifiers::default()
                    },
                },
                WindowEvent::Ime(ImeEvent::CommitText {
                    text: "A".to_owned(),
                    replacement_utf16: None
                }),
            ]
        );
    }

    #[test]
    fn control_chords_type_no_text() {
        let mut keyboard = keyboard();
        // Control is modifier bit 2.
        keyboard.update_modifiers(4, 0, 0, 0);
        let events = keyboard.key_event(A, true, Instant::now());
        assert_eq!(events.len(), 1);
        assert!(keyboard.modifiers().control);
    }

    #[test]
    fn named_keys_map_to_core_keys() {
        let mut keyboard = keyboard();
        let now = Instant::now();
        assert!(matches!(
            keyboard.key_event(LEFT, true, now)[0],
            WindowEvent::Key {
                event: KeyEvent::Down {
                    key: Key::ArrowLeft,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            keyboard.key_event(F5, true, now)[0],
            WindowEvent::Key {
                event: KeyEvent::Down { key: Key::F(5), .. },
                ..
            }
        ));
    }

    #[test]
    fn a_held_key_repeats_after_the_delay_and_stops_on_release() {
        let mut keyboard = keyboard();
        keyboard.set_repeat(RepeatInfo {
            rate: 10,
            delay: Duration::from_millis(300),
        });
        let start = Instant::now();
        keyboard.key_event(A, true, start);
        assert_eq!(
            keyboard.next_repeat(),
            Some(start + Duration::from_millis(300))
        );
        assert!(keyboard.repeat_due(start).is_empty());
        let events = keyboard.repeat_due(start + Duration::from_millis(350));
        assert!(matches!(
            events[0],
            WindowEvent::Key {
                event: KeyEvent::Down { repeat: true, .. },
                ..
            }
        ));
        assert_eq!(
            keyboard.next_repeat(),
            Some(start + Duration::from_millis(400))
        );
        keyboard.key_event(A, false, start);
        assert_eq!(keyboard.next_repeat(), None);
    }
}
