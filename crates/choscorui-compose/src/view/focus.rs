//! [`FocusFrame`]: the focus ring of a descendant, drawn outside the
//! clip of the containers between the frame and the focused node.

use super::View;
use super::host::Cx;
use super::layout::{Alignment, overlay_node};

/// Draws the focus ring around the focused node in `content`, above the
/// content and outside the clip of containers inside it. Thus a control
/// at the edge of a clipped panel still shows focus. The ring shows only
/// while focus is visible, after keyboard input. `controls::FocusFrame`
/// gives its geometry and paint. A frame inside another frame draws the
/// ring for its own content.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// let panel = FocusFrame::new(ScrollView::new(VStack::new((
///     Button::new("Save", || {}),
///     Button::new("Cancel", || {}),
/// ))));
/// ```
pub struct FocusFrame<V> {
    content: V,
}

impl<V: View> FocusFrame<V> {
    /// A frame that draws the focus ring for `content`.
    pub fn new(content: V) -> Self {
        Self { content }
    }
}

impl<V: View> View for FocusFrame<V> {
    fn mount(self, cx: &mut Cx) {
        let node = overlay_node(cx, Alignment::FILL);
        cx.inner.data(node).focus_frame = true;
        let env = cx.env.clone();
        cx.within(node, env).mount(self.content);
    }
}
