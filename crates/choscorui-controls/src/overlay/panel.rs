//! Parts that overlay panels and surfaces share: a wrapped title and
//! description, a row or column of action buttons, and the focus and
//! press tracking for buttons that are tab stops inside one control.

use crate::custom::{ControlContext, ControlEvent, MeasureContext};
use crate::form::measured::{paint_wrapped, wrapped_height};
use crate::style::{
    ButtonVariant, ControlKind, ControlSize, PaintContext, Slot, StyleQuery, TextAlign, VisualState,
};
use choscorui_core::{
    EventPhase, EventReply, Key, KeyEvent, Point, PointerButton, PointerEvent, Rect,
};

/// The title and description of an overlay panel.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PanelText {
    pub title: String,
    pub description: String,
}

impl PanelText {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
        }
    }

    /// The title and the description, one per line, for semantics.
    pub fn joined(&self) -> String {
        [self.title.as_str(), self.description.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The height of the title, the header gap, and the description,
    /// wrapped in `width`. `query` names the overlay kind and variant.
    pub fn height(&self, cx: &MeasureContext<'_>, query: StyleQuery, width: f64) -> f64 {
        let title = cx.style(query.slot(Slot::Title));
        let description = cx.style(query.slot(Slot::Description));
        let header = cx.style(query.slot(Slot::Header));
        let title_height = wrapped_height(&self.title, &title.text, width, cx.text);
        let description_height =
            wrapped_height(&self.description, &description.text, width, cx.text);
        let gap = if title_height > 0.0 && description_height > 0.0 {
            header.gap
        } else {
            0.0
        };
        title_height + gap + description_height
    }

    /// Paints the title and the description from the top of `rect`.
    pub fn paint(
        &self,
        cx: &mut PaintContext<'_>,
        query: StyleQuery,
        rect: Rect,
        align: TextAlign,
    ) {
        let title = cx.style(query.slot(Slot::Title));
        let description = cx.style(query.slot(Slot::Description));
        let header = cx.style(query.slot(Slot::Header));
        let width = rect.size.width;
        let title_height = wrapped_height(&self.title, &title.text, width, cx.text);
        let title_rect = Rect::from_xywh(rect.origin.x, rect.origin.y, width, title_height);
        paint_wrapped(cx, title_rect, &self.title, &title, align);
        let top = if title_height > 0.0 {
            title_rect.max_y() + header.gap
        } else {
            rect.origin.y
        };
        let height = wrapped_height(&self.description, &description.text, width, cx.text);
        let description_rect = Rect::from_xywh(rect.origin.x, top, width, height);
        paint_wrapped(cx, description_rect, &self.description, &description, align);
    }
}

/// One button of an overlay footer.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PanelButton {
    pub label: String,
    pub variant: ButtonVariant,
}

/// Footer buttons and where they are.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Buttons {
    pub buttons: Vec<PanelButton>,
    pub rects: Vec<Rect>,
}

impl Buttons {
    fn query(variant: ButtonVariant, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Button, Slot::Root)
            .variant(variant)
            .size(ControlSize::Default)
            .state(state)
    }

    /// The width of one button.
    pub fn width(cx: &MeasureContext<'_>, button: &PanelButton) -> f64 {
        let style = cx.style(Self::query(button.variant, VisualState::NONE));
        (cx.text.width(&button.label, &style.text) + style.padding.horizontal())
            .max(style.min_width)
    }

    /// The height of one button.
    pub fn button_height(cx: &MeasureContext<'_>) -> f64 {
        cx.style(Self::query(ButtonVariant::Default, VisualState::NONE))
            .min_height
    }

    /// The height of the buttons in a row, or stacked with `gap`.
    pub fn height(&self, cx: &MeasureContext<'_>, stacked: bool, gap: f64) -> f64 {
        let count = self.buttons.len();
        if count == 0 {
            return 0.0;
        }
        let one = Self::button_height(cx);
        if stacked {
            one * count as f64 + gap * (count - 1) as f64
        } else {
            one
        }
    }

    /// Places the buttons in a row at the end of `rect`, `gap` apart, in
    /// reading order.
    pub fn place_row(&mut self, cx: &MeasureContext<'_>, rect: Rect, gap: f64) {
        let height = Self::button_height(cx);
        let widths: Vec<f64> = self
            .buttons
            .iter()
            .map(|button| Self::width(cx, button))
            .collect();
        let total = widths.iter().sum::<f64>() + gap * widths.len().saturating_sub(1) as f64;
        let rtl = cx.rtl();
        let mut x = if rtl {
            rect.origin.x + total
        } else {
            rect.max_x() - total
        };
        self.rects = widths
            .iter()
            .map(|width| {
                let origin = if rtl { x - width } else { x };
                let button = Rect::from_xywh(origin, rect.origin.y, *width, height);
                x = if rtl {
                    x - width - gap
                } else {
                    x + width + gap
                };
                button
            })
            .collect();
    }

    /// Stacks the buttons across the width of `rect`, `gap` apart.
    pub fn place_column(&mut self, cx: &MeasureContext<'_>, rect: Rect, gap: f64) {
        let height = Self::button_height(cx);
        self.rects = (0..self.buttons.len())
            .map(|index| {
                let y = rect.origin.y + index as f64 * (height + gap);
                Rect::from_xywh(rect.origin.x, y, rect.size.width, height)
            })
            .collect();
    }

    /// The button under `point`.
    pub fn hit(&self, point: Point) -> Option<usize> {
        crate::hosted::hit(&self.rects, point)
    }

    /// Paints the buttons. `focused` shows the ring when `focus_visible`,
    /// and `pressed` shows the pressed look.
    pub fn paint(
        &self,
        cx: &mut PaintContext<'_>,
        focused: Option<usize>,
        pressed: Option<usize>,
        focus_visible: bool,
    ) {
        for (index, (button, rect)) in self.buttons.iter().zip(&self.rects).enumerate() {
            let state = VisualState::NONE
                .set(
                    VisualState::FOCUS_VISIBLE,
                    focus_visible && focused == Some(index),
                )
                .set(VisualState::PRESSED, pressed == Some(index));
            let style = cx.style(Self::query(button.variant, state));
            cx.slot(*rect, &style, |cx, inner| {
                cx.label(inner, &button.label, &style, TextAlign::Center);
            });
        }
    }
}

/// Focus and press tracking for buttons that are tab stops inside one
/// control. Tab and Shift+Tab move between the buttons and leave the
/// control at the ends. Enter, Space, and a click activate a button.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ButtonStops {
    pub focus: usize,
    pub pressed: Option<usize>,
}

impl ButtonStops {
    /// Handles `event` for `buttons`. Returns the reply and the button
    /// that the event activated.
    pub fn event(
        &mut self,
        cx: &mut ControlContext<'_>,
        event: &ControlEvent,
        buttons: &Buttons,
    ) -> (EventReply, Option<usize>) {
        if cx.phase() != EventPhase::Target {
            return (EventReply::Continue, None);
        }
        let count = buttons.buttons.len();
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) => {
                let Some(index) = buttons.hit(*position) else {
                    return (EventReply::Continue, None);
                };
                self.pressed = Some(index);
                self.focus = index;
                cx.request_focus();
                cx.capture_pointer(true);
                cx.request_paint();
                (EventReply::Stop, None)
            }
            ControlEvent::Pointer(PointerEvent::Up { position, .. }) if self.pressed.is_some() => {
                cx.capture_pointer(false);
                cx.request_paint();
                let hit = buttons.hit(*position);
                let activated = self.pressed.take().filter(|index| hit == Some(*index));
                (EventReply::Stop, activated)
            }
            ControlEvent::Key(KeyEvent::Down { key: Key::Tab, .. }) => {
                match crate::hosted::tab_stop(self.focus, count, cx.modifiers().shift) {
                    Some(next) => {
                        self.focus = next;
                        cx.request_paint();
                        cx.request_semantics();
                        (EventReply::Stop, None)
                    }
                    None => (EventReply::Continue, None),
                }
            }
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Enter | Key::Space,
                repeat: false,
            }) if self.focus < count => (EventReply::Stop, Some(self.focus)),
            ControlEvent::FocusChanged(focused) => {
                // Shift+Tab arrives at the last button.
                if *focused {
                    self.focus = if cx.modifiers().shift {
                        count.saturating_sub(1)
                    } else {
                        0
                    };
                } else if self.pressed.take().is_some() {
                    cx.capture_pointer(false);
                }
                cx.request_paint();
                cx.request_semantics();
                (EventReply::Continue, None)
            }
            _ => (EventReply::Continue, None),
        }
    }
}
