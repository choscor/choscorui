//! The toast stack: timers, stacking, and painting.

use super::{Toast, ToastId, ToastOp, ToastPosition, Toasts};
use crate::custom::{ControlContext, ControlEvent, ControlMessage, CustomControl, MeasureContext};
use crate::form::measured::{paint_wrapped, wrapped_height};
use crate::hosted::{self, Parts};
use crate::overlay::panel::{Buttons, PanelButton};
use crate::style::{
    ButtonVariant, ControlKind, PaintContext, ResolvedStyle, Slot, StyleQuery, TextAlign,
    ToastVariant, VisualState,
};
use choscorui_core::{
    AnnouncementPriority, EventPhase, EventReply, Icon, Key, KeyEvent, Localizer, Measured, Point,
    PointerButton, PointerEvent, Rect, SemanticAction, SemanticNode, SemanticParts, SemanticRole,
    Size, Transform,
};

/// The most toasts that show at once while the stack is collapsed.
const VISIBLE: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Button {
    Action,
    Close,
}

/// A toast in the stack, with its timer and its geometry.
struct Entry {
    id: ToastId,
    toast: Toast,
    /// When the toast arrived, and when it started to close.
    born: f64,
    leaving: Option<f64>,
    /// Seconds left on the timer at `started`.
    remaining: Option<f64>,
    started: f64,
    height: f64,
    rect: Rect,
    action: Rect,
    close: Rect,
}

impl Entry {
    fn due(&self) -> Option<f64> {
        self.remaining.map(|remaining| self.started + remaining)
    }
}

/// A stack of toasts at a corner of the window. The app pushes toasts
/// through a [`Toasts`] handle. The newest toast is at the front. While
/// the pointer rests on the stack, or the stack has focus, the stack
/// expands and the timers pause. The timers also pause while the window
/// is hidden.
///
/// The action and the close buttons are tab stops. An action emits
/// `ItemActivated(id)` and closes its toast. A close button, Escape on a
/// focused toast, or the end of a toast's time emits `ItemDismissed(id)`.
pub struct Toaster {
    name: String,
    toasts: Toasts,
    close_label: String,
    position: ToastPosition,
    always_expanded: bool,
    /// Oldest first; the last entry is at the front.
    entries: Vec<Entry>,
    hovered: bool,
    focused: bool,
    /// The window is fully hidden, so nobody sees the toasts.
    hidden: bool,
    /// The focused tab stop, and the pressed button.
    focus: usize,
    parts: Parts<(ToastId, Button)>,
    viewport: Rect,
    /// The smallest close button hit target, from the spacing.
    target: f64,
    motion_ms: u32,
    turn_ms: u32,
    rtl: bool,
}

impl Toaster {
    /// A toaster that shows the toasts of `toasts`. `close_label` names
    /// the close buttons.
    pub fn new(name: impl Into<String>, toasts: Toasts, close_label: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            toasts,
            close_label: close_label.into(),
            position: ToastPosition::default(),
            always_expanded: false,
            entries: Vec::new(),
            hovered: false,
            focused: false,
            hidden: false,
            focus: 0,
            parts: Parts::default(),
            viewport: Rect::default(),
            target: 0.0,
            motion_ms: 0,
            turn_ms: 0,
            rtl: false,
        }
    }

    #[must_use]
    pub fn with_position(mut self, position: ToastPosition) -> Self {
        self.position = position;
        self
    }

    /// Lists every toast at all times instead of stacking them.
    #[must_use]
    pub fn expanded(mut self) -> Self {
        self.always_expanded = true;
        self
    }

    fn query(slot: Slot) -> StyleQuery {
        StyleQuery::new(ControlKind::Toast, slot)
    }

    fn is_expanded(&self) -> bool {
        self.always_expanded || self.hovered || self.focused
    }

    fn paused(&self) -> bool {
        self.hovered || self.focused || self.hidden
    }

    /// The entries that show, front first.
    fn shown(&self) -> Vec<usize> {
        let limit = if self.is_expanded() {
            self.entries.len()
        } else {
            VISIBLE
        };
        (0..self.entries.len()).rev().take(limit).collect()
    }

    /// The tab stops in visual order, top first.
    fn stops(&self) -> Vec<(usize, Button)> {
        let mut entries = self.shown();
        if !self.position.top() {
            entries.reverse();
        }
        entries
            .into_iter()
            .filter(|index| self.entries[*index].leaving.is_none())
            .flat_map(|index| {
                let action = self.entries[index].toast.action.is_some();
                action
                    .then_some((index, Button::Action))
                    .into_iter()
                    .chain(std::iter::once((index, Button::Close)))
            })
            .collect()
    }

    fn pause(&mut self, time: f64) {
        for entry in &mut self.entries {
            if let Some(remaining) = entry.remaining.as_mut() {
                *remaining = (*remaining - (time - entry.started)).max(0.0);
            }
            entry.started = time;
        }
    }

    fn resume(&mut self, time: f64) {
        for entry in &mut self.entries {
            entry.started = time;
        }
    }

    /// Hover, focus, or window visibility changed from `was` paused.
    fn repause(&mut self, cx: &mut ControlContext<'_>, was: bool) {
        let now = self.paused();
        if was && !now {
            self.resume(cx.time());
        } else if !was && now {
            self.pause(cx.time());
        }
        cx.request_layout();
        cx.request_paint();
    }

    fn close(&mut self, index: usize, time: f64) {
        let entry = &mut self.entries[index];
        if entry.leaving.is_none() {
            entry.leaving = Some(time);
            entry.remaining = None;
        }
    }

    fn apply(&mut self, op: ToastOp, time: f64) {
        match op {
            ToastOp::Push(id, toast) => self.entries.push(Entry {
                id,
                remaining: toast.duration,
                toast,
                born: time,
                leaving: None,
                started: time,
                height: 0.0,
                rect: Rect::default(),
                action: Rect::default(),
                close: Rect::default(),
            }),
            ToastOp::Update(id, toast) => {
                if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
                    entry.remaining = toast.duration;
                    entry.started = time;
                    entry.toast = toast;
                }
            }
            ToastOp::Dismiss(id) => {
                if let Some(index) = self.entries.iter().position(|entry| entry.id == id) {
                    self.close(index, time);
                }
            }
        }
    }

    fn motion(&self) -> f64 {
        f64::from(self.motion_ms) / 1000.0
    }

    /// Applies queued changes, closes toasts whose time ran out, and drops
    /// toasts that finished closing.
    fn update(&mut self, cx: &mut ControlContext<'_>) {
        let time = cx.time();
        for op in self.toasts.take() {
            // A new or changed toast is spoken; an error interrupts.
            if let ToastOp::Push(_, toast) | ToastOp::Update(_, toast) = &op {
                let text = [toast.title.as_str(), toast.description.as_str()]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                let priority = if toast.variant == ToastVariant::Error {
                    AnnouncementPriority::High
                } else {
                    AnnouncementPriority::Medium
                };
                cx.announce(text, priority);
            }
            self.apply(op, time);
        }
        if !self.paused() {
            for index in 0..self.entries.len() {
                if self.entries[index].due().is_some_and(|due| due <= time) {
                    cx.emit(ControlMessage::ItemDismissed(
                        self.entries[index].id.0 as usize,
                    ));
                    self.close(index, time);
                }
            }
        }
        let motion = self.motion();
        self.entries
            .retain(|entry| entry.leaving.is_none_or(|leaving| leaving + motion > time));
        if self.entries.is_empty() {
            self.hovered = false;
        }
        self.focus = self.focus.min(self.stops().len().saturating_sub(1));
        let node = cx.node();
        let _ = cx.tree_mut().set_focusable(node, !self.entries.is_empty());
        cx.request_layout();
        cx.request_paint();
        cx.request_semantics();
    }

    fn activate(&mut self, cx: &mut ControlContext<'_>, index: usize, button: Button) {
        let id = self.entries[index].id;
        match button {
            Button::Action => cx.emit(ControlMessage::ItemActivated(id.0 as usize)),
            Button::Close => cx.emit(ControlMessage::ItemDismissed(id.0 as usize)),
        }
        self.close(index, cx.time());
        cx.request_paint();
        cx.request_semantics();
    }

    fn button_at(&self, point: Point) -> Option<(usize, Button)> {
        self.stops().into_iter().find(|(index, button)| {
            let entry = &self.entries[*index];
            match button {
                Button::Action => entry.action.contains(point),
                Button::Close => self.close_target(entry.close).contains(point),
            }
        })
    }

    /// The hit target of the close button `rect`: at least `target` on
    /// each side, around the icon.
    fn close_target(&self, rect: Rect) -> Rect {
        let grow = |size: f64| ((self.target - size) / 2.0).max(0.0);
        Rect::from_xywh(
            rect.origin.x - grow(rect.size.width),
            rect.origin.y - grow(rect.size.height),
            rect.size.width.max(self.target),
            rect.size.height.max(self.target),
        )
    }

    fn stack_rect(&self) -> Option<Rect> {
        self.shown()
            .into_iter()
            .map(|index| self.entries[index].rect)
            .reduce(|a, b| a.union(b))
    }

    /// The button under `point`, by toast id.
    fn part_at(&self, point: Point) -> Option<(ToastId, Button)> {
        self.button_at(point)
            .map(|(index, button)| (self.entries[index].id, button))
    }

    fn pointer(&mut self, cx: &mut ControlContext<'_>, event: &PointerEvent) -> EventReply {
        let pressed = self.parts.pressed;
        let mut parts = self.parts;
        parts.track(cx, &ControlEvent::Pointer(*event), |point| {
            self.part_at(point)
        });
        self.parts = parts;
        match event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } => match self.button_at(*position) {
                Some((index, button)) => {
                    if let Some(stop) = self
                        .stops()
                        .iter()
                        .position(|stop| *stop == (index, button))
                    {
                        self.focus = stop;
                    }
                    EventReply::Stop
                }
                None => EventReply::Continue,
            },
            PointerEvent::Up { position, .. } if pressed.is_some() => {
                if let Some((index, button)) = self.button_at(*position)
                    && pressed == Some((self.entries[index].id, button))
                {
                    self.activate(cx, index, button);
                }
                EventReply::Stop
            }
            PointerEvent::Move { position } => {
                let over = self
                    .stack_rect()
                    .is_some_and(|rect| rect.contains(*position));
                if over != self.hovered {
                    let was = self.paused();
                    self.hovered = over;
                    self.repause(cx, was);
                }
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn key(&mut self, cx: &mut ControlContext<'_>, key: Key, repeat: bool) -> EventReply {
        let stops = self.stops();
        match key {
            Key::Tab => match hosted::tab_stop(self.focus, stops.len(), cx.modifiers().shift) {
                Some(next) => {
                    self.focus = next;
                    cx.request_paint();
                    cx.request_semantics();
                    EventReply::Stop
                }
                None => EventReply::Continue,
            },
            Key::Enter | Key::Space if !repeat => {
                if let Some((index, button)) = stops.get(self.focus).copied() {
                    self.activate(cx, index, button);
                }
                EventReply::Stop
            }
            // Escape closes the toast of the focused button.
            Key::Escape => match stops.get(self.focus).copied() {
                Some((index, _)) => {
                    self.activate(cx, index, Button::Close);
                    EventReply::Stop
                }
                None => EventReply::Continue,
            },
            _ => EventReply::Continue,
        }
    }

    fn icon(variant: ToastVariant) -> Option<Icon> {
        match variant {
            ToastVariant::Default => None,
            ToastVariant::Success => Some(Icon::CircleCheck),
            ToastVariant::Info => Some(Icon::Info),
            ToastVariant::Warning => Some(Icon::TriangleAlert),
            ToastVariant::Error => Some(Icon::OctagonX),
            ToastVariant::Loading => Some(Icon::LoaderCircle),
        }
    }

    /// The text width of a toast `width` wide.
    fn text_width(cx: &MeasureContext<'_>, toast: &Toast, width: f64) -> f64 {
        let root = cx.style(Self::query(Slot::Root));
        let icon = cx.style(Self::query(Slot::Icon));
        let close = cx.style(Self::query(Slot::Close));
        let mut text = width - root.padding.horizontal() - close.icon_size - root.gap;
        if Self::icon(toast.variant).is_some() {
            text -= icon.icon_size + root.gap;
        }
        if let Some(label) = &toast.action {
            text -= Self::action_width(cx, label) + root.gap;
        }
        text.max(1.0)
    }

    fn action_width(cx: &MeasureContext<'_>, label: &str) -> f64 {
        Buttons::width(
            cx,
            &PanelButton {
                label: label.to_string(),
                variant: ButtonVariant::Default,
            },
        )
    }

    /// The space between the title and the description.
    fn line_gap(spacing: f64) -> f64 {
        spacing * 0.5
    }

    fn text_height(cx: &MeasureContext<'_>, toast: &Toast, width: f64) -> f64 {
        let title = cx.style(Self::query(Slot::Title));
        let description = cx.style(Self::query(Slot::Description));
        let title_height = wrapped_height(&toast.title, &title.text, width, cx.text);
        let description_height =
            wrapped_height(&toast.description, &description.text, width, cx.text);
        let gap = if title_height > 0.0 && description_height > 0.0 {
            Self::line_gap(cx.tokens.spacing)
        } else {
            0.0
        };
        title_height + gap + description_height
    }

    /// Places the toasts: the front one at the window edge, the others
    /// behind it, inward.
    fn place(&mut self, cx: &MeasureContext<'_>) {
        let root = cx.style(Self::query(Slot::Root));
        let group = cx.style(Self::query(Slot::Group));
        let close = cx.style(Self::query(Slot::Close));
        let viewport = self.viewport;
        let width = root
            .max_width
            .min(viewport.size.width - group.padding.horizontal())
            .max(0.0);
        let start = viewport.origin.x + group.padding.inline_start;
        let end = viewport.max_x() - group.padding.inline_end - width;
        let x = match (self.position, self.rtl) {
            (ToastPosition::TopCenter | ToastPosition::BottomCenter, _) => {
                viewport.center().x - width / 2.0
            }
            (ToastPosition::TopStart | ToastPosition::BottomStart, false)
            | (ToastPosition::TopEnd | ToastPosition::BottomEnd, true) => start,
            _ => end,
        };
        let top = self.position.top();
        let edge = if top {
            viewport.origin.y + group.padding.block_start
        } else {
            viewport.max_y() - group.padding.block_end
        };
        let button = Buttons::button_height(cx);
        for entry in &mut self.entries {
            let text = Self::text_width(cx, &entry.toast, width);
            let mut height = Self::text_height(cx, &entry.toast, text);
            if entry.toast.action.is_some() {
                height = height.max(button);
            }
            entry.height = height + root.padding.vertical();
        }
        let expanded = self.is_expanded();
        let front = self.entries.last().map_or(0.0, |entry| entry.height);
        let mut offset = 0.0;
        for (depth, index) in self.shown().into_iter().enumerate() {
            let entry = &mut self.entries[index];
            // Expanded toasts sit one after another; collapsed toasts
            // behind the front one take its height and peek out by the
            // gap.
            let height = if expanded { entry.height } else { front };
            if !expanded {
                offset = group.gap * depth as f64;
            }
            let y = if top {
                edge + offset
            } else {
                edge - offset - height
            };
            entry.rect = Rect::from_xywh(x, y, width, height);
            if expanded {
                offset += height + group.gap;
            }
            let inner_top = y + root.padding.block_start;
            let close_x = if self.rtl {
                x + root.padding.inline_end
            } else {
                x + width - root.padding.inline_end - close.icon_size
            };
            entry.close = Rect::from_xywh(close_x, inner_top, close.icon_size, close.icon_size);
            entry.action = match &entry.toast.action {
                Some(label) => {
                    let action = Self::action_width(cx, label);
                    let action_x = if self.rtl {
                        close_x + close.icon_size + root.gap
                    } else {
                        close_x - root.gap - action
                    };
                    Rect::from_xywh(
                        action_x,
                        entry.rect.center().y - button / 2.0,
                        action,
                        button,
                    )
                }
                None => Rect::default(),
            };
        }
    }

    /// The enter and the exit progress of `entry` at `time`: 0 outside
    /// the window, 1 in place.
    fn presence(&self, entry: &Entry, time: f64) -> f64 {
        let motion = self.motion();
        if motion <= 0.0 {
            // Without motion, a closing toast is gone at once.
            return if entry.leaving.is_some() { 0.0 } else { 1.0 };
        }
        let enter = ((time - entry.born) / motion).clamp(0.0, 1.0);
        let exit = entry.leaving.map_or(1.0, |leaving| {
            1.0 - ((time - leaving) / motion).clamp(0.0, 1.0)
        });
        enter.min(exit)
    }

    fn paint_entry(
        &self,
        cx: &mut PaintContext<'_>,
        index: usize,
        depth: usize,
        stops: &[(usize, Button)],
    ) {
        let entry = &self.entries[index];
        let root = cx.style(Self::query(Slot::Root));
        let presence = cx.tokens.motion.easing.solve(self.presence(entry, cx.time));
        // Entering and leaving toasts slide from the window edge.
        let slide = (1.0 - presence) * entry.height * if self.position.top() { -1.0 } else { 1.0 };
        let mut transform = Transform::translate(0.0, slide);
        let collapsed_behind = depth > 0 && !self.is_expanded();
        if collapsed_behind {
            // Each toast behind the front one is 5% smaller.
            let scale = 1.0 - 0.05 * depth as f64;
            let center = entry.rect.center();
            let anchor_y = if self.position.top() {
                entry.rect.max_y()
            } else {
                entry.rect.origin.y
            };
            transform = Transform::translate(-center.x, -anchor_y)
                .then(Transform::scale(scale, scale))
                .then(Transform::translate(center.x, anchor_y))
                .then(transform);
        }
        let pushed = cx.list.push_transform(transform).is_ok();
        let faded = cx.list.push_opacity(presence.clamp(0.0, 1.0)).is_ok();
        let focused = stops.get(self.focus).copied();
        let focus_visible = cx.state.focus_visible();
        let host = cx.state;
        cx.slot(entry.rect, &root, |cx, inner| {
            if collapsed_behind {
                return;
            }
            self.paint_content(cx, entry, inner, &root);
            for button in [Button::Action, Button::Close] {
                let rect = match button {
                    Button::Action if entry.toast.action.is_some() => entry.action,
                    Button::Close => entry.close,
                    Button::Action => continue,
                };
                let state = self.parts.state((entry.id, button), host).set(
                    VisualState::FOCUS_VISIBLE,
                    focus_visible && focused == Some((index, button)),
                );
                match button {
                    Button::Action => {
                        let style = cx.style(
                            StyleQuery::new(ControlKind::Button, Slot::Root)
                                .variant(ButtonVariant::Default)
                                .state(state),
                        );
                        let label = entry.toast.action.as_deref().unwrap_or_default();
                        cx.slot(rect, &style, |cx, inner| {
                            cx.label(inner, label, &style, TextAlign::Center);
                        });
                    }
                    Button::Close => {
                        let style = cx.style(Self::query(Slot::Close).state(state));
                        cx.slot(rect, &style, |cx, inner| cx.icon(inner, Icon::X, &style));
                    }
                }
            }
        });
        if faded {
            let _ = cx.list.pop_opacity();
        }
        if pushed {
            let _ = cx.list.pop_transform();
        }
    }

    fn paint_content(
        &self,
        cx: &mut PaintContext<'_>,
        entry: &Entry,
        inner: Rect,
        root: &ResolvedStyle,
    ) {
        let icon_style = cx.style(Self::query(Slot::Icon));
        let title = cx.style(Self::query(Slot::Title));
        let description = cx.style(Self::query(Slot::Description));
        let close = cx.style(Self::query(Slot::Close));
        let mut used = 0.0;
        if let Some(icon) = Self::icon(entry.toast.variant) {
            let size = icon_style.icon_size;
            let x = if cx.rtl {
                inner.max_x() - size
            } else {
                inner.origin.x
            };
            let rect = Rect::from_xywh(x, inner.origin.y, size, size);
            if entry.toast.variant == ToastVariant::Loading && self.turn_ms > 0 {
                let center = rect.center();
                let turn = f64::from(self.turn_ms) / 1000.0;
                let angle = (cx.time / turn).fract() * std::f64::consts::TAU;
                let rotation = Transform::translate(-center.x, -center.y)
                    .then(Transform::rotate(angle))
                    .then(Transform::translate(center.x, center.y));
                let pushed = cx.list.push_transform(rotation).is_ok();
                cx.icon(rect, icon, &icon_style);
                if pushed {
                    let _ = cx.list.pop_transform();
                }
            } else {
                cx.icon(rect, icon, &icon_style);
            }
            used = size + root.gap;
        }
        let mut trailing = close.icon_size + root.gap;
        if entry.toast.action.is_some() {
            trailing += entry.action.size.width + root.gap;
        }
        let width = (inner.size.width - used - trailing).max(1.0);
        let start = if cx.rtl { trailing } else { used };
        let text = Rect::from_xywh(
            inner.origin.x + start,
            inner.origin.y,
            width,
            inner.size.height,
        );
        let title_height =
            wrapped_height(&entry.toast.title, &title.text, text.size.width, cx.text);
        let title_rect =
            Rect::from_xywh(text.origin.x, text.origin.y, text.size.width, title_height);
        paint_wrapped(cx, title_rect, &entry.toast.title, &title, TextAlign::Start);
        let gap = if title_height > 0.0 {
            Self::line_gap(cx.tokens.spacing)
        } else {
            0.0
        };
        let description_height = wrapped_height(
            &entry.toast.description,
            &description.text,
            text.size.width,
            cx.text,
        );
        let description_rect = Rect::from_xywh(
            text.origin.x,
            title_rect.max_y() + gap,
            text.size.width,
            description_height,
        );
        paint_wrapped(
            cx,
            description_rect,
            &entry.toast.description,
            &description,
            TextAlign::Start,
        );
    }
}

impl CustomControl for Toaster {
    fn measure(&mut self, _cx: &MeasureContext<'_>) -> Measured {
        Measured {
            size: Size::new(0.0, 0.0),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, _bounds: Rect) {
        self.viewport = cx.viewport;
        self.rtl = cx.rtl();
        // Five spacing units: 20 pt at the default spacing.
        self.target = cx.tokens.spacing * 5.0;
        self.motion_ms = cx.style(Self::query(Slot::Group)).motion_ms;
        self.turn_ms = cx
            .style(StyleQuery::new(ControlKind::Spinner, Slot::Root))
            .motion_ms;
        self.place(cx);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Timer => {
                self.update(cx);
                EventReply::Stop
            }
            ControlEvent::Pointer(pointer) => self.pointer(cx, pointer),
            ControlEvent::Hover(false) => {
                self.parts.track(cx, event, |_| None);
                if self.hovered {
                    let was = self.paused();
                    self.hovered = false;
                    self.repause(cx, was);
                }
                EventReply::Continue
            }
            ControlEvent::Key(KeyEvent::Down { key, repeat }) => self.key(cx, *key, *repeat),
            ControlEvent::WindowVisible(visible) => {
                let was = self.paused();
                self.hidden = !*visible;
                self.repause(cx, was);
                EventReply::Continue
            }
            ControlEvent::FocusChanged(focused) => {
                self.parts.track(cx, event, |_| None);
                let was = self.paused();
                self.focused = *focused;
                if *focused {
                    self.focus = if cx.modifiers().shift {
                        self.stops().len().saturating_sub(1)
                    } else {
                        0
                    };
                }
                self.repause(cx, was);
                cx.request_semantics();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn deadline(&self) -> Option<f64> {
        if self.toasts.pending() {
            return Some(f64::NEG_INFINITY);
        }
        let motion = self.motion();
        self.entries
            .iter()
            .filter_map(|entry| match entry.leaving {
                Some(leaving) => Some(leaving + motion),
                None if self.paused() => None,
                None => entry.due(),
            })
            .reduce(f64::min)
    }

    fn paint(&self, _cx: &mut PaintContext<'_>) {}

    fn overlay(&self) -> Option<Rect> {
        self.stack_rect()
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        let stops = self.stops();
        // Back to front, so the front toast covers the others.
        for (depth, index) in self.shown().into_iter().enumerate().rev() {
            self.paint_entry(cx, index, depth, &stops);
        }
    }

    fn animating(&self, time: f64) -> bool {
        let motion = self.motion();
        self.entries.iter().any(|entry| {
            time - entry.born < motion
                || entry.leaving.is_some_and(|leaving| time - leaving < motion)
                || (entry.toast.variant == ToastVariant::Loading && self.turn_ms > 0)
        })
    }

    fn semantics(&self) -> SemanticNode {
        let stops = self.stops();
        let actions = if stops.is_empty() {
            Vec::new()
        } else {
            vec![SemanticAction::Activate, SemanticAction::Focus]
        };
        let mut node = hosted::node(SemanticRole::Group, &self.name, actions);
        node.value = self.entries.last().map(|entry| {
            [entry.toast.title.as_str(), entry.toast.description.as_str()]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        });
        node
    }

    /// A `Group` part for each toast in view, with its action and close
    /// buttons. The focused stop is the focused part.
    fn semantic_parts(&self, _strings: &Localizer, parts: &mut SemanticParts) {
        let mut group = None;
        for (stop, (index, button)) in self.stops().into_iter().enumerate() {
            let entry = &self.entries[index];
            if group.map(|(shown, _)| shown) != Some(index) {
                let part = parts.push(None, SemanticRole::Group, &entry.toast.title, entry.rect);
                part.node.description = entry.toast.description.clone();
                group = Some((index, parts.len() - 1));
            }
            let (label, rect) = match button {
                Button::Action => (
                    entry.toast.action.as_deref().unwrap_or_default(),
                    entry.action,
                ),
                Button::Close => (self.close_label.as_str(), self.close_target(entry.close)),
            };
            let parent = group.map(|(_, position)| position);
            hosted::pressable(parts, parent, SemanticRole::Button, label, rect, true);
            if stop == self.focus {
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
            SemanticAction::Activate => match self.stops().get(self.focus).copied() {
                Some((index, button)) => {
                    self.activate(cx, index, button);
                    true
                }
                None => false,
            },
            SemanticAction::Focus if !self.entries.is_empty() => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn focusable(&self) -> bool {
        !self.entries.is_empty()
    }
}
