//! The general recipe system: style queries, resolved styles, the recipe
//! registry with its cache, the default recipe set, and the slot painter.

mod default;
mod kind;
mod paint;
mod recipe;
mod resolved;
mod state;
mod transition;
mod variants;

pub(crate) use kind::variant_enum;
pub use kind::{ControlKind, ControlSize, Orientation, Slot, Variant};
pub use paint::{
    Content, ELLIPSIS, EstimatedText, PaintContext, ShapedText, TextAlign, TextLine, TextMeasure,
    TextOverflow, align_x, content_rect, draw_icon, draw_text, fill_shape, paint_box, paint_slot,
    truncate_end,
};
pub(crate) use paint::{choice_size, fill_area, join_segment, paint_choice, paint_in};
pub use recipe::{DefaultRecipe, Recipe, StyleCacheStats, StyleQuery, Styles};
pub use resolved::{Corners, Cursor, Edges, ResolvedStyle, Ring, Sides};
pub use state::VisualState;
pub use transition::TransitionStats;
pub(crate) use transition::Transitions;
pub use variants::*;
