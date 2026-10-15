//! The geometry of a window with client-side decorations: which part of
//! the window a point is on, and the painted frame with its shadow.

use super::{TitleBarButton, TitleBarRequest};
use crate::style::{ControlKind, PaintContext, Slot, StyleQuery, Styles, VisualState};
use choscorui_core::{DesignTokens, Insets, Point, Rect};

/// An edge or a corner of a window that an interactive resize moves.
/// The sides are physical: a right-to-left layout does not mirror them.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ResizeEdge {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// The part of a decorated window under a point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowRegion {
    /// Outside the window and its resize band, for example in the shadow:
    /// the input goes to the window below.
    Outside,
    /// The resize band of an edge or a corner.
    Resize(ResizeEdge),
    /// The title bar outside its buttons: a press there moves the window.
    TitleBar,
    /// A window button of the title bar.
    Button(TitleBarButton),
    /// The content of the window.
    Client,
}

impl WindowRegion {
    /// The request that a primary press in the region starts at once:
    /// `Resize` in a resize band, and nothing in the other regions. The
    /// title bar makes the requests of its own regions.
    pub fn press_request(self) -> Option<TitleBarRequest> {
        match self {
            Self::Resize(edge) => Some(TitleBarRequest::Resize(edge)),
            _ => None,
        }
    }
}

/// The frame of a window with client-side decorations, in surface
/// coordinates. The surface can be larger than the visible frame, so
/// that it holds the shadow and the outer resize band.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowFrame {
    /// The visible window, with the title bar at its top.
    pub frame: Rect,
    /// The title bar, from the facade's layout.
    pub title_bar: Rect,
    /// The window buttons, from [`TitleBar::button_rects`](super::TitleBar::button_rects).
    pub buttons: Vec<(TitleBarButton, Rect)>,
    /// How far the resize band reaches outside `frame`, into the shadow.
    pub outer_edge: f64,
    /// How far the resize band reaches inside `frame`.
    pub inner_edge: f64,
    /// A maximized, full-screen, or tiled window has no resize band, no
    /// corners, and no shadow.
    pub maximized: bool,
}

impl WindowFrame {
    /// A frame with no resize band.
    pub fn new(frame: Rect, title_bar: Rect, buttons: Vec<(TitleBarButton, Rect)>) -> Self {
        Self {
            frame,
            title_bar,
            buttons,
            outer_edge: 0.0,
            inner_edge: 0.0,
            maximized: false,
        }
    }

    /// The frame with a resize band from `outer` points outside the
    /// frame to `inner` points inside it.
    #[must_use]
    pub fn with_resize_edges(mut self, outer: f64, inner: f64) -> Self {
        self.outer_edge = outer.max(0.0);
        self.inner_edge = inner.max(0.0);
        self
    }

    /// The frame with the standard resize band of the theme: two spacing
    /// units outside the frame and one unit inside it.
    #[must_use]
    pub fn with_theme_edges(self, tokens: &DesignTokens) -> Self {
        self.with_resize_edges(tokens.space(2.0), tokens.space(1.0))
    }

    /// The frame of a maximized window, or of a window that is not.
    #[must_use]
    pub fn with_maximized(mut self, maximized: bool) -> Self {
        self.maximized = maximized;
        self
    }

    /// The part of the window at `point`. The resize band wins over the
    /// title bar and the content, and a window button wins over the
    /// title bar. Each corner reaches one band width along its two sides,
    /// so that a corner is easy to hit.
    pub fn region(&self, point: Point) -> WindowRegion {
        let band = if self.maximized {
            0.0
        } else {
            self.outer_edge + self.inner_edge
        };
        let frame = self.frame;
        if band > 0.0 {
            let outer = grow(frame, self.outer_edge);
            if !outer.contains(point) {
                return WindowRegion::Outside;
            }
            let inside = frame.inset(self.inner_edge);
            if !inside.contains(point) {
                return WindowRegion::Resize(edge_at(frame, point, self.inner_edge, band));
            }
        } else if !frame.contains(point) {
            return WindowRegion::Outside;
        }
        if let Some((button, _)) = self.buttons.iter().find(|(_, rect)| rect.contains(point)) {
            return WindowRegion::Button(*button);
        }
        if self.title_bar.contains(point) {
            WindowRegion::TitleBar
        } else {
            WindowRegion::Client
        }
    }

    /// The area that takes pointer input: the frame and its outer resize
    /// band. The shadow outside it lets the input go to the window
    /// below.
    pub fn input_area(&self) -> Rect {
        if self.maximized {
            self.frame
        } else {
            grow(self.frame, self.outer_edge)
        }
    }

    /// The margin that the window surface needs outside `frame`: room
    /// for the shadow of the `TitleBar` `Panel` slot, active or inactive,
    /// and for the outer resize band. The sides are whole points. A
    /// maximized frame needs no margin.
    pub fn margin(&self, styles: &Styles, tokens: &DesignTokens) -> Insets {
        if self.maximized {
            return Insets::default();
        }
        let frame = self.frame;
        let mut margin = Insets::all(self.outer_edge);
        for active in [true, false] {
            let state = VisualState::NONE.set(VisualState::FOCUSED, active);
            let query = StyleQuery::new(ControlKind::TitleBar, Slot::Panel).state(state);
            let style = styles.resolve(tokens, query);
            for shadow in style.shadow.iter().filter(|shadow| !shadow.inset) {
                let area = shadow.bounds(frame);
                margin.left = margin.left.max(frame.min_x() - area.min_x());
                margin.top = margin.top.max(frame.min_y() - area.min_y());
                margin.right = margin.right.max(area.max_x() - frame.max_x());
                margin.bottom = margin.bottom.max(area.max_y() - frame.max_y());
            }
        }
        Insets {
            left: margin.left.ceil(),
            top: margin.top.ceil(),
            right: margin.right.ceil(),
            bottom: margin.bottom.ceil(),
        }
    }

    /// Paints the frame of the window: its shadow, background, and
    /// border, from the `TitleBar` `Panel` slot. `active` gives the
    /// active-window shadow. Paint it before the content.
    pub fn paint(&self, cx: &mut PaintContext<'_>, active: bool) {
        let state = VisualState::NONE
            .set(VisualState::FOCUSED, active)
            .set(VisualState::CHECKED, self.maximized);
        let style = cx.style(StyleQuery::new(ControlKind::TitleBar, Slot::Panel).state(state));
        cx.slot(self.frame, &style, |_, _| {});
    }
}

/// `rect` grown by `amount` on each side.
fn grow(rect: Rect, amount: f64) -> Rect {
    Rect::from_xywh(
        rect.origin.x - amount,
        rect.origin.y - amount,
        rect.size.width + 2.0 * amount,
        rect.size.height + 2.0 * amount,
    )
}

/// The edge or corner of the band around `frame` at `point`. A side
/// takes the points within `inner` of it, and a corner reaches `band`
/// along each of its sides.
fn edge_at(frame: Rect, point: Point, inner: f64, band: f64) -> ResizeEdge {
    let mut left = point.x < frame.min_x() + inner;
    let mut right = point.x >= frame.max_x() - inner;
    let mut top = point.y < frame.min_y() + inner;
    let mut bottom = point.y >= frame.max_y() - inner;
    if top || bottom {
        left |= point.x < frame.min_x() + band;
        right |= point.x >= frame.max_x() - band;
    }
    if left || right {
        top |= point.y < frame.min_y() + band;
        bottom |= point.y >= frame.max_y() - band;
    }
    match (top, bottom, left, right) {
        (true, _, true, _) => ResizeEdge::TopLeft,
        (true, _, _, true) => ResizeEdge::TopRight,
        (_, true, true, _) => ResizeEdge::BottomLeft,
        (_, true, _, true) => ResizeEdge::BottomRight,
        (true, ..) => ResizeEdge::Top,
        (_, true, ..) => ResizeEdge::Bottom,
        (_, _, true, _) => ResizeEdge::Left,
        _ => ResizeEdge::Right,
    }
}
