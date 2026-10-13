//! The output of a recipe: every visual value one slot needs.

use choscorui_core::{
    Color, CursorShape, DesignTokens, FontWeight, Paint, ShadowLayers, TextSize, TypeSpec,
};

/// Insets in logical directions, so a right-to-left layout mirrors them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges {
    pub inline_start: f64,
    pub inline_end: f64,
    pub block_start: f64,
    pub block_end: f64,
}

impl Edges {
    pub const ZERO: Self = Self::all(0.0);

    pub const fn all(value: f64) -> Self {
        Self::axes(value, value)
    }

    /// `inline` on the start and end, `block` on the top and bottom.
    pub const fn axes(inline: f64, block: f64) -> Self {
        Self {
            inline_start: inline,
            inline_end: inline,
            block_start: block,
            block_end: block,
        }
    }

    pub fn horizontal(&self) -> f64 {
        self.inline_start + self.inline_end
    }

    pub fn vertical(&self) -> f64 {
        self.block_start + self.block_end
    }

    /// Left and right insets for the layout direction.
    pub fn left_right(&self, rtl: bool) -> (f64, f64) {
        if rtl {
            (self.inline_end, self.inline_start)
        } else {
            (self.inline_start, self.inline_end)
        }
    }
}

/// Corner radii in logical order: top start, top end, bottom end, bottom
/// start.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Corners {
    pub top_start: f64,
    pub top_end: f64,
    pub bottom_end: f64,
    pub bottom_start: f64,
}

impl Corners {
    pub const ZERO: Self = Self::all(0.0);

    pub const fn all(radius: f64) -> Self {
        Self {
            top_start: radius,
            top_end: radius,
            bottom_end: radius,
            bottom_start: radius,
        }
    }

    pub fn is_uniform(&self) -> bool {
        self.top_start == self.top_end
            && self.top_end == self.bottom_end
            && self.bottom_end == self.bottom_start
    }

    /// Physical radii clockwise from the top left for the direction.
    pub fn physical(&self, rtl: bool) -> [f64; 4] {
        if rtl {
            [
                self.top_end,
                self.top_start,
                self.bottom_start,
                self.bottom_end,
            ]
        } else {
            [
                self.top_start,
                self.top_end,
                self.bottom_end,
                self.bottom_start,
            ]
        }
    }

    /// Squares the corners on the start side (for joined groups).
    #[must_use]
    pub fn square_start(mut self) -> Self {
        self.top_start = 0.0;
        self.bottom_start = 0.0;
        self
    }

    /// Squares the corners on the end side.
    #[must_use]
    pub fn square_end(mut self) -> Self {
        self.top_end = 0.0;
        self.bottom_end = 0.0;
        self
    }

    /// Squares the top corners.
    #[must_use]
    pub fn square_top(mut self) -> Self {
        self.top_start = 0.0;
        self.top_end = 0.0;
        self
    }

    /// Squares the bottom corners.
    #[must_use]
    pub fn square_bottom(mut self) -> Self {
        self.bottom_start = 0.0;
        self.bottom_end = 0.0;
        self
    }
}

/// Which sides carry the border.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Sides {
    pub block_start: bool,
    pub inline_end: bool,
    pub block_end: bool,
    pub inline_start: bool,
}

impl Sides {
    pub const ALL: Self = Self {
        block_start: true,
        inline_end: true,
        block_end: true,
        inline_start: true,
    };
    pub const NONE: Self = Self {
        block_start: false,
        inline_end: false,
        block_end: false,
        inline_start: false,
    };
    pub const BOTTOM: Self = Self {
        block_end: true,
        ..Self::NONE
    };
    pub const TOP: Self = Self {
        block_start: true,
        ..Self::NONE
    };
    pub const START: Self = Self {
        inline_start: true,
        ..Self::NONE
    };
    pub const END: Self = Self {
        inline_end: true,
        ..Self::NONE
    };

    pub fn is_all(&self) -> bool {
        *self == Self::ALL
    }
}

impl Default for Sides {
    fn default() -> Self {
        Self::ALL
    }
}

/// An outset focus ring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ring {
    pub width: f64,
    pub color: Color,
}

/// The pointer appearance over a slot. The host shows the cursor of the
/// topmost painted slot under the pointer that sets one.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Cursor {
    /// The slot sets no cursor: the cursor of the slot below it shows, or
    /// the arrow.
    #[default]
    Auto,
    Arrow,
    Text,
    /// The pointing hand of a link.
    Pointer,
    ResizeHorizontal,
    ResizeVertical,
    /// A grip at the bottom end corner: the diagonal from the top start
    /// corner to the bottom end corner.
    ResizeDiagonalEnd,
    /// A grip at the bottom start corner.
    ResizeDiagonalStart,
    Grab,
    NotAllowed,
}

impl Cursor {
    /// The window cursor for the layout direction.
    pub fn shape(self, rtl: bool) -> CursorShape {
        let diagonal = |end: bool| {
            if end != rtl {
                CursorShape::ResizeNorthWestSouthEast
            } else {
                CursorShape::ResizeNorthEastSouthWest
            }
        };
        match self {
            Self::Auto | Self::Arrow => CursorShape::Arrow,
            Self::Text => CursorShape::IBeam,
            Self::Pointer => CursorShape::PointingHand,
            Self::ResizeHorizontal => CursorShape::ResizeLeftRight,
            Self::ResizeVertical => CursorShape::ResizeUpDown,
            Self::ResizeDiagonalEnd => diagonal(true),
            Self::ResizeDiagonalStart => diagonal(false),
            Self::Grab => CursorShape::OpenHand,
            Self::NotAllowed => CursorShape::NotAllowed,
        }
    }
}

/// Every visual value of one slot in one state. Recipes fill it from the
/// tokens; the slot painter and the control's layout read it.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedStyle {
    /// The fill of the slot box. A gradient's points are in the unit
    /// square of the box: (0, 0) is the top start corner and (1, 1) the
    /// bottom end corner. A radial radius is a fraction of the longer
    /// side of the box.
    pub background: Paint,
    pub foreground: Color,
    pub border: Color,
    pub border_width: f64,
    pub border_sides: Sides,
    pub radius: Corners,
    pub shadow: ShadowLayers,
    pub ring: Option<Ring>,
    pub padding: Edges,
    pub min_height: f64,
    pub min_width: f64,
    /// The widest the slot grows; infinite when unbounded.
    pub max_width: f64,
    /// Space between children, such as an icon and a label.
    pub gap: f64,
    pub text: TypeSpec,
    pub underline: bool,
    pub icon_size: f64,
    /// Opacity of the whole slot, for example 0.5 while disabled.
    pub opacity: f64,
    /// Vertical offset of the slot, for example 1 pt while pressed.
    pub offset_y: f64,
    pub cursor: Cursor,
    /// Standard deviation, in points, of the blur applied to the content
    /// behind the slot, for scrims.
    pub backdrop_blur: f64,
    /// Duration of the slot's transitions and its open and close
    /// animation, in milliseconds.
    pub motion_ms: u32,
}

impl ResolvedStyle {
    /// A transparent slot with foreground text at the small size.
    pub fn new(tokens: &DesignTokens) -> Self {
        Self {
            background: Paint::Solid(Color::TRANSPARENT),
            foreground: tokens.color(choscorui_core::ColorRole::Foreground),
            border: Color::TRANSPARENT,
            border_width: 0.0,
            border_sides: Sides::ALL,
            radius: Corners::ZERO,
            shadow: ShadowLayers::NONE,
            ring: None,
            padding: Edges::ZERO,
            min_height: 0.0,
            min_width: 0.0,
            max_width: f64::INFINITY,
            gap: 0.0,
            text: tokens.text(TextSize::Sm, FontWeight::NORMAL),
            underline: false,
            icon_size: 16.0,
            opacity: 1.0,
            offset_y: 0.0,
            cursor: Cursor::Auto,
            backdrop_blur: 0.0,
            motion_ms: if tokens.motion.reduced {
                0
            } else {
                tokens.motion.duration_ms
            },
        }
    }

    /// True when the background paints something.
    pub fn has_background(&self) -> bool {
        !self.background.is_transparent()
    }

    pub fn has_border(&self) -> bool {
        self.border_width > 0.0 && self.border.alpha > 0
    }
}
