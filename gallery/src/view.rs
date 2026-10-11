//! One gallery window's platform-neutral content: the state bar, the
//! filterable sidebar, and the selected page. Nothing here touches a native
//! API, so the tests below drive it without a window.

use crate::catalog::PAGES;
use crate::page::{Align, Category, Desktop, Env, MeasureCache, Page, PageState, sem, split};
use crate::strings::{LANGUAGE_NAMES, LANGUAGES};
use choscorui::controls::{
    Button, ButtonInput, Checkbox, CheckboxInput, CommandId, PillButtonRecipe, ShortcutModifiers,
    TextField, TextFieldChange, TextFieldInput, paint_button,
};
use choscorui::core::{
    Color, ColorRole, DesignTokens, DisplayList, Key, KeyEvent, LayoutDirection, Localizer, NodeId,
    Point, PointerButton, PointerEvent, Rect, SemanticAction, SemanticChange, SemanticEntry,
    SemanticNode, SemanticRole, SemanticTree, Size, TextShaper, Tree, WheelUnit, fold_for_search,
    grapheme_boundaries,
};
use std::time::Duration;

const BAR_HEIGHT: f64 = 52.0;
const SIDEBAR_WIDTH: f64 = 248.0;
const FILTER_HEIGHT: f64 = 30.0;
const HEADER_HEIGHT: f64 = 26.0;
const ROW_HEIGHT: f64 = 26.0;
const COUNT_HEIGHT: f64 = 24.0;
const TITLE_HEIGHT: f64 = 34.0;
const READOUT_HEIGHT: f64 = 22.0;

/// The application-wide appearance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Theme {
    Dark,
    Light,
    /// The brand theme carried over from the notes sample: light, with a
    /// teal accent and rounder corners.
    Brand,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Self::Dark, Self::Light, Self::Brand];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Brand => "brand",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|theme| theme.id() == id)
    }

    pub fn tokens(self) -> DesignTokens {
        match self {
            Self::Dark => DesignTokens::dark(),
            Self::Light => DesignTokens::light(),
            Self::Brand => {
                let light = DesignTokens::light();
                DesignTokens {
                    palette: light
                        .palette
                        .with(ColorRole::Primary, Color::rgba8(0, 128, 128, 255)),
                    radius: 8.0,
                    ..light
                }
            }
        }
    }
}

/// What a window asks the application to do.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Request {
    Theme(Theme),
    Language(&'static str),
    /// Open the page with this catalog index in a new window.
    NewWindow(usize),
}

/// Application-wide state every window reads.
pub struct Ctx<'a> {
    pub localizer: &'a Localizer,
    pub theme: Theme,
    pub shaper: &'a dyn TextShaper,
    pub desktop: &'a dyn Desktop,
    /// Time since the application started.
    pub now: Duration,
}

impl Ctx<'_> {
    /// A translated string, or `fallback` when the catalog lacks it.
    pub fn text_or(&self, id: &str, args: &[(&str, &str)], fallback: &str) -> String {
        self.localizer
            .text(id, args)
            .unwrap_or_else(|_| fallback.to_owned())
    }

    pub fn text(&self, id: &str, args: &[(&str, &str)]) -> String {
        self.text_or(id, args, id)
    }

    pub fn page_title(&self, index: usize) -> String {
        let page = &PAGES[index];
        self.text_or(&format!("page.{}", page.id), &[], page.title)
    }

    pub fn category_title(&self, category: Category) -> String {
        self.text(&format!("category.{}", category.id()), &[])
    }
}

/// Catalog indices of pages whose translated title or id contains `query`,
/// ignoring case and Latin diacritics, in sidebar order.
pub fn matching_pages(query: &str, ctx: &Ctx<'_>) -> Vec<usize> {
    let query = fold_for_search(query.trim());
    let mut order: Vec<usize> = (0..PAGES.len()).collect();
    order.sort_by_key(|index| (PAGES[*index].category, *index));
    order
        .into_iter()
        .filter(|index| {
            query.is_empty()
                || fold_for_search(&ctx.page_title(*index)).contains(&query)
                || fold_for_search(PAGES[*index].id).contains(&query)
                || fold_for_search(PAGES[*index].title).contains(&query)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum RowKind {
    Header(Category),
    Page(usize),
}

struct Row {
    kind: RowKind,
    node: NodeId,
    /// Offset from the top of the list content.
    top: f64,
    height: f64,
}

fn env<'a>(
    tokens: &'a DesignTokens,
    ctx: &'a Ctx<'a>,
    enabled: bool,
    state: &'a PageState,
    cache: &'a MeasureCache,
) -> Env<'a> {
    Env {
        tokens,
        locale: ctx.localizer.locale(),
        direction: ctx.localizer.locale().direction(),
        enabled,
        shaper: ctx.shaper,
        state,
        desktop: ctx.desktop,
        now: ctx.now,
        measure_cache: cache,
    }
}

pub struct GalleryView {
    tree: Tree,
    size: Size,
    theme_buttons: Vec<(Theme, NodeId, Button)>,
    language_buttons: Vec<(&'static str, NodeId, Button)>,
    enabled_node: NodeId,
    enabled_box: Checkbox,
    new_window_node: NodeId,
    new_window: Button,
    filter_node: NodeId,
    filter: TextField,
    list_node: NodeId,
    rows: Vec<Row>,
    scroll: f64,
    highlight: Option<usize>,
    count_node: NodeId,
    title_node: NodeId,
    readout_node: NodeId,
    page_root: NodeId,
    page: Box<dyn Page>,
    selected: usize,
    enabled: bool,
    theme: Theme,
    revision: u64,
    direction: LayoutDirection,
    page_state: PageState,
    semantics: SemanticTree,
    measure: MeasureCache,
    page_nodes: Vec<(NodeId, SemanticNode)>,
    bar: Rect,
    sidebar: Rect,
    list_rect: Rect,
    pane: Rect,
    content: Rect,
}

impl GalleryView {
    pub fn new(
        size: Size,
        selected: usize,
        filter: &str,
        page_state: PageState,
        ctx: &Ctx<'_>,
    ) -> Self {
        let selected = selected.min(PAGES.len() - 1);
        let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), size));
        let root = tree.root();
        let focusable = |tree: &mut Tree| {
            let id = tree.attach(root, Rect::default()).expect("root exists");
            tree.set_focusable(id, true).expect("node exists");
            id
        };
        let theme_buttons = Theme::ALL
            .into_iter()
            .map(|theme| {
                let id = focusable(&mut tree);
                (theme, id, Button::new(theme.id(), CommandId(0)))
            })
            .collect();
        let language_buttons = LANGUAGES
            .iter()
            .zip(LANGUAGE_NAMES)
            .map(|(tag, name)| {
                let id = focusable(&mut tree);
                (*tag, id, Button::new(name, CommandId(0)))
            })
            .collect();
        let enabled_node = focusable(&mut tree);
        let new_window_node = focusable(&mut tree);
        let filter_node = focusable(&mut tree);
        let list_node = focusable(&mut tree);
        let count_node = tree.attach(root, Rect::default()).expect("root exists");
        let title_node = tree.attach(root, Rect::default()).expect("root exists");
        let readout_node = tree.attach(root, Rect::default()).expect("root exists");
        let page_root = tree.attach(root, Rect::default()).expect("root exists");
        let tokens = ctx.theme.tokens();
        let measure = MeasureCache::default();
        let page = {
            let env = env(&tokens, ctx, true, &page_state, &measure);
            (PAGES[selected].build)(&mut tree, page_root, &env)
        };
        let mut view = Self {
            tree,
            size,
            theme_buttons,
            language_buttons,
            enabled_node,
            enabled_box: Checkbox::new(ctx.text("state.enabled", &[]), true),
            new_window_node,
            new_window: Button::new(ctx.text("state.new_window", &[]), CommandId(0)),
            filter_node,
            filter: TextField::new(ctx.text("sidebar.filter", &[]), filter),
            list_node,
            rows: Vec::new(),
            scroll: 0.0,
            highlight: Some(selected),
            count_node,
            title_node,
            readout_node,
            page_root,
            page,
            selected,
            enabled: true,
            theme: ctx.theme,
            revision: ctx.localizer.revision(),
            direction: ctx.localizer.locale().direction(),
            page_state,
            semantics: SemanticTree::new(),
            measure,
            page_nodes: Vec::new(),
            bar: Rect::default(),
            sidebar: Rect::default(),
            list_rect: Rect::default(),
            pane: Rect::default(),
            content: Rect::default(),
        };
        view.tree
            .set_focus(Some(list_node))
            .expect("list is focusable");
        view.rebuild_rows(ctx);
        view.layout(size, ctx);
        view.reveal_highlight();
        view
    }

    // Accessors used by the application and the autopilot.

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_id(&self) -> &'static str {
        PAGES[self.selected].id
    }

    pub fn filter_text(&self) -> &str {
        self.filter.value()
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The page tree, page, and environment together, for scripted checks.
    #[cfg(target_os = "macos")]
    pub fn with_page<R>(
        &mut self,
        ctx: &Ctx<'_>,
        f: impl FnOnce(&mut Tree, &mut dyn Page, &Env<'_>) -> R,
    ) -> R {
        self.sync(ctx);
        let tokens = ctx.theme.tokens();
        let env = env(&tokens, ctx, self.enabled, &self.page_state, &self.measure);
        f(&mut self.tree, self.page.as_mut(), &env)
    }

    /// Catalog indices shown in the sidebar, in order.
    pub fn visible_pages(&self) -> Vec<usize> {
        self.rows
            .iter()
            .filter_map(|row| match row.kind {
                RowKind::Page(index) => Some(index),
                RowKind::Header(_) => None,
            })
            .collect()
    }

    /// Bounds of a state bar control, for scripted clicks.
    pub fn theme_button(&self, theme: Theme) -> Rect {
        self.theme_buttons
            .iter()
            .find(|(candidate, _, _)| *candidate == theme)
            .and_then(|(_, id, _)| self.tree.bounds(*id))
            .unwrap_or_default()
    }

    pub fn enabled_toggle(&self) -> Rect {
        self.tree.bounds(self.enabled_node).unwrap_or_default()
    }

    /// Page state including the current page's latest values.
    #[cfg(target_os = "macos")]
    pub fn page_state(&mut self) -> PageState {
        self.page.persist(&mut self.page_state);
        self.page_state.clone()
    }

    pub fn window_title(&self, ctx: &Ctx<'_>) -> String {
        ctx.text_or(
            "window.title",
            &[("page", &ctx.page_title(self.selected))],
            "choscorui gallery",
        )
    }

    /// Re-reads labels, mirrors the layout, and rebuilds the page after a
    /// language or theme change made elsewhere.
    fn sync(&mut self, ctx: &Ctx<'_>) {
        let relabel = ctx.localizer.revision() != self.revision;
        if !relabel && ctx.theme == self.theme {
            return;
        }
        self.theme = ctx.theme;
        if relabel {
            self.revision = ctx.localizer.revision();
            self.direction = ctx.localizer.locale().direction();
            self.measure.clear();
            let (value, caret) = (self.filter.value().to_owned(), self.filter.caret());
            self.filter = TextField::new(ctx.text("sidebar.filter", &[]), value);
            let _ = self.filter.handle(
                &mut self.tree,
                self.filter_node,
                TextFieldInput::SetCaret {
                    caret,
                    extend: false,
                },
            );
            let checked = self.enabled_box.value();
            self.enabled_box = Checkbox::new(ctx.text("state.enabled", &[]), checked);
            self.new_window = Button::new(ctx.text("state.new_window", &[]), CommandId(0));
            self.rebuild_rows(ctx);
        }
        self.rebuild_page(ctx);
        self.layout(self.size, ctx);
    }

    fn rebuild_rows(&mut self, ctx: &Ctx<'_>) {
        for row in self.rows.drain(..) {
            let _ = self.tree.detach(row.node);
        }
        let mut top = 0.0;
        let mut current = None;
        for index in matching_pages(self.filter.value(), ctx) {
            let category = PAGES[index].category;
            if current != Some(category) {
                current = Some(category);
                let node = self
                    .tree
                    .attach(self.list_node, Rect::default())
                    .expect("list exists");
                self.rows.push(Row {
                    kind: RowKind::Header(category),
                    node,
                    top,
                    height: HEADER_HEIGHT,
                });
                top += HEADER_HEIGHT;
            }
            let node = self
                .tree
                .attach(self.list_node, Rect::default())
                .expect("list exists");
            self.rows.push(Row {
                kind: RowKind::Page(index),
                node,
                top,
                height: ROW_HEIGHT,
            });
            top += ROW_HEIGHT;
        }
        let visible = self.visible_pages();
        if !self.highlight.is_some_and(|index| visible.contains(&index)) {
            self.highlight = visible
                .contains(&self.selected)
                .then_some(self.selected)
                .or_else(|| visible.first().copied());
        }
        self.semantics.prune(&self.tree);
        self.scroll = self.scroll.min(self.max_scroll()).max(0.0);
        self.place_rows();
    }

    fn content_height(&self) -> f64 {
        self.rows.last().map_or(0.0, |row| row.top + row.height)
    }

    fn max_scroll(&self) -> f64 {
        (self.content_height() - self.list_rect.size.height).max(0.0)
    }

    fn place_rows(&mut self) {
        let list = self.list_rect;
        for row in &self.rows {
            let rect = Rect::new(
                Point::new(list.origin.x, list.origin.y + row.top - self.scroll),
                Size::new(list.size.width, row.height),
            );
            let visible = rect.max_y() > list.origin.y && rect.origin.y < list.max_y();
            let _ = self
                .tree
                .set_bounds(row.node, if visible { rect } else { Rect::default() });
            let _ = self.tree.set_visible(row.node, visible);
        }
    }

    fn reveal_highlight(&mut self) {
        let Some(row) = self
            .rows
            .iter()
            .find(|row| row.kind == RowKind::Page(self.highlight.unwrap_or(usize::MAX)))
        else {
            return;
        };
        let height = self.list_rect.size.height;
        if row.top < self.scroll {
            self.scroll = (row.top - HEADER_HEIGHT).max(0.0);
        } else if row.top + row.height > self.scroll + height {
            self.scroll = row.top + row.height - height;
        }
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
        self.place_rows();
    }

    fn rebuild_page(&mut self, ctx: &Ctx<'_>) {
        self.page.persist(&mut self.page_state);
        let focus_in_page = self.tree.focus().is_some_and(|focus| self.in_page(focus));
        let _ = self.tree.set_pointer_capture(None);
        let _ = self.tree.detach(self.page_root);
        self.page_root = self
            .tree
            .attach(self.tree.root(), self.content)
            .expect("root exists");
        let tokens = ctx.theme.tokens();
        let env = env(&tokens, ctx, self.enabled, &self.page_state, &self.measure);
        self.page = (PAGES[self.selected].build)(&mut self.tree, self.page_root, &env);
        self.page.layout(&mut self.tree, self.content, &env);
        self.semantics.prune(&self.tree);
        if focus_in_page || self.tree.focus().is_none() {
            let _ = self.tree.set_focus(Some(self.list_node));
        }
    }

    fn in_page(&self, mut node: NodeId) -> bool {
        loop {
            if node == self.page_root {
                return true;
            }
            match self.tree.parent(node) {
                Some(parent) => node = parent,
                None => return false,
            }
        }
    }

    /// Shows the page with catalog index `index`.
    pub fn select(&mut self, index: usize, ctx: &Ctx<'_>) {
        self.sync(ctx);
        if index >= PAGES.len() {
            return;
        }
        self.selected = index;
        self.highlight = Some(index);
        self.rebuild_page(ctx);
        self.reveal_highlight();
    }

    pub fn set_filter(&mut self, text: &str, ctx: &Ctx<'_>) {
        self.sync(ctx);
        let _ = self.filter.handle(
            &mut self.tree,
            self.filter_node,
            TextFieldInput::SetValue(text.to_owned()),
        );
        self.rebuild_rows(ctx);
    }

    pub fn layout(&mut self, size: Size, ctx: &Ctx<'_>) {
        self.size = size;
        let rtl = self.direction == LayoutDirection::Rtl;
        let bounds = Rect::new(Point::new(0.0, 0.0), size);
        let _ = self.tree.set_bounds(self.tree.root(), bounds);
        self.bar = Rect::new(Point::new(0.0, 0.0), Size::new(size.width, BAR_HEIGHT));
        let body_height = (size.height - BAR_HEIGHT).max(0.0);
        let sidebar_width = SIDEBAR_WIDTH.min(size.width * 0.45);
        let sidebar_x = if rtl { size.width - sidebar_width } else { 0.0 };
        self.sidebar = Rect::new(
            Point::new(sidebar_x, BAR_HEIGHT),
            Size::new(sidebar_width, body_height),
        );
        let pane_x = if rtl { 0.0 } else { sidebar_width };
        self.pane = Rect::new(
            Point::new(pane_x, BAR_HEIGHT),
            Size::new((size.width - sidebar_width).max(0.0), body_height),
        );

        // State bar, leading to trailing.
        let inner = Rect::new(
            Point::new(12.0, 10.0),
            Size::new((size.width - 24.0).max(0.0), BAR_HEIGHT - 20.0),
        );
        let cells = split(
            inner,
            &[
                62.0, 62.0, 62.0, 14.0, 78.0, 92.0, 70.0, 14.0, 22.0, 96.0, 0.0, 190.0,
            ],
            6.0,
            rtl,
        );
        for (index, (_, id, _)) in self.theme_buttons.iter().enumerate() {
            let _ = self.tree.set_bounds(*id, cells[index]);
        }
        for (index, (_, id, _)) in self.language_buttons.iter().enumerate() {
            let _ = self.tree.set_bounds(*id, cells[4 + index]);
        }
        let toggle = cells[8];
        let _ = self.tree.set_bounds(
            self.enabled_node,
            Rect::new(
                Point::new(toggle.origin.x, toggle.origin.y + 5.0),
                Size::new(toggle.size.width, 22.0),
            ),
        );
        let _ = self.tree.set_bounds(self.new_window_node, cells[11]);

        // Sidebar: filter, list, and page count.
        let side = self.sidebar;
        let _ = self.tree.set_bounds(
            self.filter_node,
            Rect::new(
                Point::new(side.origin.x + 10.0, side.origin.y + 10.0),
                Size::new((side.size.width - 20.0).max(0.0), FILTER_HEIGHT),
            ),
        );
        self.list_rect = Rect::new(
            Point::new(side.origin.x, side.origin.y + FILTER_HEIGHT + 20.0),
            Size::new(
                side.size.width,
                (side.size.height - FILTER_HEIGHT - 20.0 - COUNT_HEIGHT).max(0.0),
            ),
        );
        let _ = self.tree.set_bounds(self.list_node, self.list_rect);
        let _ = self.tree.set_bounds(
            self.count_node,
            Rect::new(
                Point::new(side.origin.x + 12.0, side.max_y() - COUNT_HEIGHT),
                Size::new((side.size.width - 24.0).max(0.0), COUNT_HEIGHT),
            ),
        );
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
        self.place_rows();

        // Page pane: title, readout, content.
        let pane = self.pane;
        let inset = 20.0;
        let width = (pane.size.width - inset * 2.0).max(0.0);
        let _ = self.tree.set_bounds(
            self.title_node,
            Rect::new(
                Point::new(pane.origin.x + inset, pane.origin.y + 14.0),
                Size::new(width, TITLE_HEIGHT),
            ),
        );
        let _ = self.tree.set_bounds(
            self.readout_node,
            Rect::new(
                Point::new(pane.origin.x + inset, pane.origin.y + 14.0 + TITLE_HEIGHT),
                Size::new(width, READOUT_HEIGHT),
            ),
        );
        let top = pane.origin.y + 14.0 + TITLE_HEIGHT + READOUT_HEIGHT + 16.0;
        self.content = Rect::new(
            Point::new(pane.origin.x + inset, top),
            Size::new(width, (pane.max_y() - top - inset).max(0.0)),
        );
        let _ = self.tree.set_bounds(self.page_root, self.content);
        let tokens = ctx.theme.tokens();
        let env = env(&tokens, ctx, self.enabled, &self.page_state, &self.measure);
        self.page.layout(&mut self.tree, self.content, &env);
    }

    #[cfg(target_os = "macos")]
    pub fn resize(&mut self, size: Size, ctx: &Ctx<'_>) {
        self.sync(ctx);
        self.layout(size, ctx);
    }

    pub fn paint(&mut self, ctx: &Ctx<'_>) -> DisplayList {
        self.sync(ctx);
        let tokens = ctx.theme.tokens();
        let env = env(&tokens, ctx, self.enabled, &self.page_state, &self.measure);
        let mut list = DisplayList::new();
        list.fill_rect(
            Rect::new(Point::new(0.0, 0.0), self.size),
            tokens.color(ColorRole::Background),
        );

        // State bar.
        list.fill_rect(self.bar, tokens.color(ColorRole::Card));
        list.fill_rect(
            Rect::new(
                Point::new(0.0, self.bar.max_y() - 1.0),
                Size::new(self.size.width, 1.0),
            ),
            env.mix(0.15),
        );
        let theme_labels: Vec<(NodeId, &Button, bool, String)> = self
            .theme_buttons
            .iter()
            .map(|(theme, id, button)| {
                (
                    *id,
                    button,
                    *theme == ctx.theme,
                    ctx.text(&format!("theme.{}", theme.id()), &[]),
                )
            })
            .chain(self.language_buttons.iter().map(|(tag, id, button)| {
                (
                    *id,
                    button,
                    ctx.localizer.locale().language() == *tag,
                    button.semantics(&self.tree, *id).name,
                )
            }))
            .chain(std::iter::once((
                self.new_window_node,
                &self.new_window,
                false,
                ctx.text("state.new_window", &[]),
            )))
            .collect();
        for (id, button, active, label) in theme_labels {
            let bounds = self.tree.bounds(id).unwrap_or_default();
            if bounds.size.width <= 0.0 {
                continue;
            }
            paint_button(
                &PillButtonRecipe { emphasized: active },
                &mut list,
                bounds,
                button.visual_state(&self.tree, id),
                &tokens,
                |list, slots| {
                    env.text(
                        list,
                        slots.content,
                        &label,
                        env.small(),
                        slots.content_color,
                        Align::Center,
                    );
                },
            );
        }
        let toggle = self.tree.bounds(self.enabled_node).unwrap_or_default();
        env.checkbox_style().paint(
            &mut list,
            toggle,
            self.enabled_box.visual_state(&self.tree, self.enabled_node),
        );
        env.label(
            &mut list,
            crate::pages::inputs::beside(toggle, 92.0, &env),
            &ctx.text("state.enabled", &[]),
        );

        // Sidebar.
        list.fill_rect(self.sidebar, env.mix(0.03));
        let filter = self.tree.bounds(self.filter_node).unwrap_or_default();
        env.field_style().paint(
            &mut list,
            filter,
            self.filter.visual_state(&self.tree, self.filter_node),
        );
        let shown = self.filter.display_value();
        let text_rect = filter.inset(8.0);
        if shown.is_empty() {
            env.text(
                &mut list,
                text_rect,
                &ctx.text("sidebar.placeholder", &[]),
                env.body(),
                tokens.color(ColorRole::Foreground).with_alpha(0.45),
                Align::Leading,
            );
        } else {
            env.text(
                &mut list,
                text_rect,
                &shown,
                env.body(),
                tokens.color(ColorRole::Foreground),
                Align::Leading,
            );
        }
        if self.tree.focus() == Some(self.filter_node) {
            let boundaries = grapheme_boundaries(&shown);
            let caret = self.filter.caret().min(boundaries.len().saturating_sub(1));
            let prefix = &shown[..boundaries.get(caret).copied().unwrap_or(shown.len())];
            let width = env.measure(&shown, env.body());
            let before = env.measure(prefix, env.body());
            let x = if env.rtl() {
                text_rect.max_x() - width + before
            } else {
                text_rect.origin.x + before
            };
            list.fill_rect(
                Rect::new(
                    Point::new(x, filter.origin.y + 6.0),
                    Size::new(1.0, filter.size.height - 12.0),
                ),
                tokens.color(ColorRole::Foreground),
            );
        }
        list.push_clip(self.list_rect);
        let list_focused = self.tree.focus() == Some(self.list_node);
        for row in &self.rows {
            let Some(bounds) = self
                .tree
                .bounds(row.node)
                .filter(|bounds| bounds.size.height > 0.0)
            else {
                continue;
            };
            let text_rect = Rect::new(
                Point::new(bounds.origin.x + 14.0, bounds.origin.y),
                Size::new((bounds.size.width - 28.0).max(0.0), bounds.size.height),
            );
            match row.kind {
                RowKind::Header(category) => env.text(
                    &mut list,
                    text_rect,
                    &ctx.category_title(category).to_uppercase(),
                    env.small(),
                    tokens.color(ColorRole::Foreground).with_alpha(0.55),
                    Align::Leading,
                ),
                RowKind::Page(index) => {
                    let selected = index == self.selected;
                    let highlight_rect = Rect::new(
                        Point::new(bounds.origin.x + 6.0, bounds.origin.y + 1.0),
                        Size::new(
                            (bounds.size.width - 12.0).max(0.0),
                            bounds.size.height - 2.0,
                        ),
                    );
                    if selected {
                        let _ = list.fill_rounded_rect(
                            highlight_rect,
                            5.0,
                            tokens.color(ColorRole::Primary),
                        );
                    }
                    if list_focused && self.highlight == Some(index) {
                        let _ = list.stroke_rounded_rect(
                            highlight_rect,
                            5.0,
                            if selected {
                                tokens.color(ColorRole::Foreground)
                            } else {
                                tokens.color(ColorRole::Primary)
                            },
                            2.0,
                        );
                    }
                    env.text(
                        &mut list,
                        text_rect,
                        &ctx.page_title(index),
                        env.body(),
                        if selected {
                            env.on_accent()
                        } else {
                            tokens.color(ColorRole::Foreground)
                        },
                        Align::Leading,
                    );
                }
            }
        }
        let _ = list.pop_clip();
        env.caption(
            &mut list,
            self.tree.bounds(self.count_node).unwrap_or_default(),
            &self.count_text(ctx),
        );

        // Page pane.
        let title = self.tree.bounds(self.title_node).unwrap_or_default();
        env.text(
            &mut list,
            title,
            &ctx.page_title(self.selected),
            (16.0_f64 * 1.3).round(),
            tokens.color(ColorRole::Foreground),
            Align::Leading,
        );
        env.text(
            &mut list,
            title,
            PAGES[self.selected].id,
            env.small(),
            tokens.color(ColorRole::Foreground).with_alpha(0.5),
            Align::Trailing,
        );
        env.caption(
            &mut list,
            self.tree.bounds(self.readout_node).unwrap_or_default(),
            &self.readout(ctx),
        );
        list.push_clip(Rect::new(
            Point::new(self.content.origin.x - 6.0, self.content.origin.y - 6.0),
            Size::new(
                self.content.size.width + 12.0,
                self.content.size.height + 12.0,
            ),
        ));
        let dim = !self.enabled && !self.page.native_disabled();
        if dim {
            let _ = list.push_opacity(0.45);
        }
        self.page.paint(&self.tree, &mut list, &env);
        if dim {
            let _ = list.pop_opacity();
        }
        let _ = list.pop_clip();
        list
    }

    fn count_text(&self, ctx: &Ctx<'_>) -> String {
        let shown = self.visible_pages().len();
        ctx.localizer
            .plural("sidebar.count", shown as u64, &[])
            .unwrap_or_else(|_| format!("{shown}"))
    }

    /// "Focus: <name> = <value>" for the focused page control.
    fn readout(&self, ctx: &Ctx<'_>) -> String {
        let focused = self.tree.focus().and_then(|focus| {
            self.page_nodes
                .iter()
                .find(|(id, _)| *id == focus)
                .map(|(_, node)| node)
        });
        match focused {
            Some(node) => {
                let value = node.value.clone().unwrap_or_default();
                if value.is_empty() {
                    ctx.text("readout.focus", &[("name", &node.name)])
                } else {
                    ctx.text("readout.value", &[("name", &node.name), ("value", &value)])
                }
            }
            None => ctx.text("readout.none", &[]),
        }
    }

    /// Updates the semantic tree; returns the snapshot and the changes for
    /// the platform accessibility bridge.
    pub fn sync_semantics(&mut self, ctx: &Ctx<'_>) -> (Vec<SemanticEntry>, Vec<SemanticChange>) {
        self.sync(ctx);
        let tokens = ctx.theme.tokens();
        let env = env(&tokens, ctx, self.enabled, &self.page_state, &self.measure);
        let mut page_nodes = self.page.semantics(&self.tree, &env);
        if !self.enabled {
            for (_, node) in &mut page_nodes {
                node.state.disabled = true;
                node.actions.clear();
            }
        }
        self.page_nodes = page_nodes.clone();
        let mut nodes = Vec::new();
        for (theme, id, button) in &self.theme_buttons {
            let mut node = button.semantics(&self.tree, *id).shared();
            node.name = ctx.text(&format!("theme.{}", theme.id()), &[]);
            node.state.selected = Some(*theme == ctx.theme);
            nodes.push((*id, node));
        }
        for (tag, id, button) in &self.language_buttons {
            let mut node = button.semantics(&self.tree, *id).shared();
            node.state.selected = Some(ctx.localizer.locale().language() == *tag);
            nodes.push((*id, node));
        }
        nodes.push((
            self.enabled_node,
            self.enabled_box
                .semantics(&self.tree, self.enabled_node)
                .shared(),
        ));
        nodes.push((
            self.new_window_node,
            self.new_window
                .semantics(&self.tree, self.new_window_node)
                .shared(),
        ));
        nodes.push((
            self.filter_node,
            self.filter.semantics(&self.tree, self.filter_node).shared(),
        ));
        let mut list_node = sem(
            SemanticRole::List,
            ctx.text("sidebar.name", &[]),
            Some(self.count_text(ctx)),
        );
        list_node.actions = vec![SemanticAction::Focus];
        nodes.push((self.list_node, list_node));
        for row in &self.rows {
            let mut node = match row.kind {
                RowKind::Header(category) => {
                    sem(SemanticRole::StaticText, ctx.category_title(category), None)
                }
                RowKind::Page(index) => {
                    let mut node = sem(SemanticRole::ListItem, ctx.page_title(index), None);
                    node.state.selected = Some(index == self.selected);
                    node.actions = vec![SemanticAction::Activate];
                    node
                }
            };
            node.state.hidden = self.tree.visible(row.node) == Some(false);
            nodes.push((row.node, node));
        }
        nodes.push((
            self.title_node,
            sem(
                SemanticRole::StaticText,
                ctx.page_title(self.selected),
                None,
            ),
        ));
        nodes.push((
            self.readout_node,
            sem(SemanticRole::StaticText, self.readout(ctx), None),
        ));
        nodes.extend(page_nodes);
        let mut changes = Vec::new();
        self.semantics.prune(&self.tree);
        for (id, node) in nodes {
            if let Ok(change) = self.semantics.set(id, node, &self.tree)
                && change != SemanticChange::None
            {
                changes.push(change);
            }
        }
        for collection in self.page.collections(&self.tree) {
            let declared = [self
                .semantics
                .set_collection(collection.node, Some(collection.info))];
            let indexes = collection
                .items
                .iter()
                .map(|(id, index)| self.semantics.set_item_index(*id, Some(*index)));
            changes.extend(
                declared
                    .into_iter()
                    .chain(indexes.collect::<Vec<_>>())
                    .filter_map(Result::ok)
                    .filter(|change| *change != SemanticChange::None),
            );
        }
        if let Some(change) = self.semantics.sync_focus(&self.tree) {
            changes.push(change);
        }
        (self.semantics.snapshot(&self.tree), changes)
    }

    /// Routes a pointer event. Returns requests for the application.
    pub fn pointer(
        &mut self,
        event: PointerEvent,
        modifiers: ShortcutModifiers,
        ctx: &Ctx<'_>,
    ) -> Vec<Request> {
        self.sync(ctx);
        let mut requests = Vec::new();
        for (theme, id, button) in &mut self.theme_buttons {
            if let Ok(Some(_)) = button.handle(&mut self.tree, *id, ButtonInput::Pointer(event)) {
                requests.push(Request::Theme(*theme));
            }
        }
        for (tag, id, button) in &mut self.language_buttons {
            if let Ok(Some(_)) = button.handle(&mut self.tree, *id, ButtonInput::Pointer(event)) {
                requests.push(Request::Language(tag));
            }
        }
        if let Ok(Some(_)) = self.new_window.handle(
            &mut self.tree,
            self.new_window_node,
            ButtonInput::Pointer(event),
        ) {
            requests.push(Request::NewWindow(self.selected));
        }
        if let Ok(Some(enabled)) = self.enabled_box.handle(
            &mut self.tree,
            self.enabled_node,
            CheckboxInput::Pointer(event),
        ) {
            self.enabled = enabled;
            self.rebuild_page(ctx);
        }
        let capture = self.tree.pointer_capture();
        let position = event.position();
        if let PointerEvent::Wheel {
            position,
            delta,
            unit,
        } = event
            && self.list_rect.contains(position)
        {
            let step = if unit == WheelUnit::Lines {
                ROW_HEIGHT
            } else {
                1.0
            };
            self.scroll = (self.scroll + delta.y * step).clamp(0.0, self.max_scroll());
            self.place_rows();
            return requests;
        }
        if capture.is_none()
            && let PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } = event
        {
            if self
                .tree
                .bounds(self.filter_node)
                .is_some_and(|b| b.contains(position))
            {
                let _ = self.tree.set_focus(Some(self.filter_node));
                return requests;
            }
            if self.list_rect.contains(position) {
                let _ = self.tree.set_focus(Some(self.list_node));
                let hit = self.rows.iter().find_map(|row| match row.kind {
                    RowKind::Page(index)
                        if self
                            .tree
                            .bounds(row.node)
                            .is_some_and(|b| b.contains(position)) =>
                    {
                        Some(index)
                    }
                    _ => None,
                });
                if let Some(index) = hit {
                    self.select(index, ctx);
                }
                return requests;
            }
        }
        let to_page = match capture {
            Some(node) => self.in_page(node),
            None => self.pane.contains(position) || matches!(event, PointerEvent::Move { .. }),
        };
        if to_page && self.enabled {
            let focus_before = self.tree.focus();
            let tokens = ctx.theme.tokens();
            let env = env(&tokens, ctx, self.enabled, &self.page_state, &self.measure);
            self.page.pointer(&mut self.tree, event, modifiers, &env);
            if let PointerEvent::Down { position, .. } = event
                && self.tree.focus() == focus_before
                && let Some(hit) = self.tree.hit_test(position)
                && self.in_page(hit)
            {
                let _ = self.tree.set_focus(Some(hit));
            }
        }
        requests
    }

    /// Routes a key event. Returns requests for the application.
    pub fn key(
        &mut self,
        event: KeyEvent,
        modifiers: ShortcutModifiers,
        ctx: &Ctx<'_>,
    ) -> Vec<Request> {
        self.sync(ctx);
        let mut requests = Vec::new();
        let plain = !modifiers.command && !modifiers.control && !modifiers.option;
        if let KeyEvent::Down { key: Key::Tab, .. } = event
            && plain
        {
            self.tree.focus_next(modifiers.shift);
            return requests;
        }
        let focus = self.tree.focus();
        if matches!(event, KeyEvent::Up { .. }) || focus.is_some_and(|f| !self.in_page(f)) {
            for (theme, id, button) in &mut self.theme_buttons {
                if (focus == Some(*id) || matches!(event, KeyEvent::Up { .. }))
                    && let Ok(Some(_)) = button.handle(&mut self.tree, *id, ButtonInput::Key(event))
                {
                    requests.push(Request::Theme(*theme));
                }
            }
            for (tag, id, button) in &mut self.language_buttons {
                if (focus == Some(*id) || matches!(event, KeyEvent::Up { .. }))
                    && let Ok(Some(_)) = button.handle(&mut self.tree, *id, ButtonInput::Key(event))
                {
                    requests.push(Request::Language(tag));
                }
            }
            if (focus == Some(self.new_window_node) || matches!(event, KeyEvent::Up { .. }))
                && let Ok(Some(_)) = self.new_window.handle(
                    &mut self.tree,
                    self.new_window_node,
                    ButtonInput::Key(event),
                )
            {
                requests.push(Request::NewWindow(self.selected));
            }
            if (focus == Some(self.enabled_node) || matches!(event, KeyEvent::Up { .. }))
                && let Ok(Some(enabled)) = self.enabled_box.handle(
                    &mut self.tree,
                    self.enabled_node,
                    CheckboxInput::Key(event),
                )
            {
                self.enabled = enabled;
                self.rebuild_page(ctx);
            }
        }
        if focus == Some(self.filter_node) {
            self.filter_key(event, modifiers, ctx);
        } else if focus == Some(self.list_node) {
            self.list_key(event, ctx);
        } else if focus.is_none_or(|f| self.in_page(f)) && self.enabled {
            let tokens = ctx.theme.tokens();
            let env = env(&tokens, ctx, self.enabled, &self.page_state, &self.measure);
            self.page.key(&mut self.tree, event, modifiers, &env);
        }
        requests
    }

    fn filter_key(&mut self, event: KeyEvent, modifiers: ShortcutModifiers, ctx: &Ctx<'_>) {
        match event {
            KeyEvent::Down {
                key: Key::ArrowDown,
                ..
            } => {
                let _ = self.tree.set_focus(Some(self.list_node));
            }
            KeyEvent::Down {
                key: Key::Enter, ..
            } => {
                if let Some(first) = self
                    .highlight
                    .or_else(|| self.visible_pages().first().copied())
                {
                    self.select(first, ctx);
                }
            }
            KeyEvent::Down {
                key: Key::Escape, ..
            } => self.set_filter("", ctx),
            _ if !modifiers.command && !modifiers.control => {
                let change = self.filter.handle(
                    &mut self.tree,
                    self.filter_node,
                    TextFieldInput::Key {
                        event,
                        extend: modifiers.shift,
                        word: modifiers.option,
                    },
                );
                if change == Ok(TextFieldChange::Edited) {
                    self.rebuild_rows(ctx);
                }
            }
            _ => {}
        }
    }

    fn list_key(&mut self, event: KeyEvent, ctx: &Ctx<'_>) {
        let KeyEvent::Down { key, .. } = event else {
            return;
        };
        let visible = self.visible_pages();
        let position = self
            .highlight
            .and_then(|index| visible.iter().position(|candidate| *candidate == index));
        let target = match key {
            Key::ArrowDown => {
                Some(position.map_or(0, |p| (p + 1).min(visible.len().saturating_sub(1))))
            }
            Key::ArrowUp => Some(position.map_or(0, |p| p.saturating_sub(1))),
            Key::Home => Some(0),
            Key::End => Some(visible.len().saturating_sub(1)),
            Key::PageDown => {
                Some(position.map_or(0, |p| (p + 10).min(visible.len().saturating_sub(1))))
            }
            Key::PageUp => Some(position.map_or(0, |p| p.saturating_sub(10))),
            Key::Enter | Key::Space => {
                if let Some(index) = self.highlight {
                    self.select(index, ctx);
                }
                None
            }
            Key::Backspace => {
                let mut text = self.filter.value().to_owned();
                text.pop();
                self.set_filter(&text, ctx);
                None
            }
            Key::Escape => {
                self.set_filter("", ctx);
                None
            }
            _ => None,
        };
        if let Some(target) = target
            && let Some(index) = visible.get(target)
        {
            self.highlight = Some(*index);
            self.reveal_highlight();
        }
    }

    /// Typed text edits the filter (from the filter or the list) or goes
    /// to the page.
    pub fn text(&mut self, text: &str, ctx: &Ctx<'_>) {
        self.sync(ctx);
        let printable: String = text.chars().filter(|c| !c.is_control()).collect();
        let focus = self.tree.focus();
        if focus == Some(self.filter_node) || focus == Some(self.list_node) {
            if printable.is_empty() {
                return;
            }
            if focus == Some(self.list_node) {
                let mut value = self.filter.value().to_owned();
                value.push_str(&printable);
                self.set_filter(&value, ctx);
            } else if let Ok(TextFieldChange::Edited) = self.filter.handle(
                &mut self.tree,
                self.filter_node,
                TextFieldInput::Text(printable),
            ) {
                self.rebuild_rows(ctx);
            }
        } else if self.enabled {
            let tokens = ctx.theme.tokens();
            let env = env(&tokens, ctx, self.enabled, &self.page_state, &self.measure);
            self.page.text(&mut self.tree, text, &env);
        }
    }

    /// Performs an assistive-technology action.
    #[cfg(target_os = "macos")]
    pub fn action(
        &mut self,
        node: NodeId,
        action: SemanticAction,
        value: Option<String>,
        ctx: &Ctx<'_>,
    ) -> Vec<Request> {
        self.sync(ctx);
        let mut requests = Vec::new();
        let activate = ButtonInput::Semantic(choscorui::controls::SemanticAction::Activate);
        if action == SemanticAction::Focus && !self.in_page(node) {
            let _ = self.tree.set_focus(Some(node));
            return requests;
        }
        if let Some((theme, id, button)) =
            self.theme_buttons.iter_mut().find(|(_, id, _)| *id == node)
        {
            if let Ok(Some(_)) = button.handle(&mut self.tree, *id, activate) {
                requests.push(Request::Theme(*theme));
            }
        } else if let Some((tag, id, button)) = self
            .language_buttons
            .iter_mut()
            .find(|(_, id, _)| *id == node)
        {
            if let Ok(Some(_)) = button.handle(&mut self.tree, *id, activate) {
                requests.push(Request::Language(tag));
            }
        } else if node == self.new_window_node {
            if let Ok(Some(_)) = self.new_window.handle(&mut self.tree, node, activate) {
                requests.push(Request::NewWindow(self.selected));
            }
        } else if node == self.enabled_node {
            if let Ok(Some(enabled)) = self.enabled_box.handle(
                &mut self.tree,
                node,
                CheckboxInput::Semantic(choscorui::controls::SemanticAction::Activate),
            ) {
                self.enabled = enabled;
                self.rebuild_page(ctx);
            }
        } else if node == self.filter_node {
            if action == SemanticAction::SetValue {
                self.set_filter(&value.unwrap_or_default(), ctx);
            }
        } else if let Some(index) = self.rows.iter().find_map(|row| match row.kind {
            RowKind::Page(index) if row.node == node => Some(index),
            _ => None,
        }) {
            if action == SemanticAction::Activate {
                self.select(index, ctx);
            }
        } else if self.in_page(node) && self.enabled {
            if action == SemanticAction::Focus {
                let _ = self.tree.set_focus(Some(node));
            }
            let tokens = ctx.theme.tokens();
            let env = env(&tokens, ctx, self.enabled, &self.page_state, &self.measure);
            self.page
                .action(&mut self.tree, node, action, value.as_deref(), &env);
        }
        requests
    }

    /// Advances page timers; returns whether to repaint.
    #[cfg(target_os = "macos")]
    pub fn tick(&mut self, ctx: &Ctx<'_>) -> bool {
        self.sync(ctx);
        let tokens = ctx.theme.tokens();
        let env = env(&tokens, ctx, self.enabled, &self.page_state, &self.measure);
        self.page.tick(&env)
    }

    /// Releases pointer capture and armed presses when the window loses
    /// focus.
    #[cfg(target_os = "macos")]
    pub fn cancel_interaction(&mut self) {
        let cancel = ButtonInput::Cancel;
        for (_, id, button) in &mut self.theme_buttons {
            let _ = button.handle(&mut self.tree, *id, cancel);
        }
        for (_, id, button) in &mut self.language_buttons {
            let _ = button.handle(&mut self.tree, *id, cancel);
        }
        let _ = self
            .new_window
            .handle(&mut self.tree, self.new_window_node, cancel);
        let _ = self
            .enabled_box
            .handle(&mut self.tree, self.enabled_node, CheckboxInput::Cancel);
        let _ = self.tree.set_pointer_capture(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::NoDesktop;
    use crate::pages::tests::StubShaper;
    use crate::strings;
    use choscorui::core::{DisplayCommand, Locale};
    use std::path::Path;

    fn localizer(tag: &str) -> Localizer {
        let (catalog, _, notes) =
            strings::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("resources"));
        assert!(notes.is_empty(), "{notes:?}");
        Localizer::new(Locale::parse(tag).unwrap(), catalog)
    }

    fn ctx(localizer: &Localizer, theme: Theme) -> Ctx<'_> {
        Ctx {
            localizer,
            theme,
            shaper: &StubShaper,
            desktop: &NoDesktop,
            now: Duration::from_secs(2),
        }
    }

    fn index(id: &str) -> usize {
        PAGES.iter().position(|page| page.id == id).unwrap()
    }

    fn bounds(view: &GalleryView, node: NodeId) -> Rect {
        view.tree.bounds(node).unwrap_or_default()
    }

    fn language_node(view: &GalleryView, tag: &str) -> NodeId {
        view.language_buttons
            .iter()
            .find(|(candidate, _, _)| *candidate == tag)
            .map(|(_, id, _)| *id)
            .unwrap()
    }

    fn view(ctx: &Ctx<'_>) -> GalleryView {
        GalleryView::new(Size::new(1100.0, 760.0), 0, "", PageState::default(), ctx)
    }

    fn click(view: &mut GalleryView, at: Rect, ctx: &Ctx<'_>) -> Vec<Request> {
        let position = at.center();
        let mut requests = Vec::new();
        for event in [
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            },
            PointerEvent::Up {
                position,
                button: PointerButton::Primary,
            },
        ] {
            requests.extend(view.pointer(event, ShortcutModifiers::default(), ctx));
        }
        requests
    }

    fn down(key: Key) -> KeyEvent {
        KeyEvent::Down { key, repeat: false }
    }

    fn texts(list: &DisplayList) -> Vec<String> {
        list.commands()
            .iter()
            .filter_map(|command| match command {
                DisplayCommand::DrawText { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn filtering_matches_titles_and_ids_without_case_or_diacritics() {
        let english = localizer("en");
        let ctx = ctx(&english, Theme::Dark);
        let found = matching_pages("SLID", &ctx);
        assert_eq!(found, [index("slider")]);
        assert_eq!(matching_pages("tool-button", &ctx), [index("tool-button")]);
        assert_eq!(matching_pages("", &ctx).len(), PAGES.len());
        assert!(matching_pages("no such control", &ctx).is_empty());

        let vietnamese = localizer("vi");
        let ctx = Ctx {
            localizer: &vietnamese,
            ..ctx
        };
        let title = ctx.page_title(index("slider"));
        assert_ne!(title, "Slider", "the Vietnamese title is translated");
        let folded = fold_for_search(&title);
        assert!(matching_pages(&folded, &ctx).contains(&index("slider")));
    }

    #[test]
    fn typing_in_the_list_filters_and_return_opens_the_highlighted_page() {
        let english = localizer("en");
        let ctx = ctx(&english, Theme::Dark);
        let mut view = view(&ctx);
        assert_eq!(view.tree.focus(), Some(view.list_node));
        view.text("check", &ctx);
        assert_eq!(view.filter_text(), "check");
        assert_eq!(view.visible_pages(), [index("checkbox")]);
        view.key(down(Key::Enter), ShortcutModifiers::default(), &ctx);
        assert_eq!(view.selected_id(), "checkbox");
        view.key(down(Key::Escape), ShortcutModifiers::default(), &ctx);
        assert_eq!(view.visible_pages().len(), PAGES.len());
        view.key(down(Key::ArrowDown), ShortcutModifiers::default(), &ctx);
        view.key(down(Key::Enter), ShortcutModifiers::default(), &ctx);
        let after = view.visible_pages()[view
            .visible_pages()
            .iter()
            .position(|i| *i == index("checkbox"))
            .unwrap()
            + 1];
        assert_eq!(view.selected(), after, "Down moves to the next page");
    }

    #[test]
    fn clicking_a_sidebar_row_selects_its_page_and_paints_its_title() {
        let english = localizer("en");
        let ctx = ctx(&english, Theme::Dark);
        let mut view = view(&ctx);
        view.set_filter("slider", &ctx);
        let row = view
            .rows
            .iter()
            .find(|row| row.kind == RowKind::Page(index("slider")))
            .map(|row| view.tree.bounds(row.node).unwrap())
            .unwrap();
        click(&mut view, row, &ctx);
        assert_eq!(view.selected_id(), "slider");
        assert!(texts(&view.paint(&ctx)).contains(&"Slider".to_owned()));
        assert_eq!(view.window_title(&ctx), "choscorui gallery — Slider");
    }

    #[test]
    fn state_bar_buttons_request_theme_language_and_new_window() {
        let english = localizer("en");
        let ctx = ctx(&english, Theme::Dark);
        let mut view = view(&ctx);
        assert_eq!(
            {
                let at = view.theme_button(Theme::Brand);
                click(&mut view, at, &ctx)
            },
            [Request::Theme(Theme::Brand)]
        );
        assert_eq!(
            {
                let at = bounds(&view, language_node(&view, "vi"));
                click(&mut view, at, &ctx)
            },
            [Request::Language("vi")]
        );
        view.select(index("dial"), &ctx);
        assert_eq!(
            {
                let at = bounds(&view, view.new_window_node);
                click(&mut view, at, &ctx)
            },
            [Request::NewWindow(index("dial"))]
        );
    }

    #[test]
    fn theme_change_repaints_with_the_new_tokens() {
        let english = localizer("en");
        let mut view = view(&ctx(&english, Theme::Dark));
        let background = |list: &DisplayList| match list.commands().first() {
            Some(DisplayCommand::FillRect { color, .. }) => *color,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            background(&view.paint(&ctx(&english, Theme::Dark))),
            Theme::Dark.tokens().color(ColorRole::Background)
        );
        assert_eq!(
            background(&view.paint(&ctx(&english, Theme::Light))),
            Theme::Light.tokens().color(ColorRole::Background)
        );
    }

    #[test]
    fn a_right_to_left_locale_mirrors_the_sidebar_and_state_bar() {
        let mut localizer = localizer("en");
        let mut view = view(&ctx(&localizer, Theme::Dark));
        assert!(view.sidebar.origin.x < view.content.origin.x);
        let ltr_dark = view.theme_button(Theme::Dark);
        assert!(ltr_dark.origin.x < bounds(&view, view.new_window_node).origin.x);
        view.set_filter("button", &ctx(&localizer, Theme::Dark));
        // No right-to-left translation ships; Arabic tests the mirroring.
        localizer.set_locale(Locale::parse("ar").unwrap());
        let ctx = ctx(&localizer, Theme::Dark);
        view.paint(&ctx);
        assert_eq!(view.direction, LayoutDirection::Rtl);
        assert!(view.sidebar.origin.x > view.content.origin.x);
        assert!(
            view.theme_button(Theme::Dark).origin.x > bounds(&view, view.new_window_node).origin.x
        );
        assert_eq!(view.filter_text(), "button", "the filter survives");
        let (snapshot, _) = view.sync_semantics(&ctx);
        let names: Vec<_> = snapshot
            .iter()
            .map(|entry| entry.node.name.clone())
            .collect();
        // `ar` has no catalog: labels show their message ids, and page
        // titles fall back to English.
        assert!(names.iter().any(|name| name == "sidebar.name"), "{names:?}");
        assert!(names.iter().any(|name| name == "Label"), "{names:?}");
    }

    #[test]
    fn disabling_marks_page_semantics_and_blocks_page_input() {
        let english = localizer("en");
        let ctx = ctx(&english, Theme::Dark);
        let mut view = view(&ctx);
        view.select(index("checkbox"), &ctx);
        {
            let at = view.enabled_toggle();
            click(&mut view, at, &ctx)
        };
        assert!(!view.enabled());
        let (snapshot, _) = view.sync_semantics(&ctx);
        let checkbox = snapshot
            .iter()
            .find(|entry| entry.node.name == "Wrap long lines")
            .expect("checkbox semantics");
        assert!(checkbox.node.state.disabled);
        assert_eq!(checkbox.node.state.checked, Some(false));
        let bounds = view.tree.bounds(checkbox.id).unwrap();
        click(&mut view, bounds, &ctx);
        let (snapshot, _) = view.sync_semantics(&ctx);
        let checkbox = snapshot
            .iter()
            .find(|entry| entry.node.name == "Wrap long lines")
            .unwrap();
        assert_eq!(checkbox.node.state.checked, Some(false), "input is blocked");
        let toggle = view.enabled_toggle();
        click(&mut view, toggle, &ctx);
        assert!(view.enabled());
        click(&mut view, bounds, &ctx);
        let (snapshot, _) = view.sync_semantics(&ctx);
        let checkbox = snapshot
            .iter()
            .find(|entry| entry.node.name == "Wrap long lines")
            .unwrap();
        assert_eq!(checkbox.node.state.checked, Some(true));
    }

    #[test]
    fn readout_names_the_focused_page_control_and_its_value() {
        let english = localizer("en");
        let ctx = ctx(&english, Theme::Dark);
        let mut view = view(&ctx);
        view.select(index("slider"), &ctx);
        while view.tree.focus().is_none_or(|focus| !view.in_page(focus)) {
            view.key(down(Key::Tab), ShortcutModifiers::default(), &ctx);
        }
        view.sync_semantics(&ctx);
        let readout = view.readout(&ctx);
        assert!(readout.starts_with("Focus: "), "{readout}");
        assert!(readout.contains(" = "), "{readout}");
    }
}
