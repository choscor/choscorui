//! The size grip page: a mock window resized by its grip.

use crate::ui::page::{PageCx, column, stateful};
use choscorui::compose::view::{AnyView, Configured, VStack, ViewExt};
use choscorui::controls::{
    ControlContext, ControlEvent, ControlKind, ControlMessage, ControlValue, CustomControl,
    MeasureContext, PaintContext, SizeGrip, Slot, StyleQuery, TextAlign, paint_box,
};
use choscorui::core::{
    ColorRole, EventPhase, EventReply, Key, KeyEvent, Measured, Point, PointerButton, PointerEvent,
    RadiusScale, Rect, SemanticAction, SemanticNode, SemanticRole, SemanticState, Size,
};

/// The grip's square, in points.
const GRIP: f64 = 16.0;
/// How far one key press or action resizes, in points.
const STEP: f64 = 10.0;

/// A mock window that its grip resizes (bottom-trailing; bottom-left in
/// right-to-left layouts), clamped to a minimum and maximum size. The
/// node is the grip: it takes focus, the arrow keys, and the increment
/// and decrement actions, and reports each new size as
/// `Changed(Range(width, height))`.
struct MockWindow {
    grip: SizeGrip,
    size: Size,
    dragging: bool,
}

impl MockWindow {
    fn new(size: Size) -> Self {
        Self {
            grip: SizeGrip::new(Size::new(220.0, 140.0), Size::new(620.0, 400.0), false),
            size,
            dragging: false,
        }
    }

    /// The grip in the window at `bounds`.
    fn grip_rect(&self, bounds: Rect) -> Rect {
        let local = self.grip.rect(self.size, GRIP);
        Rect::new(
            Point::new(
                bounds.origin.x + local.origin.x,
                bounds.origin.y + local.origin.y,
            ),
            local.size,
        )
    }

    /// Resizes to `size`, clamped, and reports a change.
    fn resize(&mut self, cx: &mut ControlContext<'_>, size: Size) {
        let (minimum, maximum) = (self.grip.minimum, self.grip.maximum);
        let clamped = Size::new(
            size.width.clamp(minimum.width, maximum.width),
            size.height.clamp(minimum.height, maximum.height),
        );
        if clamped == self.size {
            return;
        }
        self.size = clamped;
        cx.request_layout();
        cx.request_paint();
        cx.request_semantics();
        cx.emit(ControlMessage::Changed(ControlValue::Range(
            clamped.width,
            clamped.height,
        )));
    }

    fn release(&mut self, cx: &mut ControlContext<'_>) {
        self.grip.release();
        self.dragging = false;
        cx.capture_pointer(false);
    }
}

impl CustomControl for MockWindow {
    fn measure(&mut self, _cx: &MeasureContext<'_>) -> Measured {
        Measured::new(self.size)
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, _bounds: Rect) {
        self.grip.rtl = cx.rtl();
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if self.grip_rect(cx.bounds()).contains(*position) => {
                cx.request_focus();
                self.grip.press(*position, self.size);
                self.dragging = true;
                cx.capture_pointer(true);
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) if self.dragging => {
                if let Some(size) = self.grip.drag(*position) {
                    self.resize(cx, size);
                }
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Up { .. }) if self.dragging => {
                self.release(cx);
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) => {
                let grow = if self.grip.rtl { -STEP } else { STEP };
                let (dx, dy) = match key {
                    Key::ArrowRight => (grow, 0.0),
                    Key::ArrowLeft => (-grow, 0.0),
                    Key::ArrowDown => (0.0, STEP),
                    Key::ArrowUp => (0.0, -STEP),
                    _ => return EventReply::Continue,
                };
                let size = Size::new(self.size.width + dx, self.size.height + dy);
                self.resize(cx, size);
                EventReply::Stop
            }
            ControlEvent::FocusChanged(_) | ControlEvent::Dismiss => {
                if self.dragging {
                    self.release(cx);
                }
                cx.request_paint();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let bounds = cx.bounds;
        let panel = cx.style(StyleQuery::new(ControlKind::Card, Slot::Root));
        paint_box(cx.list, bounds, &panel, cx.rtl);
        let label = format!("{:.0} × {:.0}", self.size.width, self.size.height);
        cx.label(bounds, &label, &panel, TextAlign::Center);
        let grip = self.grip_rect(bounds);
        self.grip.paint(&mut cx.at(grip.inset(2.0)));
        if cx.state.focus_visible() {
            // Inside the grip, as the window clips what is outside it.
            let tokens = cx.tokens;
            let color = tokens.ring_color(tokens.color(ColorRole::Ring));
            let ring = grip.inset(tokens.ring_width);
            let radius = tokens.radius(RadiusScale::Md);
            let _ = cx.list.draw_ring(ring, radius, tokens.ring_width, color);
        }
    }

    fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Button,
            name: "Resize".into(),
            description: "Arrow keys resize by 10 points".into(),
            value: Some(format!("{:.0} by {:.0}", self.size.width, self.size.height)),
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: vec![
                SemanticAction::Increment,
                SemanticAction::Decrement,
                SemanticAction::Focus,
            ],
            text: None,
        }
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        let delta = match action {
            SemanticAction::Increment => STEP,
            SemanticAction::Decrement => -STEP,
            SemanticAction::Focus => {
                cx.request_focus();
                return true;
            }
            _ => return false,
        };
        let before = self.size;
        self.resize(
            cx,
            Size::new(self.size.width + delta, self.size.height + delta),
        );
        self.size != before
    }

    fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.grip.release();
            self.dragging = false;
        }
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Range(width, height) = *value else {
            return false;
        };
        self.size = Size::new(width, height);
        true
    }
}

/// A mock window resized by its grip, clamped to a minimum and maximum
/// size. In right-to-left layouts its trailing (right) edge stays put and
/// it grows leftward, like a mirrored window.
pub fn size_grip(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let size = cx.state((360.0, 220.0));
        let window = Configured::custom(ControlKind::SizeGrip, move |_| {
            let (width, height) = size.get_untracked().unwrap_or((360.0, 220.0));
            MockWindow::new(Size::new(width, height))
        })
        .value(move || {
            let (width, height) = size.get();
            ControlValue::Range(width, height)
        })
        .on_message(move |message| {
            if let ControlMessage::Changed(ControlValue::Range(width, height)) = message {
                size.set((*width, *height));
            }
        });
        column(VStack::new(window).accessibility_label("Mock window"))
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::controls::ShortcutModifiers;
    use choscorui::core::{Key, KeyEvent, PointerButton, PointerEvent, SemanticAction};

    #[test]
    fn the_grip_resizes_from_actions_keys_and_drags_within_the_limits() {
        let mut page = Harness::new("size-grip");
        assert!(page.host.find("Mock window").is_some());
        assert_eq!(page.value("Resize").as_deref(), Some("360 by 220"));
        assert!(page.act("Resize", SemanticAction::Increment));
        assert_eq!(page.value("Resize").as_deref(), Some("370 by 230"));

        assert!(page.act("Resize", SemanticAction::Focus));
        let event = KeyEvent::Down {
            key: Key::ArrowLeft,
            repeat: false,
        };
        assert!(page.host.key(event, ShortcutModifiers::default()));
        assert_eq!(page.value("Resize").as_deref(), Some("360 by 230"));

        let grip = page.node("Resize");
        let window = page.host.tree().bounds(grip).unwrap();
        let start = window
            .origin
            .offset(window.size.width - 8.0, window.size.height - 8.0);
        let modifiers = ShortcutModifiers::default();
        let press = PointerEvent::Down {
            position: start,
            button: PointerButton::Primary,
        };
        page.host.pointer(press, modifiers);
        let far = start.offset(1000.0, 1000.0);
        page.host
            .pointer(PointerEvent::Move { position: far }, modifiers);
        let release = PointerEvent::Up {
            position: far,
            button: PointerButton::Primary,
        };
        page.host.pointer(release, modifiers);
        page.host.layout();
        assert_eq!(page.value("Resize").as_deref(), Some("620 by 400"));
    }
}
