//! A round user image with a text fallback, and overlapping groups of
//! them.

use crate::custom::{ControlContext, ControlEvent, CustomControl, MeasureContext};
use crate::hosted;
use crate::style::{
    ControlKind, ControlSize, PaintContext, ResolvedStyle, Slot, StyleQuery, TextAlign, TextLine,
    TextOverflow, draw_text,
};
use choscorui_core::{
    EventReply, Icon, Image, ImageSampling, Measured, Path, Rect, SemanticNode, SemanticRole, Size,
};

/// A round avatar. It shows the image when there is one, and the
/// fallback text, such as initials, on a muted circle when there is
/// not. An optional badge sits at the bottom end. The avatar takes no
/// focus; its accessible name is the person's name.
#[derive(Clone, Debug, PartialEq)]
pub struct Avatar {
    name: String,
    fallback: String,
    image: Option<Image>,
    size: ControlSize,
    /// `Some` shows the badge, with the icon when there is one.
    badge: Option<Option<Icon>>,
}

impl Avatar {
    pub fn new(name: impl Into<String>, fallback: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            fallback: fallback.into(),
            image: None,
            size: ControlSize::Default,
            badge: None,
        }
    }

    #[must_use]
    pub fn with_image(mut self, image: Image) -> Self {
        self.image = Some(image);
        self
    }

    /// Sets the size: `Sm`, `Default`, or `Lg`.
    #[must_use]
    pub fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// Shows a badge with an optional icon. The small size has no room
    /// for the icon.
    #[must_use]
    pub fn with_badge(mut self, icon: Option<Icon>) -> Self {
        self.badge = Some(icon);
        self
    }

    fn query(size: ControlSize, slot: Slot) -> StyleQuery {
        StyleQuery::new(ControlKind::Avatar, slot).size(size)
    }

    /// The diameter of this avatar.
    pub(crate) fn side(&self, cx: &MeasureContext<'_>) -> f64 {
        Self::diameter(cx, self.size)
    }

    fn diameter(cx: &MeasureContext<'_>, size: ControlSize) -> f64 {
        cx.style(Self::query(size, Slot::Root)).min_width
    }

    /// Paints the avatar in the circle `rect`.
    pub(crate) fn paint_at(&self, cx: &mut PaintContext<'_>, rect: Rect) {
        match &self.image {
            Some(image) => {
                if cx.list.push_clip_path(Path::ellipse(rect)).is_ok() {
                    let _ = cx
                        .list
                        .draw_image(image.clone(), rect, ImageSampling::Linear);
                    let _ = cx.list.pop_clip();
                }
            }
            None => {
                let style = cx.style(Self::query(self.size, Slot::Fallback));
                cx.slot(rect, &style, |cx, inner| {
                    fitted_label(cx, inner, &self.fallback, &style);
                });
            }
        }
        if let Some(icon) = self.badge {
            let style = cx.style(Self::query(self.size, Slot::Badge));
            let side = style.min_width;
            // The ring stays inside the avatar, where the host clip keeps it.
            let room = style.ring.map_or(0.0, |ring| ring.width.max(0.0));
            let x = if cx.rtl {
                rect.origin.x + room
            } else {
                rect.max_x() - side - room
            };
            let badge = Rect::from_xywh(x, rect.max_y() - side - room, side, side);
            cx.slot(badge, &style, |cx, inner| {
                if let Some(icon) = icon.filter(|_| style.icon_size > 0.0) {
                    cx.icon(inner, icon, &style);
                }
            });
        }
    }
}

impl CustomControl for Avatar {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let side = Self::diameter(cx, self.size);
        Measured {
            size: Size::new(side, side),
            baseline: None,
        }
    }

    fn event(&mut self, _cx: &mut ControlContext<'_>, _event: &ControlEvent) -> EventReply {
        EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let side = cx.style(Self::query(self.size, Slot::Root)).min_width;
        let bounds = cx.bounds;
        let rect = Rect::from_xywh(bounds.origin.x, bounds.origin.y, side, side);
        self.paint_at(cx, rect);
    }

    fn semantics(&self) -> SemanticNode {
        hosted::node(SemanticRole::Image, &self.name, Vec::new())
    }

    fn focusable(&self) -> bool {
        false
    }
}

/// Avatars that overlap in reading order, each with a ring in the
/// background color, and an optional count of the people not shown.
#[derive(Clone, Debug, PartialEq)]
pub struct AvatarGroup {
    name: String,
    avatars: Vec<Avatar>,
    count: Option<String>,
    size: ControlSize,
}

impl AvatarGroup {
    /// A group of `avatars`, all shown at the group size.
    pub fn new(name: impl Into<String>, avatars: Vec<Avatar>) -> Self {
        Self {
            name: name.into(),
            avatars,
            count: None,
            size: ControlSize::Default,
        }
    }

    /// Ends the group with a circle that shows `count`, such as `+3`.
    #[must_use]
    pub fn with_count(mut self, count: impl Into<String>) -> Self {
        self.count = Some(count.into());
        self
    }

    /// Sets the size of every avatar: `Sm`, `Default`, or `Lg`.
    #[must_use]
    pub fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self.avatars
            .iter_mut()
            .for_each(|avatar| avatar.size = size);
        self
    }

    fn circles(&self) -> usize {
        self.avatars.len() + usize::from(self.count.is_some())
    }

    /// The room around the circles for the ring that each one has, so
    /// that the host clip keeps the whole ring.
    fn ring_room(root: &ResolvedStyle) -> f64 {
        root.ring.map_or(0.0, |ring| ring.width.max(0.0))
    }
}

impl CustomControl for AvatarGroup {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let root = cx.style(Avatar::query(self.size, Slot::Root));
        let count = self.circles();
        let width = root.min_width * count as f64 + root.gap * count.saturating_sub(1) as f64;
        let room = 2.0 * Self::ring_room(&root);
        Measured {
            size: Size::new(width.max(0.0) + room, root.min_height + room),
            baseline: None,
        }
    }

    fn event(&mut self, _cx: &mut ControlContext<'_>, _event: &ControlEvent) -> EventReply {
        EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let root = cx.style(Avatar::query(self.size, Slot::Root));
        let side = root.min_width;
        let bounds = cx.bounds.inset(Self::ring_room(&root));
        let step = side + root.gap;
        let circle = |index: usize, rtl: bool| {
            let offset = index as f64 * step;
            let x = if rtl {
                bounds.max_x() - side - offset
            } else {
                bounds.origin.x + offset
            };
            Rect::from_xywh(x, bounds.origin.y, side, side)
        };
        let ring = root.ring.filter(|ring| ring.width > 0.0);
        let rtl = cx.rtl;
        for (index, avatar) in self.avatars.iter().enumerate() {
            let rect = circle(index, rtl);
            if let Some(ring) = ring {
                let _ = cx.list.draw_ring(rect, side / 2.0, ring.width, ring.color);
            }
            avatar.paint_at(cx, rect);
        }
        if let Some(count) = &self.count {
            let rect = circle(self.avatars.len(), rtl);
            let mut style = cx.style(Avatar::query(self.size, Slot::Group));
            style.ring = ring;
            cx.slot(rect, &style, |cx, inner| {
                fitted_label(cx, inner, count, &style);
            });
        }
    }

    fn semantics(&self) -> SemanticNode {
        let mut node = hosted::node(SemanticRole::Group, &self.name, Vec::new());
        let mut names: Vec<&str> = self.avatars.iter().map(|a| a.name.as_str()).collect();
        if let Some(count) = &self.count {
            names.push(count);
        }
        node.value = Some(names.join(", "));
        node
    }

    fn focusable(&self) -> bool {
        false
    }
}

/// Draws `text` centered in `rect`, in a smaller type size when it is
/// wider than `rect`, and clipped at the edges of `rect`. A circle shows
/// initials whole, so they never truncate.
fn fitted_label(cx: &mut PaintContext<'_>, rect: Rect, text: &str, style: &ResolvedStyle) {
    let mut style = style.clone();
    let width = cx.text.width(text, &style.text);
    if width > rect.size.width && width > 0.0 {
        let scale = rect.size.width / width;
        style.text.size *= scale;
        style.text.line_height *= scale;
        style.text.tracking *= scale;
    }
    let line = TextLine::new(text, &style, cx.rtl)
        .align(TextAlign::Center)
        .overflow(TextOverflow::Clip);
    let _ = draw_text(cx.list, rect, &line, cx.text);
}
