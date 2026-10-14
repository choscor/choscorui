//! An empty state: media, a title, a description, and action buttons,
//! centered.

use crate::Avatar;
use crate::custom::{ControlContext, ControlEvent, ControlMessage, CustomControl, MeasureContext};
use crate::form::measured::{paint_wrapped, wrapped_height};
use crate::hosted;
use crate::overlay::panel::{ButtonStops, Buttons, PanelButton};
use crate::style::{
    ButtonVariant, ControlKind, EmptyMediaVariant, PaintContext, Slot, StyleQuery, TextAlign,
};
use choscorui_core::{
    EventReply, Icon, Localizer, Measured, Rect, SemanticAction, SemanticNode, SemanticParts,
    SemanticRole, Size,
};

/// What the empty state shows above its title.
enum Media {
    Icon(Icon),
    Avatar(Avatar),
}

/// An empty state. The media, the title, the description, and the
/// buttons sit centered in a column. The buttons are tab stops, and
/// activating one emits `ItemActivated(index)`.
pub struct Empty {
    title: String,
    description: String,
    media: Option<Media>,
    buttons: Buttons,
    stops: ButtonStops,
    /// The media, title, and description rectangles.
    parts: [Rect; 3],
    enabled: bool,
}

impl Empty {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            media: None,
            buttons: Buttons::default(),
            stops: ButtonStops::default(),
            parts: [Rect::default(); 3],
            enabled: true,
        }
    }

    /// Shows `icon` in a muted box above the title.
    #[must_use]
    pub fn with_icon(mut self, icon: Icon) -> Self {
        self.media = Some(Media::Icon(icon));
        self
    }

    /// Shows `avatar` above the title.
    #[must_use]
    pub fn with_avatar(mut self, avatar: Avatar) -> Self {
        self.media = Some(Media::Avatar(avatar));
        self
    }

    /// Adds a button after the others.
    #[must_use]
    pub fn with_action(mut self, label: impl Into<String>, variant: ButtonVariant) -> Self {
        self.buttons.buttons.push(PanelButton {
            label: label.into(),
            variant,
        });
        self
    }

    fn query(slot: Slot) -> StyleQuery {
        StyleQuery::new(ControlKind::Empty, slot)
    }

    fn media_query(&self) -> StyleQuery {
        let variant = match self.media {
            Some(Media::Icon(_)) => EmptyMediaVariant::Icon,
            _ => EmptyMediaVariant::Default,
        };
        Self::query(Slot::Media).variant(variant)
    }

    /// The media size without its bottom padding.
    fn media_size(&self, cx: &MeasureContext<'_>) -> Size {
        let media = cx.style(self.media_query());
        match &self.media {
            Some(Media::Icon(_)) => Size::new(media.min_width, media.min_height),
            Some(Media::Avatar(avatar)) => {
                let side = avatar.side(cx);
                Size::new(side, side)
            }
            None => Size::new(0.0, 0.0),
        }
    }

    /// The heights of the media with its padding, the title, the
    /// description, and the buttons, in a column of text width `width`.
    fn heights(&self, cx: &MeasureContext<'_>, width: f64) -> [f64; 4] {
        let media = cx.style(self.media_query());
        let title = cx.style(Self::query(Slot::Title));
        let description = cx.style(Self::query(Slot::Description));
        let media_height = if self.media.is_some() {
            self.media_size(cx).height + media.padding.block_end
        } else {
            0.0
        };
        [
            media_height,
            wrapped_height(&self.title, &title.text, width, cx.text),
            wrapped_height(&self.description, &description.text, width, cx.text),
            Buttons::button_height(cx) * f64::from(u8::from(!self.buttons.buttons.is_empty())),
        ]
    }

    /// The text width inside a root of `width`.
    fn text_width(cx: &MeasureContext<'_>, width: f64) -> f64 {
        let root = cx.style(Self::query(Slot::Root));
        let header = cx.style(Self::query(Slot::Header));
        (width - root.padding.horizontal())
            .min(header.max_width)
            .max(1.0)
    }

    fn total(&self, cx: &MeasureContext<'_>, heights: &[f64; 4]) -> f64 {
        let header = cx.style(Self::query(Slot::Header));
        let content = cx.style(Self::query(Slot::Content));
        let [media, title, description, buttons] = *heights;
        let mut total = media + title + description;
        if title > 0.0 && description > 0.0 {
            total += header.gap;
        }
        if buttons > 0.0 {
            total += content.gap + buttons;
        }
        total
    }
}

impl CustomControl for Empty {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let root = cx.style(Self::query(Slot::Root));
        let header = cx.style(Self::query(Slot::Header));
        let max = cx.constraints.max.width;
        let width = if max.is_finite() {
            max
        } else {
            header.max_width + root.padding.horizontal()
        };
        let heights = self.heights(cx, Self::text_width(cx, width));
        Measured {
            size: Size::new(width, self.total(cx, &heights) + root.padding.vertical()),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let header = cx.style(Self::query(Slot::Header));
        let content = cx.style(Self::query(Slot::Content));
        let width = Self::text_width(cx, bounds.size.width);
        let heights = self.heights(cx, width);
        let total = self.total(cx, &heights);
        let mut y = bounds.center().y - total / 2.0;
        let x = bounds.center().x - width / 2.0;
        let media = self.media_size(cx);
        self.parts[0] = Rect::from_xywh(
            bounds.center().x - media.width / 2.0,
            y,
            media.width,
            media.height,
        );
        y += heights[0];
        self.parts[1] = Rect::from_xywh(x, y, width, heights[1]);
        y += heights[1];
        if heights[1] > 0.0 && heights[2] > 0.0 {
            y += header.gap;
        }
        self.parts[2] = Rect::from_xywh(x, y, width, heights[2]);
        y += heights[2] + content.gap;
        // Center the button row: place it at the end of a row as wide as
        // the buttons.
        let gap = cx.tokens.spacing * 2.0;
        let buttons: f64 = self
            .buttons
            .buttons
            .iter()
            .map(|button| Buttons::width(cx, button))
            .sum::<f64>()
            + gap * self.buttons.buttons.len().saturating_sub(1) as f64;
        let row = Rect::from_xywh(bounds.center().x - buttons / 2.0, y, buttons, heights[3]);
        self.buttons.place_row(cx, row, gap);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let (reply, activated) = self.stops.event(cx, event, &self.buttons);
        if let Some(index) = activated {
            cx.emit(ControlMessage::ItemActivated(index));
        }
        reply
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let root = cx.style(Self::query(Slot::Root));
        let title = cx.style(Self::query(Slot::Title));
        let description = cx.style(Self::query(Slot::Description));
        let media = cx.style(self.media_query());
        let focus_visible = cx.state.focus_visible();
        cx.slot(cx.bounds, &root, |cx, _| {
            match &self.media {
                Some(Media::Icon(icon)) => {
                    let mut boxed = media.clone();
                    boxed.padding = Default::default();
                    cx.slot(self.parts[0], &boxed, |cx, inner| {
                        cx.icon(inner, *icon, &media);
                    });
                }
                Some(Media::Avatar(avatar)) => avatar.paint_at(cx, self.parts[0]),
                None => {}
            }
            paint_wrapped(cx, self.parts[1], &self.title, &title, TextAlign::Center);
            paint_wrapped(
                cx,
                self.parts[2],
                &self.description,
                &description,
                TextAlign::Center,
            );
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
        let mut node = hosted::node(SemanticRole::Group, &self.title, actions);
        node.value = (!self.description.is_empty()).then(|| self.description.clone());
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
