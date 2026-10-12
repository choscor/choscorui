//! What a gallery page is in the view API, and the small views that pages
//! share: the page column, titled sections, and captions.

use crate::page::{Desktop, PageState};
use choscorui::compose::view::{
    AnyView, HorizontalAlignment, ScrollView, Text, VStack, View, ViewExt,
};
use choscorui::compose::{Property, Runtime};
use choscorui::core::{ColorRole, Space, TextPreset};
use std::rc::Rc;

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
    pub desktop: Rc<dyn Desktop>,
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
pub fn section(title: &str, content: impl View) -> impl View {
    VStack::new((caption(title), content))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S2)
}

/// A small caption in `muted-foreground`.
pub fn caption(text: &str) -> impl View {
    Text::new(text.to_owned())
        .text_style(TextPreset::Small)
        .foreground(ColorRole::MutedForeground)
}
