//! Chat messages: an avatar, a header, bubbles, and a footer.

use crate::Avatar;
use crate::custom::{ControlContext, ControlEvent, CustomControl, MeasureContext};
use crate::form::measured::{measured_layout, paint_wrapped, wrapped_height};
use crate::hosted;
use crate::style::{
    BubbleVariant, ControlKind, PaintContext, Slot, StyleQuery, TextAlign, VisualState,
};
use choscorui_core::{
    EventPhase, EventReply, Measured, PointerEvent, Rect, SemanticNode, SemanticRole, Size,
};

/// One chat bubble: wrapped text in a box of the bubble recipe.
#[derive(Clone, Debug, PartialEq)]
pub struct Bubble {
    text: String,
    variant: BubbleVariant,
    reactions: String,
}

impl Bubble {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            variant: BubbleVariant::Default,
            reactions: String::new(),
        }
    }

    #[must_use]
    pub fn with_variant(mut self, variant: BubbleVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Shows a reactions pill, such as "👍 2", over the bottom end of the
    /// bubble.
    #[must_use]
    pub fn with_reactions(mut self, reactions: impl Into<String>) -> Self {
        self.reactions = reactions.into();
        self
    }

    fn query(&self, slot: Slot) -> StyleQuery {
        StyleQuery::new(ControlKind::Bubble, slot).variant(self.variant)
    }
}

/// Where the parts of one bubble are.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct BubbleLayout {
    pub frame: Rect,
    pub text: Rect,
    pub reactions: Option<Rect>,
}

/// Where the parts of one message are.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MessageLayout {
    pub height: f64,
    pub avatar: Rect,
    pub header: Rect,
    pub bubbles: Vec<BubbleLayout>,
    pub footer: Rect,
}

/// A chat message. The avatar sits at the start, or at the end for
/// `align_end()`, such as the messages of the current user. The header,
/// the bubbles, and the footer stack beside it. A bubble is at most 80%
/// of the content width, except a ghost bubble.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    author: String,
    avatar: Option<Avatar>,
    header: String,
    footer: String,
    bubbles: Vec<Bubble>,
    end: bool,
    layout: MessageLayout,
    hovered: Option<usize>,
}

impl Message {
    /// A message from `author`. The author names the message for
    /// assistive apps.
    pub fn new(author: impl Into<String>, bubbles: Vec<Bubble>) -> Self {
        Self {
            author: author.into(),
            avatar: None,
            header: String::new(),
            footer: String::new(),
            bubbles,
            end: false,
            layout: MessageLayout::default(),
            hovered: None,
        }
    }

    #[must_use]
    pub fn with_avatar(mut self, avatar: Avatar) -> Self {
        self.avatar = Some(avatar);
        self
    }

    /// Text above the bubbles, such as the author and the time.
    #[must_use]
    pub fn with_header(mut self, header: impl Into<String>) -> Self {
        self.header = header.into();
        self
    }

    /// Text below the bubbles, such as a read receipt.
    #[must_use]
    pub fn with_footer(mut self, footer: impl Into<String>) -> Self {
        self.footer = footer.into();
        self
    }

    /// Aligns the message to the end.
    #[must_use]
    pub fn align_end(mut self) -> Self {
        self.end = true;
        self
    }

    fn query(slot: Slot) -> StyleQuery {
        StyleQuery::new(ControlKind::Message, slot)
    }

    /// The bubble texts, for assistive apps.
    pub(crate) fn text(&self) -> String {
        self.bubbles
            .iter()
            .map(|bubble| bubble.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub(crate) fn author(&self) -> &str {
        &self.author
    }

    /// Lays the message out across `rect`; the height of `rect` is
    /// ignored.
    pub(crate) fn layout(&self, cx: &MeasureContext<'_>, rect: Rect) -> MessageLayout {
        let root = cx.style(Self::query(Slot::Root));
        let content = cx.style(Self::query(Slot::Content));
        let group = cx.style(Self::query(Slot::Group));
        let header = cx.style(Self::query(Slot::Header));
        let footer = cx.style(Self::query(Slot::Footer));
        let side = self.avatar.as_ref().map_or(0.0, |avatar| avatar.side(cx));
        let avatar_used = if self.avatar.is_some() {
            side + root.gap
        } else {
            0.0
        };
        let width = (rect.size.width - avatar_used).max(1.0);
        // Parts go at the right for an end message in a left-to-right
        // layout, and for a start message in a right-to-left one.
        let right = self.end != cx.rtl();
        let start_x = if right {
            rect.origin.x
        } else {
            rect.origin.x + avatar_used
        };
        let place = |part: f64| {
            if right {
                start_x + width - part
            } else {
                start_x
            }
        };
        let mut y = rect.origin.y;
        let mut gap = 0.0;
        let line = |height: f64, y: &mut f64, gap: &mut f64| {
            *y += *gap;
            let top = *y;
            *y += height;
            *gap = content.gap;
            top
        };
        let header_rect = if self.header.is_empty() {
            Rect::from_xywh(start_x, y, width, 0.0)
        } else {
            let top = line(header.text.line_height, &mut y, &mut gap);
            Rect::from_xywh(start_x, top, width, header.text.line_height)
        };
        let mut bubbles = Vec::new();
        let mut first_bubble = None;
        for (index, bubble) in self.bubbles.iter().enumerate() {
            let frame_style = cx.style(bubble.query(Slot::Root));
            let style = cx.style(bubble.query(Slot::Content));
            let max = if frame_style.max_width.is_finite() {
                width * frame_style.max_width
            } else {
                width
            };
            let text_max = (max - style.padding.horizontal()).max(1.0);
            let lines = measured_layout(&bubble.text, &style.text, text_max, cx.text);
            let natural = lines
                .lines()
                .iter()
                .map(|line| line.width)
                .fold(0.0, f64::max);
            let text_width = natural.ceil().min(text_max);
            let text_height = wrapped_height(&bubble.text, &style.text, text_max, cx.text);
            let frame_size = Size::new(
                text_width + style.padding.horizontal(),
                text_height + style.padding.vertical(),
            );
            if index > 0 {
                gap = group.gap;
            }
            let top = line(frame_size.height, &mut y, &mut gap);
            let frame = Rect::from_xywh(
                place(frame_size.width),
                top,
                frame_size.width,
                frame_size.height,
            );
            first_bubble.get_or_insert(frame);
            let text = Rect::from_xywh(
                frame.origin.x + style.padding.inline_start,
                frame.origin.y + style.padding.block_start,
                text_width,
                text_height,
            );
            let reactions = (!bubble.reactions.is_empty()).then(|| {
                let pill = cx.style(bubble.query(Slot::Badge));
                let size = Size::new(
                    cx.text.width(&bubble.reactions, &pill.text) + pill.padding.horizontal(),
                    pill.text.line_height + pill.padding.vertical(),
                );
                // The pill hangs half below the bubble, inset from its end.
                let inset = style.padding.inline_end;
                let x = if right {
                    frame.origin.x + inset
                } else {
                    frame.max_x() - inset - size.width
                };
                y += size.height / 2.0;
                Rect::from_xywh(
                    x,
                    frame.max_y() - size.height / 2.0,
                    size.width,
                    size.height,
                )
            });
            bubbles.push(BubbleLayout {
                frame,
                text,
                reactions,
            });
        }
        let footer_rect = if self.footer.is_empty() {
            Rect::from_xywh(start_x, y, width, 0.0)
        } else {
            gap = content.gap;
            let top = line(footer.text.line_height, &mut y, &mut gap);
            Rect::from_xywh(start_x, top, width, footer.text.line_height)
        };
        let height = (y - rect.origin.y).max(side);
        let avatar_x = if right {
            rect.max_x() - side
        } else {
            rect.origin.x
        };
        let avatar_y = first_bubble.map_or(rect.origin.y, |frame| frame.origin.y);
        MessageLayout {
            height,
            avatar: Rect::from_xywh(avatar_x, avatar_y, side, side),
            header: header_rect,
            bubbles,
            footer: footer_rect,
        }
    }

    /// Paints the message with `layout`. `hovered` is the bubble under
    /// the pointer.
    pub(crate) fn paint_layout(
        &self,
        cx: &mut PaintContext<'_>,
        layout: &MessageLayout,
        hovered: Option<usize>,
    ) {
        let header = cx.style(Self::query(Slot::Header));
        let align = if self.end {
            TextAlign::End
        } else {
            TextAlign::Start
        };
        if let Some(avatar) = &self.avatar {
            avatar.paint_at(cx, layout.avatar);
        }
        cx.slot(layout.header, &header, |cx, inner| {
            cx.label(inner, &self.header, &header, align);
        });
        for (index, (bubble, parts)) in self.bubbles.iter().zip(&layout.bubbles).enumerate() {
            let state = VisualState::NONE.set(VisualState::HOVERED, hovered == Some(index));
            let style = cx.style(bubble.query(Slot::Content).state(state));
            cx.slot(parts.frame, &style, |_, _| {});
            paint_wrapped(cx, parts.text, &bubble.text, &style, TextAlign::Start);
            if let Some(pill) = parts.reactions {
                let badge = cx.style(bubble.query(Slot::Badge));
                cx.slot(pill, &badge, |cx, inner| {
                    cx.label(inner, &bubble.reactions, &badge, TextAlign::Center);
                });
            }
        }
        let footer = cx.style(Self::query(Slot::Footer));
        cx.slot(layout.footer, &footer, |cx, inner| {
            cx.label(inner, &self.footer, &footer, align);
        });
    }

    /// The bubble under `point` in `layout`.
    pub(crate) fn bubble_at(layout: &MessageLayout, point: choscorui_core::Point) -> Option<usize> {
        layout
            .bubbles
            .iter()
            .position(|bubble| bubble.frame.contains(point))
    }
}

impl CustomControl for Message {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let max = cx.constraints.max.width;
        // An unbounded message takes the width of a medium container.
        let width = if max.is_finite() {
            max
        } else {
            cx.tokens.spacing * 120.0
        };
        let layout = self.layout(cx, Rect::from_xywh(0.0, 0.0, width, 0.0));
        Measured {
            size: Size::new(width, layout.height),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.layout = self.layout(cx, bounds);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let hovered = match event {
            ControlEvent::Pointer(PointerEvent::Move { position }) => {
                Self::bubble_at(&self.layout, *position)
            }
            ControlEvent::Hover(false) => None,
            _ => return EventReply::Continue,
        };
        if hovered != self.hovered {
            self.hovered = hovered;
            cx.request_paint();
        }
        EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        self.paint_layout(cx, &self.layout, self.hovered);
    }

    fn semantics(&self) -> SemanticNode {
        let mut node = hosted::node(SemanticRole::Group, &self.author, Vec::new());
        node.value = Some(self.text());
        node.description = [self.header.as_str(), self.footer.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        node
    }

    fn focusable(&self) -> bool {
        false
    }
}
