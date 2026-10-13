//! Item rows: an optional header line, media, a title and a description,
//! actions, and an optional footer line, stacked in a group.

use crate::Avatar;
use crate::custom::{ControlContext, ControlEvent, ControlMessage, CustomControl, MeasureContext};
use crate::form::measured::{paint_wrapped, wrapped_height};
use crate::hosted::{self, Parts};
use crate::overlay::panel::{Buttons, PanelButton};
use crate::style::fill_area;
use crate::style::{
    ButtonVariant, ControlKind, ControlSize, ItemVariant, PaintContext, Slot, StyleQuery,
    TextAlign, Variant, VisualState,
};
use choscorui_core::{
    EventPhase, EventReply, Icon, Image, ImageSampling, Key, KeyEvent, Measured, Path, Point,
    PointerButton, PointerEvent, Rect, SemanticAction, SemanticNode, SemanticRole, Size,
};

/// What an item shows at its start.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemMedia {
    Icon(Icon),
    /// An image in a rounded square.
    Image(Image),
    Avatar(Avatar),
}

impl ItemMedia {
    /// The media variant of the item recipe: default, icon, or image.
    fn variant(&self) -> Variant {
        match self {
            Self::Avatar(_) => Variant(0),
            Self::Icon(_) => Variant(1),
            Self::Image(_) => Variant(2),
        }
    }
}

/// One item row. Mount items through an [`ItemGroup`].
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    title: String,
    description: String,
    header: String,
    footer: String,
    media: Option<ItemMedia>,
    actions: Vec<PanelButton>,
    link: bool,
    variant: ItemVariant,
    size: ControlSize,
}

impl Item {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: String::new(),
            header: String::new(),
            footer: String::new(),
            media: None,
            actions: Vec::new(),
            link: false,
            variant: ItemVariant::Default,
            size: ControlSize::Default,
        }
    }

    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Adds a line across the full width of the row, above the media and
    /// the title.
    #[must_use]
    pub fn with_header(mut self, header: impl Into<String>) -> Self {
        self.header = header.into();
        self
    }

    /// Adds a line across the full width of the row, below the media and
    /// the title.
    #[must_use]
    pub fn with_footer(mut self, footer: impl Into<String>) -> Self {
        self.footer = footer.into();
        self
    }

    /// The text of the row for assistive technology: the header, the
    /// title, and the footer.
    fn spoken(&self) -> String {
        [&self.header, &self.title, &self.footer]
            .into_iter()
            .filter(|text| !text.is_empty())
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The height of the header or footer `text` in `slot` at `width`, or
    /// zero without text.
    fn edge_height(&self, cx: &MeasureContext<'_>, slot: Slot, text: &str, width: f64) -> f64 {
        if text.is_empty() {
            return 0.0;
        }
        let style = cx.style(self.query(slot));
        wrapped_height(text, &style.text, width, cx.text)
    }

    #[must_use]
    pub fn with_media(mut self, media: ItemMedia) -> Self {
        self.media = Some(media);
        self
    }

    /// Adds an action button at the end.
    #[must_use]
    pub fn with_action(mut self, label: impl Into<String>, variant: ButtonVariant) -> Self {
        self.actions.push(PanelButton {
            label: label.into(),
            variant,
        });
        self
    }

    /// Makes the whole row a link with a chevron at the end. A link row
    /// has no action buttons.
    #[must_use]
    pub fn as_link(mut self) -> Self {
        self.link = true;
        self.actions.clear();
        self
    }

    #[must_use]
    pub fn with_variant(mut self, variant: ItemVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Sets the size: `Default`, `Sm`, or `Xs`.
    #[must_use]
    pub fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    fn query(&self, slot: Slot) -> StyleQuery {
        StyleQuery::new(ControlKind::Item, slot)
            .size(self.size)
            .variant(self.variant)
    }

    fn media_query(&self) -> Option<StyleQuery> {
        let media = self.media.as_ref()?;
        Some(
            StyleQuery::new(ControlKind::Item, Slot::Media)
                .size(self.size)
                .variant(media.variant()),
        )
    }

    fn media_size(&self, cx: &MeasureContext<'_>) -> Size {
        let Some(query) = self.media_query() else {
            return Size::new(0.0, 0.0);
        };
        let style = cx.style(query);
        match &self.media {
            Some(ItemMedia::Icon(_)) => Size::new(style.icon_size, style.icon_size),
            Some(ItemMedia::Image(_)) => Size::new(style.min_width, style.min_height),
            Some(ItemMedia::Avatar(avatar)) => {
                let side = avatar.side(cx);
                Size::new(side, side)
            }
            None => Size::new(0.0, 0.0),
        }
    }

    /// The width of the end part: the action buttons or the chevron.
    fn end_width(&self, cx: &MeasureContext<'_>) -> f64 {
        let action = cx.style(self.query(Slot::Action));
        if self.link {
            return cx.style(self.query(Slot::Root)).icon_size.max(16.0);
        }
        let buttons: f64 = self
            .actions
            .iter()
            .map(|button| Buttons::width(cx, button))
            .sum();
        buttons + action.gap * self.actions.len().saturating_sub(1) as f64
    }

    fn text_heights(&self, cx: &MeasureContext<'_>, width: f64) -> (f64, f64, f64) {
        let title = cx.style(self.query(Slot::Title));
        let description = cx.style(self.query(Slot::Description));
        let content = cx.style(self.query(Slot::Content));
        let title_height = wrapped_height(&self.title, &title.text, width, cx.text);
        let description_height =
            wrapped_height(&self.description, &description.text, width, cx.text);
        let gap = if title_height > 0.0 && description_height > 0.0 {
            content.gap
        } else {
            0.0
        };
        (title_height, gap, description_height)
    }

    /// Lays the row out in `rect`, which has the row's width; the height
    /// is ignored.
    fn layout(&self, cx: &MeasureContext<'_>, rect: Rect) -> RowLayout {
        let root = cx.style(self.query(Slot::Root));
        let rtl = cx.rtl();
        let inner_width = (rect.size.width - root.padding.horizontal()).max(0.0);
        let media = self.media_size(cx);
        let media_used = if self.media.is_some() {
            media.width + root.gap
        } else {
            0.0
        };
        let end = self.end_width(cx);
        let end_used = if end > 0.0 { end + root.gap } else { 0.0 };
        let text_width = (inner_width - media_used - end_used).max(1.0);
        let (title, gap, description) = self.text_heights(cx, text_width);
        let text_height = title + gap + description;
        let buttons = if self.actions.is_empty() {
            0.0
        } else {
            Buttons::button_height(cx)
        };
        let content = text_height.max(media.height).max(buttons);
        let header = self.edge_height(cx, Slot::Header, &self.header, inner_width);
        let footer = self.edge_height(cx, Slot::Footer, &self.footer, inner_width);
        // A header or footer sits one row gap away from the main row.
        let header_used = if header > 0.0 { header + root.gap } else { 0.0 };
        let footer_used = if footer > 0.0 { footer + root.gap } else { 0.0 };
        let height = header_used + content + footer_used + root.padding.vertical();
        let top = rect.origin.y + root.padding.block_start;
        let inner = Rect::from_xywh(
            rect.origin.x + root.padding.inline_start,
            top + header_used,
            inner_width,
            content,
        );
        let header_rect = Rect::from_xywh(inner.origin.x, top, inner_width, header);
        let footer_rect = Rect::from_xywh(
            inner.origin.x,
            inner.max_y() + footer_used - footer,
            inner_width,
            footer,
        );
        // Places a part of `width` at `offset` from the start of `inner`.
        let at = |offset: f64, width: f64, height: f64| {
            let x = if rtl {
                inner.max_x() - offset - width
            } else {
                inner.origin.x + offset
            };
            Rect::from_xywh(x, inner.center().y - height / 2.0, width, height)
        };
        let media_rect = at(0.0, media.width, media.height);
        let text = at(media_used, text_width, text_height);
        let end_rect = at(
            inner_width - end,
            end,
            buttons.max(root.icon_size.max(16.0)),
        );
        let action = cx.style(self.query(Slot::Action));
        let mut row = Buttons {
            buttons: self.actions.clone(),
            rects: Vec::new(),
        };
        row.place_row(cx, end_rect, action.gap);
        RowLayout {
            rect: Rect::from_xywh(rect.origin.x, rect.origin.y, rect.size.width, height),
            header: header_rect,
            footer: footer_rect,
            media: media_rect,
            text,
            title,
            gap,
            end: end_rect,
            buttons: row.rects,
        }
    }
}

/// Where the parts of one row are.
#[derive(Clone, Debug, Default, PartialEq)]
struct RowLayout {
    rect: Rect,
    header: Rect,
    footer: Rect,
    media: Rect,
    text: Rect,
    title: f64,
    gap: f64,
    end: Rect,
    buttons: Vec<Rect>,
}

/// A tab stop: a link row, or an action of a row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Stop {
    item: usize,
    action: Option<usize>,
}

/// Item rows stacked with the group gap, or with separators between them.
/// Link rows and action buttons are tab stops, in order. Enter, Space, or
/// a click on a stop emits `ItemActivated(stop)`.
pub struct ItemGroup {
    name: String,
    items: Vec<Item>,
    separators: bool,
    rows: Vec<RowLayout>,
    focus: usize,
    /// Hover and press over the stops, by stop index.
    parts: Parts,
    enabled: bool,
}

impl ItemGroup {
    pub fn new(name: impl Into<String>, items: Vec<Item>) -> Self {
        Self {
            name: name.into(),
            items,
            separators: false,
            rows: Vec::new(),
            focus: 0,
            parts: Parts::default(),
            enabled: true,
        }
    }

    /// Draws a separator between rows.
    #[must_use]
    pub fn with_separators(mut self) -> Self {
        self.separators = true;
        self
    }

    fn stops(&self) -> Vec<Stop> {
        self.items
            .iter()
            .enumerate()
            .flat_map(|(item, row)| {
                let link = row.link.then_some(Stop { item, action: None });
                let actions = (0..row.actions.len()).map(move |action| Stop {
                    item,
                    action: Some(action),
                });
                link.into_iter().chain(actions)
            })
            .collect()
    }

    /// The space between two rows.
    fn spacing(&self, cx: &MeasureContext<'_>) -> f64 {
        let size = self
            .items
            .first()
            .map_or(ControlSize::Default, |item| item.size);
        if self.separators {
            let separator = cx.style(StyleQuery::new(ControlKind::Item, Slot::Separator));
            separator.min_height + separator.padding.vertical()
        } else {
            cx.style(StyleQuery::new(ControlKind::Item, Slot::Group).size(size))
                .gap
        }
    }

    fn layout_rows(&self, cx: &MeasureContext<'_>, origin: Point, width: f64) -> Vec<RowLayout> {
        let spacing = self.spacing(cx);
        let mut y = origin.y;
        self.items
            .iter()
            .map(|item| {
                let row = item.layout(cx, Rect::from_xywh(origin.x, y, width, 0.0));
                y = row.rect.max_y() + spacing;
                row
            })
            .collect()
    }

    fn stop_at(&self, point: Point) -> Option<Stop> {
        for (item, row) in self.rows.iter().enumerate() {
            if let Some(action) = hosted::hit(&row.buttons, point) {
                return Some(Stop {
                    item,
                    action: Some(action),
                });
            }
            if self.items[item].link && row.rect.contains(point) {
                return Some(Stop { item, action: None });
            }
        }
        None
    }

    fn activate(&mut self, cx: &mut ControlContext<'_>, stop: Stop) {
        if let Some(index) = self.stops().iter().position(|other| *other == stop) {
            cx.emit(ControlMessage::ItemActivated(index));
        }
    }

    fn label(&self, stop: Stop) -> String {
        let item = &self.items[stop.item];
        match stop.action {
            Some(action) => item.actions[action].label.clone(),
            None => item.title.clone(),
        }
    }

    fn paint_row(
        &self,
        cx: &mut PaintContext<'_>,
        index: usize,
        focused: Option<Stop>,
        stops: &[Stop],
    ) {
        let item = &self.items[index];
        let row = &self.rows[index];
        let host = cx.state;
        let link = Stop {
            item: index,
            action: None,
        };
        let link_state = stops.iter().position(|stop| *stop == link).map_or(
            VisualState::NONE.set(VisualState::DISABLED, host.disabled()),
            |stop| self.parts.state(stop, host),
        );
        let state = link_state.set(VisualState::HIGHLIGHTED, item.link).set(
            VisualState::FOCUS_VISIBLE,
            host.focus_visible() && focused == Some(link),
        );
        let root = cx.style(item.query(Slot::Root).state(state));
        let title = cx.style(item.query(Slot::Title));
        let description = cx.style(item.query(Slot::Description));
        let (header, footer) = (
            cx.style(item.query(Slot::Header)),
            cx.style(item.query(Slot::Footer)),
        );
        cx.slot(row.rect, &root, |cx, _| {
            paint_wrapped(cx, row.header, &item.header, &header, TextAlign::Start);
            paint_wrapped(cx, row.footer, &item.footer, &footer, TextAlign::Start);
            if let (Some(media), Some(query)) = (&item.media, item.media_query()) {
                let style = cx.style(query);
                match media {
                    ItemMedia::Icon(icon) => cx.icon(row.media, *icon, &style),
                    ItemMedia::Image(image) => {
                        let clip = Path::rounded_rect(row.media, style.radius.top_start);
                        if cx.list.push_clip_path(clip).is_ok() {
                            let _ =
                                cx.list
                                    .draw_image(image.clone(), row.media, ImageSampling::Linear);
                            let _ = cx.list.pop_clip();
                        }
                    }
                    ItemMedia::Avatar(avatar) => avatar.paint_at(cx, row.media),
                }
            }
            let title_rect = Rect::from_xywh(
                row.text.origin.x,
                row.text.origin.y,
                row.text.size.width,
                row.title,
            );
            paint_wrapped(cx, title_rect, &item.title, &title, TextAlign::Start);
            let description_rect = Rect::from_xywh(
                row.text.origin.x,
                title_rect.max_y() + row.gap,
                row.text.size.width,
                row.text.max_y() - title_rect.max_y() - row.gap,
            );
            paint_wrapped(
                cx,
                description_rect,
                &item.description,
                &description,
                TextAlign::Start,
            );
            if item.link {
                let icon = if cx.rtl {
                    Icon::ChevronLeft
                } else {
                    Icon::ChevronRight
                };
                let style = cx.style(item.query(Slot::Description));
                let mut chevron = style;
                chevron.icon_size = row.end.size.width;
                cx.icon(row.end, icon, &chevron);
            }
            let buttons = Buttons {
                buttons: item.actions.clone(),
                rects: row.buttons.clone(),
            };
            let focused_action = focused
                .filter(|stop| stop.item == index)
                .and_then(|stop| stop.action);
            let parts = self.parts.map(|stop| {
                stops
                    .get(stop)
                    .filter(|stop| stop.item == index)
                    .and_then(|stop| stop.action)
            });
            buttons.paint(cx, focused_action, &parts, host.focus_visible());
        });
    }
}

impl CustomControl for ItemGroup {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let max = cx.constraints.max.width;
        // An unbounded group takes the width of a medium container.
        let width = if max.is_finite() {
            max
        } else {
            cx.tokens.spacing * 120.0
        };
        let rows = self.layout_rows(cx, Point::new(0.0, 0.0), width);
        let height = rows.last().map_or(0.0, |row| row.rect.max_y());
        Measured {
            size: Size::new(width, height),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.rows = self.layout_rows(cx, bounds.origin, bounds.size.width);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let stops = self.stops();
        let pressed = self.parts.pressed;
        let mut parts = self.parts;
        parts.track(cx, event, |point| {
            let stop = self.stop_at(point)?;
            stops.iter().position(|other| *other == stop)
        });
        self.parts = parts;
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                button: PointerButton::Primary,
                ..
            }) => {
                let Some(index) = self.parts.pressed else {
                    return EventReply::Continue;
                };
                self.focus = index;
                cx.request_focus();
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Up { .. }) if pressed.is_some() => {
                let activated = pressed.filter(|index| self.parts.hovered == Some(*index));
                if let Some(stop) = activated.and_then(|index| stops.get(index).copied()) {
                    self.activate(cx, stop);
                }
                EventReply::Stop
            }
            ControlEvent::Key(KeyEvent::Down { key, repeat }) => match key {
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
                    if let Some(stop) = stops.get(self.focus).copied() {
                        self.activate(cx, stop);
                    }
                    EventReply::Stop
                }
                _ => EventReply::Continue,
            },
            ControlEvent::FocusChanged(focused) => {
                if *focused && self.parts.pressed.is_none() {
                    self.focus = if cx.modifiers().shift {
                        stops.len().saturating_sub(1)
                    } else {
                        0
                    };
                }
                cx.request_paint();
                cx.request_semantics();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let stops = self.stops();
        let focused = stops.get(self.focus).copied();
        let separator = cx.style(StyleQuery::new(ControlKind::Item, Slot::Separator));
        for index in 0..self.rows.len() {
            self.paint_row(cx, index, focused, &stops);
            if self.separators && index + 1 < self.rows.len() {
                let row = self.rows[index].rect;
                let line = Rect::from_xywh(
                    row.origin.x,
                    row.max_y() + separator.padding.block_start,
                    row.size.width,
                    separator.min_height,
                );
                fill_area(cx.list, line, cx.rtl, &separator.background);
            }
        }
    }

    fn semantics(&self) -> SemanticNode {
        let stops = self.stops();
        let actions = if self.enabled && !stops.is_empty() {
            vec![SemanticAction::Activate, SemanticAction::Focus]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::List, &self.name, actions);
        node.value = Some(
            self.items
                .iter()
                .map(Item::spoken)
                .collect::<Vec<_>>()
                .join(", "),
        );
        node.description = stops
            .get(self.focus)
            .map(|stop| self.label(*stop))
            .unwrap_or_default();
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
            SemanticAction::Activate => match self.stops().get(self.focus).copied() {
                Some(stop) => {
                    self.activate(cx, stop);
                    true
                }
                None => false,
            },
            SemanticAction::Focus if self.focusable() => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn focusable(&self) -> bool {
        !self.stops().is_empty()
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}
