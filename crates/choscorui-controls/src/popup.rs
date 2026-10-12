//! Popup placement, the popup stack (dismissal and focus restoration),
//! and tooltips.
//!
//! Dismissal rules follow macOS: Escape closes the topmost popup; a press
//! outside every open popup closes them all and is consumed (it does not
//! reach the control underneath); deactivating the window closes all
//! popups; closing restores keyboard focus to the popup's owner.

use crate::style::{
    Content, ControlKind, PaintContext, Slot, StyleQuery, Styles, TextAlign, TextMeasure,
};
use choscorui_core::{
    DesignTokens, FillRule, LayoutDirection, NodeId, Paint, Path, Point, Rect, SemanticNode,
    SemanticRole, SemanticState, Size, Tree, TreeError,
};
use std::time::Duration;

/// Preferred side of the anchor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Placement {
    Below,
    Above,
    /// Submenus: beside the anchor, toward the reading direction's end.
    After,
    Before,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupPlacement {
    pub rect: Rect,
    pub placement: Placement,
}

/// Places a popup of `size` next to `anchor` inside `bounds` (the screen's
/// visible frame or the window). The preferred side flips when the popup
/// does not fit there but fits on the other side; the popup is then
/// shifted (never resized) to stay inside `bounds`. In right-to-left
/// layouts start alignment uses the anchor's right edge and `After` means
/// to the left.
pub fn place_popup(
    anchor: Rect,
    size: Size,
    bounds: Rect,
    preferred: Placement,
    direction: LayoutDirection,
) -> PopupPlacement {
    let rtl = direction == LayoutDirection::Rtl;
    let (preferred, opposite) = match (preferred, rtl) {
        (Placement::After, true) => (Placement::Before, Placement::After),
        (Placement::Before, true) => (Placement::After, Placement::Before),
        (Placement::Below, _) => (Placement::Below, Placement::Above),
        (Placement::Above, _) => (Placement::Above, Placement::Below),
        (Placement::After, false) => (Placement::After, Placement::Before),
        (Placement::Before, false) => (Placement::Before, Placement::After),
    };
    let origin_for = |placement: Placement| -> Point {
        let start_x = if rtl {
            anchor.origin.x + anchor.size.width - size.width
        } else {
            anchor.origin.x
        };
        match placement {
            Placement::Below => Point::new(start_x, anchor.origin.y + anchor.size.height),
            Placement::Above => Point::new(start_x, anchor.origin.y - size.height),
            Placement::After => Point::new(anchor.origin.x + anchor.size.width, anchor.origin.y),
            Placement::Before => Point::new(anchor.origin.x - size.width, anchor.origin.y),
        }
    };
    let fits = |placement: Placement| -> bool {
        let origin = origin_for(placement);
        match placement {
            Placement::Below | Placement::Above => {
                origin.y >= bounds.origin.y
                    && origin.y + size.height <= bounds.origin.y + bounds.size.height
            }
            Placement::After | Placement::Before => {
                origin.x >= bounds.origin.x
                    && origin.x + size.width <= bounds.origin.x + bounds.size.width
            }
        }
    };
    let placement = if fits(preferred) || !fits(opposite) {
        preferred
    } else {
        opposite
    };
    let origin = origin_for(placement);
    let clamp = |value: f64, low: f64, extent: f64, high_edge: f64| -> f64 {
        value.min(high_edge - extent).max(low)
    };
    let rect = Rect::new(
        Point::new(
            clamp(
                origin.x,
                bounds.origin.x,
                size.width,
                bounds.origin.x + bounds.size.width,
            ),
            clamp(
                origin.y,
                bounds.origin.y,
                size.height,
                bounds.origin.y + bounds.size.height,
            ),
        ),
        size,
    );
    PopupPlacement { rect, placement }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PopupId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
struct OpenPopup {
    id: PopupId,
    owner: NodeId,
    rect: Rect,
    /// Focus before the popup opened, restored on close.
    restore_focus: Option<NodeId>,
}

/// What a pointer press did to the popup stack.
#[derive(Clone, Debug, PartialEq)]
pub enum PopupPress {
    /// The press landed in this popup; route it there.
    Inside(PopupId),
    /// Popups were closed and the press is consumed.
    Dismissed(Vec<PopupId>),
    /// No popup is open; route normally.
    None,
}

#[derive(Default)]
pub struct PopupStack {
    open: Vec<OpenPopup>,
    next: u64,
}

impl PopupStack {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn is_open(&self) -> bool {
        !self.open.is_empty()
    }
    pub fn top(&self) -> Option<PopupId> {
        self.open.last().map(|popup| popup.id)
    }
    pub fn rect(&self, id: PopupId) -> Option<Rect> {
        self.open
            .iter()
            .find(|popup| popup.id == id)
            .map(|popup| popup.rect)
    }
    pub fn owner(&self, id: PopupId) -> Option<NodeId> {
        self.open
            .iter()
            .find(|popup| popup.id == id)
            .map(|popup| popup.owner)
    }

    /// Opens a popup owned by `owner` (which must exist).
    pub fn open(&mut self, tree: &Tree, owner: NodeId, rect: Rect) -> Result<PopupId, TreeError> {
        if !tree.contains(owner) {
            return Err(TreeError::MissingNode);
        }
        self.next += 1;
        let id = PopupId(self.next);
        self.open.push(OpenPopup {
            id,
            owner,
            rect,
            restore_focus: tree.focus(),
        });
        Ok(id)
    }

    pub fn reposition(&mut self, id: PopupId, rect: Rect) {
        if let Some(popup) = self.open.iter_mut().find(|popup| popup.id == id) {
            popup.rect = rect;
        }
    }

    /// Closes `id` and every popup opened after it, restoring focus to the
    /// focus that was current when `id` opened (if that node still exists).
    pub fn close(&mut self, tree: &mut Tree, id: PopupId) -> Vec<PopupId> {
        let Some(index) = self.open.iter().position(|popup| popup.id == id) else {
            return Vec::new();
        };
        let closed: Vec<_> = self.open.drain(index..).collect();
        if let Some(focus) = closed[0].restore_focus.filter(|node| tree.contains(*node)) {
            let _ = tree.set_focus(Some(focus));
        }
        closed.iter().rev().map(|popup| popup.id).collect()
    }

    pub fn close_all(&mut self, tree: &mut Tree) -> Vec<PopupId> {
        match self.open.first() {
            Some(first) => {
                let id = first.id;
                self.close(tree, id)
            }
            None => Vec::new(),
        }
    }

    /// Escape closes the topmost popup.
    pub fn escape(&mut self, tree: &mut Tree) -> Option<PopupId> {
        let top = self.top()?;
        self.close(tree, top);
        Some(top)
    }

    pub fn press(&mut self, tree: &mut Tree, point: Point) -> PopupPress {
        if self.open.is_empty() {
            return PopupPress::None;
        }
        if let Some(popup) = self
            .open
            .iter()
            .rev()
            .find(|popup| popup.rect.contains(point))
        {
            return PopupPress::Inside(popup.id);
        }
        PopupPress::Dismissed(self.close_all(tree))
    }

    /// Window lost key status or a popup's owner was removed.
    pub fn window_deactivated(&mut self, tree: &mut Tree) -> Vec<PopupId> {
        self.close_all(tree)
    }

    /// Closes popups whose owner left the tree.
    pub fn prune(&mut self, tree: &mut Tree) -> Vec<PopupId> {
        match self
            .open
            .iter()
            .position(|popup| !tree.contains(popup.owner))
        {
            Some(index) => {
                let id = self.open[index].id;
                self.close(tree, id)
            }
            None => Vec::new(),
        }
    }
}

/// Hover delay before a tooltip shows, and how long a second tooltip can
/// follow without delay while moving between controls.
pub const TOOLTIP_DELAY: Duration = Duration::from_millis(600);
pub const TOOLTIP_GRACE: Duration = Duration::from_millis(400);

#[derive(Clone, Debug, PartialEq)]
pub struct VisibleTooltip {
    pub target: NodeId,
    pub text: String,
    pub anchor: Rect,
}

impl VisibleTooltip {
    /// The size of the bubble: the text in the `Tooltip` recipe, at most
    /// its maximum width.
    pub fn size(&self, styles: &Styles, tokens: &DesignTokens, text: &dyn TextMeasure) -> Size {
        let style = styles.resolve(tokens, StyleQuery::new(ControlKind::Tooltip, Slot::Root));
        let size = Content::label(&self.text).size(&style, text);
        Size::new(size.width.min(style.max_width), size.height)
    }

    /// Places the bubble above the anchor, centered, with room for the
    /// arrow. It flips below when it does not fit inside `bounds`.
    pub fn place(
        &self,
        bounds: Rect,
        styles: &Styles,
        tokens: &DesignTokens,
        text: &dyn TextMeasure,
        direction: LayoutDirection,
    ) -> PopupPlacement {
        let size = self.size(styles, tokens, text);
        let arrow = styles.resolve(
            tokens,
            StyleQuery::new(ControlKind::Tooltip, Slot::Indicator),
        );
        let reach = arrow.min_height / 2.0;
        let center = self.anchor.origin.x + self.anchor.size.width / 2.0;
        let anchor = Rect::from_xywh(
            center - size.width / 2.0,
            self.anchor.origin.y - reach,
            size.width,
            self.anchor.size.height + 2.0 * reach,
        );
        place_popup(anchor, size, bounds, Placement::Above, direction)
    }

    /// Paints the bubble in the context's bounds, from
    /// [`VisibleTooltip::place`], with the arrow toward the anchor.
    pub fn paint(&self, cx: &mut PaintContext<'_>, placement: Placement) {
        let query = |slot| StyleQuery::new(ControlKind::Tooltip, slot);
        let (bubble, arrow) = (
            cx.style(query(Slot::Root)),
            cx.style(query(Slot::Indicator)),
        );
        let rect = cx.bounds;
        // The arrow is a triangle half the indicator's height, at the
        // anchor's center and clamped inside the bubble's corners.
        let half = arrow.min_width / 2.0;
        let corner = bubble.radius.top_start;
        let x = (self.anchor.origin.x + self.anchor.size.width / 2.0)
            .clamp(rect.origin.x + corner + half, rect.max_x() - corner - half);
        let reach = arrow.min_height / 2.0;
        let (base, tip) = if placement == Placement::Below {
            (rect.origin.y, rect.origin.y - reach)
        } else {
            (rect.max_y(), rect.max_y() + reach)
        };
        let mut path = Path::new();
        path.move_to(Point::new(x - half, base))
            .line_to(Point::new(x, tip))
            .line_to(Point::new(x + half, base))
            .close();
        let _ = cx
            .list
            .fill_path(path, Paint::Solid(arrow.background), FillRule::NonZero);
        cx.slot(rect, &bubble, |cx, inner| {
            cx.label(inner, &self.text, &bubble, TextAlign::Center);
        });
    }
}

/// Hover tooltips with show delay, warm-up grace between targets, and
/// hiding on press, key, scroll, or leave.
#[derive(Default)]
pub struct TooltipController {
    hover: Option<(NodeId, String, Rect, Duration)>,
    visible: Option<VisibleTooltip>,
    last_hidden: Option<Duration>,
}

impl TooltipController {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn visible(&self) -> Option<&VisibleTooltip> {
        self.visible.as_ref()
    }

    /// Pointer entered or moved within `target`.
    pub fn hover(&mut self, target: NodeId, text: &str, anchor: Rect, now: Duration) {
        if text.is_empty() {
            self.leave(now);
            return;
        }
        if self
            .hover
            .as_ref()
            .is_some_and(|(current, ..)| *current == target)
        {
            return;
        }
        let warm = self.visible.is_some()
            || self
                .last_hidden
                .is_some_and(|hidden| now.saturating_sub(hidden) <= TOOLTIP_GRACE);
        self.visible = None;
        let start = if warm {
            now.saturating_sub(TOOLTIP_DELAY)
        } else {
            now
        };
        self.hover = Some((target, text.to_owned(), anchor, start));
        self.poll(now);
    }

    /// Pointer left every tooltip target.
    pub fn leave(&mut self, now: Duration) {
        if self.visible.take().is_some() {
            self.last_hidden = Some(now);
        }
        self.hover = None;
    }

    /// Press, key, or scroll: hide and do not reshow until the pointer
    /// moves to another target.
    pub fn interrupt(&mut self, now: Duration) {
        if self.visible.take().is_some() {
            self.last_hidden = Some(now);
        }
        if let Some(hover) = &mut self.hover {
            hover.3 = Duration::MAX;
        }
    }

    /// Shows the pending tooltip once its delay passed. Returns the time
    /// at which to poll again, if one is pending.
    pub fn poll(&mut self, now: Duration) -> Option<Duration> {
        let (target, text, anchor, start) = self.hover.as_ref()?;
        if *start == Duration::MAX || self.visible.is_some() {
            return None;
        }
        let due = start.saturating_add(TOOLTIP_DELAY);
        if now >= due {
            self.visible = Some(VisibleTooltip {
                target: *target,
                text: text.clone(),
                anchor: *anchor,
            });
            None
        } else {
            Some(due)
        }
    }

    /// The target that the pointer rests on, whether its tooltip shows,
    /// waits, or was hidden by `interrupt`.
    pub fn target(&self) -> Option<NodeId> {
        self.hover.as_ref().map(|(target, ..)| *target)
    }

    /// The target of the tooltip that waits for its delay, and the time
    /// at which it shows. `None` while a tooltip shows or none waits.
    pub fn pending(&self) -> Option<(NodeId, Duration)> {
        let (target, _, _, start) = self.hover.as_ref()?;
        if *start == Duration::MAX || self.visible.is_some() {
            return None;
        }
        Some((*target, start.saturating_add(TOOLTIP_DELAY)))
    }

    pub fn semantics(&self) -> Option<SemanticNode> {
        let visible = self.visible.as_ref()?;
        Some(SemanticNode {
            role: SemanticRole::Tooltip,
            name: visible.text.clone(),
            description: String::new(),
            value: None,
            state: SemanticState::default(),
            relations: vec![choscorui_core::SemanticRelation::DescribedBy(
                visible.target,
            )],
            actions: Vec::new(),
            text: None,
        })
    }
}
