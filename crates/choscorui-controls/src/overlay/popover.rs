//! A trigger with a floating panel: a popover that opens on a press, or
//! a hover card that opens while the pointer rests on a link.

use super::panel::PanelText;
use crate::custom::{ControlContext, ControlEvent, ControlMessage, CustomControl, MeasureContext};
use crate::hosted::{self, Press, PressOutcome, Reveal};
use crate::popup::{Placement, gap_anchor, place_popup};
use crate::style::{
    ButtonVariant, ControlKind, ControlSize, PaintContext, Slot, StyleQuery, TextAlign, VisualState,
};
use choscorui_core::{
    EventPhase, EventReply, Key, Measured, Rect, SemanticAction, SemanticNode, SemanticRole, Size,
};

/// How long the pointer rests on a hover card's link before it opens, in
/// seconds.
pub const HOVER_CARD_OPEN_DELAY: f64 = 0.7;

/// How long a hover card stays after the pointer leaves, in seconds.
pub const HOVER_CARD_CLOSE_DELAY: f64 = 0.3;

/// A popover: an outline button that opens a panel with a title and a
/// description below it. `Popover::hover_card` makes a link that shows
/// the panel while the pointer rests on the link or the panel.
pub struct Popover {
    trigger: String,
    text: PanelText,
    hover_card: bool,
    reveal: Reveal,
    frame: Rect,
    /// True when the panel opens above the trigger.
    above: bool,
    motion_ms: u32,
    press: Press,
    /// True while the pointer is on the trigger or the panel.
    hovered: bool,
    deadline: Option<f64>,
    enabled: bool,
}

impl Popover {
    pub fn new(
        trigger: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            trigger: trigger.into(),
            text: PanelText::new(title, description),
            hover_card: false,
            reveal: Reveal::default(),
            frame: Rect::default(),
            above: false,
            motion_ms: 0,
            press: Press::default(),
            hovered: false,
            deadline: None,
            enabled: true,
        }
    }

    /// A link that opens the panel after the pointer rests on it for
    /// [`HOVER_CARD_OPEN_DELAY`]. Enter or Space on the link emits
    /// `Activated`.
    pub fn hover_card(
        trigger: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            hover_card: true,
            ..Self::new(trigger, title, description)
        }
    }

    pub fn is_open(&self) -> bool {
        self.reveal.open
    }

    fn kind(&self) -> ControlKind {
        if self.hover_card {
            ControlKind::HoverCard
        } else {
            ControlKind::Popover
        }
    }

    fn trigger_query(&self, state: VisualState) -> StyleQuery {
        let variant = if self.hover_card {
            ButtonVariant::Link
        } else {
            ButtonVariant::Outline
        };
        StyleQuery::new(ControlKind::Button, Slot::Root)
            .variant(variant)
            .size(ControlSize::Default)
            .state(state)
    }

    /// Opens or closes the panel at `time` and cancels a hover delay.
    /// Returns true when it changed.
    fn apply_open(&mut self, open: bool, time: f64) -> bool {
        self.deadline = None;
        if self.reveal.open == open {
            return false;
        }
        self.reveal.set(open, time);
        true
    }

    /// Opens or closes the panel and reports it. Returns true when it
    /// changed.
    fn change_open(&mut self, cx: &mut ControlContext<'_>, open: bool) -> bool {
        let changed = self.apply_open(open, cx.time());
        if changed {
            cx.emit(ControlMessage::OpenChanged(open));
            cx.request_paint();
            cx.request_semantics();
        }
        changed
    }

    /// Closes the panel without a choice. A popover reports `Dismissed`;
    /// a hover card closes quietly and lets the press through.
    fn dismiss(&mut self, cx: &mut ControlContext<'_>) -> EventReply {
        let closed = self.change_open(cx, false);
        hosted::dismissed(cx, closed && !self.hover_card)
    }

    /// Starts the open or close delay of a hover card.
    fn hover(&mut self, cx: &mut ControlContext<'_>, hovered: bool) {
        self.hovered = hovered;
        self.deadline = match (hovered, self.reveal.open) {
            (true, false) => Some(cx.time() + HOVER_CARD_OPEN_DELAY),
            (false, true) => Some(cx.time() + HOVER_CARD_CLOSE_DELAY),
            _ => None,
        };
    }
}

impl CustomControl for Popover {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = cx.style(self.trigger_query(VisualState::NONE));
        let width = cx.text.width(&self.trigger, &style.text) + style.padding.horizontal();
        Measured {
            size: Size::new(width.max(style.min_width), style.min_height),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let query = StyleQuery::new(self.kind(), Slot::Content);
        let panel = cx.style(query);
        self.motion_ms = panel.motion_ms;
        let room = cx.popup_bounds();
        // The panel width, limited to the window.
        let width = panel.min_width.min(room.size.width);
        let inner = width - panel.padding.horizontal();
        let height = self.text.height(cx, query, inner) + panel.padding.vertical();
        // Centered on the trigger, kept in the window.
        let anchor = gap_anchor(bounds, cx.tokens.spacing, Some(width));
        let placed = place_popup(
            anchor,
            Size::new(width, height),
            room,
            Placement::Below,
            cx.direction,
        );
        self.above = placed.placement == Placement::Above;
        self.frame = placed.rect;
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(pointer)
                if self.reveal.open && self.frame.contains(pointer.position()) =>
            {
                return EventReply::Stop;
            }
            ControlEvent::Hover(hovered) if self.hover_card => {
                self.hover(cx, *hovered);
                return EventReply::Continue;
            }
            ControlEvent::Timer => {
                self.deadline = None;
                let open = self.hovered;
                self.change_open(cx, open);
                return EventReply::Stop;
            }
            ControlEvent::Key(choscorui_core::KeyEvent::Down {
                key: Key::Escape, ..
            }) if self.reveal.open => {
                self.change_open(cx, false);
                cx.emit(ControlMessage::Dismissed);
                return EventReply::Stop;
            }
            ControlEvent::Dismiss => {
                let reply = self.dismiss(cx);
                self.press.event(cx, event);
                return reply;
            }
            ControlEvent::FocusChanged(false) => {
                self.dismiss(cx);
            }
            _ => {}
        }
        let outcome = self.press.event(cx, event);
        if let PressOutcome::Activated(_) = outcome {
            if self.hover_card {
                cx.emit(ControlMessage::Activated);
            } else if self.reveal.open {
                self.dismiss(cx);
            } else {
                self.change_open(cx, true);
            }
        }
        hosted::reply(outcome)
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx
            .state
            .set(VisualState::PRESSED, self.press.pressed)
            .set(VisualState::OPEN, self.reveal.open);
        let style = cx.style(self.trigger_query(state));
        let label = &self.trigger;
        cx.slot(cx.bounds, &style, |cx, inner| {
            cx.label(inner, label, &style, TextAlign::Center);
        });
    }

    fn overlay(&self) -> Option<Rect> {
        self.reveal.open.then_some(self.frame)
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        let progress = self.reveal.progress(cx.tokens, self.motion_ms, cx.time);
        if progress <= 0.0 {
            return;
        }
        let query = StyleQuery::new(self.kind(), Slot::Content);
        let panel = cx.style(query.state(VisualState::OPEN));
        let slide = cx.tokens.spacing * 2.0;
        let slide = (0.0, if self.above { slide } else { -slide });
        let (frame, text) = (self.frame, &self.text);
        Reveal::paint(cx, frame, progress, slide, |cx| {
            cx.slot(frame, &panel, |cx, inner| {
                text.paint(cx, query, inner, TextAlign::Start);
            });
        });
    }

    fn animating(&self, time: f64) -> bool {
        self.reveal.animating(self.motion_ms, time)
    }

    fn deadline(&self) -> Option<f64> {
        self.deadline
    }

    fn semantics(&self) -> SemanticNode {
        let actions = match (self.enabled, self.hover_card) {
            (false, _) => Vec::new(),
            (true, true) => vec![SemanticAction::Activate, SemanticAction::Focus],
            (true, false) => vec![
                SemanticAction::Activate,
                if self.reveal.open {
                    SemanticAction::Collapse
                } else {
                    SemanticAction::Expand
                },
                SemanticAction::Focus,
            ],
        };
        let role = if self.hover_card {
            SemanticRole::Link
        } else {
            SemanticRole::Button
        };
        let mut node = hosted::node(role, &self.trigger, actions);
        if self.reveal.open {
            node.description = self.text.joined();
        }
        if !self.hover_card {
            node.state.expanded = Some(self.reveal.open);
        }
        node.state.disabled = !self.enabled;
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Activate if self.hover_card => {
                cx.emit(ControlMessage::Activated);
                true
            }
            SemanticAction::Activate => {
                let open = !self.reveal.open;
                self.change_open(cx, open);
                true
            }
            SemanticAction::Expand => {
                self.change_open(cx, true);
                true
            }
            SemanticAction::Collapse => {
                self.change_open(cx, false);
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
            self.deadline = None;
            self.hovered = false;
        }
    }

    fn set_open(&mut self, open: bool, time: f64) -> bool {
        if open && !self.enabled {
            return false;
        }
        self.apply_open(open, time);
        true
    }
}
