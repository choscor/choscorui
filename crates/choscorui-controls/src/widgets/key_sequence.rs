//! Keyboard shortcut capture and the text of a shortcut.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::form::input::default_width;
use crate::style::{Content, ControlKind, PaintContext, Slot, StyleQuery, TextAlign, VisualState};
use crate::{Shortcut, ShortcutModifiers};
use choscorui_core::{
    Catalog, EventPhase, EventReply, Icon, Key, KeyEvent, Locale, Localizer, Measured,
    PointerButton, PointerEvent, Rect, SemanticAction, SemanticNode, SemanticRole, SemanticState,
    Size,
};

/// The pause after a chord, in seconds, that commits a shorter sequence
/// in a control host.
const PAUSE: f64 = 1.0;

/// Records up to `max_chords` shortcuts. Presses of keys without a
/// `Key` mapping (`Key::Other`, which includes modifier-only presses) are
/// ignored. The first chord needs Command, Control, or Option, except a
/// function key. Plain Return commits the chords so far, and Tab ends the
/// recording; neither is a chord. Backspace with no modifiers clears;
/// Escape cancels and keeps the previous sequence; recording finishes
/// when full or on [`KeySequenceEdit::finish`] (for example after a pause
/// or focus loss).
#[derive(Clone, Debug, PartialEq)]
pub struct KeySequenceEdit {
    committed: Vec<Shortcut>,
    recording: Option<Vec<Shortcut>>,
    max_chords: usize,
    /// The name and the placeholder in a control host.
    name: String,
    placeholder: String,
    /// The host time at which a pause commits the recording.
    deadline: Option<f64>,
    /// Shows a clear button while a sequence is set.
    clear_button: bool,
    /// The layout direction of a hosted field, from `arrange`.
    rtl: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeySequenceChange {
    None,
    Recording,
    Committed,
    Cancelled,
    Cleared,
    /// The key cannot start a chord: it has no Command, Control, or
    /// Option, and it is not a function key.
    Rejected,
}

impl KeySequenceEdit {
    pub fn new(max_chords: usize) -> Self {
        Self {
            committed: Vec::new(),
            recording: None,
            max_chords: max_chords.clamp(1, 4),
            name: String::new(),
            placeholder: String::new(),
            deadline: None,
            clear_button: false,
            rtl: false,
        }
    }

    /// The accessible name in a control host.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// The text that an empty sequence shows in a control host.
    #[must_use]
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Shows a clear button at the end of a hosted field while a sequence
    /// is set. A click on it clears the sequence.
    #[must_use]
    pub fn with_clear_button(mut self, clear_button: bool) -> Self {
        self.clear_button = clear_button;
        self
    }
    pub fn sequence(&self) -> &[Shortcut] {
        &self.committed
    }
    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }
    pub fn start(&mut self) {
        self.recording = Some(Vec::new());
    }
    pub fn press(&mut self, key: Key, modifiers: ShortcutModifiers) -> KeySequenceChange {
        let Some(recording) = &mut self.recording else {
            return KeySequenceChange::None;
        };
        let plain = modifiers == ShortcutModifiers::default();
        // Shift alone does not make a shortcut: Shift+A types "A".
        let unmodified = !(modifiers.command || modifiers.control || modifiers.option);
        match key {
            Key::Escape if plain => {
                self.recording = None;
                KeySequenceChange::Cancelled
            }
            Key::Backspace | Key::Delete if plain && recording.is_empty() => self.clear(),
            Key::Enter if plain && recording.is_empty() => KeySequenceChange::None,
            Key::Enter if plain => self.finish(),
            Key::Tab if unmodified => self.finish(),
            Key::Other(_) => KeySequenceChange::None,
            Key::F(_) => self.push(Shortcut { key, modifiers }),
            _ if unmodified && recording.is_empty() => KeySequenceChange::Rejected,
            _ => self.push(Shortcut { key, modifiers }),
        }
    }

    /// Adds `chord` to the recording, and finishes when it is full.
    fn push(&mut self, chord: Shortcut) -> KeySequenceChange {
        let Some(recording) = &mut self.recording else {
            return KeySequenceChange::None;
        };
        recording.push(chord);
        if recording.len() >= self.max_chords {
            self.finish()
        } else {
            KeySequenceChange::Recording
        }
    }

    fn clear(&mut self) -> KeySequenceChange {
        self.committed.clear();
        self.recording = None;
        KeySequenceChange::Cleared
    }

    pub fn finish(&mut self) -> KeySequenceChange {
        match self.recording.take() {
            Some(recording) if !recording.is_empty() => {
                self.committed = recording;
                KeySequenceChange::Committed
            }
            Some(_) => KeySequenceChange::Cancelled,
            None => KeySequenceChange::None,
        }
    }
    /// macOS-style text such as "⌃⌥⇧⌘S, ⌘K".
    pub fn text(&self) -> String {
        self.committed
            .iter()
            .map(|shortcut| shortcut_text(*shortcut))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The text of the sequence in the language of `strings`: the key
    /// names, such as Space, are localized.
    fn localized_text(&self, strings: &Localizer) -> String {
        self.committed
            .iter()
            .map(|shortcut| compose_text(*shortcut, Some(strings)))
            .collect::<Vec<_>>()
            .join(", ")
    }
    /// The semantics, with the recording hint in the language of
    /// `strings`.
    pub fn semantics(&self, name: &str, strings: &Localizer) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::TextField,
            name: name.to_owned(),
            description: if self.is_recording() {
                strings.message("choscorui.shortcut.recording", &[])
            } else {
                String::new()
            },
            value: Some(self.localized_text(strings)),
            state: SemanticState {
                read_only: true,
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions: vec![SemanticAction::Activate, SemanticAction::Focus],
            text: None,
        }
    }

    /// Paints the input box with one `Kbd` key cap per chord, or
    /// `placeholder` when the sequence is empty. While recording, the
    /// chords so far show, or a prompt to type a shortcut. `host` carries
    /// focus flags.
    pub fn paint(&self, cx: &mut PaintContext<'_>, host: VisualState, placeholder: &str) {
        let input = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root).state(host));
        let muted = cx.style(StyleQuery::new(ControlKind::Input, Slot::Placeholder));
        let cap = cx.style(StyleQuery::new(ControlKind::Kbd, Slot::Root));
        let group = cx.style(StyleQuery::new(ControlKind::Kbd, Slot::Group));
        let chords = self.recording.as_ref().unwrap_or(&self.committed);
        let bounds = cx.bounds;
        let prompt = cx.strings.message("choscorui.shortcut.prompt", &[]);
        cx.slot(bounds, &input, |cx, inner| {
            if chords.is_empty() {
                let text = if self.is_recording() {
                    &prompt
                } else {
                    placeholder
                };
                cx.label(inner, text, &muted, TextAlign::Start);
                return;
            }
            let mut x = if cx.rtl {
                inner.max_x()
            } else {
                inner.origin.x
            };
            let height = cap.min_height.min(inner.size.height);
            let y = inner.origin.y + (inner.size.height - height) / 2.0;
            for chord in chords {
                let text = compose_text(*chord, Some(cx.strings));
                let width = Content::label(&text).size(&cap, cx.text).width;
                let left = if cx.rtl { x - width } else { x };
                let rect = Rect::from_xywh(left, y, width, height);
                cx.slot(rect, &cap, |cx, inner| {
                    cx.label(inner, &text, &cap, TextAlign::Center);
                });
                x = if cx.rtl {
                    x - width - group.gap
                } else {
                    x + width + group.gap
                };
            }
        });
    }
}

impl KeySequenceEdit {
    /// Reports a committed or cleared sequence in a host, and beeps for a
    /// rejected key. A new chord starts the pause that commits.
    fn report(&mut self, cx: &mut ControlContext<'_>, change: KeySequenceChange) {
        match change {
            KeySequenceChange::Committed | KeySequenceChange::Cleared => {
                cx.emit(ControlMessage::Changed(ControlValue::Text(self.text())));
            }
            KeySequenceChange::Rejected => cx.beep(),
            _ => {}
        }
        self.deadline = (change == KeySequenceChange::Recording).then(|| cx.time() + PAUSE);
        cx.request_paint();
        cx.request_semantics();
    }

    fn start_recording(&mut self, cx: &mut ControlContext<'_>) {
        if !self.is_recording() {
            self.start();
            cx.request_paint();
            cx.request_semantics();
        }
    }

    /// The clear button at the end of `bounds`, while it shows.
    fn clear_rect(&self, bounds: Rect) -> Option<Rect> {
        if !self.clear_button || self.is_recording() || self.committed.is_empty() {
            return None;
        }
        let side = bounds.size.height.min(bounds.size.width);
        let x = if self.rtl {
            bounds.origin.x
        } else {
            bounds.max_x() - side
        };
        Some(Rect::from_xywh(
            x,
            bounds.origin.y,
            side,
            bounds.size.height,
        ))
    }
}

/// A click, Space, Enter, or `Activate` starts the recording. While it
/// records, the control takes every key but Tab, and a pause commits the
/// chords so far. A loss of focus finishes the recording. Plain Backspace
/// or Delete clears a set sequence. It reports `Changed(Text)` with the
/// sequence text.
impl CustomControl for KeySequenceEdit {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root));
        let height = (style.text.line_height + style.padding.vertical()).max(style.min_height);
        Measured::new(Size::new(default_width(cx, &style), height))
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, _bounds: Rect) {
        self.rtl = cx.rtl();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if cx.bounds().contains(*position) => {
                if self
                    .clear_rect(cx.bounds())
                    .is_some_and(|clear| clear.contains(*position))
                {
                    let change = self.clear();
                    self.report(cx, change);
                } else {
                    self.start_recording(cx);
                }
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down { key, repeat: false }) if self.is_recording() => {
                let change = self.press(*key, cx.modifiers());
                self.report(cx, change);
                // Tab ends the recording and moves the focus.
                if *key == Key::Tab && !self.is_recording() {
                    EventReply::Continue
                } else {
                    EventReply::Stop
                }
            }
            // Recording takes every key, repeats and releases included.
            ControlEvent::Key(_) if self.is_recording() => EventReply::Stop,
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Space | Key::Enter,
                repeat: false,
            }) => {
                self.start_recording(cx);
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Backspace | Key::Delete,
                ..
            }) if !self.committed.is_empty() && cx.modifiers() == ShortcutModifiers::default() => {
                let change = self.clear();
                self.report(cx, change);
                EventReply::Stop
            }
            ControlEvent::Timer => {
                let change = self.finish();
                self.report(cx, change);
                EventReply::Stop
            }
            ControlEvent::FocusChanged(false) => {
                let change = self.finish();
                self.report(cx, change);
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        KeySequenceEdit::paint(self, cx, host, &self.placeholder);
        if let Some(clear) = self.clear_rect(cx.bounds) {
            let icon = cx.style(StyleQuery::new(ControlKind::Input, Slot::Icon).state(host));
            cx.icon(clear, Icon::X, &icon);
        }
    }

    fn semantics(&self) -> SemanticNode {
        self.semantics_in(Localizer::builtin())
    }

    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        KeySequenceEdit::semantics(self, &self.name, strings)
    }

    fn deadline(&self) -> Option<f64> {
        self.deadline.filter(|_| self.is_recording())
    }

    /// True while it records: the window gives it the keys of menu
    /// shortcuts first.
    fn captures_keys(&self) -> bool {
        self.is_recording()
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Activate => self.start_recording(cx),
            SemanticAction::Focus => cx.request_focus(),
            _ => return false,
        }
        true
    }

    fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.recording = None;
        }
    }

    /// Takes a sequence in the text that `text` gives, for example
    /// `⌘K, ⇧⌘S`. A key name can also be in the language of a built-in
    /// catalog, such as `⌘Phím cách`. An empty text clears the sequence.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Text(text) = value else {
            return false;
        };
        let Some(sequence) = parse_sequence(text).filter(|chords| chords.len() <= self.max_chords)
        else {
            return false;
        };
        self.committed = sequence;
        self.recording = None;
        true
    }
}

/// The modifier glyphs in canonical order.
const MODIFIER_GLYPHS: [char; 4] = ['⌃', '⌥', '⇧', '⌘'];

/// The canonical text of each named key.
const KEY_NAMES: [(Key, &str); 15] = [
    (Key::Enter, "↩"),
    (Key::Tab, "⇥"),
    (Key::Space, "Space"),
    (Key::Escape, "⎋"),
    (Key::Backspace, "⌫"),
    (Key::Delete, "⌦"),
    (Key::ArrowUp, "↑"),
    (Key::ArrowDown, "↓"),
    (Key::ArrowLeft, "←"),
    (Key::ArrowRight, "→"),
    (Key::Home, "↖"),
    (Key::End, "↘"),
    (Key::PageUp, "⇞"),
    (Key::PageDown, "⇟"),
    (Key::ContextMenu, "Menu"),
];

/// The message ids of the key names that are words, not symbols. The
/// field shows these names in the window language.
const KEY_MESSAGES: [(Key, &str); 2] = [
    (Key::Space, "choscorui.key.space"),
    (Key::ContextMenu, "choscorui.key.menu"),
];

/// macOS modifier glyphs in canonical order, then the key. The key names
/// are the canonical English names, such as `Space`, which `set_value`
/// reads in each language.
pub fn shortcut_text(shortcut: Shortcut) -> String {
    compose_text(shortcut, None)
}

/// The modifier glyphs, then the key name: the localized name from
/// `strings` when it is given, otherwise the canonical name.
fn compose_text(shortcut: Shortcut, strings: Option<&Localizer>) -> String {
    let modifiers = shortcut.modifiers;
    let held = [
        modifiers.control,
        modifiers.option,
        modifiers.shift,
        modifiers.command,
    ];
    let mut text: String = MODIFIER_GLYPHS
        .iter()
        .zip(held)
        .filter(|(_, held)| *held)
        .map(|(glyph, _)| *glyph)
        .collect();
    let named = KEY_NAMES.iter().find(|(key, _)| *key == shortcut.key);
    let message = KEY_MESSAGES.iter().find(|(key, _)| *key == shortcut.key);
    match (shortcut.key, named, message.zip(strings)) {
        (Key::Character(character), _, _) => text.extend(character.to_uppercase()),
        (Key::F(number), _, _) => text.push_str(&format!("F{number}")),
        (_, _, Some(((_, id), strings))) => text.push_str(&strings.message(id, &[])),
        (_, Some((_, name)), None) => text.push_str(name),
        (other, None, None) => text.push_str(&format!("{other:?}")),
    }
    text
}

/// The key of a localized key name in a language of the built-in
/// catalog.
fn localized_key(name: &str) -> Option<Key> {
    let catalog = Catalog::builtin();
    catalog
        .locales()
        .filter_map(|tag| Locale::parse(tag).ok())
        .find_map(|locale| {
            KEY_MESSAGES.iter().find_map(|(key, id)| {
                let text = catalog.text(&locale, id, &[]).ok()?;
                (text == name).then_some(*key)
            })
        })
}

/// The chords of a sequence text such as `⌘K, ⇧⌘S`, or `None` for a
/// text that `shortcut_text` does not make.
fn parse_sequence(text: &str) -> Option<Vec<Shortcut>> {
    if text.is_empty() {
        return Some(Vec::new());
    }
    text.split(", ").map(parse_shortcut).collect()
}

fn parse_shortcut(text: &str) -> Option<Shortcut> {
    let key_text = text.trim_start_matches(MODIFIER_GLYPHS);
    let glyphs = &text[..text.len() - key_text.len()];
    let held = |glyph| glyphs.contains(glyph);
    let modifiers = ShortcutModifiers {
        control: held('⌃'),
        option: held('⌥'),
        shift: held('⇧'),
        command: held('⌘'),
    };
    let named = KEY_NAMES
        .iter()
        .find(|(_, name)| *name == key_text)
        .map(|(key, _)| *key)
        .or_else(|| localized_key(key_text));
    let mut characters = key_text.chars();
    let key = match (named, characters.next(), characters.next()) {
        (Some(key), _, _) => key,
        (None, Some('F'), Some(_)) => Key::F(key_text[1..].parse().ok().filter(|n| *n > 0)?),
        (None, Some(character), None) => Key::Character(character.to_lowercase().next()?),
        _ => return None,
    };
    Some(Shortcut { key, modifiers })
}
