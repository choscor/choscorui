//! A one-time code input: one character per slot, in groups.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted;
use crate::style::{
    ControlKind, PaintContext, ResolvedStyle, Slot, StyleQuery, TextAlign, VisualState,
};
use choscorui_core::{
    EventPhase, EventReply, Icon, Key, KeyEvent, Measured, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticNode, SemanticRole, Size,
};

/// A one-time code input. Typing fills the slots in order, Backspace
/// clears the last one, and pasted text fills as many slots as it can.
/// The caret is always after the last character.
#[derive(Clone, Debug)]
pub struct InputOtp {
    name: String,
    groups: Vec<usize>,
    value: Vec<char>,
    digits_only: bool,
    invalid: bool,
    enabled: bool,
    slots: Vec<Rect>,
    separators: Vec<Rect>,
}

impl InputOtp {
    /// A code of `length` characters in one group.
    pub fn new(name: impl Into<String>, length: usize) -> Self {
        Self {
            name: name.into(),
            groups: vec![length.max(1)],
            value: Vec::new(),
            digits_only: true,
            invalid: false,
            enabled: true,
            slots: Vec::new(),
            separators: Vec::new(),
        }
    }

    /// Splits the slots into groups of these sizes, with a separator
    /// between groups.
    #[must_use]
    pub fn with_groups(mut self, groups: Vec<usize>) -> Self {
        let groups: Vec<usize> = groups.into_iter().filter(|size| *size > 0).collect();
        if !groups.is_empty() {
            self.groups = groups;
        }
        self
    }

    /// Accepts letters as well as digits.
    #[must_use]
    pub fn alphanumeric(mut self) -> Self {
        self.digits_only = false;
        self
    }

    pub fn length(&self) -> usize {
        self.groups.iter().sum()
    }

    pub fn value(&self) -> String {
        self.value.iter().collect()
    }

    pub fn set_invalid(&mut self, invalid: bool) {
        self.invalid = invalid;
    }

    fn accepts(&self, c: char) -> bool {
        if self.digits_only {
            c.is_ascii_digit()
        } else {
            c.is_alphanumeric()
        }
    }

    fn query(slot: Slot, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::InputOtp, slot).state(state)
    }

    /// Replaces the value with the accepted characters of `text`, cut to
    /// the length. Returns whether it changed.
    fn replace(&mut self, cx: &mut ControlContext<'_>, value: Vec<char>) -> bool {
        if value == self.value {
            return false;
        }
        self.value = value;
        cx.emit(ControlMessage::Changed(ControlValue::Text(self.value())));
        if self.value.len() == self.length() {
            cx.emit(ControlMessage::Activated);
        }
        cx.request_paint();
        cx.request_semantics();
        true
    }

    fn typed(&self, text: &str) -> Vec<char> {
        let room = self.length().saturating_sub(self.value.len());
        let mut value = self.value.clone();
        value.extend(text.chars().filter(|c| self.accepts(*c)).take(room));
        value
    }

    /// The slot that shows the caret: the first empty one, or the last.
    fn active(&self) -> usize {
        self.value.len().min(self.length() - 1)
    }
}

impl CustomControl for InputOtp {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let root = cx.style(Self::query(Slot::Root, VisualState::NONE));
        let item = cx.style(Self::query(Slot::Item, VisualState::NONE));
        let separator = cx.style(Self::query(Slot::Separator, VisualState::NONE));
        let separators = self.groups.len() - 1;
        let width = item.min_width * self.length() as f64
            + (separator.min_width + root.gap * 2.0) * separators as f64;
        Measured {
            size: Size::new(width, item.min_height),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let root = cx.style(Self::query(Slot::Root, VisualState::NONE));
        let item = cx.style(Self::query(Slot::Item, VisualState::NONE));
        let separator = cx.style(Self::query(Slot::Separator, VisualState::NONE));
        let rtl = cx.rtl();
        let mut offset = 0.0;
        let place = |offset: f64, width: f64| {
            let x = if rtl {
                bounds.max_x() - offset - width
            } else {
                bounds.origin.x + offset
            };
            Rect::from_xywh(x, bounds.origin.y, width, item.min_height)
        };
        self.slots.clear();
        self.separators.clear();
        for (group, size) in self.groups.iter().enumerate() {
            if group > 0 {
                offset += root.gap;
                self.separators.push(place(offset, separator.min_width));
                offset += separator.min_width + root.gap;
            }
            for _ in 0..*size {
                self.slots.push(place(offset, item.min_width));
                offset += item.min_width;
            }
        }
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
                cx.request_focus();
                cx.request_paint();
                EventReply::Stop
            }
            ControlEvent::Text(text) => {
                let value = self.typed(text);
                self.replace(cx, value);
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Backspace | Key::Delete,
                ..
            }) => {
                let mut value = self.value.clone();
                value.pop();
                self.replace(cx, value);
                EventReply::Stop
            }
            ControlEvent::FocusChanged(_) => {
                cx.request_paint();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        let base = VisualState::NONE
            .set(VisualState::INVALID, host.invalid() || self.invalid)
            .set(VisualState::DISABLED, host.disabled());
        let focused = host.focused();
        // The active slot shows the focus ring while the control has
        // focus, so the group ring shows only without focus.
        let group = cx.style(Self::query(
            Slot::Group,
            base.set(VisualState::FOCUS_VISIBLE, focused),
        ));
        let radius = group.radius.top_start;
        let active = self.active();
        let mut index = 0;
        if group.ring.is_some() {
            let ring = ResolvedStyle {
                border_width: 0.0,
                ..group.clone()
            };
            let mut first = 0;
            for size in &self.groups {
                let slots = self.slots.get(first..first + size);
                if let Some((start, end)) = slots.and_then(|slots| slots.first().zip(slots.last()))
                {
                    cx.slot(start.union(*end), &ring, |_, _| {});
                }
                first += size;
            }
        }
        for size in &self.groups {
            for position in 0..*size {
                let Some(rect) = self.slots.get(index).copied() else {
                    return;
                };
                let state = base.set(VisualState::SELECTED, focused && index == active);
                let mut style = cx.style(Self::query(Slot::Item, state));
                if position == 0 {
                    style.border_sides.inline_start = true;
                    (style.radius.top_start, style.radius.bottom_start) = (radius, radius);
                }
                if position + 1 == *size {
                    (style.radius.top_end, style.radius.bottom_end) = (radius, radius);
                }
                let character = self.value.get(index).map(|c| c.to_string());
                let caret = focused && index == active && character.is_none();
                let indicator = cx.style(Self::query(Slot::Indicator, state));
                cx.slot(rect, &style, |cx, inner| {
                    if let Some(character) = &character {
                        cx.label(inner, character, &style, TextAlign::Center);
                    } else if caret {
                        let line = Rect::from_xywh(
                            rect.center().x - indicator.min_width / 2.0,
                            rect.center().y - indicator.min_height / 2.0,
                            indicator.min_width,
                            indicator.min_height,
                        );
                        cx.slot(line, &indicator, |_, _| {});
                    }
                });
                index += 1;
            }
        }
        let separator = cx.style(Self::query(Slot::Separator, base));
        for rect in &self.separators {
            cx.icon(*rect, Icon::Minus, &separator);
        }
    }

    fn semantics(&self) -> SemanticNode {
        let actions = if self.enabled {
            vec![SemanticAction::SetValue, SemanticAction::Focus]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::TextField, &self.name, actions);
        node.value = Some(self.value());
        node.state.invalid = self.invalid;
        node.state.disabled = !self.enabled;
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::SetValue => {
                let value = value
                    .unwrap_or_default()
                    .chars()
                    .filter(|c| self.accepts(*c))
                    .take(self.length())
                    .collect();
                self.replace(cx, value);
                true
            }
            SemanticAction::Focus => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Takes `Text`. Rejects text longer than the code, or with a
    /// character that the code does not accept.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Text(text) = value else {
            return false;
        };
        let value: Vec<char> = text.chars().collect();
        let fits = value.len() <= self.length() && value.iter().all(|c| self.accepts(*c));
        if fits {
            self.value = value;
        }
        fits
    }
}
