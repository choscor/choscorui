//! A box with a fixed ratio of width to height, filled with an image.

use crate::custom::{ControlContext, ControlEvent, CustomControl, MeasureContext};
use crate::hosted;
use crate::style::{ControlKind, PaintContext, Slot, StyleQuery};
use choscorui_core::{
    EventReply, Image, ImageSampling, Measured, Path, Rect, SemanticNode, SemanticRole, Size,
};

/// A box whose height follows its width at a fixed ratio. An image
/// covers the box and is cropped at the center; without one, the box
/// shows its muted placeholder. It takes no focus.
pub struct AspectRatio {
    name: String,
    ratio: f64,
    image: Option<Image>,
}

impl AspectRatio {
    /// A box `ratio` times as wide as it is tall, such as `16.0 / 9.0`.
    /// A ratio that is not positive counts as 1.
    pub fn new(name: impl Into<String>, ratio: f64) -> Self {
        let ratio = if ratio.is_finite() && ratio > 0.0 {
            ratio
        } else {
            1.0
        };
        Self {
            name: name.into(),
            ratio,
            image: None,
        }
    }

    #[must_use]
    pub fn with_image(mut self, image: Image) -> Self {
        self.image = Some(image);
        self
    }
}

/// The rectangle that `image` covers `rect` with, centered, keeping its
/// own ratio.
fn cover(image: &Image, rect: Rect) -> Rect {
    let (width, height) = (f64::from(image.width()), f64::from(image.height()));
    if width <= 0.0 || height <= 0.0 {
        return rect;
    }
    let scale = (rect.size.width / width).max(rect.size.height / height);
    let size = Size::new(width * scale, height * scale);
    Rect::from_xywh(
        rect.center().x - size.width / 2.0,
        rect.center().y - size.height / 2.0,
        size.width,
        size.height,
    )
}

impl CustomControl for AspectRatio {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let max = cx.constraints.max.width;
        // An unbounded box is as wide as a small container.
        let width = if max.is_finite() {
            max
        } else {
            cx.tokens.spacing * 112.0
        };
        Measured {
            size: Size::new(width, width / self.ratio),
            baseline: None,
        }
    }

    fn event(&mut self, _cx: &mut ControlContext<'_>, _event: &ControlEvent) -> EventReply {
        EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let style = cx.style(StyleQuery::new(ControlKind::AspectRatio, Slot::Root));
        let bounds = cx.bounds;
        cx.slot(bounds, &style, |_, _| {});
        if let Some(image) = &self.image {
            let clip = Path::rounded_rect(bounds, style.radius.top_start);
            if cx.list.push_clip_path(clip).is_ok() {
                let _ =
                    cx.list
                        .draw_image(image.clone(), cover(image, bounds), ImageSampling::Linear);
                let _ = cx.list.pop_clip();
            }
        }
    }

    fn semantics(&self) -> SemanticNode {
        hosted::node(SemanticRole::Image, &self.name, Vec::new())
    }

    fn focusable(&self) -> bool {
        false
    }
}
