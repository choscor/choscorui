//! What a gallery page is in the view API, and the small views that pages
//! share: the page column, titled sections, and captions.
//!
//! Sample strings come from the `en` and `vi` catalogs under
//! `resources/strings/`, with keys `<page id>.<name>`. Labels take
//! `t!(key)` and follow the language live. Other strings come from
//! `Env::text` while the page builds; the shell builds the page again when
//! the language changes. The names of the sample people are catalog
//! strings under `people.`, which [`person`] reads. Addresses and file
//! names stand for user data and stay literal.

use crate::page::PageState;
use choscorui::compose::view::{
    AnyView, Cx, Env, HStack, HorizontalAlignment, ScrollView, State, Text, TextContent, VStack,
    View, ViewExt,
};
use choscorui::compose::{Property, Runtime};
use choscorui::controls::{self, ControlSize};
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

/// A 16:9 landscape scene that stands in for a photo: a sky gradient, a
/// sun, two rows of hills, and a lake.
pub fn landscape() -> Image {
    const WIDTH: u32 = 192;
    const HEIGHT: u32 = 108;
    let mix = |a: [f32; 3], b: [f32; 3], t: f32| {
        let t = t.clamp(0.0, 1.0);
        let channel = |i: usize| (a[i] + (b[i] - a[i]) * t) as u8;
        [channel(0), channel(1), channel(2), 255]
    };
    Image::from_fn(WIDTH, HEIGHT, move |x, y| {
        let (u, v) = (x as f32 / WIDTH as f32, y as f32 / HEIGHT as f32);
        let far = 0.52 + 0.06 * (u * 9.0).sin() + 0.03 * (u * 23.0).cos();
        let near = 0.68 + 0.05 * (u * 5.0 + 1.3).sin();
        let lake = 0.84;
        let (dx, dy) = (u - 0.72, (v - 0.3) * 9.0 / 16.0);
        if v > lake {
            mix(
                [70.0, 130.0, 190.0],
                [30.0, 70.0, 120.0],
                (v - lake) / (1.0 - lake),
            )
        } else if v > near {
            mix(
                [70.0, 140.0, 70.0],
                [40.0, 95.0, 45.0],
                (v - near) / (lake - near),
            )
        } else if v > far {
            mix(
                [120.0, 150.0, 170.0],
                [90.0, 125.0, 120.0],
                (v - far) / (near - far),
            )
        } else if (dx * dx + dy * dy).sqrt() < 0.06 {
            [255, 236, 170, 255]
        } else {
            mix([70.0, 130.0, 220.0], [250.0, 200.0, 160.0], v / far)
        }
    })
}

/// A small diagonal gradient that stands in for a photo.
pub fn portrait() -> Image {
    let side = 32u32;
    Image::from_fn(side, side, |x, y| {
        let shade = ((x + y) * 255 / (2 * side - 2)) as u8;
        [shade, 96, 255 - shade, 255]
    })
}

/// The avatar of the sample person `key`: the name is the catalog string
/// `people.<key>`, and the initials are `people.<key>_initials`.
pub fn person(env: &Env, key: &str) -> controls::Avatar {
    controls::Avatar::new(
        env.text(&format!("people.{key}"), &[]),
        env.text(&format!("people.{key}_initials"), &[]),
    )
}

/// A muted line that shows the result of the last action in `status`,
/// once there is one.
pub fn status_line(status: State<String>) -> impl View {
    Text::new(move || status.get())
        .text_style(TextPreset::Small)
        .foreground(ColorRole::MutedForeground)
        .hidden(move || status.get().is_empty())
}
