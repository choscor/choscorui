//! Keyboard shortcut capture and the text of a shortcut.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::form::input::default_width;
use crate::style::{Content, ControlKind, PaintContext, Slot, StyleQuery, TextAlign, VisualState};
use crate::{Shortcut, ShortcutModifiers};
use choscorui_core::{
    EventPhase, EventReply, Key, KeyEvent, Localizer, Measured, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticNode, SemanticRole, SemanticState, Size,
};

/// Records up to `max_chords` shortcuts. Presses of keys without a
/// `Key` mapping (`Key::Other`, which includes modifier-only presses) are
/// ignored; Backspace with no modifiers clears; Escape cancels and keeps
/// the previous sequence; recording finishes when full or on
/// [`KeySequenceEdit::finish`] (for example after a pause or focus loss).
#[derive(Clone, Debug, PartialEq)]
pub struct KeySequenceEdit {
    committed: Vec<Shortcut>,
    recording: Option<Vec<Shortcut>>,
    max_chords: usize,
    /// The name and the placeholder in a control host.
    name: String,
    placeholder: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeySequenceChange {
    None,
    Recording,
    Committed,
    Cancelled,
    Cleared,
}

impl KeySequenceEdit {
    pub fn new(max_chords: usize) -> Self {
        Self {
            committed: Vec::new(),
            recording: None,
            max_chords: max_chords.clamp(1, 4),
            name: String::new(),
            placeholder: String::new(),
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
        match key {
            Key::Escape if plain => {
                self.recording = None;
                KeySequenceChange::Cancelled
            }
            Key::Backspace | Key::Delete if plain && recording.is_empty() => {
                self.committed.clear();
                self.recording = None;
                KeySequenceChange::Cleared
            }
            Key::Other(_) => KeySequenceChange::None,
            _ => {
                recording.push(Shortcut { key, modifiers });
                if recording.len() >= self.max_chords {
                    self.finish()
                } else {
                    KeySequenceChange::Recording
                }
            }
        }
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
            value: Some(self.text()),
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
    /// chords so far show. `host` carries focus flags.
    pub fn paint(&self, cx: &mut PaintContext<'_>, host: VisualState, placeholder: &str) {
        let input = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root).state(host));
        let muted = cx.style(StyleQuery::new(ControlKind::Input, Slot::Placeholder));
        let cap = cx.style(StyleQuery::new(ControlKind::Kbd, Slot::Root));
        let group = cx.style(StyleQuery::new(ControlKind::Kbd, Slot::Group));
        let chords = self.recording.as_ref().unwrap_or(&self.committed);
        let bounds = cx.bounds;
        cx.slot(bounds, &input, |cx, inner| {
            if chords.is_empty() {
                cx.label(inner, placeholder, &muted, TextAlign::Start);
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
                let text = shortcut_text(*chord);
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
    /// Reports a committed or cleared sequence in a host.
    fn report(&self, cx: &mut ControlContext<'_>, change: KeySequenceChange) {
        if matches!(
            change,
            KeySequenceChange::Committed | KeySequenceChange::Cleared
        ) {
            cx.emit(ControlMessage::Changed(ControlValue::Text(self.text())));
        }
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
}

/// A click, Space, Enter, or `Activate` starts the recording. While it
/// records, the control takes every key. A loss of focus finishes the
/// recording. It reports `Changed(Text)` with the sequence text.
impl CustomControl for KeySequenceEdit {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root));
        let height = (style.text.line_height + style.padding.vertical()).max(style.min_height);
        Measured::new(Size::new(default_width(cx, &style), height))
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
                self.start_recording(cx);
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down { key, repeat: false }) if self.is_recording() => {
                let change = self.press(*key, cx.modifiers());
                self.report(cx, change);
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Space | Key::Enter,
                repeat: false,
            }) => {
                self.start_recording(cx);
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
    }

    fn semantics(&self) -> SemanticNode {
        self.semantics_in(Localizer::builtin())
    }

    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        KeySequenceEdit::semantics(self, &self.name, strings)
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
    /// `⌘K, ⇧⌘S`. An empty text clears the sequence.
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

/// The text of each named key.
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

/// macOS modifier glyphs in canonical order, then the key.
pub fn shortcut_text(shortcut: Shortcut) -> String {
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
    match (shortcut.key, named) {
        (Key::Character(character), _) => text.extend(character.to_uppercase()),
        (Key::F(number), _) => text.push_str(&format!("F{number}")),
        (_, Some((_, name))) => text.push_str(name),
        (other, None) => text.push_str(&format!("{other:?}")),
    }
    text
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
    let named = KEY_NAMES.iter().find(|(_, name)| *name == key_text);
    let mut characters = key_text.chars();
    let key = match (named, characters.next(), characters.next()) {
        (Some((key, _)), _, _) => *key,
        (None, Some('F'), Some(_)) => Key::F(key_text[1..].parse().ok().filter(|n| *n > 0)?),
        (None, Some(character), None) => Key::Character(character.to_lowercase().next()?),
        _ => return None,
    };
    Some(Shortcut { key, modifiers })
}
