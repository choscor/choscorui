//! What a gallery page is in the view API, and the small views that pages
//! share: the page column, titled sections, and captions.
//!
//! Sample strings come from the `en` and `vi` catalogs under
//! `resources/strings/`, with keys `<page id>.<name>`. Labels take
//! `t!(key)` and follow the language live. Other strings come from
//! `Env::text` while the page builds; the shell builds the page again when
//! the language changes. Personal names, addresses, and file names stand
//! for user data and stay literal.

use crate::page::PageState;
use choscorui::compose::view::{
    AnyView, Cx, HStack, HorizontalAlignment, ScrollView, Text, TextContent, VStack, View, ViewExt,
};
use choscorui::compose::{Property, Runtime};
use choscorui::controls::ControlSize;
use choscorui::core::{ColorRole, Image, Space, TextPreset};

/// Builds a page's view.
pub type Build = fn(&PageCx) -> AnyView;

/// Page state that survives restarts, as properties that pages read and
/// write.
#[derive(Clone)]
pub struct PageStore {
    pub canvas_zoom: Property<f64>,
    /// `DockLayout::save` text; empty means the default arrangement.
    pub dock: Property<String>,
}

impl PageStore {
    pub fn new(runtime: &Runtime, state: &PageState) -> Self {
        Self {
            canvas_zoom: runtime.property(state.canvas_zoom),
            dock: runtime.property(state.dock.clone()),
        }
    }

    /// The state to save.
    pub fn snapshot(&self) -> PageState {
        PageState {
            canvas_zoom: self.canvas_zoom.get(),
            dock: self.dock.get(),
        }
    }
}

/// What a page is built from.
#[derive(Clone)]
pub struct PageCx {
    pub store: PageStore,
}

/// The page column: `sections` top to bottom with the page spacing,
/// scrolling when they are taller than the window.
pub fn column(sections: impl View) -> AnyView {
    ScrollView::new(
        VStack::new(sections)
            .alignment(HorizontalAlignment::Leading)
            .spacing(Space::S6),
    )
    .into_any()
}

/// A titled group of samples: a small muted caption above `content`.
pub fn section(title: impl Into<TextContent>, content: impl View) -> AnyView {
    VStack::new((caption(title), content))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S2)
        .into_any()
}

/// A small caption in `muted-foreground`.
pub fn caption(text: impl Into<TextContent>) -> impl View {
    Text::new(text.into())
        .text_style(TextPreset::Small)
        .foreground(ColorRole::MutedForeground)
}

/// Samples side by side with the gap between controls.
pub fn row(controls: impl View) -> impl View {
    HStack::new(controls).spacing(Space::S2)
}

/// The catalog key of the name of `size`, shared by the pages that show
/// a control in each of its sizes.
pub fn size_key(size: ControlSize) -> &'static str {
    match size {
        ControlSize::Xs => "button.size_xs",
        ControlSize::Sm => "button.size_sm",
        ControlSize::Default => "button.size_default",
        ControlSize::Lg => "button.size_lg",
        ControlSize::Icon => "button.size_icon",
        ControlSize::IconXs => "button.size_icon_xs",
        ControlSize::IconSm => "button.size_icon_sm",
        ControlSize::IconLg => "button.size_icon_lg",
    }
}

/// A view whose body `build` makes from the mount context.
struct Body<F>(F);

impl<F, V> View for Body<F>
where
    F: Fn(&mut Cx) -> V + 'static,
    V: View,
{
    fn body(&self, cx: &mut Cx) -> impl View {
        (self.0)(cx)
    }
}

/// A page view that creates its state from the mount context:
/// `stateful(|cx| { let open = cx.state(false); … })`.
pub fn stateful<V: View>(build: impl Fn(&mut Cx) -> V + 'static) -> AnyView {
    Body(build).into_any()
}

/// A small diagonal gradient that stands in for a photo.
pub fn portrait() -> Image {
    let side = 32u32;
    Image::from_fn(side, side, |x, y| {
        let shade = ((x + y) * 255 / (2 * side - 2)) as u8;
        [shade, 96, 255 - shade, 255]
    })
}
