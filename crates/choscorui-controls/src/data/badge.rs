//! A small status label, optionally a link.

use crate::custom::{ControlContext, ControlEvent, ControlMessage, CustomControl, MeasureContext};
use crate::hosted::{self, Press, PressOutcome};
use crate::style::{
    BadgeVariant, Content, ControlKind, PaintContext, ResolvedStyle, Slot, StyleQuery, TextAlign,
    VisualState,
};
use choscorui_core::{
    EventReply, Icon, Key, Measured, SemanticAction, SemanticNode, SemanticRole, Size,
};

/// A badge: a short label with an optional leading icon. A plain badge
/// is static text and takes no focus. A badge made with
/// [`Badge::as_link`] is a link: it takes focus, shows the hover look,
/// and emits `ItemActivated(0)` on a click, Enter, or Space.
pub struct Badge {
    label: String,
    icon: Option<Icon>,
    variant: BadgeVariant,
    link: bool,
    press: Press,
    enabled: bool,
}

impl Badge {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            variant: BadgeVariant::Default,
            link: false,
            press: Press::default(),
            enabled: true,
        }
    }

    #[must_use]
    pub fn with_variant(mut self, variant: BadgeVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Shows `icon` before the label.
    #[must_use]
    pub fn with_icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Makes the badge a link.
    #[must_use]
    pub fn as_link(mut self) -> Self {
        self.link = true;
        self
    }

    fn style(
        &self,
        resolve: impl Fn(StyleQuery) -> ResolvedStyle,
        state: VisualState,
    ) -> ResolvedStyle {
        // Only a link shows the hover and pressed looks.
        let state = if self.link {
            state.set(VisualState::PRESSED, self.press.pressed)
        } else {
            state
                .set(VisualState::HOVERED, false)
                .set(VisualState::PRESSED, false)
        };
        let mut style = resolve(
            StyleQuery::new(ControlKind::Badge, Slot::Root)
                .variant(self.variant)
                .state(state),
        );
        // A link shows the pointing hand, in every variant; static text
        // keeps the arrow.
        style.cursor = if self.link {
            crate::style::Cursor::Pointer
        } else {
            crate::style::Cursor::Auto
        };
        style
    }

    fn content(&self) -> Content<'_> {
        Content {
            leading: self.icon,
            label: &self.label,
            trailing: None,
        }
    }
}

impl CustomControl for Badge {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = self.style(|query| cx.style(query), VisualState::NONE);
        let size = self.content().size(&style, cx.text);
        Measured {
            size: Size::new(size.width, size.height.max(style.min_height)),
            baseline: None,
        }
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        // A link takes Enter only; Space is for buttons.
        if !self.link || hosted::key_down(event) == Some(Key::Space) {
            return EventReply::Continue;
        }
        let outcome = self.press.event(cx, event);
        if let PressOutcome::Activated(_) = outcome {
            cx.emit(ControlMessage::ItemActivated(0));
        }
        hosted::reply(outcome)
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let style = self.style(|query| cx.style(query), cx.state);
        cx.slot(cx.bounds, &style, |cx, inner| {
            self.content().draw(cx, inner, &style, TextAlign::Center);
        });
    }

    fn semantics(&self) -> SemanticNode {
        let (role, actions) = if self.link && self.enabled {
            (
                SemanticRole::Link,
                vec![SemanticAction::Activate, SemanticAction::Focus],
            )
        } else if self.link {
            (SemanticRole::Link, Vec::new())
        } else {
            (SemanticRole::StaticText, Vec::new())
        };
        let mut node = hosted::node(role, &self.label, actions);
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
            SemanticAction::Activate if self.link => {
                cx.emit(ControlMessage::ItemActivated(0));
                true
            }
            SemanticAction::Focus if self.link => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn focusable(&self) -> bool {
        self.link
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}
