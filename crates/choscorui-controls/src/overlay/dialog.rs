//! A trigger that opens a modal panel: a centered dialog, a sheet at an
//! edge of the window, or a drawer that the pointer can drag away.

use super::panel::{Buttons, PanelButton, PanelText};
use crate::custom::{ControlContext, ControlEvent, ControlMessage, CustomControl, MeasureContext};
use crate::hosted::{self, Parts, Press, PressOutcome, Reveal};
use crate::style::{
    ButtonVariant, ControlKind, ControlSize, Edges, PaintContext, ResolvedStyle, SheetSide, Slot,
    StyleQuery, TextAlign, VisualState,
};
use choscorui_core::{
    EventPhase, EventReply, Icon, Key, KeyEvent, Measured, Point, PointerButton, PointerEvent,
    Rect, SemanticAction, SemanticNode, SemanticRole, Size, Transform,
};

/// The share of a drawer's size that a drag must pass to dismiss it.
const DRAG_DISMISS: f64 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Form {
    Dialog,
    Sheet(SheetSide),
    Drawer(SheetSide),
}

/// A pointer drag on a drawer panel.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Drag {
    start: Point,
    /// How far the panel has moved toward its edge, in points.
    offset: f64,
}

/// An outline button that opens a modal panel with a title, a
/// description, and action buttons. `Dialog::new` centers the panel;
/// `Dialog::sheet` and `Dialog::drawer` slide it in from an edge. While
/// open, focus and pointer input stay in the panel.
pub struct Dialog {
    form: Form,
    trigger: String,
    text: PanelText,
    buttons: Buttons,
    /// The accessible name of the close button; `None` hides it.
    close_label: Option<String>,
    reveal: Reveal,
    viewport: Rect,
    panel: Rect,
    header: Rect,
    close: Rect,
    handle: Option<Rect>,
    motion_ms: u32,
    /// The focused part while open: a button index, or the button count
    /// for the close button.
    focus: usize,
    /// The part under a pointer press.
    /// Hover and press of the buttons and the close button.
    parts: Parts,
    trigger_press: Press,
    drag: Option<Drag>,
    enabled: bool,
}

impl Dialog {
    /// A dialog centered in the window.
    pub fn new(
        trigger: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            form: Form::Dialog,
            trigger: trigger.into(),
            text: PanelText::new(title, description),
            buttons: Buttons::default(),
            close_label: None,
            reveal: Reveal::default(),
            viewport: Rect::default(),
            panel: Rect::default(),
            header: Rect::default(),
            close: Rect::default(),
            handle: None,
            motion_ms: 0,
            focus: 0,
            parts: Parts::default(),
            trigger_press: Press::default(),
            drag: None,
            enabled: true,
        }
    }

    /// A sheet that slides in from `side` and fills the window along it.
    pub fn sheet(
        side: SheetSide,
        trigger: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            form: Form::Sheet(side),
            ..Self::new(trigger, title, description)
        }
    }

    /// A drawer that slides in from `side`. A drag toward `side` past a
    /// quarter of its size dismisses it.
    pub fn drawer(
        side: SheetSide,
        trigger: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            form: Form::Drawer(side),
            ..Self::new(trigger, title, description)
        }
    }

    /// Adds a footer button. Activating it emits `ItemActivated` with its
    /// index and closes the panel.
    #[must_use]
    pub fn with_action(mut self, label: impl Into<String>, variant: ButtonVariant) -> Self {
        self.buttons.buttons.push(PanelButton {
            label: label.into(),
            variant,
        });
        self
    }

    /// Shows a close button at the top-end corner with `label` as its
    /// accessible name. A drawer has no close button.
    #[must_use]
    pub fn with_close(mut self, label: impl Into<String>) -> Self {
        self.close_label = Some(label.into());
        self
    }

    pub fn is_open(&self) -> bool {
        self.reveal.open
    }

    fn kind(&self) -> ControlKind {
        match self.form {
            Form::Dialog => ControlKind::Dialog,
            Form::Sheet(_) => ControlKind::Sheet,
            Form::Drawer(_) => ControlKind::Drawer,
        }
    }

    fn side(&self) -> Option<SheetSide> {
        match self.form {
            Form::Dialog => None,
            Form::Sheet(side) | Form::Drawer(side) => Some(side),
        }
    }

    fn query(&self, slot: Slot) -> StyleQuery {
        let query = StyleQuery::new(self.kind(), slot);
        match self.side() {
            Some(side) => query.variant(side),
            None => query,
        }
    }

    fn has_close(&self) -> bool {
        self.close_label.is_some() && !matches!(self.form, Form::Drawer(_))
    }

    /// The number of focusable parts while open.
    fn parts(&self) -> usize {
        self.buttons.buttons.len() + usize::from(self.has_close())
    }

    fn part_at(&self, point: Point) -> Option<usize> {
        if let Some(index) = self.buttons.hit(point) {
            return Some(index);
        }
        (self.has_close() && self.close.contains(point)).then_some(self.buttons.buttons.len())
    }

    fn part_label(&self, part: usize) -> Option<&str> {
        match self.buttons.buttons.get(part) {
            Some(button) => Some(&button.label),
            None => self.close_label.as_deref().filter(|_| self.has_close()),
        }
    }

    /// Opens or closes the dialog at `time` and ends a press or drag.
    /// Returns true when it changed.
    fn apply_open(&mut self, open: bool, time: f64) -> bool {
        self.drag = None;
        self.parts.clear();
        if self.reveal.open == open {
            return false;
        }
        self.reveal.set(open, time);
        self.focus = 0;
        true
    }

    fn change_open(&mut self, cx: &mut ControlContext<'_>, open: bool) {
        if !self.apply_open(open, cx.time()) {
            return;
        }
        if open {
            cx.request_focus();
        }
        cx.emit(ControlMessage::OpenChanged(open));
        cx.request_paint();
        cx.request_semantics();
    }

    fn dismiss(&mut self, cx: &mut ControlContext<'_>) {
        self.change_open(cx, false);
        cx.emit(ControlMessage::Dismissed);
    }

    fn activate(&mut self, cx: &mut ControlContext<'_>, part: usize) {
        if part < self.buttons.buttons.len() {
            cx.emit(ControlMessage::ItemActivated(part));
            self.change_open(cx, false);
        } else if self.has_close() {
            self.dismiss(cx);
        }
    }

    /// The direction a drawer moves to close: a unit vector.
    fn dismiss_direction(&self) -> (f64, f64) {
        match self.side() {
            Some(SheetSide::Top) => (0.0, -1.0),
            Some(SheetSide::Bottom) | None => (0.0, 1.0),
            Some(SheetSide::Left) => (-1.0, 0.0),
            Some(SheetSide::Right) => (1.0, 0.0),
        }
    }

    /// The panel's size along its dismiss direction.
    fn extent(&self) -> f64 {
        match self.side() {
            Some(SheetSide::Left | SheetSide::Right) => self.panel.size.width,
            _ => self.panel.size.height,
        }
    }

    fn open_key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> EventReply {
        let parts = self.parts();
        match key {
            Key::Escape => self.dismiss(cx),
            Key::Tab if parts > 0 => {
                // Focus cycles inside the panel.
                self.focus = if cx.modifiers().shift {
                    (self.focus + parts - 1) % parts
                } else {
                    (self.focus + 1) % parts
                };
                cx.request_paint();
                cx.request_semantics();
            }
            Key::Enter | Key::Space if parts > 0 => self.activate(cx, self.focus),
            _ => return EventReply::Continue,
        }
        EventReply::Stop
    }

    fn open_pointer(&mut self, cx: &mut ControlContext<'_>, event: &PointerEvent) -> EventReply {
        let pressed = self.parts.pressed;
        let mut parts = self.parts;
        parts.track(cx, &ControlEvent::Pointer(*event), |point| {
            self.part_at(point)
        });
        self.parts = parts;
        match *event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } => {
                if let Some(part) = self.parts.pressed {
                    self.focus = part;
                } else if !self.panel.contains(position) {
                    self.dismiss(cx);
                } else if matches!(self.form, Form::Drawer(_)) {
                    self.drag = Some(Drag {
                        start: position,
                        offset: 0.0,
                    });
                    cx.capture_pointer(true);
                }
                cx.request_paint();
            }
            PointerEvent::Move { position } => {
                let (ux, uy) = self.dismiss_direction();
                if let Some(drag) = &mut self.drag {
                    let (dx, dy) = (position.x - drag.start.x, position.y - drag.start.y);
                    drag.offset = (dx * ux + dy * uy).max(0.0);
                    cx.request_paint();
                }
            }
            PointerEvent::Up { position, .. } => {
                cx.capture_pointer(false);
                if let Some(drag) = self.drag.take() {
                    if drag.offset > self.extent() * DRAG_DISMISS {
                        self.dismiss(cx);
                    }
                    cx.request_paint();
                } else if let Some(part) = pressed {
                    if self.part_at(position) == Some(part) {
                        self.activate(cx, part);
                    }
                    cx.request_paint();
                }
            }
            _ => {}
        }
        EventReply::Stop
    }

    fn arrange_dialog(&mut self, cx: &MeasureContext<'_>, panel: &ResolvedStyle) {
        let viewport = self.viewport;
        let footer = cx.style(self.query(Slot::Footer));
        let width = panel
            .max_width
            .min(viewport.size.width - 2.0 * cx.tokens.spacing * 4.0);
        let inner = width - panel.padding.horizontal();
        let text = self.text.height(cx, self.query(Slot::Root), inner);
        let buttons = self.buttons.height(cx, false, footer.gap);
        let gap = if buttons > 0.0 { panel.gap } else { 0.0 };
        let height = panel.padding.vertical() + text + gap + buttons;
        let x = viewport.center().x - width / 2.0;
        let y = viewport.center().y - height / 2.0;
        self.panel = Rect::from_xywh(x, y, width, height);
        let inner_x = x + panel.padding.inline_start;
        self.header = Rect::from_xywh(inner_x, y + panel.padding.block_start, inner, text);
        let footer_rect = Rect::from_xywh(inner_x, self.header.max_y() + gap, inner, buttons);
        self.buttons.place_row(cx, footer_rect, footer.gap);
    }

    fn arrange_edge(&mut self, cx: &MeasureContext<'_>, panel: &ResolvedStyle, side: SheetSide) {
        let viewport = self.viewport;
        let drawer = matches!(self.form, Form::Drawer(_));
        let header = cx.style(self.query(Slot::Header));
        let footer = cx.style(self.query(Slot::Footer));
        let handle = cx.style(self.query(Slot::Handle));
        let vertical = matches!(side, SheetSide::Left | SheetSide::Right);
        let width = if vertical {
            (viewport.size.width * 0.75).min(384.0)
        } else {
            viewport.size.width
        };
        let inner = width - header.padding.horizontal();
        let text = self.text.height(cx, self.query(Slot::Root), inner);
        let buttons = self.buttons.height(cx, true, footer.gap);
        let handle_height = if drawer && side == SheetSide::Bottom {
            handle.padding.block_start + handle.min_height
        } else {
            0.0
        };
        let header_height = text + header.padding.vertical();
        let footer_height = if buttons > 0.0 {
            buttons + footer.padding.vertical()
        } else {
            0.0
        };
        let height = if vertical {
            viewport.size.height
        } else {
            (handle_height + header_height + panel.gap + footer_height)
                .min(viewport.size.height * 0.8)
        };
        let (x, y) = match side {
            SheetSide::Right => (viewport.max_x() - width, viewport.origin.y),
            SheetSide::Left => (viewport.origin.x, viewport.origin.y),
            SheetSide::Top => (viewport.origin.x, viewport.origin.y),
            SheetSide::Bottom => (viewport.origin.x, viewport.max_y() - height),
        };
        self.panel = Rect::from_xywh(x, y, width, height);
        self.handle = (handle_height > 0.0).then(|| {
            Rect::from_xywh(
                self.panel.center().x - handle.min_width / 2.0,
                y + handle.padding.block_start,
                handle.min_width,
                handle.min_height,
            )
        });
        let inner_x = x + header.padding.inline_start;
        self.header = Rect::from_xywh(
            inner_x,
            y + handle_height + header.padding.block_start,
            inner,
            text,
        );
        // The footer sits at the bottom of the panel.
        let footer_top = self.panel.max_y() - footer_height + footer.padding.block_start;
        let footer_rect = Rect::from_xywh(
            x + footer.padding.inline_start,
            footer_top,
            width - footer.padding.horizontal(),
            buttons,
        );
        self.buttons.place_column(cx, footer_rect, footer.gap);
    }

    fn paint_panel(&self, cx: &mut PaintContext<'_>, panel: &ResolvedStyle, focus_visible: bool) {
        cx.slot(self.panel, panel, |_, _| {});
        if let Some(handle) = self.handle {
            let style = cx.style(self.query(Slot::Handle));
            let bar = ResolvedStyle {
                padding: Edges::ZERO,
                ..style
            };
            cx.slot(handle, &bar, |_, _| {});
        }
        let align = match self.side() {
            Some(SheetSide::Top | SheetSide::Bottom) if matches!(self.form, Form::Drawer(_)) => {
                TextAlign::Center
            }
            _ => TextAlign::Start,
        };
        self.text
            .paint(cx, self.query(Slot::Root), self.header, align);
        let focused = Some(self.focus);
        self.buttons.paint(cx, focused, &self.parts, focus_visible);
        if self.has_close() {
            let part = self.buttons.buttons.len();
            let state = self.parts.state(part, cx.state).set(
                VisualState::FOCUS_VISIBLE,
                focus_visible && self.focus == part,
            );
            let style = cx.style(self.query(Slot::Close).state(state));
            let button = ResolvedStyle {
                padding: Edges::ZERO,
                ..style
            };
            cx.slot(self.close, &button, |cx, inner| {
                cx.icon(inner, Icon::X, &button);
            });
        }
    }
}

impl CustomControl for Dialog {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = cx.style(trigger_query(VisualState::NONE));
        let width = cx.text.width(&self.trigger, &style.text) + style.padding.horizontal();
        Measured {
            size: Size::new(width.max(style.min_width), style.min_height),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, _bounds: Rect) {
        self.viewport = cx.viewport;
        let panel = cx.style(self.query(Slot::Root));
        self.motion_ms = panel.motion_ms;
        match self.side() {
            None => self.arrange_dialog(cx, &panel),
            Some(side) => self.arrange_edge(cx, &panel, side),
        }
        // The close button sits at the top-end corner, inset by its padding.
        let close = cx.style(self.query(Slot::Close));
        let size = close.min_width.max(close.min_height);
        let x = if cx.rtl() {
            self.panel.origin.x + close.padding.inline_end
        } else {
            self.panel.max_x() - close.padding.inline_end - size
        };
        self.close = Rect::from_xywh(
            x,
            self.panel.origin.y + close.padding.block_start,
            size,
            size,
        );
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        if self.reveal.open {
            return match event {
                ControlEvent::Pointer(pointer) => self.open_pointer(cx, pointer),
                ControlEvent::Hover(false) | ControlEvent::FocusChanged(false) => {
                    self.parts.track(cx, event, |_| None);
                    EventReply::Continue
                }
                ControlEvent::Key(KeyEvent::Down { key, repeat: false }) => self.open_key(cx, *key),
                ControlEvent::Key(_) => EventReply::Stop,
                _ => EventReply::Continue,
            };
        }
        let outcome = self.trigger_press.event(cx, event);
        if let PressOutcome::Activated(_) = outcome {
            self.change_open(cx, true);
        }
        hosted::reply(outcome)
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx
            .state
            .set(VisualState::PRESSED, self.trigger_press.pressed)
            .set(VisualState::OPEN, self.reveal.open);
        let style = cx.style(trigger_query(state));
        let label = &self.trigger;
        cx.slot(cx.bounds, &style, |cx, inner| {
            cx.label(inner, label, &style, TextAlign::Center);
        });
    }

    fn overlay(&self) -> Option<Rect> {
        self.reveal.open.then_some(self.viewport)
    }

    fn modal(&self) -> bool {
        self.reveal.open
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        let progress = self.reveal.progress(cx.tokens, self.motion_ms, cx.time);
        if progress <= 0.0 {
            return;
        }
        let focus_visible = cx.state.focus_visible();
        let scrim = cx.style(self.query(Slot::Overlay));
        let _ = cx.list.push_opacity(progress.clamp(0.0, 1.0));
        cx.slot(self.viewport, &scrim, |_, _| {});
        let _ = cx.list.pop_opacity();
        let panel = cx.style(self.query(Slot::Root).state(VisualState::OPEN));
        if self.side().is_none() {
            Reveal::paint(cx, self.panel, progress, (0.0, 0.0), |cx| {
                self.paint_panel(cx, &panel, focus_visible);
            });
            return;
        }
        // An edge panel slides in from its edge and follows a drag.
        let (ux, uy) = self.dismiss_direction();
        let drag = self.drag.map_or(0.0, |drag| drag.offset);
        let distance = (1.0 - progress) * self.extent() + drag;
        let _ = cx
            .list
            .push_transform(Transform::translate(ux * distance, uy * distance));
        self.paint_panel(cx, &panel, focus_visible);
        let _ = cx.list.pop_transform();
    }

    fn animating(&self, time: f64) -> bool {
        self.reveal.animating(self.motion_ms, time)
    }

    fn semantics(&self) -> SemanticNode {
        if !self.reveal.open {
            let actions = if self.enabled {
                vec![SemanticAction::Activate, SemanticAction::Focus]
            } else {
                Vec::new()
            };
            let mut node = hosted::node(SemanticRole::Button, &self.trigger, actions);
            node.state.expanded = Some(false);
            node.state.disabled = !self.enabled;
            return node;
        }
        let mut actions = vec![SemanticAction::Dismiss, SemanticAction::Focus];
        if self.parts() > 0 {
            actions.insert(0, SemanticAction::Activate);
        }
        let mut node = hosted::node(SemanticRole::Dialog, &self.text.title, actions);
        node.description = self.text.description.clone();
        node.value = self.part_label(self.focus).map(str::to_string);
        node.state.expanded = Some(true);
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Activate if self.reveal.open => {
                self.activate(cx, self.focus);
                true
            }
            SemanticAction::Activate => {
                self.change_open(cx, true);
                true
            }
            SemanticAction::Dismiss => {
                self.dismiss(cx);
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
            self.drag = None;
            self.parts.clear();
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

fn trigger_query(state: VisualState) -> StyleQuery {
    StyleQuery::new(ControlKind::Button, Slot::Root)
        .variant(ButtonVariant::Outline)
        .size(ControlSize::Default)
        .state(state)
}
