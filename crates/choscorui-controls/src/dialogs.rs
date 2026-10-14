//! Custom-drawn dialog contracts: platform button ordering, modal and
//! modeless dialog stacking with focus restoration, and the color, font,
//! single-value input, and alert dialog models.
//!
//! Native dialogs (open/save/directory panels and alerts) live in the
//! macOS backend (`choscorui_macos::{FileDialogSpec, AlertSpec}`); the
//! dialogs here are drawn by the toolkit and use the same ordering and
//! keyboard rules so both kinds behave alike.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::form::measured::{paint_wrapped, wrapped_height};
use crate::hosted::{self, Parts, Reveal};
use crate::style::{
    ButtonVariant, Content, ControlKind, ControlSize, Edges, PaintContext, ResolvedStyle, Slot,
    StyleQuery, Styles, TextAlign, TextMeasure, VisualState, content_rect,
};
use crate::{
    ComboChange, ComboInput, FontChooser, InputFilter, NumberError, NumberInput, ValidationError,
    ValidationState,
};
use choscorui_core::{
    Color, ColorRole, DesignTokens, EventPhase, EventReply, FontSlant, FontWeight, Icon, Key,
    KeyEvent, Locale, Localizer, Measured, NodeId, Path, Point, PointerButton, PointerEvent, Rect,
    SemanticAction, SemanticNode, SemanticParts, SemanticRange, SemanticRole, SemanticState, Size,
    TextStyle, Tree, TreeError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogButtonRole {
    /// Confirms (Return); the default button.
    Accept,
    /// Dismisses without changes (Escape; `DialogKeys::route` also maps ⌘.).
    Reject,
    /// Confirms a destructive action (for example "Delete").
    Destructive,
    /// Leaves without saving ("Don't Save").
    Discard,
    Help,
    /// Any other action ("Apply", "Reset").
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogButton {
    pub title: String,
    pub role: DialogButtonRole,
}

impl DialogButton {
    pub fn new(title: impl Into<String>, role: DialogButtonRole) -> Self {
        Self {
            title: title.into(),
            role,
        }
    }
}

/// Orders buttons left to right following the macOS Human Interface
/// Guidelines: Help at the far left, then Discard ("Don't Save"), a
/// flexible gap, other actions, Reject (Cancel), and the default button at
/// the far right. Right-to-left layouts mirror the whole row.
/// Returns indexes into `buttons` and the position of the gap.
pub fn order_dialog_buttons(buttons: &[DialogButton], rtl: bool) -> (Vec<usize>, usize) {
    let rank = |role: DialogButtonRole| match role {
        DialogButtonRole::Help => 0,
        DialogButtonRole::Discard => 1,
        DialogButtonRole::Other => 2,
        DialogButtonRole::Reject => 3,
        DialogButtonRole::Destructive | DialogButtonRole::Accept => 4,
    };
    let mut order: Vec<usize> = (0..buttons.len()).collect();
    order.sort_by_key(|index| rank(buttons[*index].role));
    let gap = order
        .iter()
        .position(|index| rank(buttons[*index].role) >= 2)
        .unwrap_or(order.len());
    if rtl {
        order.reverse();
        (order, buttons.len() - gap)
    } else {
        (order, gap)
    }
}

/// Which button the keyboard triggers.
pub fn dialog_key_button(buttons: &[DialogButton], key: Key) -> Option<usize> {
    let role = match key {
        // A destructive button is never triggered by Return.
        Key::Enter => DialogButtonRole::Accept,
        Key::Escape => DialogButtonRole::Reject,
        _ => return None,
    };
    buttons.iter().position(|button| button.role == role)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Modality {
    /// Blocks input to the rest of the window until closed.
    Modal,
    /// Floats while the rest of the window stays usable.
    Modeless,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DialogId(pub u64);

#[derive(Clone, Copy, Debug)]
struct OpenDialog {
    id: DialogId,
    root: NodeId,
    modality: Modality,
    restore_focus: Option<NodeId>,
}

/// Open dialogs in a window. Modal dialogs confine input routing to their
/// subtree; closing a dialog restores the focus it saved.
#[derive(Default)]
pub struct DialogStack {
    open: Vec<OpenDialog>,
    next: u64,
}

impl DialogStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a dialog whose content is the subtree at `root`, moving
    /// focus to `initial` (for example the first field or default button),
    /// or to the first focusable node of `root` when `initial` is `None`.
    /// Returns `TreeError::MissingNode` when `root` is not in `tree` or
    /// `initial` is not in the subtree of `root`.
    pub fn open(
        &mut self,
        tree: &mut Tree,
        root: NodeId,
        modality: Modality,
        initial: Option<NodeId>,
    ) -> Result<DialogId, TreeError> {
        if !tree.contains(root) || initial.is_some_and(|node| !tree.is_within(node, root)) {
            return Err(TreeError::MissingNode);
        }
        let restore_focus = tree.focus();
        match initial {
            Some(initial) => tree.set_focus(Some(initial))?,
            None => {
                tree.focus_next_within(root, false);
            }
        }
        self.next += 1;
        let id = DialogId(self.next);
        self.open.push(OpenDialog {
            id,
            root,
            modality,
            restore_focus,
        });
        Ok(id)
    }

    /// Closes a dialog. Only the top dialog gives focus back, and only
    /// while the focus is still inside it. A dialog above the closed one
    /// that saved a focus inside it saves the focus of the closed dialog in
    /// its place.
    pub fn close(&mut self, tree: &mut Tree, id: DialogId) -> bool {
        let Some(index) = self.open.iter().position(|dialog| dialog.id == id) else {
            return false;
        };
        let dialog = self.open.remove(index);
        if index == self.open.len() {
            let inside = tree
                .focus()
                .is_none_or(|focus| tree.is_within(focus, dialog.root));
            if let Some(focus) = dialog
                .restore_focus
                .filter(|node| inside && tree.contains(*node))
            {
                let _ = tree.set_focus(Some(focus));
            }
        } else {
            for above in &mut self.open[index..] {
                if above
                    .restore_focus
                    .is_some_and(|node| tree.is_within(node, dialog.root))
                {
                    above.restore_focus = dialog.restore_focus;
                }
            }
        }
        true
    }

    /// The modal dialog that currently owns input, if any.
    pub fn active_modal(&self) -> Option<DialogId> {
        self.open
            .iter()
            .rev()
            .find(|dialog| dialog.modality == Modality::Modal)
            .map(|dialog| dialog.id)
    }

    /// Whether pointer or keyboard input may reach `node`: anything when
    /// no modal is open, otherwise only the top modal's subtree.
    pub fn accepts_input(&self, tree: &Tree, node: NodeId) -> bool {
        match self
            .open
            .iter()
            .rev()
            .find(|dialog| dialog.modality == Modality::Modal)
        {
            Some(modal) => tree.is_within(node, modal.root),
            None => true,
        }
    }

    /// Tab-cycles focus within the top modal dialog.
    pub fn focus_next(&self, tree: &mut Tree, reverse: bool) -> Option<NodeId> {
        let modal = self
            .open
            .iter()
            .rev()
            .find(|dialog| dialog.modality == Modality::Modal)?;
        tree.focus_next_within(modal.root, reverse)
    }

    /// The semantics of dialog `id`, with the description in the
    /// language of `strings`.
    pub fn semantics(
        &self,
        id: DialogId,
        title: &str,
        strings: &Localizer,
    ) -> Option<SemanticNode> {
        let dialog = self.open.iter().find(|dialog| dialog.id == id)?;
        Some(SemanticNode {
            role: SemanticRole::Dialog,
            name: title.to_owned(),
            description: match dialog.modality {
                Modality::Modal => strings.message("choscorui.modal", &[]),
                Modality::Modeless => String::new(),
            },
            value: None,
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: vec![SemanticAction::Dismiss],
            text: None,
        })
    }
}

/// One channel a [`ColorPicker`] edits as a fraction from 0 to 1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorChannel {
    Hue,
    Saturation,
    Value,
    Alpha,
}

impl ColorChannel {
    pub const ALL: [Self; 4] = [Self::Hue, Self::Saturation, Self::Value, Self::Alpha];
}

/// Color chooser model: HSV editing with RGB and hex text, alpha, and
/// recent colors. All channels are kept in floating point so repeated
/// edits do not drift.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorPicker {
    hue: f64,
    saturation: f64,
    value: f64,
    alpha: f64,
    recent: Vec<Color>,
    /// The accessible name in a control host.
    name: String,
    /// The hosted parts: the focused channel, the channel under a drag,
    /// and the geometry of the last arrangement.
    focus: ColorChannel,
    drag: Option<ColorChannel>,
    swatch: Rect,
    strips: [Rect; 4],
    rtl: bool,
}

impl ColorPicker {
    pub fn new(color: Color) -> Self {
        let mut picker = Self {
            hue: 0.0,
            saturation: 0.0,
            value: 0.0,
            alpha: 1.0,
            recent: Vec::new(),
            name: String::new(),
            focus: ColorChannel::Hue,
            drag: None,
            swatch: Rect::default(),
            strips: [Rect::default(); 4],
            rtl: false,
        };
        picker.set_color(color);
        picker
    }

    /// Sets the accessible name that the picker has in a control host.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn color(&self) -> Color {
        let (r, g, b) = hsv_to_rgb(self.hue, self.saturation, self.value);
        let byte = |channel: f64| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color::rgba8(byte(r), byte(g), byte(b), byte(self.alpha))
    }

    pub fn set_color(&mut self, color: Color) {
        let channel = |byte: u8| f64::from(byte) / 255.0;
        let (h, s, v) = rgb_to_hsv(
            channel(color.red),
            channel(color.green),
            channel(color.blue),
        );
        // Keep the previous hue for grays so dragging saturation back up
        // does not jump to red.
        if s > 0.0 {
            self.hue = h;
        }
        self.saturation = s;
        self.value = v;
        self.alpha = channel(color.alpha);
    }

    pub fn hsv(&self) -> (f64, f64, f64) {
        (self.hue, self.saturation, self.value)
    }
    pub fn set_hsv(&mut self, hue: f64, saturation: f64, value: f64) {
        self.hue = hue.rem_euclid(360.0);
        self.saturation = saturation.clamp(0.0, 1.0);
        self.value = value.clamp(0.0, 1.0);
    }
    pub fn set_alpha(&mut self, alpha: f64) {
        self.alpha = alpha.clamp(0.0, 1.0);
    }

    /// `channel` as a fraction of its range.
    pub fn channel(&self, channel: ColorChannel) -> f64 {
        match channel {
            ColorChannel::Hue => self.hue / 360.0,
            ColorChannel::Saturation => self.saturation,
            ColorChannel::Value => self.value,
            ColorChannel::Alpha => self.alpha,
        }
    }

    /// Sets `channel` from a fraction of its range, clamped to 0..=1.
    pub fn set_channel(&mut self, channel: ColorChannel, fraction: f64) {
        let fraction = fraction.clamp(0.0, 1.0);
        match channel {
            // A full turn would wrap back to red, so the hue stops short.
            ColorChannel::Hue => self.hue = fraction * 359.99,
            ColorChannel::Saturation => self.saturation = fraction,
            ColorChannel::Value => self.value = fraction,
            ColorChannel::Alpha => self.alpha = fraction,
        }
    }

    /// Paints `channel` in the context's bounds: a stepped gradient of the
    /// channel inside an `Input` border, and the `Slider` thumb at the
    /// current value. `state` carries focus flags for the thumb. The hue,
    /// saturation, and value strips are opaque. The alpha strip shows a
    /// checkerboard through the color.
    pub fn paint_channel(
        &self,
        cx: &mut PaintContext<'_>,
        channel: ColorChannel,
        state: VisualState,
    ) {
        const STEPS: usize = 24;
        let bounds = cx.bounds;
        let frame = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root));
        let width = bounds.size.width / STEPS as f64;
        if channel == ColorChannel::Alpha {
            checkerboard(cx, bounds);
        }
        for step in 0..STEPS {
            let mut sample = self.clone();
            let t = step as f64 / (STEPS - 1) as f64;
            if channel == ColorChannel::Hue {
                sample.set_hsv(0.0, 1.0, 1.0);
            }
            sample.alpha = 1.0;
            sample.set_channel(channel, t);
            let x = if cx.rtl {
                bounds.max_x() - (step + 1) as f64 * width
            } else {
                bounds.origin.x + step as f64 * width
            };
            cx.list.fill_rect(
                Rect::from_xywh(x, bounds.origin.y, width + 0.5, bounds.size.height),
                sample.color(),
            );
        }
        let _ = cx
            .list
            .stroke_rect(bounds, frame.border, frame.border_width);
        let thumb = cx.style(StyleQuery::new(ControlKind::Slider, Slot::Thumb).state(state));
        let fraction = self.channel(channel);
        let along = bounds.size.width * if cx.rtl { 1.0 - fraction } else { fraction };
        let center = Point::new(bounds.origin.x + along, bounds.center().y);
        let rect = Rect::from_xywh(
            center.x - thumb.min_width / 2.0,
            center.y - thumb.min_height / 2.0,
            thumb.min_width,
            thumb.min_height,
        );
        cx.slot(rect, &thumb, |_, _| {});
    }

    /// Paints a swatch of the current color in the context's bounds,
    /// framed like an `Input`, over a checkerboard that shows through a
    /// color that is not opaque.
    pub fn paint_swatch(&self, cx: &mut PaintContext<'_>) {
        let frame = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root));
        let mut swatch = frame;
        swatch.background = self.color().into();
        swatch.shadow = choscorui_core::ShadowLayers::NONE;
        let bounds = cx.bounds;
        let shape = Path::rounded_rect_corners(bounds, swatch.radius.physical(cx.rtl));
        if cx.list.push_clip_path(shape).is_ok() {
            checkerboard(cx, bounds);
            let _ = cx.list.pop_clip();
        }
        cx.slot(bounds, &swatch, |_, _| {});
    }

    /// `#RRGGBB`, or `#RRGGBBAA` when not opaque.
    pub fn hex(&self) -> String {
        let color = self.color();
        if color.alpha == 255 {
            format!("#{:02X}{:02X}{:02X}", color.red, color.green, color.blue)
        } else {
            format!(
                "#{:02X}{:02X}{:02X}{:02X}",
                color.red, color.green, color.blue, color.alpha
            )
        }
    }

    /// Accepts `#RGB`, `#RGBA`, `#RRGGBB`, or `#RRGGBBAA`. The one `#` is
    /// optional, and the other characters must be hex digits. A color
    /// that equals the current color keeps the channels, so a hue on a
    /// gray does not change.
    pub fn set_hex(&mut self, text: &str) -> ValidationState {
        let text = text.trim();
        let digits = text.strip_prefix('#').unwrap_or(text);
        if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return ValidationState::Invalid(ValidationError::Color);
        }
        let parse = |at: usize, len: usize| {
            u8::from_str_radix(&digits[at..at + len], 16)
                .map_or(0, |value| if len == 1 { value * 17 } else { value })
        };
        let (len, count) = match digits.len() {
            3 | 4 => (1, digits.len()),
            6 | 8 => (2, digits.len() / 2),
            0..=7 => return ValidationState::Intermediate,
            _ => return ValidationState::Invalid(ValidationError::Color),
        };
        let alpha = if count == 4 { parse(3 * len, len) } else { 255 };
        let color = Color::rgba8(parse(0, len), parse(len, len), parse(2 * len, len), alpha);
        if color != self.color() {
            self.set_color(color);
        }
        ValidationState::Valid
    }

    /// Records the accepted color at the front of the recent list.
    pub fn accept(&mut self) -> Color {
        let color = self.color();
        self.recent.retain(|recent| *recent != color);
        self.recent.insert(0, color);
        self.recent.truncate(12);
        color
    }
    pub fn recent(&self) -> &[Color] {
        &self.recent
    }

    /// Accessible description such as "Red 30, Green 144, Blue 255,
    /// Opacity 100%", in the language of `strings`.
    pub fn semantics(&self, name: &str, strings: &Localizer) -> SemanticNode {
        let color = self.color();
        let opacity = (self.alpha * 100.0).round().to_string();
        SemanticNode {
            role: SemanticRole::Group,
            name: name.to_owned(),
            description: strings.message(
                "choscorui.color.channels",
                &[
                    ("red", &color.red.to_string()),
                    ("green", &color.green.to_string()),
                    ("blue", &color.blue.to_string()),
                    ("opacity", &opacity),
                ],
            ),
            value: Some(self.hex()),
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: vec![SemanticAction::SetValue, SemanticAction::Focus],
            text: None,
        }
    }
}

/// The keyboard step of a channel, and the step of PageUp and PageDown.
const CHANNEL_STEP: f64 = 0.01;
const CHANNEL_PAGE: f64 = 0.1;

impl ColorPicker {
    /// The hosted row heights: the swatch, one channel strip, and the gap
    /// between rows.
    fn rows(styles: &Styles, tokens: &DesignTokens) -> (f64, f64, f64) {
        let input = styles.resolve(tokens, StyleQuery::new(ControlKind::Input, Slot::Root));
        let thumb = styles.resolve(tokens, StyleQuery::new(ControlKind::Slider, Slot::Thumb));
        (input.min_height, thumb.min_height, tokens.spacing * 2.0)
    }

    fn channel_index(channel: ColorChannel) -> usize {
        ColorChannel::ALL
            .iter()
            .position(|candidate| *candidate == channel)
            .unwrap_or(0)
    }

    /// The channel strip that `point` is on, across the full row height.
    fn strip_at(&self, point: Point) -> Option<ColorChannel> {
        ColorChannel::ALL.into_iter().find(|channel| {
            let strip = self.strips[Self::channel_index(*channel)];
            let row = Rect::from_xywh(
                self.swatch.origin.x,
                strip.origin.y,
                self.swatch.size.width,
                strip.size.height,
            );
            row.contains(point)
        })
    }

    /// Sets `channel` from a pointer at `x`, in reading order.
    fn drag_to(&mut self, cx: &mut ControlContext<'_>, channel: ColorChannel, x: f64) {
        let strip = self.strips[Self::channel_index(channel)];
        let along = ((x - strip.origin.x) / strip.size.width.max(1.0)).clamp(0.0, 1.0);
        let fraction = if self.rtl { 1.0 - along } else { along };
        self.change(cx, channel, fraction);
    }

    /// Sets `channel` to `fraction` and reports the color when it moved.
    fn change(&mut self, cx: &mut ControlContext<'_>, channel: ColorChannel, fraction: f64) {
        let before = self.channel(channel);
        self.set_channel(channel, fraction);
        cx.request_paint();
        if self.channel(channel) != before {
            cx.request_semantics();
            self.report(cx);
        }
    }

    /// Reports the color: first its hex text, then its channels. The
    /// channels show a change that the hex text does not, such as a new
    /// hue of a gray.
    fn report(&self, cx: &mut ControlContext<'_>) {
        cx.emit(ControlMessage::Changed(ControlValue::Text(self.hex())));
        let channels = ColorChannel::ALL.map(|channel| self.channel(channel));
        cx.emit(ControlMessage::Changed(ControlValue::Channels(
            channels.to_vec(),
        )));
    }

    fn key(&mut self, cx: &mut ControlContext<'_>, key: Key) -> EventReply {
        let channel = self.focus;
        let value = self.channel(channel);
        let (forward, back) = if self.rtl {
            (Key::ArrowLeft, Key::ArrowRight)
        } else {
            (Key::ArrowRight, Key::ArrowLeft)
        };
        let target = match key {
            Key::ArrowUp | Key::ArrowDown => {
                let current = Self::channel_index(channel);
                if let Some(next) =
                    hosted::rove(key, current, ColorChannel::ALL.len(), false, self.rtl)
                {
                    self.focus = ColorChannel::ALL[next];
                    cx.request_paint();
                    cx.request_semantics();
                }
                return EventReply::Stop;
            }
            Key::Enter => {
                cx.emit(ControlMessage::Activated);
                return EventReply::Stop;
            }
            k if k == forward => value + CHANNEL_STEP,
            k if k == back => value - CHANNEL_STEP,
            Key::PageUp => value + CHANNEL_PAGE,
            Key::PageDown => value - CHANNEL_PAGE,
            Key::Home => 0.0,
            Key::End => 1.0,
            _ => return EventReply::Continue,
        };
        self.change(cx, channel, target);
        EventReply::Stop
    }
}

impl CustomControl for ColorPicker {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let (swatch, strip, gap) = Self::rows(cx.styles, cx.tokens);
        let padding = cx
            .style(StyleQuery::new(ControlKind::ColorPicker, Slot::Root))
            .padding;
        let height = swatch + 4.0 * (strip + gap) - gap + padding.vertical();
        // A strip is 60 spacing units wide, so each 1% step moves visibly.
        let width = cx.tokens.spacing * 60.0 + padding.horizontal();
        Measured::new(cx.constraints.constrain(Size::new(width, height)))
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let (swatch, strip, gap) = Self::rows(cx.styles, cx.tokens);
        let thumb = cx.style(StyleQuery::new(ControlKind::Slider, Slot::Thumb));
        self.rtl = cx.rtl();
        let root = cx.style(StyleQuery::new(ControlKind::ColorPicker, Slot::Root));
        let bounds = content_rect(bounds, &root, self.rtl);
        self.swatch = Rect::from_xywh(bounds.origin.x, bounds.origin.y, bounds.size.width, swatch);
        // The strips leave room for half a thumb at each end.
        let inset = thumb.min_width / 2.0;
        let mut y = bounds.origin.y + swatch + gap;
        for rect in &mut self.strips {
            *rect = Rect::from_xywh(
                bounds.origin.x + inset,
                y,
                (bounds.size.width - 2.0 * inset).max(0.0),
                strip,
            );
            y += strip + gap;
        }
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) => {
                let Some(channel) = self.strip_at(*position) else {
                    return EventReply::Continue;
                };
                self.focus = channel;
                self.drag = Some(channel);
                cx.capture_pointer(true);
                self.drag_to(cx, channel, position.x);
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) => match self.drag {
                Some(channel) => {
                    self.drag_to(cx, channel, position.x);
                    EventReply::Stop
                }
                None => EventReply::Continue,
            },
            ControlEvent::Pointer(PointerEvent::Up { .. }) if self.drag.is_some() => {
                self.drag = None;
                cx.capture_pointer(false);
                cx.request_paint();
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) => self.key(cx, *key),
            ControlEvent::FocusChanged(false) | ControlEvent::Dismiss if self.drag.is_some() => {
                self.drag = None;
                cx.capture_pointer(false);
                cx.request_paint();
                EventReply::Continue
            }
            ControlEvent::FocusChanged(_) => {
                cx.request_paint();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        // The picker box rings the whole control just inside its bounds,
        // which clip it.
        let mut root = cx.style(StyleQuery::new(ControlKind::ColorPicker, Slot::Root).state(host));
        let ring = root.ring.take();
        cx.slot(cx.bounds, &root, |cx, _| {
            self.paint_swatch(&mut cx.at(self.swatch));
            for (channel, strip) in ColorChannel::ALL.into_iter().zip(self.strips) {
                let focused = channel == self.focus;
                let state = VisualState::NONE
                    .set(VisualState::FOCUSED, focused && host.focused())
                    .set(VisualState::FOCUS_VISIBLE, focused && host.focus_visible())
                    .set(VisualState::PRESSED, self.drag == Some(channel));
                self.paint_channel(&mut cx.at(strip), channel, state);
            }
        });
        root.ring = ring;
        cx.ring_inside(cx.bounds, &root);
    }

    fn semantics(&self) -> SemanticNode {
        self.semantics_in(Localizer::builtin())
    }

    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        let mut node = ColorPicker::semantics(self, &self.name, strings);
        node.actions
            .extend([SemanticAction::Increment, SemanticAction::Decrement]);
        node
    }

    /// A `Slider` part for each channel, in percent with its range. The
    /// focused channel is the focused part.
    fn semantic_parts(&self, strings: &Localizer, parts: &mut SemanticParts) {
        let ids = [
            "choscorui.color.hue",
            "choscorui.color.saturation",
            "choscorui.color.brightness",
            "choscorui.color.opacity",
        ];
        for ((channel, strip), id) in ColorChannel::ALL.into_iter().zip(self.strips).zip(ids) {
            let percent = (self.channel(channel) * 100.0).round();
            let name = strings.message(id, &[]);
            let part = parts.push(None, SemanticRole::Slider, &name, strip);
            part.node.value = Some(format!("{percent}%"));
            part.node.state.range = Some(SemanticRange {
                value: percent,
                minimum: 0.0,
                maximum: 100.0,
                step: CHANNEL_STEP * 100.0,
            });
            part.node.actions = vec![SemanticAction::Increment, SemanticAction::Decrement];
            if channel == self.focus {
                parts.set_focused(Some(parts.len() - 1));
            }
        }
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        match (action, value) {
            (SemanticAction::SetValue, Some(text)) => {
                if self.set_hex(text) != ValidationState::Valid {
                    return false;
                }
                cx.request_paint();
                cx.request_semantics();
                self.report(cx);
                true
            }
            (SemanticAction::Focus, _) => {
                cx.request_focus();
                true
            }
            (SemanticAction::Increment | SemanticAction::Decrement, _) => {
                let sign = if action == SemanticAction::Increment {
                    1.0
                } else {
                    -1.0
                };
                let target = self.channel(self.focus) + sign * CHANNEL_STEP;
                self.change(cx, self.focus, target);
                true
            }
            _ => false,
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.drag = None;
        }
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        match value {
            ControlValue::Text(text) => self.set_hex(text) == ValidationState::Valid,
            _ => false,
        }
    }
}

/// Fills `rect` with squares of two spacing units in the `background` and
/// `muted` colors, which show through a color that is not opaque.
fn checkerboard(cx: &mut PaintContext<'_>, rect: Rect) {
    let side = cx.tokens.spacing * 2.0;
    if side <= 0.0 {
        return;
    }
    cx.list
        .fill_rect(rect, cx.tokens.color(ColorRole::Background));
    let muted = cx.tokens.color(ColorRole::Muted);
    let (columns, rows) = (
        (rect.size.width / side).ceil() as usize,
        (rect.size.height / side).ceil() as usize,
    );
    for row in 0..rows {
        for column in (row % 2..columns).step_by(2) {
            let x = rect.origin.x + column as f64 * side;
            let y = rect.origin.y + row as f64 * side;
            let width = side.min(rect.max_x() - x);
            let height = side.min(rect.max_y() - y);
            cx.list
                .fill_rect(Rect::from_xywh(x, y, width, height), muted);
        }
    }
}

fn hsv_to_rgb(hue: f64, saturation: f64, value: f64) -> (f64, f64, f64) {
    let chroma = value * saturation;
    let sector = (hue / 60.0).rem_euclid(6.0);
    let x = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (r, g, b) = match sector as u8 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = value - chroma;
    (r + m, g + m, b + m)
}

fn rgb_to_hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    (hue, saturation, max)
}

/// Font dialog model: family (with missing-font fallback), size, weight,
/// slant, and a preview style.
pub struct FontDialog {
    family: FontChooser,
    size: f64,
    weight: FontWeight,
    slant: FontSlant,
}

impl FontDialog {
    pub fn new(families: Vec<String>, current: &TextStyle) -> Self {
        Self {
            family: FontChooser::new(families, &current.family, "Helvetica"),
            size: current.size,
            weight: current.weight,
            slant: current.slant,
        }
    }
    pub fn family(&mut self, input: ComboInput) -> ComboChange {
        self.family.handle(input)
    }
    pub fn chooser(&self) -> &FontChooser {
        &self.family
    }
    /// The family chooser, to mount it in a control host.
    pub fn into_chooser(self) -> FontChooser {
        self.family
    }
    pub fn set_size(&mut self, size: f64) -> ValidationState {
        if size.is_finite() && (1.0..=4096.0).contains(&size) {
            self.size = size;
            ValidationState::Valid
        } else {
            ValidationState::Invalid(ValidationError::Number(NumberError::OutOfRange {
                min: 1.0,
                max: 4096.0,
            }))
        }
    }
    pub fn set_weight(&mut self, weight: FontWeight) {
        self.weight = FontWeight(weight.0.clamp(1, 1000));
    }
    pub fn set_slant(&mut self, slant: FontSlant) {
        self.slant = slant;
    }
    /// The chosen style (missing families resolve to the fallback).
    pub fn style(&self) -> TextStyle {
        TextStyle::new(self.family.resolved_family(), self.size)
            .with_weight(self.weight)
            .with_slant(self.slant)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum InputKind {
    Text {
        allow_empty: bool,
    },
    Integer {
        minimum: i64,
        maximum: i64,
    },
    Decimal {
        minimum: f64,
        maximum: f64,
        fraction_digits: usize,
    },
    Choice(Vec<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum InputValue {
    Text(String),
    Integer(i64),
    Decimal(f64),
    Choice(usize),
}

/// Single-value input dialog ("Rename", "Go to line"). Return accepts only
/// a valid value; the accept button's enabled state follows validation.
#[derive(Clone, Debug)]
pub struct InputDialog {
    pub title: String,
    pub prompt: String,
    kind: InputKind,
    locale: Locale,
    text: String,
}

impl InputDialog {
    pub fn new(
        title: impl Into<String>,
        prompt: impl Into<String>,
        kind: InputKind,
        locale: Locale,
        initial: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            prompt: prompt.into(),
            kind,
            locale,
            text: initial.into(),
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }
    /// The number rule of an Integer or Decimal dialog. Both use the same
    /// rule, so they reject the same text with the same messages.
    pub fn number(&self) -> Option<NumberInput> {
        let (minimum, maximum, fraction_digits) = match &self.kind {
            InputKind::Integer { minimum, maximum } => (*minimum as f64, *maximum as f64, 0),
            InputKind::Decimal {
                minimum,
                maximum,
                fraction_digits,
            } => (*minimum, *maximum, *fraction_digits),
            InputKind::Text { .. } | InputKind::Choice(_) => return None,
        };
        Some(
            NumberInput::new(self.locale.clone(), fraction_digits)
                .with_range(Some(minimum), Some(maximum)),
        )
    }

    /// The filter of the text field: the number rule for an Integer or
    /// Decimal dialog.
    pub fn filter(&self) -> InputFilter {
        self.number()
            .map_or_else(InputFilter::new, |number| number.filter())
    }

    /// The validation while the user types, and the value when valid. A
    /// number below the minimum is Intermediate.
    pub fn validate(&self) -> (ValidationState, Option<InputValue>) {
        self.check(false)
    }

    /// The validation when the user accepts the text, and the value when
    /// valid. A number outside the range is Invalid.
    pub fn commit(&self) -> (ValidationState, Option<InputValue>) {
        self.check(true)
    }

    fn check(&self, commit: bool) -> (ValidationState, Option<InputValue>) {
        let trimmed = self.text.trim();
        if let Some(number) = self.number() {
            let (state, value) = if commit {
                number.commit(trimmed)
            } else {
                number.validate(trimmed)
            };
            let value = match self.kind {
                InputKind::Integer { .. } => value
                    .and_then(|_| self.locale.parse_integer(trimmed).ok())
                    .map(InputValue::Integer),
                _ => value.map(InputValue::Decimal),
            };
            return (state, value);
        }
        match &self.kind {
            InputKind::Choice(options) => match options.iter().position(|option| option == trimmed)
            {
                Some(index) => (ValidationState::Valid, Some(InputValue::Choice(index))),
                None => (ValidationState::Invalid(ValidationError::Choice), None),
            },
            InputKind::Text { allow_empty } if trimmed.is_empty() && !allow_empty => {
                (ValidationState::Intermediate, None)
            }
            _ => (
                ValidationState::Valid,
                Some(InputValue::Text(trimmed.to_owned())),
            ),
        }
    }

    pub fn can_accept(&self) -> bool {
        self.validate().0 == ValidationState::Valid
    }
    /// Return: the value when valid; Escape: `Err(())` (cancelled).
    pub fn handle_key(&self, key: Key) -> Option<Result<InputValue, ()>> {
        match key {
            Key::Enter => self.commit().1.map(Ok),
            Key::Escape => Some(Err(())),
            _ => None,
        }
    }
}

/// Custom-drawn alert dialog with ordered buttons and keyboard rules.
#[derive(Clone, Debug, PartialEq)]
pub struct AlertDialog {
    pub title: String,
    pub message: String,
    pub detail: Option<String>,
    pub buttons: Vec<DialogButton>,
    /// `Sm` makes the panel at most 320 pt wide.
    pub size: ControlSize,
    /// An icon in a muted box above the title.
    pub media: Option<Icon>,
}

/// Deprecated name of [`AlertDialog`].
#[deprecated(since = "0.1.0", note = "renamed to `AlertDialog`")]
pub type MessageDialog = AlertDialog;

impl AlertDialog {
    pub fn new(
        title: impl Into<String>,
        message: impl Into<String>,
        buttons: Vec<DialogButton>,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            detail: None,
            buttons,
            size: ControlSize::Default,
            media: None,
        }
    }

    /// The alert at `size`: `Sm` is narrower.
    #[must_use]
    pub fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// The alert with `icon` in a muted box above the title.
    #[must_use]
    pub fn with_media(mut self, icon: Icon) -> Self {
        self.media = Some(icon);
        self
    }

    fn query(&self, slot: Slot) -> StyleQuery {
        StyleQuery::new(ControlKind::AlertDialog, slot).size(self.size)
    }

    /// The height of the media box and the gap under it.
    fn media_height(&self, styles: &Styles, tokens: &DesignTokens) -> f64 {
        self.media.map_or(0.0, |_| {
            let media = styles.resolve(tokens, self.query(Slot::Media));
            media.min_height + media.padding.block_end
        })
    }

    /// The button sizes in display order, the gap index, and whether the
    /// row fits `inner`. A row that does not fit stacks.
    fn button_row(
        &self,
        inner: f64,
        styles: &Styles,
        tokens: &DesignTokens,
        text: &dyn TextMeasure,
        rtl: bool,
    ) -> (Vec<usize>, Vec<Size>, usize, bool) {
        let footer = styles.resolve(tokens, self.query(Slot::Footer));
        let (order, gap) = order_dialog_buttons(&self.buttons, rtl);
        let sizes: Vec<Size> = order
            .iter()
            .map(|index| self.button_size(*index, styles, tokens, text))
            .collect();
        let width = sizes.iter().map(|size| size.width).sum::<f64>()
            + sizes.len().saturating_sub(1) as f64 * footer.gap;
        (order, sizes, gap, width <= inner)
    }

    /// Buttons in display order with the gap index.
    pub fn layout(&self, rtl: bool) -> (Vec<&DialogButton>, usize) {
        let (order, gap) = order_dialog_buttons(&self.buttons, rtl);
        (
            order
                .into_iter()
                .map(|index| &self.buttons[index])
                .collect(),
            gap,
        )
    }
    pub fn handle_key(&self, key: Key) -> Option<usize> {
        dialog_key_button(&self.buttons, key)
    }

    /// The button look of a role: the default button for Accept, the
    /// destructive button for Destructive, and outline for the rest.
    pub fn button_variant(role: DialogButtonRole) -> ButtonVariant {
        match role {
            DialogButtonRole::Accept => ButtonVariant::Default,
            DialogButtonRole::Destructive => ButtonVariant::Destructive,
            _ => ButtonVariant::Outline,
        }
    }

    fn texts(&self) -> impl Iterator<Item = (Slot, &str)> {
        [
            (Slot::Title, self.title.as_str()),
            (Slot::Description, &self.message),
        ]
        .into_iter()
        .chain(
            self.detail
                .as_deref()
                .map(|detail| (Slot::Description, detail)),
        )
    }

    fn button_size(
        &self,
        index: usize,
        styles: &Styles,
        tokens: &DesignTokens,
        text: &dyn TextMeasure,
    ) -> Size {
        let button = &self.buttons[index];
        let query = StyleQuery::new(ControlKind::Button, Slot::Root)
            .variant(Self::button_variant(button.role));
        Content::label(&button.title).size(&styles.resolve(tokens, query), text)
    }

    /// The panel size inside `available`: the padding, the title, the
    /// message and the detail wrapped in the panel width, and the button
    /// row.
    pub fn measure(
        &self,
        available: Size,
        styles: &Styles,
        tokens: &DesignTokens,
        text: &dyn TextMeasure,
    ) -> Size {
        let style = |slot| styles.resolve(tokens, self.query(slot));
        let (panel, header) = (style(Slot::Root), style(Slot::Header));
        let width = available.width.min(panel.max_width);
        let inner = (width - panel.padding.horizontal()).max(0.0);
        let (_, sizes, _, fits) = self.button_row(inner, styles, tokens, text, false);
        let lines: Vec<f64> = self
            .texts()
            .map(|(slot, line)| {
                let spec = style(slot).text;
                wrapped_height(line, &spec, inner, text).max(spec.line_height)
            })
            .collect();
        let gaps = lines.len().saturating_sub(1) as f64 * header.gap;
        let tallest = sizes.iter().map(|size| size.height).fold(0.0, f64::max);
        // A row that does not fit stacks the buttons.
        let row = if fits || sizes.is_empty() {
            tallest
        } else {
            let gap = style(Slot::Footer).gap;
            sizes.len() as f64 * (tallest + gap) - gap
        };
        let height = panel.padding.vertical()
            + self.media_height(styles, tokens)
            + lines.iter().sum::<f64>()
            + gaps
            + panel.gap
            + row;
        Size::new(width, height.min(available.height))
    }

    /// The rect of each button in a panel at `panel`, by index into
    /// `buttons`. Help and Discard sit at the start of the row, and the
    /// rest at the end, in platform order. When the row does not fit, the
    /// buttons stack at the full width, the default button at the top.
    pub fn button_rects(
        &self,
        panel: Rect,
        styles: &Styles,
        tokens: &DesignTokens,
        text: &dyn TextMeasure,
        rtl: bool,
    ) -> Vec<(usize, Rect)> {
        let root = styles.resolve(tokens, self.query(Slot::Root));
        let footer = styles.resolve(tokens, self.query(Slot::Footer));
        let (left, right) = (
            panel.origin.x + root.padding.inline_start,
            panel.max_x() - root.padding.inline_end,
        );
        let (order, sizes, gap, fits) = self.button_row(right - left, styles, tokens, text, rtl);
        let row_height = sizes.iter().map(|size| size.height).fold(0.0, f64::max);
        let bottom = panel.max_y() - root.padding.block_end;
        if !fits {
            // The last button in display order is the default one, so
            // the stack lists the order from the end.
            let count = order.len() as f64;
            let top = bottom - count * (row_height + footer.gap) + footer.gap;
            return order
                .iter()
                .rev()
                .enumerate()
                .map(|(row, index)| {
                    let y = top + row as f64 * (row_height + footer.gap);
                    (*index, Rect::from_xywh(left, y, right - left, row_height))
                })
                .collect();
        }
        let y = bottom - row_height;
        let mut rects = Vec::with_capacity(order.len());
        let mut x = left;
        for (index, size) in order.iter().zip(&sizes).take(gap) {
            rects.push((*index, Rect::from_xywh(x, y, size.width, row_height)));
            x += size.width + footer.gap;
        }
        let trailing: f64 = sizes[gap..].iter().map(|size| size.width).sum::<f64>()
            + sizes.len().saturating_sub(gap).saturating_sub(1) as f64 * footer.gap;
        let mut x = right - trailing;
        for (index, size) in order.iter().zip(&sizes).skip(gap) {
            rects.push((*index, Rect::from_xywh(x, y, size.width, row_height)));
            x += size.width + footer.gap;
        }
        rects
    }

    /// Paints the scrim of a modal alert over the context's bounds. Paint
    /// it before the panel.
    pub fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        let style = cx.style(self.query(Slot::Overlay));
        let bounds = cx.bounds;
        cx.slot(bounds, &style, |_, _| {});
    }

    /// Paints the panel, and the title, the message, and the detail
    /// wrapped in the panel width, in the context's bounds. The host paints the buttons at [`AlertDialog::button_rects`]
    /// with [`AlertDialog::button_variant`].
    pub fn paint(&self, cx: &mut PaintContext<'_>) {
        let query = |slot| self.query(slot);
        let (panel, header) = (cx.style(query(Slot::Root)), cx.style(query(Slot::Header)));
        let media = cx.style(query(Slot::Media));
        let bounds = cx.bounds;
        cx.slot(bounds, &panel, |cx, inner| {
            let mut y = inner.origin.y;
            if let Some(icon) = self.media {
                let x = if cx.rtl {
                    inner.max_x() - media.min_width
                } else {
                    inner.origin.x
                };
                let rect = Rect::from_xywh(x, y, media.min_width, media.min_height);
                let boxed = ResolvedStyle {
                    padding: Edges::ZERO,
                    ..media.clone()
                };
                cx.slot(rect, &boxed, |cx, inner| cx.icon(inner, icon, &boxed));
                y += media.min_height + media.padding.block_end;
            }
            for (slot, line) in self.texts() {
                let style = cx.style(query(slot));
                let height = wrapped_height(line, &style.text, inner.size.width, cx.text)
                    .max(style.text.line_height);
                let rect = Rect::from_xywh(inner.origin.x, y, inner.size.width, height);
                paint_wrapped(cx, rect, line, &style, TextAlign::Start);
                y += height + header.gap;
            }
        });
    }

    /// The alert as a modal control for a [`crate::ControlHost`]. It
    /// starts closed; open it with [`crate::ControlHost::set_open`].
    pub fn hosted(self) -> HostedAlertDialog {
        HostedAlertDialog {
            alert: self,
            reveal: Reveal::default(),
            viewport: Rect::default(),
            panel: Rect::default(),
            buttons: Vec::new(),
            focus: 0,
            parts: Parts::default(),
            motion_ms: 0,
        }
    }

    pub fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Dialog,
            name: self.title.clone(),
            description: match &self.detail {
                Some(detail) => format!("{} {detail}", self.message),
                None => self.message.clone(),
            },
            value: None,
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: vec![SemanticAction::Dismiss],
            text: None,
        }
    }
}

/// An [`AlertDialog`] mounted in a [`crate::ControlHost`]: a modal panel
/// centered in the window over a scrim. The node itself takes no space.
/// Make it with [`AlertDialog::hosted`].
///
/// - Tab and Shift+Tab move focus between the buttons in logical order.
///   Space activates the focused button.
/// - Enter activates the `Accept` button. Without one, it activates the
///   focused button unless that button is `Destructive`. Escape and
///   Command+period activate the `Reject` button, or dismiss.
/// - A button reports `ItemActivated(index into buttons)` and closes the
///   alert. A dismissal reports `Dismissed`. Each close also reports
///   `OpenChanged(false)`. A press outside the panel does nothing.
pub struct HostedAlertDialog {
    alert: AlertDialog,
    reveal: Reveal,
    viewport: Rect,
    panel: Rect,
    /// The button rectangles in display order, by index into `buttons`.
    buttons: Vec<(usize, Rect)>,
    /// The focused button, by index into `buttons`.
    focus: usize,
    /// The hovered and pressed buttons.
    parts: Parts,
    motion_ms: u32,
}

impl HostedAlertDialog {
    pub fn alert(&self) -> &AlertDialog {
        &self.alert
    }

    /// The alert with `detail` as its informative text under the message.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.alert.detail = Some(detail.into());
        self
    }

    pub fn is_open(&self) -> bool {
        self.reveal.open
    }

    /// The default button: `Accept`, else the first button in display
    /// order.
    fn default_focus(&self) -> usize {
        self.alert
            .buttons
            .iter()
            .position(|button| button.role == DialogButtonRole::Accept)
            .or_else(|| self.buttons.first().map(|(index, _)| *index))
            .unwrap_or(0)
    }

    fn open(&mut self, open: bool, time: f64) {
        self.parts.clear();
        if open && !self.reveal.open {
            self.focus = self.default_focus();
        }
        self.reveal.set(open, time);
    }

    fn close(&mut self, cx: &mut ControlContext<'_>) {
        if self.parts.pressed.is_some() {
            cx.capture_pointer(false);
        }
        self.open(false, cx.time());
        cx.emit(ControlMessage::OpenChanged(false));
        cx.request_paint();
        cx.request_semantics();
    }

    /// Reports the button. Each button closes the alert, but Help keeps
    /// it open.
    fn activate(&mut self, cx: &mut ControlContext<'_>, index: usize) {
        let Some(button) = self.alert.buttons.get(index) else {
            return;
        };
        cx.emit(ControlMessage::ItemActivated(index));
        if button.role != DialogButtonRole::Help {
            self.close(cx);
        }
    }

    fn dismiss(&mut self, cx: &mut ControlContext<'_>) {
        self.close(cx);
        cx.emit(ControlMessage::Dismissed);
    }

    fn button_at(&self, point: Point) -> Option<usize> {
        self.buttons
            .iter()
            .find(|(_, rect)| rect.contains(point))
            .map(|(index, _)| *index)
    }

    fn key(&mut self, cx: &mut ControlContext<'_>, key: Key) {
        let command = cx.modifiers().command;
        match key {
            Key::Tab if !self.buttons.is_empty() => {
                // Tab follows the logical order, also in right-to-left
                // layouts, where the row mirrors.
                let (order, _) = order_dialog_buttons(&self.alert.buttons, false);
                let count = order.len();
                let current = order
                    .iter()
                    .position(|index| *index == self.focus)
                    .unwrap_or(0);
                let next = if cx.modifiers().shift {
                    (current + count - 1) % count
                } else {
                    (current + 1) % count
                };
                self.focus = order[next];
                cx.request_paint();
            }
            Key::Space => self.activate(cx, self.focus),
            // Return never runs a destructive button.
            Key::Enter => {
                let focused = Some(self.focus).filter(|index| {
                    self.alert
                        .buttons
                        .get(*index)
                        .is_some_and(|button| button.role != DialogButtonRole::Destructive)
                });
                if let Some(index) = self.alert.handle_key(Key::Enter).or(focused) {
                    self.activate(cx, index);
                }
            }
            Key::Escape | Key::Character('.') if key == Key::Escape || command => {
                match self.alert.handle_key(Key::Escape) {
                    Some(index) => self.activate(cx, index),
                    None => self.dismiss(cx),
                }
            }
            _ => {}
        }
    }

    fn pointer(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) {
        let pressed = self.parts.pressed;
        let mut parts = self.parts;
        parts.track(cx, event, |point| self.button_at(point));
        self.parts = parts;
        match event {
            ControlEvent::Pointer(PointerEvent::Down { .. }) => {
                if let Some(index) = self.parts.pressed {
                    self.focus = index;
                }
            }
            ControlEvent::Pointer(PointerEvent::Up { .. }) => {
                if let Some(index) = pressed.filter(|index| self.parts.hovered == Some(*index)) {
                    self.activate(cx, index);
                }
            }
            _ => {}
        }
    }

    /// Paints the buttons. `host` carries the node's focus flags, which
    /// the focused button shows.
    fn paint_buttons(&self, cx: &mut PaintContext<'_>, host: VisualState) {
        for (index, rect) in &self.buttons {
            let button = &self.alert.buttons[*index];
            let focused = self.focus == *index;
            let state = self
                .parts
                .state(*index, host)
                .set(VisualState::FOCUSED, focused && host.focused())
                .set(VisualState::FOCUS_VISIBLE, focused && host.focus_visible());
            let style = cx.style(
                StyleQuery::new(ControlKind::Button, Slot::Root)
                    .variant(AlertDialog::button_variant(button.role))
                    .state(state),
            );
            let content = Content::label(&button.title);
            cx.slot(*rect, &style, |cx, inner| {
                content.draw(cx, inner, &style, TextAlign::Center);
            });
        }
    }
}

impl CustomControl for HostedAlertDialog {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        Measured::new(cx.constraints.constrain(Size::new(0.0, 0.0)))
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, _bounds: Rect) {
        self.viewport = cx.viewport;
        let root = cx.style(self.alert.query(Slot::Root));
        self.motion_ms = root.motion_ms;
        // The panel keeps a margin of 4 spacing units from the window edges.
        let margin = cx.tokens.spacing * 4.0;
        let available = Size::new(
            (self.viewport.size.width - 2.0 * margin).max(0.0),
            (self.viewport.size.height - 2.0 * margin).max(0.0),
        );
        let size = self.alert.measure(available, cx.styles, cx.tokens, cx.text);
        let center = self.viewport.center();
        self.panel = Rect::from_xywh(
            center.x - size.width / 2.0,
            center.y - size.height / 2.0,
            size.width,
            size.height,
        );
        self.buttons = self
            .alert
            .button_rects(self.panel, cx.styles, cx.tokens, cx.text, cx.rtl());
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target || !self.reveal.open {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(_) => self.pointer(cx, event),
            ControlEvent::Hover(false) => {
                self.pointer(cx, event);
                return EventReply::Continue;
            }
            ControlEvent::Key(KeyEvent::Down { key, repeat: false }) => self.key(cx, *key),
            ControlEvent::Key(_) => {}
            // The host sends `Dismiss` to an open alert that it disables.
            ControlEvent::Dismiss => self.dismiss(cx),
            _ => return EventReply::Continue,
        }
        EventReply::Stop
    }

    fn paint(&self, _cx: &mut PaintContext<'_>) {
        // The node takes no space; the panel paints above every control.
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
        let host = cx.state;
        let _ = cx.list.push_opacity(progress.clamp(0.0, 1.0));
        self.alert.paint_overlay(&mut cx.at(self.viewport));
        let _ = cx.list.pop_opacity();
        Reveal::paint(cx, self.panel, progress, (0.0, 0.0), |cx| {
            self.alert.paint(&mut cx.at(self.panel));
            self.paint_buttons(cx, host);
        });
    }

    fn animating(&self, time: f64) -> bool {
        self.reveal.animating(self.motion_ms, time)
    }

    fn semantics(&self) -> SemanticNode {
        let mut node = self.alert.semantics();
        node.state.modal = self.reveal.open;
        if !self.reveal.open {
            node.state.hidden = true;
            node.actions.clear();
        }
        node
    }

    /// While open, each button is a `Button` part, and the focused one has
    /// the focus.
    fn semantic_parts(&self, _strings: &Localizer, parts: &mut SemanticParts) {
        if !self.reveal.open {
            return;
        }
        for (index, rect) in &self.buttons {
            let Some(button) = self.alert.buttons.get(*index) else {
                continue;
            };
            let role = SemanticRole::Button;
            crate::hosted::pressable(parts, None, role, &button.title, *rect, true);
            if self.focus == *index {
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
        if action != SemanticAction::Dismiss || !self.reveal.open {
            return false;
        }
        self.dismiss(cx);
        true
    }

    fn focusable(&self) -> bool {
        false
    }

    fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.reveal = Reveal::default();
            self.parts.clear();
        }
    }

    fn set_open(&mut self, open: bool, time: f64) -> bool {
        self.open(open, time);
        true
    }
}
