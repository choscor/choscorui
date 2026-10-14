//! A callout with an icon, a title, a description, and optional action
//! buttons.

use crate::custom::{ControlContext, ControlEvent, ControlMessage, CustomControl, MeasureContext};
use crate::form::measured::{paint_wrapped, wrapped_height};
use crate::hosted;
use crate::overlay::panel::{ButtonStops, Buttons, PanelButton};
use crate::style::{
    AlertVariant, ButtonVariant, ControlKind, PaintContext, Slot, StyleQuery, TextAlign,
};
use choscorui_core::{
    EventReply, Icon, Localizer, Measured, Rect, SemanticAction, SemanticNode, SemanticParts,
    SemanticRole, Size,
};

/// An alert. The icon sits at the start, and the title and the
/// description wrap beside it. Action buttons sit at the top end; they
/// are tab stops, and activating one emits `ItemActivated(index)`. An
/// alert without actions takes no focus.
pub struct Alert {
    title: String,
    description: String,
    icon: Option<Icon>,
    variant: AlertVariant,
    buttons: Buttons,
    stops: ButtonStops,
    /// The icon and the text rectangles.
    icon_rect: Rect,
    text: Rect,
    enabled: bool,
}

impl Alert {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            icon: None,
            variant: AlertVariant::Default,
            buttons: Buttons::default(),
            stops: ButtonStops::default(),
            icon_rect: Rect::default(),
            text: Rect::default(),
            enabled: true,
        }
    }

    #[must_use]
    pub fn with_variant(mut self, variant: AlertVariant) -> Self {
        self.variant = variant;
        self
    }

    #[must_use]
    pub fn with_icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Adds an action button after the others.
    #[must_use]
    pub fn with_action(mut self, label: impl Into<String>, variant: ButtonVariant) -> Self {
        self.buttons.buttons.push(PanelButton {
            label: label.into(),
            variant,
        });
        self
    }

    fn query(&self, slot: Slot) -> StyleQuery {
        StyleQuery::new(ControlKind::Alert, slot).variant(self.variant)
    }

    /// The width the icon column takes, with its gap.
    fn icon_width(&self, cx: &MeasureContext<'_>) -> f64 {
        if self.icon.is_none() {
            return 0.0;
        }
        let icon = cx.style(self.query(Slot::Icon));
        // The icon column is 12 pt from the text.
        icon.icon_size + cx.tokens.spacing * 3.0
    }

    /// The width the action buttons take at the end, with their inset.
    fn actions_width(&self, cx: &MeasureContext<'_>) -> f64 {
        if self.buttons.buttons.is_empty() {
            return 0.0;
        }
        let action = cx.style(self.query(Slot::Action));
        let gap = cx.tokens.spacing * 2.0;
        let buttons: f64 = self
            .buttons
            .buttons
            .iter()
            .map(|button| Buttons::width(cx, button))
            .sum();
        buttons + gap * self.buttons.buttons.len() as f64 + action.padding.inline_end
    }

    fn text_height(&self, cx: &MeasureContext<'_>, width: f64) -> f64 {
        let root = cx.style(self.query(Slot::Root));
        let title = cx.style(self.query(Slot::Title));
        let description = cx.style(self.query(Slot::Description));
        let title_height = wrapped_height(&self.title, &title.text, width, cx.text);
        let description_height =
            wrapped_height(&self.description, &description.text, width, cx.text);
        let gap = if title_height > 0.0 && description_height > 0.0 {
            root.gap
        } else {
            0.0
        };
        title_height + gap + description_height
    }
}

impl CustomControl for Alert {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let root = cx.style(self.query(Slot::Root));
        let max = cx.constraints.max.width;
        // An unbounded alert takes the width of a medium container.
        let width = if max.is_finite() {
            max
        } else {
            cx.tokens.spacing * 120.0
        };
        let text =
            (width - root.padding.horizontal() - self.icon_width(cx) - self.actions_width(cx))
                .max(1.0);
        let mut height = self.text_height(cx, text);
        if !self.buttons.buttons.is_empty() {
            let action = cx.style(self.query(Slot::Action));
            height = height.max(
                Buttons::button_height(cx) + action.padding.block_start - root.padding.block_start,
            );
        }
        Measured {
            size: Size::new(width, height + root.padding.vertical()),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let root = cx.style(self.query(Slot::Root));
        let icon = cx.style(self.query(Slot::Icon));
        let action = cx.style(self.query(Slot::Action));
        let rtl = cx.rtl();
        let inner = Rect::from_xywh(
            bounds.origin.x + root.padding.inline_start,
            bounds.origin.y + root.padding.block_start,
            bounds.size.width - root.padding.horizontal(),
            bounds.size.height - root.padding.vertical(),
        );
        let icon_width = self.icon_width(cx);
        let actions = self.actions_width(cx);
        self.icon_rect = Rect::from_xywh(
            if rtl {
                inner.max_x() - icon.icon_size
            } else {
                inner.origin.x
            },
            inner.origin.y,
            icon.icon_size,
            icon.icon_size,
        );
        let width = (inner.size.width - icon_width - actions).max(1.0);
        self.text = Rect::from_xywh(
            if rtl {
                inner.max_x() - icon_width - width
            } else {
                inner.origin.x + icon_width
            },
            inner.origin.y,
            width,
            self.text_height(cx, width),
        );
        let row = Rect::from_xywh(
            bounds.origin.x + action.padding.inline_end,
            bounds.origin.y + action.padding.block_start,
            bounds.size.width - action.padding.inline_end * 2.0,
            Buttons::button_height(cx),
        );
        self.buttons.place_row(cx, row, cx.tokens.spacing * 2.0);
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
        let title = cx.style(self.query(Slot::Title));
        let description = cx.style(self.query(Slot::Description));
        let icon_style = cx.style(self.query(Slot::Icon));
        let focus_visible = cx.state.focus_visible();
        cx.slot(cx.bounds, &root, |cx, _| {
            if let Some(icon) = self.icon {
                cx.icon(
                    self.icon_rect.translate(0.0, icon_style.offset_y),
                    icon,
                    &icon_style,
                );
            }
            let line = |style: &crate::style::ResolvedStyle, text: &str, cx: &PaintContext<'_>| {
                wrapped_height(text, &style.text, self.text.size.width, cx.text)
            };
            let title_height = line(&title, &self.title, cx);
            let title_rect = Rect::from_xywh(
                self.text.origin.x,
                self.text.origin.y,
                self.text.size.width,
                title_height,
            );
            paint_wrapped(cx, title_rect, &self.title, &title, TextAlign::Start);
            let gap = if title_height > 0.0 { root.gap } else { 0.0 };
            let description_rect = Rect::from_xywh(
                self.text.origin.x,
                title_rect.max_y() + gap,
                self.text.size.width,
                line(&description, &self.description, cx),
            );
            paint_wrapped(
                cx,
                description_rect,
                &self.description,
                &description,
                TextAlign::Start,
            );
            self.buttons
                .paint(cx, Some(self.stops.focus), &self.stops.parts, focus_visible);
        });
    }

    fn semantics(&self) -> SemanticNode {
        self.semantics_in(Localizer::builtin())
    }

    /// A destructive alert says so in its description.
    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        let actions = if self.enabled && !self.buttons.buttons.is_empty() {
            vec![SemanticAction::Activate, SemanticAction::Focus]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::Group, &self.title, actions);
        node.value = (!self.description.is_empty()).then(|| self.description.clone());
        if self.variant == AlertVariant::Destructive {
            node.description = strings.message("choscorui.alert.destructive", &[]);
        }
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
            SemanticAction::Activate if self.stops.focus < self.buttons.buttons.len() => {
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
