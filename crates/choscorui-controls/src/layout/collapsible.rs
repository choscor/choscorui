//! A section that shows or hides its items behind a toggle button.

use crate::custom::{ControlContext, ControlEvent, ControlMessage, CustomControl, MeasureContext};
use crate::hosted::{self, Parts, Press, PressOutcome, Reveal};
use crate::style::{
    ButtonVariant, ControlKind, ControlSize, ItemVariant, PaintContext, Slot, StyleQuery,
    TextAlign, VisualState,
};
use choscorui_core::{
    EventReply, Icon, Measured, Rect, SemanticAction, SemanticNode, SemanticRole, Size,
};

/// A collapsible section. The header shows the title and a toggle
/// button. The summary item always shows; the other items show while the
/// section is open. The toggle, Enter, Space, `Expand`, and `Collapse`
/// open or close it and emit `OpenChanged`.
pub struct Collapsible {
    title: String,
    toggle_label: String,
    summary: String,
    items: Vec<String>,
    reveal: Reveal,
    press: Press,
    /// Hover and press of the toggle.
    tracked: Parts<()>,
    /// The header, the toggle, and the item boxes.
    header: Rect,
    toggle: Rect,
    boxes: Vec<Rect>,
    motion_ms: u32,
    enabled: bool,
}

impl Collapsible {
    /// A closed section. `toggle_label` names the toggle button.
    pub fn new(
        title: impl Into<String>,
        toggle_label: impl Into<String>,
        summary: impl Into<String>,
        items: Vec<String>,
    ) -> Self {
        Self {
            title: title.into(),
            toggle_label: toggle_label.into(),
            summary: summary.into(),
            items,
            reveal: Reveal::default(),
            press: Press::default(),
            tracked: Parts::default(),
            header: Rect::default(),
            toggle: Rect::default(),
            boxes: Vec::new(),
            motion_ms: 0,
            enabled: true,
        }
    }

    /// Opens the section at the start.
    #[must_use]
    pub fn open(mut self) -> Self {
        self.reveal.open = true;
        self
    }

    fn toggle_query(state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Button, Slot::Root)
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::IconSm)
            .state(state)
    }

    fn box_query() -> StyleQuery {
        StyleQuery::new(ControlKind::Item, Slot::Root)
            .variant(ItemVariant::Outline)
            .size(ControlSize::Xs)
    }

    fn shown(&self) -> usize {
        1 + if self.reveal.open {
            self.items.len()
        } else {
            0
        }
    }

    fn change_open(&mut self, cx: &mut ControlContext<'_>, open: bool) {
        if open == self.reveal.open {
            return;
        }
        self.reveal.set(open, cx.time());
        cx.emit(ControlMessage::OpenChanged(open));
        cx.request_layout();
        cx.request_paint();
        cx.request_semantics();
    }

    /// The height of one item box.
    fn box_height(cx: &MeasureContext<'_>) -> f64 {
        let item = cx.style(Self::box_query());
        item.text.line_height + item.padding.vertical()
    }

    /// The gap between the header and the boxes.
    fn gap(spacing: f64) -> f64 {
        spacing * 2.0
    }
}

impl CustomControl for Collapsible {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let toggle = cx.style(Self::toggle_query(VisualState::NONE));
        let max = cx.constraints.max.width;
        // An unbounded section is as wide as an input.
        let width = if max.is_finite() {
            max
        } else {
            cx.tokens.spacing * 88.0
        };
        let gap = Self::gap(cx.tokens.spacing);
        let boxes = self.shown() as f64;
        let height = toggle.min_height + (Self::box_height(cx) + gap) * boxes;
        Measured {
            size: Size::new(width, height),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let toggle = cx.style(Self::toggle_query(VisualState::NONE));
        let content = cx.style(StyleQuery::new(ControlKind::Collapsible, Slot::Content));
        self.motion_ms = content.motion_ms;
        let side = toggle.min_height;
        self.header = Rect::from_xywh(bounds.origin.x, bounds.origin.y, bounds.size.width, side);
        let x = if cx.rtl() {
            bounds.origin.x
        } else {
            bounds.max_x() - side
        };
        self.toggle = Rect::from_xywh(x, bounds.origin.y, side, side);
        let gap = Self::gap(cx.tokens.spacing);
        let height = Self::box_height(cx);
        self.boxes = (0..self.shown())
            .map(|index| {
                let y = self.header.max_y() + gap + (height + gap) * index as f64;
                Rect::from_xywh(bounds.origin.x, y, bounds.size.width, height)
            })
            .collect();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let toggle = self.toggle;
        let pressed = self.tracked.pressed;
        self.tracked
            .track(cx, event, |point| toggle.contains(point).then_some(()));
        let outcome = self.press.event(cx, event);
        match outcome {
            PressOutcome::Activated(Some(_))
                if pressed.is_none() || self.tracked.hovered.is_none() => {}
            PressOutcome::Activated(_) => self.change_open(cx, !self.reveal.open),
            _ => {}
        }
        hosted::reply(outcome)
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        let title = cx.style(StyleQuery::new(ControlKind::Collapsible, Slot::Trigger));
        let item = cx.style(Self::box_query());
        let state = host
            .without(VisualState::HOVERED.with(VisualState::PRESSED))
            .with(self.tracked.state((), host))
            .set(VisualState::OPEN, self.reveal.open);
        let toggle = cx.style(Self::toggle_query(state));
        let label = Rect::from_xywh(
            if cx.rtl {
                self.header.origin.x + self.toggle.size.width
            } else {
                self.header.origin.x
            },
            self.header.origin.y,
            self.header.size.width - self.toggle.size.width,
            self.header.size.height,
        );
        cx.label(label, &self.title, &title, TextAlign::Start);
        let icon = if self.reveal.open {
            Icon::ChevronUp
        } else {
            Icon::ChevronDown
        };
        cx.slot(self.toggle, &toggle, |cx, inner| {
            cx.icon(inner, icon, &toggle)
        });
        let progress = self.reveal.progress(cx.tokens, self.motion_ms, cx.time);
        for (index, rect) in self.boxes.iter().enumerate() {
            let text = if index == 0 {
                &self.summary
            } else {
                &self.items[index - 1]
            };
            let faded = index > 0 && progress < 1.0;
            let pushed = faded && cx.list.push_opacity(progress.clamp(0.0, 1.0)).is_ok();
            cx.slot(*rect, &item, |cx, inner| {
                cx.label(inner, text, &item, TextAlign::Start);
            });
            if pushed {
                let _ = cx.list.pop_opacity();
            }
        }
    }

    fn animating(&self, time: f64) -> bool {
        self.reveal.animating(self.motion_ms, time)
    }

    fn semantics(&self) -> SemanticNode {
        let actions = if self.enabled {
            vec![
                SemanticAction::Activate,
                if self.reveal.open {
                    SemanticAction::Collapse
                } else {
                    SemanticAction::Expand
                },
                SemanticAction::Focus,
            ]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::Group, &self.title, actions);
        node.description = self.toggle_label.clone();
        node.value = Some(self.summary.clone());
        node.state.expanded = Some(self.reveal.open);
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
            SemanticAction::Activate => self.change_open(cx, !self.reveal.open),
            SemanticAction::Expand if !self.reveal.open => self.change_open(cx, true),
            SemanticAction::Collapse if self.reveal.open => self.change_open(cx, false),
            SemanticAction::Focus => cx.request_focus(),
            _ => return false,
        }
        true
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn set_open(&mut self, open: bool, time: f64) -> bool {
        self.reveal.set(open, time);
        true
    }
}
