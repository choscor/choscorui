//! Model-backed views: the virtualized item view, table headers, cell
//! editing, form mapping, column browsing, undo history, and the canvas.
//! Item views mount as hosted controls, which paint only the rows in view
//! and realize no node per row.

pub(crate) mod canvas;
mod hosting;
mod products;

use crate::ui::page::{Build, PageCx, column};
use choscorui::compose::t;
use choscorui::compose::view::{
    Alignment, AnyView, Button, ColumnBrowser, Configured, Cx, Env, ForEach, Frame, HStack,
    HorizontalAlignment, State, Text, VStack, VerticalAlignment, View, ViewExt,
};
use choscorui::controls::{
    self, ButtonVariant, ControlKind, ControlMessage, ControlSize, ControlValue, ItemView,
    ShortcutModifiers, TextItemDelegate, ViewLayout, undo_history, undo_steps_to,
};
use choscorui::core::{
    ColorRole, ItemKey, ItemModel, Key, SemanticRole, Space, StringListModel, TextFilter,
    TextPreset, TreeStore, UndoStack,
};
use hosting::{Handle, SharedModel};
use std::cell::RefCell;
use std::rc::Rc;

/// The height of an item view row.
const ROW: f64 = 24.0;

/// A frame that takes all the space it is offered and stretches its
/// content to it.
fn fill() -> Frame {
    Frame::new()
        .max_width(f64::INFINITY)
        .max_height(f64::INFINITY)
        .alignment(Alignment::FILL)
}

/// An item view of `model`'s rows, painted by `delegate`.
fn item_view<M: ItemModel + 'static>(
    name: &str,
    role: SemanticRole,
    model: &SharedModel<M>,
    delegate: TextItemDelegate,
) -> ItemView {
    ItemView::new(name, role, ROW).with_model(Box::new(model.clone()), Box::new(delegate))
}

/// An item view that a page keeps while it builds its hosted control
/// again, so the view keeps its scroll offset, selection, and expansion.
/// A new locale makes a new view, for the new names.
#[derive(Clone, Default)]
struct Kept(Rc<RefCell<Option<(String, SharedView)>>>);

/// An item view that the page and its hosted control share.
type SharedView = Rc<RefCell<ItemView>>;

impl Kept {
    /// The kept view, or the view that `make` makes for a new locale.
    fn get(&self, env: &Env, make: impl FnOnce() -> ItemView) -> SharedView {
        let tag = env.locale().tag();
        let mut kept = self.0.borrow_mut();
        match &*kept {
            Some((locale, view)) if *locale == tag => view.clone(),
            _ => {
                let view = Rc::new(RefCell::new(make()));
                *kept = Some((tag, view.clone()));
                view
            }
        }
    }
}

/// The hosted item view that `build` makes. It builds again when
/// `revision` changes; `selected` keeps the selection by key, so it
/// survives sorting, filtering, and rebuilds.
fn rows<M: ItemModel + 'static>(
    kind: ControlKind,
    model: &SharedModel<M>,
    revision: State<u64>,
    selected: State<Vec<ItemKey>>,
    build: impl Fn(&Env) -> Handle<ItemView> + 'static,
) -> Configured<Handle<ItemView>> {
    let (shown, chosen) = (model.clone(), model.clone());
    Configured::custom(kind, move |env| {
        // Reading the revision builds the view again after the rows change.
        revision.get();
        build(env)
    })
    .value(move || {
        revision.get();
        ControlValue::Indices(shown.positions(&selected.get()))
    })
    .on_message(move |message| {
        if let ControlMessage::Changed(ControlValue::Indices(rows)) = message {
            selected.set(chosen.keys(rows));
        }
    })
}

/// What assistive technology reads for a list: its first selected row.
fn first_selected<M: ItemModel>(model: &SharedModel<M>, selected: &[ItemKey]) -> String {
    selected
        .first()
        .map(|key| model.describe(*key))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Item view

pub const RECORDS: usize = 100_000;
const CITIES: [&str; 6] = ["Hà Nội", "Huế", "Đà Lạt", "Hội An", "Sa Pa", "Tel Aviv"];

/// The records, each starting with the word `record`.
fn records(record: &str) -> StringListModel {
    StringListModel::from_strings((0..RECORDS).map(|i| {
        format!(
            "{record} {i:06} · {} · {}.{:02} ₫",
            CITIES[i % CITIES.len()],
            (i * 37) % 10_000,
            i % 100
        )
    }))
}

/// 100,000 records behind a folded (case- and diacritic-insensitive)
/// search, in list or grid layout. Only visible rows are painted.
struct ItemViewPage;

impl View for ItemViewPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let model = SharedModel::proxied(records(&cx.env().text("item-view.record", &[])));
        let matches = cx.state(RECORDS);
        let revision = cx.state(0_u64);
        let search = cx.state(String::new());
        let grid = cx.state(false);
        let selected = cx.state(Vec::new());
        let list_focused = cx.state(false);
        let filter: Rc<dyn Fn(String)> = {
            let model = model.clone();
            Rc::new(move |text: String| {
                let pattern = text.trim().to_owned();
                model.reproxy(|proxy| {
                    proxy.set_filter((!pattern.is_empty()).then(|| TextFilter::contains(pattern)));
                });
                matches.set(model.row_count(None));
                revision.update(|revision| *revision += 1);
                search.set(text);
            })
        };
        let on_key: Rc<dyn Fn(Key)> = {
            let filter = filter.clone();
            Rc::new(move |key| match key {
                Key::Escape => filter(String::new()),
                _ => list_focused.set(true),
            })
        };
        let field = Configured::custom(ControlKind::Input, move |env| {
            Handle::keyed(
                controls::Input::new(env.text("item-view.search", &[]), "")
                    .with_placeholder(env.text("item-view.search_placeholder", &[])),
                &[Key::Escape, Key::ArrowDown],
                on_key.clone(),
            )
        })
        .value(move || ControlValue::Text(search.get()))
        .on_message(move |message| {
            if let ControlMessage::Changed(ControlValue::Text(text)) = message {
                filter(text.clone());
            }
        })
        .on_activate(move || list_focused.set(true));
        let layout = Button::new(
            move || {
                if grid.get() {
                    t!("item-view.list_layout")
                } else {
                    t!("item-view.grid_layout")
                }
            },
            move || grid.update(|grid| *grid = !*grid),
        )
        .variant(ButtonVariant::Outline);
        let (shown, named, kept) = (model.clone(), model.clone(), Kept::default());
        let list = rows(
            ControlKind::ListView,
            &model,
            revision,
            selected,
            move |env| {
                // A search or a layout change keeps the view, so it keeps
                // its scroll offset and shows the current record.
                let view = kept.get(env, || {
                    item_view(
                        &env.text("item-view.records", &[]),
                        SemanticRole::List,
                        &shown,
                        TextItemDelegate::default(),
                    )
                });
                view.borrow_mut().set_layout(if grid.get() {
                    ViewLayout::Grid { item_width: 300.0 }
                } else {
                    ViewLayout::List
                });
                Handle::shared(view)
            },
        )
        .focused(list_focused.binding())
        .accessibility_value(move || first_selected(&named, &selected.get()));
        VStack::new((
            HStack::new((
                field.frame(Frame::new().width(300.0)),
                layout,
                Text::new(t!("item-view.matches", n = matches, total = RECORDS))
                    .text_style(TextPreset::Small)
                    .foreground(ColorRole::MutedForeground),
            ))
            .alignment(VerticalAlignment::Center)
            .spacing(Space::S3),
            list.frame(fill()),
        ))
        .alignment(HorizontalAlignment::Fill)
        .spacing(Space::S3)
        .frame(fill())
    }
}

pub fn item_view_page(_: &PageCx) -> AnyView {
    ItemViewPage.into_any()
}

// ---------------------------------------------------------------------------
// Column browser

/// A country and its cities.
type Country = (&'static str, &'static [&'static str]);

/// Continents by catalog key, with their countries. Country and city
/// names are place data and stay in their own spelling.
const PLACES: &[(&str, &[Country])] = &[
    (
        "column-browser.asia",
        &[
            (
                "Việt Nam",
                &["Hà Nội", "Huế", "Đà Nẵng", "Thành phố Hồ Chí Minh"],
            ),
            ("Japan", &["Tokyo", "Kyoto", "Osaka"]),
            ("Israel", &["ירושלים", "תל אביב", "חיפה"]),
        ],
    ),
    (
        "column-browser.europe",
        &[
            ("France", &["Paris", "Lyon"]),
            ("Deutschland", &["Berlin", "München", "Köln"]),
        ],
    ),
    (
        "column-browser.americas",
        &[
            ("Canada", &["Montréal", "Toronto"]),
            ("Brasil", &["São Paulo", "Rio de Janeiro"]),
        ],
    ),
];

/// The places as a three-level tree: continents, countries, cities.
fn places(env: &Env) -> TreeStore<String> {
    let mut model = TreeStore::<String>::new(1);
    for (continent, countries) in PLACES {
        let continent_key = model
            .push(None, env.text(continent, &[]))
            .expect("the root is a valid parent");
        for (country, cities) in *countries {
            let country_key = model
                .push(Some(continent_key), (*country).to_owned())
                .expect("a continent is a valid parent");
            for city in *cities {
                model
                    .push(Some(country_key), (*city).to_owned())
                    .expect("a country is a valid parent");
            }
        }
    }
    model
}

/// The names along a path of rows: a continent, a country, and a city.
fn place_names(env: &Env, rows: &[usize]) -> Vec<String> {
    let mut names = Vec::new();
    let Some((continent, countries)) = rows.first().and_then(|row| PLACES.get(*row)) else {
        return names;
    };
    names.push(env.text(continent, &[]));
    let Some((country, cities)) = rows.get(1).and_then(|row| countries.get(*row)) else {
        return names;
    };
    names.push((*country).to_owned());
    names.extend(
        rows.get(2)
            .and_then(|row| cities.get(*row))
            .map(|city| (*city).to_owned()),
    );
    names
}

/// Places browsed column by column: Right opens, Left goes back.
struct ColumnBrowserPage;

impl View for ColumnBrowserPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let path = cx.state(vec![0_usize]);
        let env = cx.env().clone();
        let browser = ColumnBrowser::new(
            t!("column-browser.places"),
            |env: &Env| Box::new(places(env)) as Box<dyn ItemModel>,
            path.binding(),
        );
        column(
            VStack::new((
                browser.frame(
                    Frame::new()
                        .height(ROW * 8.0)
                        .max_width(f64::INFINITY)
                        .alignment(Alignment::FILL),
                ),
                Text::new(move || place_names(&env, &path.get()).join(" › "))
                    .text_style(TextPreset::Small)
                    .foreground(ColorRole::MutedForeground),
            ))
            .alignment(HorizontalAlignment::Fill)
            .spacing(Space::S2),
        )
    }
}

pub fn column_browser(_: &PageCx) -> AnyView {
    ColumnBrowserPage.into_any()
}

// ---------------------------------------------------------------------------
// Undo history

const HISTORY_LIMIT: usize = 12;

/// What a counter button does.
#[derive(Clone, Copy)]
enum Step {
    Add1,
    Add10,
    Double,
    Reset,
}

impl Step {
    /// The catalog key of the button and of the history rows.
    fn key(self) -> &'static str {
        match self {
            Self::Add1 => "undo-history.add_1",
            Self::Add10 => "undo-history.add_10",
            Self::Double => "undo-history.double",
            Self::Reset => "undo-history.reset",
        }
    }
}

/// A counter and the undo stack of its changes.
struct Counter {
    value: i64,
    stack: UndoStack<i64>,
    env: Env,
}

impl Counter {
    fn apply(&mut self, step: Step) {
        let delta = match step {
            Step::Add1 => 1,
            Step::Add10 => 10,
            Step::Double => self.value,
            Step::Reset => -self.value,
        };
        self.value += delta;
        self.stack.push(self.env.text(step.key(), &[]), delta);
    }

    fn undo(&mut self) {
        if let Some(group) = self.stack.undo() {
            self.value -= group.entries.iter().sum::<i64>();
        }
    }

    fn redo(&mut self) {
        if let Some(group) = self.stack.redo() {
            self.value += group.entries.iter().sum::<i64>();
        }
    }

    /// Undoes or redoes until history row `row` is current.
    fn go_to(&mut self, row: usize) {
        let steps = undo_steps_to(&self.stack, row);
        for _ in 0..steps.unsigned_abs() {
            if steps < 0 {
                self.undo();
            } else {
                self.redo();
            }
        }
    }
}

/// The counter, with a version that changes with it so views update.
#[derive(Clone)]
struct History {
    counter: Rc<RefCell<Counter>>,
    version: State<u64>,
}

impl History {
    fn change(&self, change: impl FnOnce(&mut Counter)) {
        change(&mut self.counter.borrow_mut());
        self.version.update(|version| *version += 1);
    }

    fn read<R>(&self, read: impl FnOnce(&Counter) -> R) -> R {
        self.version.get();
        read(&self.counter.borrow())
    }
}

/// A counter edited by buttons, with an undo history list: choosing a row
/// undoes or redoes to that state.
struct UndoHistoryPage;

impl View for UndoHistoryPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let history = History {
            counter: Rc::new(RefCell::new(Counter {
                value: 0,
                stack: UndoStack::new(HISTORY_LIMIT),
                env: cx.env().clone(),
            })),
            version: cx.state(0),
        };
        let start = cx.env().text("undo-history.start", &[]);
        let redoable = cx.env().text("undo-history.redoable", &[]);
        let push = |label: &'static str, change: fn(&mut Counter)| {
            let history = history.clone();
            Button::new(t!(label), move || history.change(change)).variant(ButtonVariant::Outline)
        };
        let shown = history.clone();
        let listed = history.clone();
        let chosen = history.clone();
        let (undo, redo) = (history.clone(), history.clone());
        let command = ShortcutModifiers {
            command: true,
            ..ShortcutModifiers::default()
        };
        let shifted = ShortcutModifiers {
            shift: true,
            ..command
        };
        column((
            Text::new(t!(
                "undo-history.value",
                value = move || shown.read(|counter| counter.value)
            )),
            HStack::new((
                push(Step::Add1.key(), |counter| counter.apply(Step::Add1)),
                push(Step::Add10.key(), |counter| counter.apply(Step::Add10)),
                push(Step::Double.key(), |counter| counter.apply(Step::Double)),
                push(Step::Reset.key(), |counter| counter.apply(Step::Reset)),
            ))
            .spacing(Space::S2),
            HStack::new((
                push("undo-history.undo", Counter::undo),
                push("undo-history.redo", Counter::redo),
            ))
            .spacing(Space::S2),
            VStack::new(ForEach::new(
                move || {
                    listed
                        .read(|counter| undo_history(&counter.stack, &start))
                        .into_iter()
                        .enumerate()
                        .collect::<Vec<_>>()
                },
                |(row, entry)| (*row, entry.label.clone(), entry.current, entry.redoable),
                move |(row, entry)| {
                    let (row, history) = (*row, chosen.clone());
                    // Redoable rows are muted; the current row is selected.
                    Button::new(entry.label.clone(), move || {
                        history.change(|counter| counter.go_to(row))
                    })
                    .variant(if entry.current {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Ghost
                    })
                    .size(ControlSize::Sm)
                    .accessibility_hint(if entry.redoable {
                        redoable.clone()
                    } else {
                        String::new()
                    })
                    .opacity(if entry.redoable { 0.5 } else { 1.0 })
                },
            ))
            .alignment(HorizontalAlignment::Leading),
        ))
        .keyboard_shortcut(Key::Character('z'), command, move || {
            undo.change(Counter::undo)
        })
        .keyboard_shortcut(Key::Character('z'), shifted, move || {
            redo.change(Counter::redo)
        })
    }
}

pub fn undo_history_page(_: &PageCx) -> AnyView {
    UndoHistoryPage.into_any()
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "item-view" => item_view_page,
        "header-view" => products::header_view,
        "cell-edit" => products::cell_edit,
        "data-mapper" => products::data_mapper,
        "column-browser" => column_browser,
        "undo-history" => undo_history_page,
        "canvas" => canvas::canvas,
        _ => return None,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::ui::pages::tests::Harness;
    use choscorui::core::{KeyEvent, NodeId, SemanticAction};

    /// The most rows that may have nodes at once.
    const REALIZED_ROW_LIMIT: usize = 200;

    /// Presses `key` with `modifiers` in the page.
    pub fn press(page: &mut Harness, key: Key, modifiers: ShortcutModifiers) {
        page.host
            .key(KeyEvent::Down { key, repeat: false }, modifiers);
        page.host.key(KeyEvent::Up { key }, modifiers);
        page.host.layout();
    }

    /// Sets the value of the node named `name`, as assistive technology
    /// would.
    pub fn set_value(page: &mut Harness, name: &str, value: &str) {
        let node = page.node(name);
        assert!(
            page.host
                .perform(node, SemanticAction::SetValue, Some(value))
        );
        page.host.layout();
    }

    /// Every published node named `name`, in tree order.
    fn named(page: &mut Harness, name: &str) -> Vec<NodeId> {
        let (snapshot, _) = page.host.semantics();
        snapshot
            .into_iter()
            .filter(|entry| entry.node.name == name)
            .map(|entry| entry.id)
            .collect()
    }

    /// The record count that the item view readout shows.
    fn matches(page: &mut Harness) -> usize {
        let suffix = format!(" of {RECORDS} records");
        let (snapshot, _) = page.host.semantics();
        snapshot
            .into_iter()
            .find_map(|entry| {
                let count = entry.node.name.strip_suffix(&suffix)?;
                count.parse().ok()
            })
            .expect("the readout shows a count")
    }

    #[test]
    fn item_view_searches_100k_records_and_keeps_paging_bounded() {
        let mut page = Harness::new("item-view");
        assert!(page.host.find("100000 of 100000 records").is_some());
        set_value(&mut page, "Search records", "7");
        let matches = matches(&mut page);
        assert!(matches > 0 && matches < RECORDS, "{matches} records match");
        assert!(page.act("Records", SemanticAction::Focus));
        let mut peak = 0;
        for _ in 0..2_000 {
            press(&mut page, Key::PageDown, ShortcutModifiers::default());
            peak = peak.max(page.host.semantics().0.len());
        }
        assert!(
            peak > 0 && peak <= REALIZED_ROW_LIMIT,
            "{peak} nodes at most"
        );
        let current = page.value("Records").unwrap_or_default();
        assert!(
            current.starts_with("Record ") && current.contains('7'),
            "{current}"
        );
    }

    /// The texts that `page` paints.
    pub fn painted(page: &mut Harness) -> Vec<String> {
        page.host
            .paint()
            .commands()
            .iter()
            .filter_map(|command| match command {
                choscorui::core::DisplayCommand::DrawText { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn item_view_search_keeps_the_current_record_in_view() {
        let mut page = Harness::new("item-view");
        assert!(page.act("Records", SemanticAction::Focus));
        press(&mut page, Key::End, ShortcutModifiers::default());
        let last = |page: &mut Harness| {
            painted(page)
                .iter()
                .any(|text| text.starts_with("Record 099999"))
        };
        assert!(last(&mut page));
        // The search keeps the view, so the current record stays in view
        // and the list does not jump back to the first match.
        set_value(&mut page, "Search records", "9");
        assert!(last(&mut page));
        assert!(page.act("Grid layout", SemanticAction::Activate));
        assert!(last(&mut page), "the layout change keeps it in view");
    }

    #[test]
    fn item_view_layout_change_keeps_the_scroll_offset() {
        let mut page = Harness::new("item-view");
        let first = |page: &mut Harness| {
            painted(page)
                .iter()
                .any(|text| text.starts_with("Record 000000"))
        };
        assert!(first(&mut page));
        let list = page.node("Records");
        let center = page
            .host
            .tree()
            .bounds(list)
            .expect("the list has bounds")
            .center();
        page.host.pointer(
            choscorui::core::PointerEvent::Wheel {
                position: center,
                delta: choscorui::core::Point::new(0.0, 2_000.0),
                unit: choscorui::core::WheelUnit::Points,
            },
            ShortcutModifiers::default(),
        );
        page.host.layout();
        assert!(!first(&mut page));
        assert!(page.act("Grid layout", SemanticAction::Activate));
        assert!(!first(&mut page), "the view did not scroll back to the top");
    }

    #[test]
    fn item_view_escape_clears_the_search() {
        let mut page = Harness::new("item-view");
        set_value(&mut page, "Search records", "tel aviv");
        let count = RECORDS / CITIES.len();
        assert!(
            page.host
                .find(&format!("{count} of {RECORDS} records"))
                .is_some()
        );
        assert!(page.act("Search records", SemanticAction::Focus));
        press(&mut page, Key::Escape, ShortcutModifiers::default());
        assert_eq!(page.value("Search records").as_deref(), Some(""));
        assert!(page.host.find("100000 of 100000 records").is_some());
        assert!(page.act("Grid layout", SemanticAction::Activate));
        assert!(page.host.find("List layout").is_some());
    }

    #[test]
    fn column_browser_opens_columns_with_the_arrow_keys() {
        let mut page = Harness::new("column-browser");
        assert!(page.host.find("Asia").is_some());
        assert!(page.act("Places", SemanticAction::Focus));
        press(&mut page, Key::ArrowRight, ShortcutModifiers::default());
        press(&mut page, Key::ArrowDown, ShortcutModifiers::default());
        assert!(page.host.find("Asia › Japan").is_some());
        assert_eq!(page.focused_part("Places").as_deref(), Some("Japan"));
    }

    #[test]
    fn undo_history_jumps_back_and_forward_through_the_stack() {
        let mut page = Harness::new("undo-history");
        assert!(page.act("Add 10", SemanticAction::Activate));
        assert!(page.act("Add 10", SemanticAction::Activate));
        assert!(page.host.find("Value: 20").is_some());
        assert!(page.act("Start (0)", SemanticAction::Activate));
        assert!(page.host.find("Value: 0").is_some());
        let rows = named(&mut page, "Add 10");
        assert_eq!(rows.len(), 3, "the button and two history rows");
        let last = rows[2];
        let row = page.host.semantic(last).unwrap();
        assert_eq!(row.description, "Redoable");
        assert!(page.host.perform(last, SemanticAction::Activate, None));
        page.host.layout();
        assert!(page.host.find("Value: 20").is_some());
        let command = ShortcutModifiers {
            command: true,
            ..ShortcutModifiers::default()
        };
        press(&mut page, Key::Character('z'), command);
        assert!(page.host.find("Value: 10").is_some());
        let shifted = ShortcutModifiers {
            shift: true,
            ..command
        };
        press(&mut page, Key::Character('z'), shifted);
        assert!(page.host.find("Value: 20").is_some());
    }
}
