//! File attachments with media, a name, a detail line, and a progress
//! state.

use crate::custom::{ControlContext, ControlEvent, ControlMessage, CustomControl, MeasureContext};
use crate::feedback::loading::paint_loader;
use crate::hosted;
use crate::layout::cover;
use crate::style::fill_area;
use crate::style::{
    AttachmentState, ButtonVariant, ControlKind, ControlSize, PaintContext, Slot, StyleQuery,
    TextAlign, VisualState,
};
use choscorui_core::{
    EventPhase, EventReply, Icon, Image, ImageSampling, Key, KeyEvent, Measured, Path,
    PointerEvent, Rect, SemanticAction, SemanticNode, SemanticRole, Size,
};

/// What an attachment shows in its media box.
#[derive(Clone, Debug, PartialEq)]
pub enum AttachmentMedia {
    Icon(Icon),
    /// An image that covers the media box.
    Image(Image),
}

/// A file attachment. The media box, the name, and the detail line sit
/// in a row, or in a column for `vertical()`. An uploading attachment
/// shows a progress line; a processing one spins a loader in its media
/// box; an attachment with an error shows the destructive border.
/// `with_remove(label)` adds a remove button, which is the only tab
/// stop. Enter, Space, a click, or `Dismiss` emits `Dismissed`.
pub struct Attachment {
    name: String,
    detail: String,
    media: AttachmentMedia,
    vertical: bool,
    state: AttachmentState,
    progress: f64,
    size: ControlSize,
    remove: Option<String>,
    /// The hover and press of the remove button.
    button: hosted::Parts<()>,
    parts: Parts,
    turn_ms: u32,
    enabled: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Parts {
    media: Rect,
    title: Rect,
    detail: Rect,
    remove: Rect,
    bar: Rect,
}

impl Attachment {
    /// An idle attachment named `name`, with a file icon.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            detail: String::new(),
            media: AttachmentMedia::Icon(Icon::File),
            vertical: false,
            state: AttachmentState::Idle,
            progress: 0.0,
            size: ControlSize::Default,
            remove: None,
            button: hosted::Parts::default(),
            parts: Parts::default(),
            turn_ms: 0,
            enabled: true,
        }
    }

    /// The line under the name, such as the file size or an error.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    #[must_use]
    pub fn with_media(mut self, media: AttachmentMedia) -> Self {
        self.media = media;
        self
    }

    /// Stacks the media box over the text.
    #[must_use]
    pub fn vertical(mut self) -> Self {
        self.vertical = true;
        self
    }

    #[must_use]
    pub fn with_state(mut self, state: AttachmentState) -> Self {
        self.state = state;
        self
    }

    /// The uploaded fraction, from 0 to 1, that an uploading attachment
    /// shows.
    #[must_use]
    pub fn with_progress(mut self, progress: f64) -> Self {
        self.progress = progress.clamp(0.0, 1.0);
        self
    }

    /// Sets the size: `Default`, `Sm`, or `Xs`.
    #[must_use]
    pub fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// Adds a remove button that `label` names.
    #[must_use]
    pub fn with_remove(mut self, label: impl Into<String>) -> Self {
        self.remove = Some(label.into());
        self
    }

    fn query(&self, slot: Slot, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Attachment, slot)
            .variant(self.state)
            .size(self.size)
            .state(state.set(VisualState::OPEN, self.vertical))
    }

    /// The remove button: ghost in a row, and outline over the media
    /// box of a vertical attachment, so it shows on any image.
    fn remove_query(&self, state: VisualState) -> StyleQuery {
        let variant = if self.vertical {
            ButtonVariant::Outline
        } else {
            ButtonVariant::Ghost
        };
        StyleQuery::new(ControlKind::Button, Slot::Root)
            .variant(variant)
            .size(ControlSize::IconXs)
            .state(state)
    }

    /// The media edge, the remove button edge, and the text heights.
    fn metrics(&self, cx: &MeasureContext<'_>) -> (f64, f64, f64, f64) {
        let media = cx.style(self.query(Slot::Media, VisualState::NONE));
        let title = cx.style(self.query(Slot::Title, VisualState::NONE));
        let detail = cx.style(self.query(Slot::Description, VisualState::NONE));
        let remove = if self.remove.is_some() {
            cx.style(self.remove_query(VisualState::NONE)).min_height
        } else {
            0.0
        };
        let detail_height = if self.detail.is_empty() {
            0.0
        } else {
            detail.text.line_height + detail.padding.block_start
        };
        (
            media.min_width,
            remove,
            title.text.line_height,
            detail_height,
        )
    }

    fn place(&self, cx: &MeasureContext<'_>, bounds: Rect) -> Parts {
        let root = cx.style(self.query(Slot::Root, VisualState::NONE));
        let detail = cx.style(self.query(Slot::Description, VisualState::NONE));
        let (edge, remove, title, detail_height) = self.metrics(cx);
        let rtl = cx.rtl();
        let inner = Rect::from_xywh(
            bounds.origin.x + root.padding.inline_start,
            bounds.origin.y + root.padding.block_start,
            (bounds.size.width - root.padding.horizontal()).max(0.0),
            (bounds.size.height - root.padding.vertical()).max(0.0),
        );
        let from_start = |offset: f64, width: f64| {
            if rtl {
                inner.max_x() - offset - width
            } else {
                inner.origin.x + offset
            }
        };
        let remove_rect = Rect::from_xywh(
            from_start(inner.size.width - remove, remove),
            inner.origin.y,
            remove,
            remove,
        );
        let text_height = title + detail_height;
        let (media, text) = if self.vertical {
            let media = Rect::from_xywh(
                inner.origin.x,
                inner.origin.y,
                inner.size.width,
                inner.size.width,
            );
            let text = Rect::from_xywh(
                inner.origin.x,
                media.max_y() + root.gap,
                inner.size.width,
                text_height,
            );
            (media, text)
        } else {
            let media = Rect::from_xywh(
                from_start(0.0, edge),
                inner.center().y - edge / 2.0,
                edge,
                edge,
            );
            let used = edge + root.gap;
            let end = if remove > 0.0 { remove + root.gap } else { 0.0 };
            let width = (inner.size.width - used - end).max(0.0);
            let text = Rect::from_xywh(
                from_start(used, width),
                inner.center().y - text_height / 2.0,
                width,
                text_height,
            );
            (media, text)
        };
        let remove_rect = if self.vertical || remove == 0.0 {
            remove_rect
        } else {
            Rect::from_xywh(
                remove_rect.origin.x,
                inner.center().y - remove / 2.0,
                remove,
                remove,
            )
        };
        let bar = cx.style(self.query(Slot::Progress, VisualState::NONE));
        Parts {
            media,
            title: Rect::from_xywh(text.origin.x, text.origin.y, text.size.width, title),
            detail: Rect::from_xywh(
                text.origin.x,
                text.origin.y + title + detail.padding.block_start,
                text.size.width,
                detail.text.line_height,
            ),
            remove: remove_rect,
            bar: Rect::from_xywh(
                text.origin.x,
                text.max_y() + bar.min_height,
                text.size.width,
                bar.min_height,
            ),
        }
    }

    fn remove(&mut self, cx: &mut ControlContext<'_>) {
        if self.remove.is_some() && self.enabled {
            cx.emit(ControlMessage::Dismissed);
        }
    }

    fn paint_media(&self, cx: &mut PaintContext<'_>, host: VisualState) {
        let state = host.set(VisualState::SELECTED, true);
        let style = cx.style(self.query(Slot::Media, state));
        let rect = self.parts.media;
        let pushed = style.opacity < 1.0 && cx.list.push_opacity(style.opacity).is_ok();
        match &self.media {
            AttachmentMedia::Image(image) => {
                let clip = Path::rounded_rect(rect, style.radius.top_start);
                if cx.list.push_clip_path(clip).is_ok() {
                    let _ = cx.list.draw_image(
                        image.clone(),
                        cover(image, rect),
                        ImageSampling::Linear,
                    );
                    let _ = cx.list.pop_clip();
                }
            }
            AttachmentMedia::Icon(icon) => {
                let mut plain = style;
                plain.opacity = 1.0;
                cx.slot(rect, &plain, |cx, inner| {
                    if self.state != AttachmentState::Processing {
                        cx.icon(inner, *icon, &plain);
                    }
                });
            }
        }
        if pushed {
            let _ = cx.list.pop_opacity();
        }
        if self.state == AttachmentState::Processing {
            let spinner = cx.style(StyleQuery::new(ControlKind::Spinner, Slot::Root));
            let center = rect.center();
            let side = spinner.icon_size;
            let mark = Rect::from_xywh(center.x - side / 2.0, center.y - side / 2.0, side, side);
            paint_loader(cx, mark, &spinner, spinner.motion_ms);
        }
    }
}

impl CustomControl for Attachment {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let root = cx.style(self.query(Slot::Root, VisualState::NONE));
        let title = cx.style(self.query(Slot::Title, VisualState::NONE));
        let detail = cx.style(self.query(Slot::Description, VisualState::NONE));
        let bar = cx.style(self.query(Slot::Progress, VisualState::NONE));
        let (edge, remove, title_height, detail_height) = self.metrics(cx);
        let max = cx.constraints.max.width;
        // Only an uploading attachment keeps room for its progress line.
        let bar = if self.state == AttachmentState::Uploading {
            bar.min_height * 2.0
        } else {
            0.0
        };
        let text_height = title_height + detail_height + bar;
        let size = if self.vertical {
            let width = root.min_width.min(max);
            let inner = width - root.padding.horizontal();
            Size::new(
                width,
                inner + root.gap + text_height + root.padding.vertical(),
            )
        } else {
            let text = cx
                .text
                .width(&self.name, &title.text)
                .max(cx.text.width(&self.detail, &detail.text));
            let end = if remove > 0.0 { remove + root.gap } else { 0.0 };
            let natural = root.padding.horizontal() + edge + root.gap + text.ceil() + end;
            // A wide name grows the row up to 1.6 times its least width.
            let width = natural.clamp(root.min_width, root.min_width * 1.6).min(max);
            let content = edge.max(text_height).max(remove);
            Size::new(width, content + root.padding.vertical())
        };
        Measured {
            size,
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.parts = self.place(cx, bounds);
        self.turn_ms = cx
            .style(StyleQuery::new(ControlKind::Spinner, Slot::Root))
            .motion_ms;
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if self.remove.is_none() || cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let pressed = self.button.pressed;
        let remove = self.parts.remove;
        self.button
            .track(cx, event, |point| remove.contains(point).then_some(()));
        match event {
            ControlEvent::Pointer(PointerEvent::Down { .. }) if self.button.pressed.is_some() => {}
            ControlEvent::Pointer(PointerEvent::Up { .. }) if pressed.is_some() => {
                if self.button.hovered.is_some() {
                    self.remove(cx);
                }
            }
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Space | Key::Enter,
                repeat: false,
            }) => self.remove(cx),
            _ => return EventReply::Continue,
        }
        EventReply::Stop
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        let root = cx.style(self.query(Slot::Root, host));
        let title = cx.style(self.query(Slot::Title, host));
        let detail = cx.style(self.query(Slot::Description, host));
        let bounds = cx.bounds;
        cx.slot(bounds, &root, |_, _| {});
        self.paint_media(cx, host);
        cx.label(self.parts.title, &self.name, &title, TextAlign::Start);
        cx.label(self.parts.detail, &self.detail, &detail, TextAlign::Start);
        if self.state == AttachmentState::Uploading {
            let bar = cx.style(self.query(Slot::Progress, host));
            let rail = cx.style(self.query(Slot::Track, host));
            let track = self.parts.bar;
            fill_area(cx.list, track, cx.rtl, &rail.background);
            let filled = track.size.width * self.progress;
            let x = if cx.rtl {
                track.max_x() - filled
            } else {
                track.origin.x
            };
            fill_area(
                cx.list,
                Rect::from_xywh(x, track.origin.y, filled, track.size.height),
                cx.rtl,
                &bar.background,
            );
        }
        if self.remove.is_some() {
            let state = self
                .button
                .state((), host)
                .set(VisualState::FOCUS_VISIBLE, host.focus_visible());
            let style = cx.style(self.remove_query(state));
            cx.slot(self.parts.remove, &style, |cx, inner| {
                cx.icon(inner, Icon::X, &style);
            });
        }
    }

    fn animating(&self, _time: f64) -> bool {
        self.state == AttachmentState::Processing && self.turn_ms > 0
    }

    fn semantics(&self) -> SemanticNode {
        let actions = if self.enabled && self.remove.is_some() {
            vec![SemanticAction::Dismiss, SemanticAction::Focus]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::Group, &self.name, actions);
        node.value = (!self.detail.is_empty()).then(|| self.detail.clone());
        node.description = self.remove.clone().unwrap_or_default();
        node.state.invalid = self.state == AttachmentState::Error;
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
            SemanticAction::Dismiss if self.remove.is_some() => self.remove(cx),
            SemanticAction::Focus if self.remove.is_some() => cx.request_focus(),
            _ => return false,
        }
        true
    }

    fn focusable(&self) -> bool {
        self.remove.is_some()
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}
