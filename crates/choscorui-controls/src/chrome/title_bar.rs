//! The title bar of a window with client-side decorations.

use super::{TitleBarButton, TitleBarHandle, TitleBarRequest};
use crate::custom::{ControlContext, ControlEvent, ControlMessage, CustomControl, MeasureContext};
use crate::hosted::{self, Parts};
use crate::style::{
    ControlKind, PaintContext, ResolvedStyle, Slot, StyleQuery, Styles, TextAlign, VisualState,
};
use crate::{DRAG_THRESHOLD, Shortcut};
use choscorui_core::{
    DesignTokens, EventPhase, EventReply, Icon, Key, KeyEvent, Localizer, Measured, Point,
    PointerButton, PointerEvent, Rect, SemanticAction, SemanticNode, SemanticParts, SemanticRole,
    Size, SvgDocument,
};
use std::sync::OnceLock;

/// The title bar that a window draws without server-side decorations: the
/// title, and the minimize, maximize or restore, and close buttons. It
/// makes [`TitleBarRequest`]s, which the backend reads with
/// [`TitleBarHandle::take_requests`].
///
/// - A press and a drag on the title area request `Move`. A double click
///   requests `ToggleMaximize`. A secondary press requests
///   `ShowWindowMenu` at the pointer.
/// - A button requests its action when a press and a release land on it.
/// - Each button is a tab stop. Enter, or Space on its release,
///   activates the focused button. Shift+F10 or the Menu key requests
///   the window menu.
/// - Each request also reports `Activated`, so that a host that reads
///   messages knows when to read the queue.
/// - Right-to-left layouts mirror the order of the buttons.
pub struct TitleBar {
    handle: TitleBarHandle,
    buttons: Vec<TitleBarButton>,
    /// The button rectangles, in logical order.
    rects: Vec<(TitleBarButton, Rect)>,
    title: Rect,
    /// The focused button, by index into `buttons`.
    focus: usize,
    parts: Parts<TitleBarButton>,
    /// Space holds the focused button down.
    space: bool,
    /// The point of a primary press on the title area, until the pointer
    /// moves far enough to start a move.
    drag: Option<Point>,
    enabled: bool,
    rtl: bool,
}

impl TitleBar {
    /// A title bar that shows `title`, with every window button. The
    /// window shares its state through `handle`.
    pub fn new(title: impl Into<String>, handle: TitleBarHandle) -> Self {
        handle.set_title(title);
        Self {
            handle,
            buttons: TitleBarButton::ALL.to_vec(),
            rects: Vec::new(),
            title: Rect::default(),
            focus: 0,
            parts: Parts::default(),
            space: false,
            drag: None,
            enabled: true,
            rtl: false,
        }
    }

    /// The title bar with only `buttons`, for example only `Close` for a
    /// dialog window. The buttons keep their logical order.
    #[must_use]
    pub fn with_buttons(mut self, buttons: &[TitleBarButton]) -> Self {
        self.buttons = TitleBarButton::ALL
            .into_iter()
            .filter(|button| buttons.contains(button))
            .collect();
        self
    }

    /// The handle that the window shares with the title bar.
    pub fn handle(&self) -> &TitleBarHandle {
        &self.handle
    }

    /// The height of a title bar, from the `TitleBar` `Root` slot.
    pub fn height(styles: &Styles, tokens: &DesignTokens) -> f64 {
        styles
            .resolve(tokens, StyleQuery::new(ControlKind::TitleBar, Slot::Root))
            .min_height
    }

    /// The rectangle of each of `buttons` in a title bar at `bar`, in
    /// logical order. The buttons sit at the end of the bar with the close
    /// button last; right-to-left layouts mirror the row. The facade gives
    /// them to [`WindowFrame`](super::WindowFrame).
    pub fn button_rects(
        bar: Rect,
        buttons: &[TitleBarButton],
        rtl: bool,
        styles: &Styles,
        tokens: &DesignTokens,
    ) -> Vec<(TitleBarButton, Rect)> {
        let root = styles.resolve(tokens, query(Slot::Root, VisualState::NONE));
        let action = styles.resolve(tokens, query(Slot::Action, VisualState::NONE));
        let (width, height) = (action.min_width, action.min_height);
        let y = bar.origin.y + (bar.size.height - height) / 2.0;
        let count = buttons.len() as f64;
        let row = count * width + (count - 1.0).max(0.0) * root.gap;
        let mut x = if rtl {
            bar.origin.x + root.padding.inline_end + row - width
        } else {
            bar.max_x() - root.padding.inline_end - row
        };
        let step = if rtl {
            -(width + root.gap)
        } else {
            width + root.gap
        };
        buttons
            .iter()
            .map(|button| {
                let rect = Rect::from_xywh(x, y, width, height);
                x += step;
                (*button, rect)
            })
            .collect()
    }

    fn maximized(&self) -> bool {
        self.handle.is_maximized()
    }

    fn button_at(&self, point: Point) -> Option<TitleBarButton> {
        self.rects
            .iter()
            .find(|(_, rect)| rect.contains(point))
            .map(|(button, _)| *button)
    }

    fn request(&self, cx: &mut ControlContext<'_>, request: TitleBarRequest) {
        self.handle.push(request);
        cx.emit(ControlMessage::Activated);
    }

    /// The point where a window menu opens from the keyboard: the start
    /// of the title area, under the bar.
    fn menu_point(&self) -> Point {
        let x = if self.rtl {
            self.title.max_x()
        } else {
            self.title.min_x()
        };
        Point::new(x, self.title.max_y())
    }

    fn pointer(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let pressed = self.parts.pressed;
        let mut parts = self.parts;
        parts.track(cx, event, |point| self.button_at(point));
        self.parts = parts;
        let ControlEvent::Pointer(pointer) = event else {
            return EventReply::Continue;
        };
        match *pointer {
            PointerEvent::Down { position, button } => {
                if self.button_at(position).is_some() {
                    return EventReply::Stop;
                }
                match button {
                    PointerButton::Primary if cx.click_count() >= 2 => {
                        self.drag = None;
                        self.request(cx, TitleBarRequest::ToggleMaximize);
                    }
                    PointerButton::Primary => {
                        self.drag = Some(position);
                        cx.capture_pointer(true);
                    }
                    PointerButton::Secondary => {
                        self.request(cx, TitleBarRequest::ShowWindowMenu(position));
                    }
                    _ => return EventReply::Continue,
                }
                EventReply::Stop
            }
            PointerEvent::Move { position } => {
                if let Some(start) = self.drag
                    && start.distance(position) > DRAG_THRESHOLD
                {
                    // The window system moves the window from here on.
                    self.drag = None;
                    cx.capture_pointer(false);
                    self.request(cx, TitleBarRequest::Move);
                }
                EventReply::Continue
            }
            PointerEvent::Up { .. } => {
                if self.drag.take().is_some() {
                    cx.capture_pointer(false);
                }
                if let Some(button) = pressed.filter(|button| self.parts.hovered == Some(*button)) {
                    self.request(cx, button.request());
                }
                EventReply::Stop
            }
            PointerEvent::Wheel { .. } => EventReply::Continue,
        }
    }

    fn key(&mut self, cx: &mut ControlContext<'_>, event: KeyEvent) -> EventReply {
        let modifiers = cx.modifiers();
        let focused = self.buttons.get(self.focus).copied();
        match event {
            KeyEvent::Down { key, repeat } => {
                let shortcut = Shortcut { key, modifiers };
                if shortcut.opens_context_menu() {
                    if !repeat {
                        let point = self.menu_point();
                        self.request(cx, TitleBarRequest::ShowWindowMenu(point));
                    }
                    return EventReply::Stop;
                }
                if modifiers.is_chord() {
                    return EventReply::Continue;
                }
                match key {
                    Key::Tab => {
                        match hosted::tab_stop(self.focus, self.buttons.len(), modifiers.shift) {
                            Some(next) => {
                                self.focus = next;
                                self.space = false;
                                cx.request_paint();
                                cx.request_semantics();
                                EventReply::Stop
                            }
                            None => EventReply::Continue,
                        }
                    }
                    Key::Enter => {
                        if let (false, Some(button)) = (repeat, focused) {
                            self.request(cx, button.request());
                        }
                        EventReply::Stop
                    }
                    Key::Space => {
                        if !repeat {
                            self.space = true;
                            cx.request_paint();
                        }
                        EventReply::Stop
                    }
                    _ => EventReply::Continue,
                }
            }
            KeyEvent::Up { key: Key::Space } if self.space => {
                self.space = false;
                cx.request_paint();
                if let Some(button) = focused {
                    self.request(cx, button.request());
                }
                EventReply::Stop
            }
            KeyEvent::Up { .. } => EventReply::Continue,
        }
    }

    fn paint_button(
        &self,
        cx: &mut PaintContext<'_>,
        index: usize,
        button: TitleBarButton,
        rect: Rect,
    ) {
        let host = cx.state;
        let focused = self.focus == index;
        let state = self.parts.state(button, host);
        let state = state
            .set(
                VisualState::PRESSED,
                state.pressed() || (focused && self.space),
            )
            .set(VisualState::FOCUSED, focused && host.focused())
            .set(VisualState::FOCUS_VISIBLE, focused && host.focus_visible());
        let slot = if button == TitleBarButton::Close {
            Slot::Close
        } else {
            Slot::Action
        };
        let style = cx.style(query(slot, state));
        let maximized = self.maximized();
        cx.slot(rect, &style, |cx, inner| match button {
            TitleBarButton::Minimize => cx.icon(inner, Icon::Minus, &style),
            TitleBarButton::Maximize if maximized => cx.icon(inner, Icon::Copy, &style),
            TitleBarButton::Maximize => draw_square(cx, inner, &style),
            TitleBarButton::Close => cx.icon(inner, Icon::X, &style),
        });
    }
}

fn query(slot: Slot, state: VisualState) -> StyleQuery {
    StyleQuery::new(ControlKind::TitleBar, slot).state(state)
}

/// Draws the maximize glyph, a rounded square in the stroke of the
/// icons, centered in `rect` at the style's icon size.
fn draw_square(cx: &mut PaintContext<'_>, rect: Rect, style: &ResolvedStyle) {
    static SQUARE: OnceLock<Option<SvgDocument>> = OnceLock::new();
    let document = SQUARE.get_or_init(|| {
        SvgDocument::parse(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" fill=\"none\" \
             stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" \
             stroke-linejoin=\"round\"><rect width=\"18\" height=\"18\" x=\"3\" y=\"3\" \
             rx=\"2\"/></svg>",
        )
        .ok()
    });
    let size = style.icon_size;
    if let Some(document) = document
        && size > 0.0
    {
        let center = rect.center();
        let dest = Rect::from_xywh(center.x - size / 2.0, center.y - size / 2.0, size, size);
        let _ = document.draw(cx.list, dest, style.foreground);
    }
}

impl CustomControl for TitleBar {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let root = cx.style(query(Slot::Root, VisualState::NONE));
        let title = cx.style(query(Slot::Title, VisualState::FOCUSED));
        let action = cx.style(query(Slot::Action, VisualState::NONE));
        let count = self.buttons.len() as f64;
        let row = count * (action.min_width + root.gap);
        let width = cx.text.width(&self.handle.title(), &title.text)
            + 2.0 * row
            + root.padding.horizontal();
        let size = Size::new(width, root.min_height);
        Measured::new(cx.constraints.constrain(size))
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.rtl = cx.rtl();
        self.rects = Self::button_rects(bounds, &self.buttons, cx.rtl(), cx.styles, cx.tokens);
        let root = cx.style(query(Slot::Root, VisualState::NONE));
        // The title centers in the bar. It keeps the width of the buttons
        // free on both sides, so that it stays centered.
        let row = self
            .rects
            .iter()
            .map(|(_, rect)| rect.size.width + root.gap)
            .sum::<f64>()
            + root.padding.inline_end;
        let width = (bounds.size.width - 2.0 * row).max(0.0);
        self.title = Rect::from_xywh(
            bounds.center().x - width / 2.0,
            bounds.origin.y,
            width,
            bounds.size.height,
        );
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target || !self.enabled {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(_) | ControlEvent::Hover(_) => self.pointer(cx, event),
            ControlEvent::Key(key) if cx.is_focused() => self.key(cx, *key),
            ControlEvent::FocusChanged(focused) => {
                if *focused {
                    self.focus = if cx.modifiers().shift {
                        self.buttons.len().saturating_sub(1)
                    } else {
                        0
                    };
                }
                self.space = false;
                let mut parts = self.parts;
                parts.track(cx, event, |_| None);
                self.parts = parts;
                cx.request_paint();
                cx.request_semantics();
                EventReply::Continue
            }
            ControlEvent::Dismiss => {
                self.space = false;
                if self.drag.take().is_some() {
                    cx.capture_pointer(false);
                }
                let mut parts = self.parts;
                parts.track(cx, event, |_| None);
                self.parts = parts;
                EventReply::Continue
            }
            ControlEvent::Timer => {
                if self.handle.take_changed() {
                    cx.request_layout();
                    cx.request_paint();
                    cx.request_semantics();
                }
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let maximized = self.maximized();
        let root = cx.style(query(
            Slot::Root,
            VisualState::NONE.set(VisualState::CHECKED, maximized),
        ));
        let bounds = cx.bounds;
        cx.slot(bounds, &root, |_, _| {});
        let active = self.handle.is_active();
        let title = cx.style(query(
            Slot::Title,
            VisualState::NONE.set(VisualState::FOCUSED, active),
        ));
        let text = self.handle.title();
        cx.label(self.title, &text, &title, TextAlign::Center);
        for (index, (button, rect)) in self.rects.iter().enumerate() {
            self.paint_button(cx, index, *button, *rect);
        }
    }

    fn semantics(&self) -> SemanticNode {
        self.semantics_in(Localizer::builtin())
    }

    fn semantics_in(&self, _strings: &Localizer) -> SemanticNode {
        let actions = if self.enabled {
            vec![SemanticAction::ShowMenu, SemanticAction::Focus]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::Toolbar, self.handle.title(), actions);
        node.state.disabled = !self.enabled;
        node
    }

    /// A `Button` part for each window button, named by the built-in
    /// messages. The focused button is the focused part.
    fn semantic_parts(&self, strings: &Localizer, parts: &mut SemanticParts) {
        let maximized = self.maximized();
        for (index, (button, rect)) in self.rects.iter().enumerate() {
            let name = strings.message(button.message_id(maximized), &[]);
            let role = SemanticRole::Button;
            hosted::pressable(parts, None, role, &name, *rect, self.enabled);
            if self.focus == index {
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
        if !self.enabled {
            return false;
        }
        match action {
            SemanticAction::ShowMenu => {
                let point = self.menu_point();
                self.request(cx, TitleBarRequest::ShowWindowMenu(point));
                true
            }
            SemanticAction::Focus => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn focusable(&self) -> bool {
        self.enabled && !self.buttons.is_empty()
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.parts.clear();
            self.drag = None;
            self.space = false;
        }
    }

    /// Shows a change of the handle at the next advance.
    fn deadline(&self) -> Option<f64> {
        self.handle.changed().then_some(f64::NEG_INFINITY)
    }
}
