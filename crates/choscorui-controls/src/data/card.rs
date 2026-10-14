//! A card surface with a header, content text, and footer buttons.

use crate::custom::{ControlContext, ControlEvent, ControlMessage, CustomControl, MeasureContext};
use crate::form::measured::{paint_wrapped, wrapped_height};
use crate::hosted;
use crate::overlay::panel::{ButtonStops, Buttons, PanelButton, PanelText};
use crate::style::{
    ButtonVariant, ControlKind, ControlSize, PaintContext, Slot, StyleQuery, TextAlign,
};
use choscorui_core::{
    EventReply, Localizer, Measured, Rect, SemanticAction, SemanticNode, SemanticParts,
    SemanticRole, Size,
};

/// A card: a title and a description in the header, an optional header
/// action, wrapped content text, and a row of footer buttons. The
/// header action and the footer buttons are tab stops inside the card.
/// Activating one emits `ItemActivated` with its index, the header action
/// first.
pub struct Card {
    text: PanelText,
    content: String,
    /// Whether the first button is the header action.
    action: bool,
    buttons: Buttons,
    stops: ButtonStops,
    size: ControlSize,
    /// The header text rectangle and the content rectangle.
    header: Rect,
    body: Rect,
    enabled: bool,
}

impl Card {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            text: PanelText::new(title, description),
            content: String::new(),
            action: false,
            buttons: Buttons::default(),
            stops: ButtonStops::default(),
            size: ControlSize::Default,
            header: Rect::default(),
            body: Rect::default(),
            enabled: true,
        }
    }

    /// Adds a button at the end of the header.
    #[must_use]
    pub fn with_action(mut self, label: impl Into<String>, variant: ButtonVariant) -> Self {
        let button = PanelButton {
            label: label.into(),
            variant,
        };
        if self.action {
            self.buttons.buttons[0] = button;
        } else {
            self.buttons.buttons.insert(0, button);
            self.action = true;
        }
        self
    }

    /// Sets the content text, which wraps to the card width.
    #[must_use]
    pub fn with_content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self
    }

    /// Adds a footer button after the others.
    #[must_use]
    pub fn with_footer(mut self, label: impl Into<String>, variant: ButtonVariant) -> Self {
        self.buttons.buttons.push(PanelButton {
            label: label.into(),
            variant,
        });
        self
    }

    /// Sets the size: `Default` or `Sm`, which has less spacing and a
    /// smaller title.
    #[must_use]
    pub fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    fn query(&self, slot: Slot) -> StyleQuery {
        StyleQuery::new(ControlKind::Card, slot).size(self.size)
    }

    fn footer_count(&self) -> usize {
        self.buttons.buttons.len() - usize::from(self.action)
    }

    /// The header text width inside a card of `width`.
    fn header_width(&self, cx: &MeasureContext<'_>, width: f64) -> f64 {
        let header = cx.style(self.query(Slot::Header));
        let mut inner = width - header.padding.horizontal();
        if self.action {
            inner -= Buttons::width(cx, &self.buttons.buttons[0]) + header.gap;
        }
        inner.max(1.0)
    }

    /// The section heights: header, content, and footer, zero when empty.
    fn sections(&self, cx: &MeasureContext<'_>, width: f64) -> [f64; 3] {
        let content = cx.style(self.query(Slot::Content));
        let text = self
            .text
            .height(cx, self.query(Slot::Root), self.header_width(cx, width));
        let button = Buttons::button_height(cx);
        let header = if self.action { text.max(button) } else { text };
        let body = wrapped_height(
            &self.content,
            &content.text,
            (width - content.padding.horizontal()).max(1.0),
            cx.text,
        );
        let footer = if self.footer_count() > 0 { button } else { 0.0 };
        [header, body, footer]
    }

    fn focused_label(&self) -> Option<&str> {
        self.buttons
            .buttons
            .get(self.stops.focus)
            .map(|button| button.label.as_str())
    }
}

impl CustomControl for Card {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let root = cx.style(self.query(Slot::Root));
        let max = cx.constraints.max.width;
        // An unbounded card takes the width of a small container.
        let width = if max.is_finite() {
            max
        } else {
            cx.tokens.spacing * 96.0
        };
        let sections = self.sections(cx, width);
        let shown = sections.iter().filter(|height| **height > 0.0).count();
        let height = sections.iter().sum::<f64>()
            + root.gap * shown.saturating_sub(1) as f64
            + root.padding.vertical();
        Measured {
            size: cx.constraints.constrain(Size::new(width, height)),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let root = cx.style(self.query(Slot::Root));
        let header = cx.style(self.query(Slot::Header));
        let footer = cx.style(self.query(Slot::Footer));
        let width = bounds.size.width;
        let [header_height, body_height, footer_height] = self.sections(cx, width);
        let mut y = bounds.origin.y + root.padding.block_start;
        let inset = |padding: f64, top: f64, height: f64| {
            Rect::from_xywh(
                bounds.origin.x + padding / 2.0,
                top,
                (width - padding).max(0.0),
                height,
            )
        };
        let header_row = inset(header.padding.horizontal(), y, header_height);
        let text_width = self.header_width(cx, width);
        self.header = Rect::from_xywh(
            if cx.rtl() {
                header_row.max_x() - text_width
            } else {
                header_row.origin.x
            },
            y,
            text_width,
            header_height,
        );
        if header_height > 0.0 {
            y += header_height + root.gap;
        }
        let content = cx.style(self.query(Slot::Content));
        self.body = inset(content.padding.horizontal(), y, body_height);
        if body_height > 0.0 {
            y += body_height + root.gap;
        }
        let footer_row = inset(footer.padding.horizontal(), y, footer_height);
        let mut rects = Vec::new();
        if self.action {
            let action = PanelButton::clone(&self.buttons.buttons[0]);
            let mut row = Buttons {
                buttons: vec![action],
                rects: Vec::new(),
            };
            row.place_row(cx, header_row, header.gap);
            rects.extend(row.rects);
        }
        let mut row = Buttons {
            buttons: self.buttons.buttons[usize::from(self.action)..].to_vec(),
            rects: Vec::new(),
        };
        row.place_row(cx, footer_row, footer.gap);
        rects.extend(row.rects);
        self.buttons.rects = rects;
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let (reply, activated) = self.stops.event(cx, event, &self.buttons);
        if let Some(index) = activated {
            cx.emit(ControlMessage::ItemActivated(index));
        }
        reply
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let root = cx.style(self.query(Slot::Root));
        let content = cx.style(self.query(Slot::Content));
        let focus_visible = cx.state.focus_visible();
        cx.slot(cx.bounds, &root, |cx, _| {
            self.text
                .paint(cx, self.query(Slot::Root), self.header, TextAlign::Start);
            paint_wrapped(cx, self.body, &self.content, &content, TextAlign::Start);
            self.buttons
                .paint(cx, Some(self.stops.focus), &self.stops.parts, focus_visible);
        });
    }

    fn semantics(&self) -> SemanticNode {
        let actions = if self.enabled && !self.buttons.buttons.is_empty() {
            vec![SemanticAction::Activate, SemanticAction::Focus]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::Group, &self.text.title, actions);
        node.value = Some(
            [self.text.description.as_str(), self.content.as_str()]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .filter(|value| !value.is_empty());
        node.state.disabled = !self.enabled;
        node
    }

    fn semantic_parts(&self, _strings: &Localizer, parts: &mut SemanticParts) {
        self.buttons
            .semantic_parts(Some(self.stops.focus), self.enabled, None, parts);
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Activate if self.focused_label().is_some() => {
                cx.emit(ControlMessage::ItemActivated(self.stops.focus));
                true
            }
            SemanticAction::Focus if self.focusable() => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn focusable(&self) -> bool {
        !self.buttons.buttons.is_empty()
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}
