//! Shared pieces of the pages that mount hosted controls. The old
//! captioned-row page is a `section` of controls in the view API; what
//! is left is a view whose body builds from the mount context, for pages
//! that own state.

use crate::ui::page::Build;
use choscorui::compose::view::{AnyView, Cx, View};

/// A view whose body `build` makes from the mount context.
struct Body<F>(F);

impl<F, V> View for Body<F>
where
    F: Fn(&mut Cx<'_>) -> V + 'static,
    V: View,
{
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        (self.0)(cx)
    }
}

/// A page view that creates its state from the mount context:
/// `stateful(|cx| { let open = cx.state(false); … })`.
pub fn stateful<V: View>(build: impl Fn(&mut Cx<'_>) -> V + 'static) -> AnyView {
    Body(build).into_any()
}

/// The view of page `id`, when this module has it. The group has no
/// pages of its own.
pub fn page(_id: &str) -> Option<Build> {
    None
}
