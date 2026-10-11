//! A trigger that opens a popup list of options.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted::{self, Reveal};
use crate::listbox::{ListEntry, ListPopup};
use crate::style::{
    ControlKind, ControlSize, PaintContext, Slot, StyleQuery, TextAlign, VisualState,
};
use choscorui_core::{
    EventPhase, EventReply, Icon, Key, KeyEvent, Measured, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticNode, SemanticRole, Size, WheelUnit,
};

/// A select: a trigger that shows the chosen option, and a popup list
/// below it. The list scrolls past eight options.
#[derive(Clone, Debug)]
pub struct Select {
    name: String,
    placeholder: String,
    list: ListPopup,
    selected: Option<usize>,
    size: ControlSize,
    invalid: bool,
    enabled: bool,
    reveal: Reveal,
    motion_ms: u32,
    /// The scroll step of one wheel line: the first row's height.
    line: f64,
}

impl Select {
    pub fn new(name: impl Into<String>, entries: Vec<ListEntry>) -> Self {
        Self {
            name: name.into(),
            placeholder: String::new(),
            list: ListPopup::new(ControlKind::Select, entries),
            selected: None,
            size: ControlSize::Default,
            invalid: false,
            enabled: true,
            reveal: Reveal::default(),
            motion_ms: 0,
            line: 0.0,
        }
    }

    /// The text the trigger shows while no option is chosen.
    #[must_use]
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// `Default` or `Sm`.
    #[must_use]
    pub fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn set_invalid(&mut self, invalid: bool) {
        self.invalid = invalid;
    }

    /// The chosen entry index.
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Chooses the entry at `index`. An index that is not an enabled
    /// option clears the choice.
    pub fn set_selected(&mut self, index: Option<usize>) {
        self.selected = index.filter(|index| {
            self.list
                .entries
                .get(*index)
                .is_some_and(ListEntry::is_enabled_option)
        });
    }

    /// The label of the chosen option.
    pub fn value(&self) -> Option<&str> {
        self.selected.map(|index| self.list.entries[index].label())
    }

    pub fn is_open(&self) -> bool {
        self.reveal.open
    }

    /// The flags the recipe reads: `host` with open and invalid.
    pub fn visual_state(&self, host: VisualState) -> VisualState {
        host.set(VisualState::OPEN, self.reveal.open)
            .set(VisualState::INVALID, self.invalid)
    }

    fn query(&self, slot: Slot, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Select, slot)
            .size(self.size)
            .state(state)
    }

    fn set_open(&mut self, cx: &mut ControlContext<'_>, open: bool) {
        if self.reveal.open == open {
            return;
        }
        self.reveal.set(open, cx.time());
        if open {
            let start = self.selected.or_else(|| self.list.options().next());
            self.list.highlight(start);
        }
        cx.emit(ControlMessage::OpenChanged(open));
        cx.request_paint();
        cx.request_semantics();
    }

    fn choose(&mut self, cx: &mut ControlContext<'_>, index: usize) {
        if self.selected != Some(index) {
            self.selected = Some(index);
            cx.emit(ControlMessage::Changed(ControlValue::Index(Some(index))));
        }
        self.set_open(cx, false);
        cx.request_paint();
        cx.request_semantics();
    }

    fn pointer(&mut self, cx: &mut ControlContext<'_>, event: &PointerEvent) -> EventReply {
        let open = self.reveal.open;
        match *event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } => {
                if open && self.list.frame.contains(position) {
                    if let Some(direction) = self.list.scroll_button(position) {
                        self.list.scroll_by(direction * self.line);
                        cx.request_paint();
                    }
                    return EventReply::Stop;
                }
                if cx.bounds().contains(position) {
                    cx.request_focus();
                    self.set_open(cx, !open);
                    return EventReply::Stop;
                }
                EventReply::Continue
            }
            PointerEvent::Up { position, .. } if open => {
                match self.list.hit(position) {
                    Some(index) => self.choose(cx, index),
                    None => return EventReply::Continue,
                }
                EventReply::Stop
            }
            PointerEvent::Move { position } if open => {
                if let Some(index) = self.list.hit(position)
                    && self.list.highlighted != Some(index)
                {
                    self.list.highlighted = Some(index);
                    cx.request_paint();
                }
                EventReply::Continue
            }
            PointerEvent::Wheel {
                position,
                delta,
                unit,
            } if open && self.list.frame.contains(position) => {
                let step = match unit {
                    WheelUnit::Points => delta.y,
                    WheelUnit::Lines => delta.y * self.line,
                };
                self.list.scroll_by(step);
                cx.request_paint();
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }

    fn key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> EventReply {
        if !self.reveal.open {
            return match key {
                Key::ArrowDown | Key::ArrowUp | Key::Enter | Key::Space => {
                    self.set_open(cx, true);
                    EventReply::Stop
                }
                _ => EventReply::Continue,
            };
        }
        match key {
            Key::Enter | Key::Space => {
                match self.list.highlighted {
                    Some(index) => self.choose(cx, index),
                    None => self.set_open(cx, false),
                }
                EventReply::Stop
            }
            Key::Escape => {
                self.set_open(cx, false);
                cx.emit(ControlMessage::Dismissed);
                EventReply::Stop
            }
            Key::Tab => {
                self.set_open(cx, false);
                EventReply::Continue
            }
            key if self.list.step(key) => {
                cx.request_paint();
                cx.request_semantics();
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }
}

impl CustomControl for Select {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let trigger = cx.style(self.query(Slot::Trigger, VisualState::NONE));
        let value = cx.style(self.query(Slot::Value, VisualState::NONE));
        let placeholder = cx.style(self.query(Slot::Placeholder, VisualState::NONE));
        let icon = cx.style(self.query(Slot::Icon, VisualState::NONE));
        let widest = self
            .list
            .entries
            .iter()
            .filter(|entry| matches!(entry, ListEntry::Option { .. }))
            .map(|entry| cx.text.width(entry.label(), &value.text))
            .fold(
                cx.text.width(&self.placeholder, &placeholder.text),
                f64::max,
            );
        let width = trigger.padding.horizontal() + widest + trigger.gap + icon.icon_size;
        Measured {
            size: Size::new(width.max(trigger.min_width), trigger.min_height),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.list
            .place(cx, bounds, bounds.size.width, cx.tokens.spacing);
        self.motion_ms = cx
            .style(self.query(Slot::Content, VisualState::OPEN))
            .motion_ms;
        self.line = cx
            .style(self.query(Slot::Item, VisualState::NONE))
            .min_height;
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(pointer) => self.pointer(cx, pointer),
            ControlEvent::Key(KeyEvent::Down { key, .. }) => self.key(cx, *key),
            ControlEvent::Text(text) => {
                let Some(index) = self.list.type_ahead(text, cx.time()) else {
                    return EventReply::Stop;
                };
                if self.reveal.open {
                    self.list.highlight(Some(index));
                    cx.request_paint();
                    cx.request_semantics();
                } else if self.selected != Some(index) {
                    // Typing on the closed trigger chooses directly.
                    self.list.highlighted = Some(index);
                    self.selected = Some(index);
                    cx.emit(ControlMessage::Changed(ControlValue::Index(Some(index))));
                    cx.request_paint();
                    cx.request_semantics();
                }
                EventReply::Stop
            }
            ControlEvent::Dismiss | ControlEvent::FocusChanged(false) => {
                self.set_open(cx, false);
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = self.visual_state(cx.state);
        let trigger = cx.style(self.query(Slot::Trigger, state));
        let icon = cx.style(self.query(Slot::Icon, state));
        let (text, style) = match self.value() {
            Some(value) => (value, cx.style(self.query(Slot::Value, state))),
            None => (
                self.placeholder.as_str(),
                cx.style(self.query(Slot::Placeholder, state)),
            ),
        };
        cx.slot(cx.bounds, &trigger, |cx, inner| {
            let size = icon.icon_size;
            let text_width = (inner.size.width - size - trigger.gap).max(0.0);
            let (text_x, icon_x) = if cx.rtl {
                (inner.max_x() - text_width, inner.origin.x)
            } else {
                (inner.origin.x, inner.max_x() - size)
            };
            let line = Rect::from_xywh(text_x, inner.origin.y, text_width, inner.size.height);
            cx.label(line, text, &style, TextAlign::Start);
            let mark = Rect::from_xywh(icon_x, inner.origin.y, size, inner.size.height);
            cx.icon(mark, Icon::ChevronDown, &icon);
        });
    }

    fn overlay(&self) -> Option<Rect> {
        self.reveal.open.then_some(self.list.frame)
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        let progress = self.reveal.progress(cx.tokens, self.motion_ms, cx.time);
        if progress <= 0.0 {
            return;
        }
        let selected = self.selected;
        let slide = (0.0, -cx.tokens.spacing * 2.0);
        Reveal::paint(cx, self.list.frame, progress, slide, |cx| {
            self.list
                .paint(cx, &|index| selected == Some(index), Icon::Check);
        });
    }

    fn animating(&self, time: f64) -> bool {
        self.reveal.animating(self.motion_ms, time)
    }

    fn semantics(&self) -> SemanticNode {
        let actions = if self.enabled {
            vec![
                SemanticAction::ShowMenu,
                if self.reveal.open {
                    SemanticAction::Collapse
                } else {
                    SemanticAction::Expand
                },
                SemanticAction::SetValue,
                SemanticAction::Focus,
            ]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::ComboBox, &self.name, actions);
        node.value = Some(self.value().unwrap_or(&self.placeholder).to_string());
        if self.reveal.open
            && let Some(index) = self.list.highlighted
        {
            node.description = self.list.entries[index].label().to_string();
        }
        node.state.expanded = Some(self.reveal.open);
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
            SemanticAction::ShowMenu | SemanticAction::Expand => {
                self.set_open(cx, true);
                true
            }
            SemanticAction::Collapse => {
                self.set_open(cx, false);
                true
            }
            SemanticAction::SetValue => {
                let Some(index) = value.and_then(|value| {
                    self.list
                        .options()
                        .find(|index| self.list.entries[*index].label() == value)
                }) else {
                    return false;
                };
                self.choose(cx, index);
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
        if !enabled {
            self.reveal = Reveal::default();
        }
    }
}
