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
    DesignTokens, EventPhase, EventReply, Icon, Key, KeyEvent, Localizer, Measured, Point,
    PointerButton, PointerEvent, Rect, SemanticAction, SemanticNode, SemanticParts, SemanticRole,
    Size, Transform, WheelUnit,
};

/// The share of a drawer's size that a drag must pass to dismiss it.
const DRAG_DISMISS: f64 = 0.25;

/// The speed toward the edge, in points per second, at which a release
/// flicks a drawer closed after a shorter drag.
const FLICK_SPEED: f64 = 600.0;

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
    /// The host time of the last move.
    time: f64,
    /// The speed toward the edge at the last move, in points per second.
    speed: f64,
}

/// A drawer that springs back after a short drag: from `offset` points
/// at host `time`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Spring {
    offset: f64,
    time: f64,
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
    /// The visible part of the title and the description. A taller text
    /// scrolls in it.
    body: Rect,
    text_height: f64,
    scroll: f64,
    /// The description line height: one wheel line.
    line: f64,
    motion_ms: u32,
    /// The focused part while open: a button index, or the button count
    /// for the close button.
    focus: usize,
    /// The part under a pointer press.
    /// Hover and press of the buttons and the close button.
    parts: Parts,
    trigger_press: Press,
    drag: Option<Drag>,
    /// The drag offset where the close motion starts.
    closing_from: f64,
    spring: Option<Spring>,
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
            body: Rect::default(),
            text_height: 0.0,
            scroll: 0.0,
            line: 0.0,
            motion_ms: 0,
            focus: 0,
            parts: Parts::default(),
            trigger_press: Press::default(),
            drag: None,
            closing_from: 0.0,
            spring: None,
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

    /// The default button: the last button with the `Default` look.
    fn default_part(&self) -> Option<usize> {
        self.buttons
            .buttons
            .iter()
            .rposition(|button| button.variant == ButtonVariant::Default)
    }

    /// Opens or closes the dialog at `time` and ends a press or drag.
    /// Focus starts on the default button. Returns true when it changed.
    /// A close during a drag starts its motion at the dragged offset.
    fn apply_open(&mut self, open: bool, time: f64) -> bool {
        let dragged = self.drag.take().map_or(0.0, |drag| drag.offset);
        self.spring = None;
        self.parts.clear();
        if self.reveal.open == open {
            return false;
        }
        self.reveal.set(open, time);
        self.closing_from = if open { 0.0 } else { dragged };
        self.scroll = 0.0;
        self.focus = self.default_part().unwrap_or(0);
        true
    }

    fn change_open(&mut self, cx: &mut ControlContext<'_>, open: bool) {
        if self.drag.is_some() {
            cx.capture_pointer(false);
        }
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

    /// How far a drawer that springs back still is from its place at
    /// `time`.
    fn spring_offset(&self, tokens: &DesignTokens, time: f64) -> f64 {
        let Some(spring) = self.spring else {
            return 0.0;
        };
        let back = Reveal {
            open: true,
            changed_at: spring.time,
        };
        spring.offset * (1.0 - back.progress(tokens, self.motion_ms, time))
    }

    /// How far an edge panel is from its open place at `time`: the open
    /// and close motion, a drag, and a spring back. A close after a drag
    /// starts at the dragged offset.
    fn edge_distance(&self, tokens: &DesignTokens, time: f64) -> f64 {
        let progress = self.reveal.progress(tokens, self.motion_ms, time);
        let extent = self.extent();
        let from = self.closing_from.min(extent);
        let drag = self.drag.map_or(0.0, |drag| drag.offset);
        from + (1.0 - progress) * (extent - from) + drag + self.spring_offset(tokens, time)
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
            Key::Character('.') if cx.modifiers().command => self.dismiss(cx),
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
            // Return runs the default button, never a destructive one.
            Key::Enter => {
                if let Some(part) = self.default_part() {
                    self.activate(cx, part);
                }
            }
            Key::Space if parts > 0 => self.activate(cx, self.focus),
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
                    // A press during the spring back grabs the panel where
                    // it is.
                    let offset = self.spring.map_or(0.0, |spring| {
                        let linear = if self.motion_ms == 0 {
                            1.0
                        } else {
                            (cx.time() - spring.time) * 1000.0 / f64::from(self.motion_ms)
                        };
                        spring.offset * (1.0 - linear.clamp(0.0, 1.0))
                    });
                    let (ux, uy) = self.dismiss_direction();
                    self.spring = None;
                    self.drag = Some(Drag {
                        start: Point::new(position.x - ux * offset, position.y - uy * offset),
                        offset,
                        time: cx.time(),
                        speed: 0.0,
                    });
                    cx.capture_pointer(true);
                }
                cx.request_paint();
            }
            PointerEvent::Move { position } => {
                let (ux, uy) = self.dismiss_direction();
                let time = cx.time();
                if let Some(drag) = &mut self.drag {
                    let (dx, dy) = (position.x - drag.start.x, position.y - drag.start.y);
                    let offset = (dx * ux + dy * uy).max(0.0);
                    if time > drag.time {
                        drag.speed = (offset - drag.offset) / (time - drag.time);
                        drag.time = time;
                    }
                    drag.offset = offset;
                    cx.request_paint();
                }
            }
            PointerEvent::Wheel {
                position,
                delta,
                unit,
            } if self.panel.contains(position) => {
                let step = match unit {
                    WheelUnit::Lines => delta.y * self.line,
                    _ => delta.y,
                };
                let limit = (self.text_height - self.body.size.height).max(0.0);
                let scroll = (self.scroll + step).clamp(0.0, limit);
                if scroll != self.scroll {
                    self.scroll = scroll;
                    cx.request_paint();
                }
            }
            PointerEvent::Up { position, .. } => {
                cx.capture_pointer(false);
                if let Some(drag) = self.drag {
                    let flick = drag.speed >= FLICK_SPEED && drag.offset > 0.0;
                    if drag.offset > self.extent() * DRAG_DISMISS || flick {
                        self.dismiss(cx);
                    } else {
                        // A short drag springs back.
                        self.drag = None;
                        self.spring = (drag.offset > 0.0).then_some(Spring {
                            offset: drag.offset,
                            time: cx.time(),
                        });
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

    /// The room that the title keeps free for the close button, past
    /// `padding` at the end of the panel: the button and its inset on
    /// both sides.
    fn close_room(&self, cx: &MeasureContext<'_>, padding: f64) -> f64 {
        if !self.has_close() {
            return 0.0;
        }
        let close = cx.style(self.query(Slot::Close));
        let size = close.min_width.max(close.min_height);
        (size + 2.0 * close.padding.inline_end - padding).max(0.0)
    }

    fn arrange_dialog(&mut self, cx: &MeasureContext<'_>, panel: &ResolvedStyle) {
        let viewport = self.viewport;
        let footer = cx.style(self.query(Slot::Footer));
        let width = panel
            .max_width
            .min(viewport.size.width - 2.0 * cx.tokens.spacing * 4.0);
        let inner = width - panel.padding.horizontal();
        self.text.title_end = self.close_room(cx, panel.padding.inline_end);
        let text = self.text.height(cx, self.query(Slot::Root), inner);
        let buttons = self.buttons.height(cx, false, footer.gap);
        let gap = if buttons > 0.0 { panel.gap } else { 0.0 };
        // The panel stays inside the window, and a taller text scrolls.
        let room = viewport.size.height - 2.0 * cx.tokens.spacing * 4.0;
        let chrome = panel.padding.vertical() + gap + buttons;
        let body = text.min((room - chrome).max(0.0));
        let height = chrome + body;
        let x = viewport.center().x - width / 2.0;
        let y = viewport.center().y - height / 2.0;
        self.panel = Rect::from_xywh(x, y, width, height);
        let inner_x = x + panel.padding.inline_start;
        self.header = Rect::from_xywh(inner_x, y + panel.padding.block_start, inner, text);
        self.set_body(
            Rect::from_xywh(inner_x, self.header.origin.y, inner, body),
            text,
        );
        let footer_rect = Rect::from_xywh(inner_x, self.body.max_y() + gap, inner, buttons);
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
        self.text.title_end = self.close_room(cx, header.padding.inline_end);
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
        // Under the 80% cap, or in a short window, a taller text scrolls
        // above the footer.
        let bottom = self.panel.max_y() - footer_height - header.padding.block_end;
        let body = text.min((bottom - self.header.origin.y).max(0.0));
        self.set_body(
            Rect::from_xywh(inner_x, self.header.origin.y, inner, body),
            text,
        );
    }

    /// Records the visible text area and keeps the scroll inside it.
    fn set_body(&mut self, body: Rect, text_height: f64) {
        self.body = body;
        self.text_height = text_height;
        self.scroll = self
            .scroll
            .clamp(0.0, (text_height - body.size.height).max(0.0));
    }

    fn paint_panel(&self, cx: &mut PaintContext<'_>, panel: &ResolvedStyle, focus_visible: bool) {
        // The recipe puts the border of a sheet on the side that faces
        // the window, by the physical edge.
        cx.edge_panel(self.panel, panel, false);
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
        // The text clips to the body and moves by the scroll.
        cx.list.push_clip(self.body);
        let header = self.header.translate(0.0, -self.scroll);
        self.text.paint(cx, self.query(Slot::Root), header, align);
        let _ = cx.list.pop_clip();
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
        self.line = cx.style(self.query(Slot::Description)).text.line_height;
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
        let panel = cx.style(self.query(Slot::Root).state(VisualState::OPEN));
        if self.side().is_none() {
            paint_scrim(cx, self.viewport, &scrim, progress);
            Reveal::paint(cx, self.panel, progress, (0.0, 0.0), |cx| {
                self.paint_panel(cx, &panel, focus_visible);
            });
            return;
        }
        // An edge panel slides in from its edge and follows a drag. The
        // scrim fades with the share of the panel that shows.
        let (ux, uy) = self.dismiss_direction();
        let distance = self.edge_distance(cx.tokens, cx.time);
        let shown = 1.0 - distance / self.extent().max(1.0);
        paint_scrim(cx, self.viewport, &scrim, shown);
        let _ = cx
            .list
            .push_transform(Transform::translate(ux * distance, uy * distance));
        self.paint_panel(cx, &panel, focus_visible);
        let _ = cx.list.pop_transform();
    }

    fn animating(&self, time: f64) -> bool {
        let spring = self
            .spring
            .is_some_and(|spring| (time - spring.time) * 1000.0 < f64::from(self.motion_ms));
        self.reveal.animating(self.motion_ms, time) || spring
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
        node.state.modal = true;
        node
    }

    /// While open, the buttons and the close button are parts, and the
    /// focused one has the focus.
    fn semantic_parts(&self, _strings: &Localizer, parts: &mut SemanticParts) {
        if !self.reveal.open {
            return;
        }
        self.buttons
            .semantic_parts(Some(self.focus), true, None, parts);
        if let Some(label) = self.close_label.as_deref().filter(|_| self.has_close()) {
            hosted::pressable(parts, None, SemanticRole::Button, label, self.close, true);
            if self.focus == self.buttons.buttons.len() {
                parts.set_focused(Some(parts.len() - 1));
            }
        }
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
            self.spring = None;
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

/// Paints the scrim over `viewport` at `opacity`.
fn paint_scrim(cx: &mut PaintContext<'_>, viewport: Rect, scrim: &ResolvedStyle, opacity: f64) {
    let _ = cx.list.push_opacity(opacity.clamp(0.0, 1.0));
    cx.slot(viewport, scrim, |_, _| {});
    let _ = cx.list.pop_opacity();
}

fn trigger_query(state: VisualState) -> StyleQuery {
    StyleQuery::new(ControlKind::Button, Slot::Root)
        .variant(ButtonVariant::Outline)
        .size(ControlSize::Default)
        .state(state)
}
