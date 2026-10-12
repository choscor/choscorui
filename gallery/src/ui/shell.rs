//! One gallery window as a view: the state bar, the filterable sidebar,
//! and the selected page.

use crate::catalog::PAGES;
use crate::page::{Category, Desktop};
use crate::strings::{LANGUAGE_NAMES, LANGUAGES};
use crate::ui::page::{PageCx, PageStore};
use crate::view::Theme;
use choscorui::compose::Property;
use choscorui::compose::view::{
    Alignment, AnyView, Binding, Button, Checkbox, Divider, Dynamic, Either, ForEach, Frame,
    HStack, HorizontalAlignment, Input, Localized, ScrollView, Spacer, Text, VStack,
    VerticalAlignment, View, ViewExt,
};
use choscorui::controls::{ButtonVariant, ControlSize};
use choscorui::core::{BaseColor, Catalog, ColorRole, Locale, Space, TextPreset, fold_for_search};
use std::cell::RefCell;
use std::rc::Rc;

const SIDEBAR_WIDTH: f64 = 248.0;

/// What a window asks the application to do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Request {
    /// Open the page with this catalog index in a new window.
    NewWindow(usize),
}

/// State that every window shares.
#[derive(Clone)]
pub struct Shared {
    pub theme: Property<Theme>,
    pub base: Property<BaseColor>,
    pub language: Property<&'static str>,
    pub catalog: Rc<Catalog>,
    pub desktop: Rc<dyn Desktop>,
    /// What windows asked for since the application last looked.
    pub requests: Rc<RefCell<Vec<Request>>>,
}

impl Shared {
    /// The locale of the current language.
    pub fn locale(&self) -> Locale {
        crate::strings::language(self.language.get())
            .unwrap_or_else(|| Locale::parse(crate::strings::REFERENCE).expect("valid tag"))
    }

    /// A translated string, or `fallback` when the catalog lacks it.
    pub fn text_or(&self, id: &str, args: &[(&str, &str)], fallback: &str) -> String {
        self.catalog
            .text(&self.locale(), id, args)
            .unwrap_or_else(|_| fallback.to_owned())
    }

    pub fn text(&self, id: &str, args: &[(&str, &str)]) -> String {
        self.text_or(id, args, id)
    }

    pub fn page_title(&self, index: usize) -> String {
        let page = &PAGES[index];
        self.text_or(&format!("page.{}", page.id), &[], page.title)
    }

    /// The title of a window that shows page `index`.
    pub fn window_title(&self, index: usize) -> String {
        self.text_or(
            "window.title",
            &[("page", &self.page_title(index))],
            "choscorui gallery",
        )
    }
}

/// The state of one window.
#[derive(Clone)]
pub struct WindowState {
    /// The catalog index of the page shown.
    pub page: Property<usize>,
    pub filter: Property<String>,
    pub enabled: Property<bool>,
    /// What the state bar says about the focused control.
    pub readout: Property<String>,
    pub store: PageStore,
}

/// Catalog indices of pages whose translated title or id contains `query`,
/// ignoring case and Latin diacritics, in sidebar order.
pub fn matching_pages(query: &str, shared: &Shared) -> Vec<usize> {
    let query = fold_for_search(query.trim());
    let mut order: Vec<usize> = (0..PAGES.len()).collect();
    order.sort_by_key(|index| (PAGES[*index].category, *index));
    order
        .into_iter()
        .filter(|index| {
            query.is_empty()
                || fold_for_search(&shared.page_title(*index)).contains(&query)
                || fold_for_search(PAGES[*index].id).contains(&query)
                || fold_for_search(PAGES[*index].title).contains(&query)
        })
        .collect()
}

/// A row of the sidebar list.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Row {
    Header(Category),
    Page(usize),
}

fn rows(pages: &[usize]) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut category = None;
    for index in pages {
        let page = &PAGES[*index];
        if category != Some(page.category) {
            category = Some(page.category);
            rows.push(Row::Header(page.category));
        }
        rows.push(Row::Page(*index));
    }
    rows
}

fn binding<T: Clone + PartialEq + 'static>(property: &Property<T>) -> Binding<T> {
    let (get, set) = (property.clone(), property.clone());
    Binding::new(move || get.get(), move |value| set.set(value))
}

/// One gallery window.
pub struct GalleryRoot {
    pub shared: Shared,
    pub window: WindowState,
}

impl View for GalleryRoot {
    fn body(&self, _cx: &mut choscorui::compose::view::Cx<'_>) -> impl View {
        HStack::new((
            sidebar(&self.shared, &self.window),
            Divider,
            VStack::new((
                state_bar(&self.shared, &self.window),
                Divider,
                page_area(&self.shared, &self.window),
            ))
            .alignment(HorizontalAlignment::Leading)
            .frame(fill()),
        ))
        .alignment(VerticalAlignment::Fill)
        .frame(fill())
        .background(ColorRole::Background)
    }
}

/// The filter field, the page count, and the grouped page list.
fn sidebar(shared: &Shared, window: &WindowState) -> impl View {
    let filter = window.filter.clone();
    let counted = (shared.clone(), filter.clone());
    let listed = (shared.clone(), filter.clone());
    let page = window.page.clone();
    VStack::new((
        Input::new(Localized::new("sidebar.filter"), binding(&filter)),
        Text::new(
            Localized::new("sidebar.count")
                .count(move || matching_pages(&counted.1.get(), &counted.0).len() as u64),
        )
        .text_style(TextPreset::Small)
        .foreground(ColorRole::MutedForeground),
        ScrollView::new(
            VStack::new(ForEach::new(
                move || rows(&matching_pages(&listed.1.get(), &listed.0)),
                Row::clone,
                move |row| match row {
                    Row::Header(category) => Either::Left(
                        Text::new(Localized::new(format!("category.{}", category.id())))
                            .text_style(TextPreset::Small)
                            .foreground(ColorRole::MutedForeground)
                            .padding(Space::S2),
                    ),
                    Row::Page(index) => {
                        let index = *index;
                        let (selected, select) = (page.clone(), page.clone());
                        Either::Right(
                            Button::new(
                                Localized::new(format!("page.{}", PAGES[index].id)),
                                move || select.set(index),
                            )
                            .variant(move || {
                                if selected.get() == index {
                                    ButtonVariant::Secondary
                                } else {
                                    ButtonVariant::Ghost
                                }
                            })
                            .size(ControlSize::Sm),
                        )
                    }
                },
            ))
            .alignment(HorizontalAlignment::Leading)
            .spacing(Space::S1),
        ),
    ))
    .alignment(HorizontalAlignment::Fill)
    .spacing(Space::S2)
    .padding(Space::S3)
    .frame(
        Frame::new()
            .width(SIDEBAR_WIDTH)
            .max_height(f64::INFINITY)
            .alignment(Alignment::FILL),
    )
}

/// Theme, base color, and language selectors, the Enabled toggle, the new
/// window button, and the focus readout.
fn state_bar(shared: &Shared, window: &WindowState) -> impl View {
    let themes = ForEach::new(Theme::ALL.to_vec(), |theme| *theme, {
        let theme = shared.theme.clone();
        move |choice: &Theme| {
            let choice = *choice;
            let (current, set) = (theme.clone(), theme.clone());
            Button::new(
                Localized::new(format!("theme.{}", choice.id())),
                move || set.set(choice),
            )
            .variant(move || selected(current.get() == choice))
            .size(ControlSize::Sm)
        }
    });
    let bases = ForEach::new(BaseColor::ALL.to_vec(), |base| *base, {
        let base = shared.base.clone();
        move |choice: &BaseColor| {
            let choice = *choice;
            let (current, set) = (base.clone(), base.clone());
            Button::new(Localized::new(format!("base.{}", choice.id())), move || {
                set.set(choice)
            })
            .variant(move || selected(current.get() == choice))
            .size(ControlSize::Sm)
        }
    });
    let languages = ForEach::new(
        LANGUAGES
            .iter()
            .copied()
            .zip(LANGUAGE_NAMES)
            .collect::<Vec<_>>(),
        |(tag, _)| *tag,
        {
            let language = shared.language.clone();
            move |(tag, name): &(&'static str, &'static str)| {
                let tag = *tag;
                let (current, set) = (language.clone(), language.clone());
                Button::new(*name, move || set.set(tag))
                    .variant(move || selected(current.get() == tag))
                    .size(ControlSize::Sm)
            }
        },
    );
    let requests = shared.requests.clone();
    let page = window.page.clone();
    let readout = window.readout.clone();
    VStack::new((
        HStack::new((
            HStack::new(themes).spacing(Space::S1),
            HStack::new(languages).spacing(Space::S1),
            Checkbox::new(Localized::new("state.enabled"), binding(&window.enabled)),
            Button::new(Localized::new("state.new_window"), move || {
                requests.borrow_mut().push(Request::NewWindow(page.get()));
            })
            .variant(ButtonVariant::Outline)
            .size(ControlSize::Sm),
            Spacer::new(),
        ))
        .spacing(Space::S4),
        HStack::new((
            HStack::new(bases).spacing(Space::S1),
            Spacer::new(),
            Text::new(move || readout.get())
                .text_style(TextPreset::Small)
                .foreground(ColorRole::MutedForeground),
        ))
        .spacing(Space::S4),
    ))
    .alignment(HorizontalAlignment::Fill)
    .spacing(Space::S2)
    .padding(Space::S3)
}

/// A frame that takes all the space it is offered and stretches its
/// content to it.
fn fill() -> Frame {
    Frame::new()
        .max_width(f64::INFINITY)
        .max_height(f64::INFINITY)
        .alignment(Alignment::FILL)
}

fn selected(on: bool) -> ButtonVariant {
    if on {
        ButtonVariant::Default
    } else {
        ButtonVariant::Outline
    }
}

/// The page title and the selected page, dimmed while not enabled.
fn page_area(shared: &Shared, window: &WindowState) -> impl View {
    let shown = window.page.clone();
    let enabled = window.enabled.clone();
    let cx = PageCx {
        desktop: shared.desktop.clone(),
        store: window.store.clone(),
    };
    VStack::new((Dynamic::new(move || {
        let index = shown.get();
        let title = Text::new(Localized::new(format!("page.{}", PAGES[index].id)))
            .text_style(TextPreset::H3);
        let content: AnyView = match crate::ui::pages::find(PAGES[index].id) {
            Some(build) => build(&cx),
            None => Text::new(format!("{} is not a view yet", PAGES[index].title)).into_any(),
        };
        VStack::new((title, content))
            .alignment(HorizontalAlignment::Leading)
            .spacing(Space::S4)
            .into_any()
    })
    .disabled(move || !enabled.get()),))
    .alignment(HorizontalAlignment::Fill)
    .padding(Space::S6)
    .frame(fill())
}
