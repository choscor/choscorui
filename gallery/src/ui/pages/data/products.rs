//! The typed product table and the pages built on it: a sortable,
//! resizable header, cell editing, and a form mapped to the selected row.

use super::hosting::{Handle, SharedModel};
use super::{fill, first_selected, item_view, rows};
use crate::ui::page::PageCx;
use choscorui::compose::t;
use choscorui::compose::view::{
    Alignment, AnyView, Binding, Button, Configured, Cx, Dynamic, Edges, Either, Env, Frame,
    HStack, HorizontalAlignment, Input, Spacer, State, Text, VStack, VerticalAlignment, View,
    ViewExt, ZStack,
};
use choscorui::controls::{
    self, CellEdit, ControlKind, ControlMessage, ControlValue, DataMapper, DefaultEditorFactory,
    HeaderColumn, HeaderView, ItemDelegate, ItemState, ItemView, PaintContext, Slot, StyleQuery,
    TextItemDelegate, ValidationError, ValidationState, VisualState,
};
use choscorui::core::{
    ColorRole, ItemData, ItemKey, ItemModel, Key, LayoutDirection, Locale, ModelError, Rect, Role,
    SelectionMode, SemanticRole, Size, Space, TextPreset, TreeStore, Value,
};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub struct Product {
    pub name: String,
    pub quantity: i64,
    pub price: f64,
    pub in_stock: bool,
    /// The texts and the number format that the columns show.
    pub text: Rc<ProductText>,
}

/// What the columns of every product show, shared by the rows.
#[derive(Debug, PartialEq)]
pub struct ProductText {
    /// What the in-stock column shows when `in_stock` is true, and when
    /// it is false.
    pub stock: [String; 2],
    /// The number format of the quantity and price columns, the same as
    /// the cell editor's.
    pub locale: Locale,
}

impl ItemData for Product {
    fn data(&self, column: usize, role: Role) -> Value {
        let display = matches!(role, Role::Display | Role::Accessibility);
        match column {
            0 if display || matches!(role, Role::Edit | Role::Sort) => {
                Value::Text(self.name.clone())
            }
            1 if display => Value::Text(self.text.locale.format_integer(self.quantity)),
            1 if matches!(role, Role::Edit | Role::Sort) => Value::Int(self.quantity),
            2 if display => Value::Text(
                self.text
                    .locale
                    .format_decimal(self.price, 2)
                    .unwrap_or_default(),
            ),
            2 if matches!(role, Role::Edit | Role::Sort) => Value::Float(self.price),
            3 if display => Value::Text(self.text.stock[usize::from(!self.in_stock)].clone()),
            3 if matches!(role, Role::Edit | Role::Sort) => Value::Bool(self.in_stock),
            _ => Value::None,
        }
    }

    fn set_data(&mut self, column: usize, _role: Role, value: Value) -> Result<(), ModelError> {
        match (column, value) {
            (0, Value::Text(name)) if !name.trim().is_empty() => self.name = name.trim().to_owned(),
            (1, Value::Int(quantity)) if quantity >= 0 => self.quantity = quantity,
            (2, Value::Float(price)) if price >= 0.0 => self.price = price,
            (3, Value::Bool(in_stock)) => self.in_stock = in_stock,
            (0..=3, _) => return Err(ModelError::InvalidValue),
            _ => return Err(ModelError::InvalidColumn),
        }
        Ok(())
    }

    fn editable(&self, column: usize) -> bool {
        column < 4
    }
}

/// The catalog keys of the column titles.
const COLUMNS: [&str; 4] = [
    "header-view.name",
    "header-view.quantity",
    "header-view.price",
    "header-view.in_stock",
];

/// The title of column `column` in `env`.
fn column_title(env: &Env, column: usize) -> String {
    env.text(COLUMNS[column], &[])
}

const PRODUCT_NAMES: [&str; 12] = [
    "Cà phê sữa đá",
    "Bánh mì",
    "Phở bò",
    "Trà sen",
    "Chè ba màu",
    "Bún chả",
    "Gỏi cuốn",
    "Cơm tấm",
    "Nước mía",
    "Bánh xèo",
    "Xôi gấc",
    "Hủ tiếu",
];

/// The quantity of product `index`.
fn quantity(index: usize) -> i64 {
    ((index * 7) % 50) as i64
}

fn products(env: &Env, count: usize) -> TreeStore<Product> {
    let mut store = TreeStore::new(4);
    store.set_headers((0..COLUMNS.len()).map(|column| column_title(env, column)));
    let text = Rc::new(ProductText {
        stock: [
            env.text("header-view.yes", &[]),
            env.text("header-view.no", &[]),
        ],
        locale: env.locale(),
    });
    let rows = (0..count).map(|i| Product {
        name: if i < PRODUCT_NAMES.len() {
            PRODUCT_NAMES[i].to_owned()
        } else {
            format!(
                "{} #{}",
                PRODUCT_NAMES[i % PRODUCT_NAMES.len()],
                i / PRODUCT_NAMES.len() + 1
            )
        },
        quantity: quantity(i),
        price: 1.5 + (i * 13 % 40) as f64 * 0.75,
        in_stock: i % 4 != 3,
        text: text.clone(),
    });
    store.insert(None, 0, rows).expect("root insert is valid");
    store
}

fn product_header(env: &Env) -> Rc<RefCell<HeaderView>> {
    Rc::new(RefCell::new(HeaderView::new(
        (0..COLUMNS.len())
            .zip([200.0, 100.0, 100.0, 90.0])
            .map(|(column, width)| HeaderColumn {
                title: column_title(env, column),
                width,
                min_width: 50.0,
            })
            .collect(),
    )))
}

/// The hosted `header`. A resize writes the new widths to `widths`;
/// `on_sort` runs after a sort change.
fn header_bar(
    header: Rc<RefCell<HeaderView>>,
    widths: State<Vec<f64>>,
    on_sort: impl Fn(&HeaderView) + 'static,
) -> impl View {
    let mounted = header.clone();
    Configured::custom(ControlKind::Table, move |_| Handle::shared(mounted.clone())).on_message(
        move |message| match message {
            ControlMessage::ItemActivated(_) => on_sort(&header.borrow()),
            ControlMessage::Changed(ControlValue::Index(_)) => widths.set(header.borrow().widths()),
            _ => {}
        },
    )
}

// ---------------------------------------------------------------------------
// Header view

/// A sortable, resizable table header over 60 products, with a total
/// row and a caption.
struct HeaderViewPage;

impl View for HeaderViewPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let count = 60;
        let total: i64 = (0..count).map(quantity).sum();
        let model = SharedModel::proxied(products(cx.env(), count));
        let header = product_header(cx.env());
        let widths = cx.state(header.borrow().widths());
        let revision = cx.state(0_u64);
        let selected = cx.state(Vec::new());
        let sorted = model.clone();
        let bar = header_bar(header, widths, move |header| {
            let sort = header.sort();
            sorted.reproxy(|proxy| proxy.set_sort(sort));
            revision.update(|revision| *revision += 1);
        });
        let (shown, named) = (model.clone(), model.clone());
        let list = rows(ControlKind::Table, &model, revision, selected, move |env| {
            let mut view = item_view(
                &env.text("header-view.products", &[]),
                SemanticRole::Table,
                &shown,
                TextItemDelegate::table(),
            )
            .with_footer(vec![vec![
                env.text("header-view.total", &[]),
                total.to_string(),
            ]])
            .with_caption(env.text("header-view.caption", &[]));
            view.set_column_widths(widths.get());
            Handle::new(view)
        })
        .accessibility_value(move || first_selected(&named, &selected.get()));
        VStack::new((bar, list.frame(fill())))
            .alignment(HorizontalAlignment::Fill)
            .spacing(0.0)
            .frame(fill())
    }
}

pub fn header_view(_: &PageCx) -> AnyView {
    HeaderViewPage.into_any()
}

// ---------------------------------------------------------------------------
// Cell editing

/// The table row look, with the focus ring on the current cell: the cell
/// of the chosen column in the current row, while the table has focus.
struct CellDelegate {
    column: State<usize>,
}

impl ItemDelegate for CellDelegate {
    fn paint_cell(
        &self,
        cx: &mut PaintContext<'_>,
        model: &dyn ItemModel,
        key: ItemKey,
        column: usize,
        state: ItemState,
    ) {
        let row = ItemState {
            focus_visible: false,
            ..state
        };
        TextItemDelegate::table().paint_cell(cx, model, key, column, row);
        if state.current && state.focused && self.column.get_untracked().ok() == Some(column) {
            let query = StyleQuery::new(ControlKind::Table, Slot::Root);
            let ring = cx.style(query.state(VisualState::FOCUS_VISIBLE));
            let bounds = cx.bounds;
            cx.slot(bounds, &ring, |_, _| {});
        }
    }
}

/// The table view of the cell editing page, once it mounts.
type TableView = Rc<RefCell<Option<Rc<RefCell<ItemView>>>>>;

/// The rectangle of the cell of `key` and `column` in the table, from its
/// top-left corner, or `None` when the row is not in the view.
fn cell_rect(table: &TableView, widths: &[f64], key: ItemKey, column: usize) -> Option<Rect> {
    let table = table.borrow();
    let view = table.as_ref()?.borrow();
    let index = view.index_of(key)?;
    let x = widths.iter().take(column).sum();
    let height = view.row_height();
    let y = index as f64 * height - view.offset();
    Some(Rect::from_xywh(x, y, *widths.get(column)?, height))
}

/// A product table edited in place: Left and Right or a click choose the
/// column, Return (or a double click) edits over the cell, Return
/// commits, and Escape cancels. A row change or blur commits a valid
/// edit and keeps an invalid one open with its message.
struct CellEditPage;

impl View for CellEditPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let model = SharedModel::new(products(cx.env(), 30));
        let header = product_header(cx.env());
        let widths = cx.state(header.borrow().widths());
        // Edits change cells, never rows, so the view never builds again
        // for them.
        let revision = cx.state(0_u64);
        let selected = cx.state(Vec::new());
        let column = cx.state(1_usize);
        let edit = cx.state(None::<CellEdit>);
        let error = cx.state(None::<ValidationError>);
        let table_focused = cx.state(false);
        let edit_focused = cx.state(false);
        let validation = cx.memo(move || match error.get() {
            Some(message) => ValidationState::Invalid(message),
            None => edit
                .get()
                .map_or(ValidationState::Valid, |edit| edit.validation()),
        });
        let editor = Editor {
            model: model.clone(),
            edit,
            error,
            validation,
            table_focused,
            edit_focused,
            env: cx.env().clone(),
        };
        let env = cx.env().clone();
        let opened = model.clone();
        let begin = move || {
            let Some(key) = selected.get().first().copied() else {
                return;
            };
            let started = CellEdit::begin(
                &*opened.model.borrow(),
                &DefaultEditorFactory,
                key,
                column.get(),
                env.locale(),
            )
            .map(|edit| edit.with_range(Some(0.0), None));
            error.set(None);
            let started_any = started.is_some();
            edit.set(started);
            if started_any {
                edit_focused.set(true);
            }
        };
        // Blur commits a valid edit, and keeps an invalid one open.
        let blurred = editor.clone();
        cx.effect(move || {
            if !edit_focused.get() && blurred.edit.get_untracked().ok().flatten().is_some() {
                blurred.commit();
            }
        });
        let bar = header_bar(header, widths, |_| {});
        let table_view: TableView = Rc::default();
        let (shown, named, mounted) = (model.clone(), model.clone(), table_view.clone());
        let table = rows(ControlKind::Table, &model, revision, selected, move |env| {
            let name = env.text("cell-edit.products", &[]);
            let mut view = ItemView::new(name, SemanticRole::Table, super::ROW)
                .with_model(Box::new(shown.clone()), Box::new(CellDelegate { column }));
            view.set_column_widths(widths.get());
            let view = Rc::new(RefCell::new(view));
            *mounted.borrow_mut() = Some(view.clone());
            let rtl = env.direction() == LayoutDirection::Rtl;
            Handle::shared(view)
                .with_keys(
                    &[Key::ArrowLeft, Key::ArrowRight],
                    Rc::new(move |key| {
                        let next = (key == Key::ArrowRight) != rtl;
                        column.update(|column| {
                            *column = if next {
                                (*column + 1).min(COLUMNS.len() - 1)
                            } else {
                                column.saturating_sub(1)
                            }
                        });
                    }),
                )
                .with_press(Rc::new(move |point, bounds| {
                    let x = point.x - bounds.origin.x;
                    let widths = widths.get_untracked().unwrap_or_default();
                    let mut end = 0.0;
                    if let Some(picked) = widths.iter().position(|width| {
                        end += width;
                        x < end
                    }) {
                        column.set(picked);
                    }
                }))
        })
        .focused(table_focused.binding())
        .on_message({
            let editor = editor.clone();
            move |message| match message {
                ControlMessage::ItemActivated(_) => begin(),
                ControlMessage::Changed(_) => editor.commit(),
                _ => {}
            }
        })
        .accessibility_value({
            let env = cx.env().clone();
            move || match edit.get() {
                Some(edit) => env.text(
                    "cell-edit.editing_value",
                    &[
                        ("column", &column_title(&env, edit.column)),
                        ("text", &edit.text),
                    ],
                ),
                None => first_selected(&named, &selected.get()),
            }
        });
        let cell = cx.memo(move || edit.get().map(|edit| (edit.key, edit.column)));
        let (placed, rtl) = (editor.clone(), cx.env().direction() == LayoutDirection::Rtl);
        let over = Dynamic::new(move || {
            let (key, column) = cell.with(|cell| *cell).ok().flatten()?;
            let widths = widths.get_untracked().unwrap_or_default();
            let rect = cell_rect(&table_view, &widths, key, column)?;
            let (top, start) = (rect.origin.y.max(0.0), rect.origin.x);
            // The table columns run left to right in both directions.
            let (edges, alignment) = if rtl {
                (Edges::new(top, 0.0, 0.0, start), Alignment::TOP_TRAILING)
            } else {
                (Edges::new(top, start, 0.0, 0.0), Alignment::TOP_LEADING)
            };
            let field = placed.clone().field(column, rect.size);
            Some(field.padding(edges).frame(fill().alignment(alignment)))
        });
        let (hint_env, message) = (cx.env().clone(), editor);
        VStack::new((
            bar,
            ZStack::new((table.frame(fill()), over)).frame(fill()),
            Text::new(
                move || match (cell.with(Option::is_some), message.validation()) {
                    (Ok(true), ValidationState::Invalid(error)) => {
                        error.message(&message.env.localizer())
                    }
                    (Ok(true), _) => String::new(),
                    _ => {
                        let title = column_title(&hint_env, column.get());
                        hint_env.text("cell-edit.hint", &[("column", &title)])
                    }
                },
            )
            .text_style(TextPreset::Small)
            .foreground(ColorRole::MutedForeground)
            .padding(Space::S2),
        ))
        .alignment(HorizontalAlignment::Fill)
        .spacing(0.0)
        .frame(fill())
    }
}

/// What the cell editor field reads and writes.
#[derive(Clone)]
struct Editor {
    model: SharedModel<TreeStore<Product>>,
    edit: State<Option<CellEdit>>,
    error: State<Option<ValidationError>>,
    validation: choscorui::compose::view::Memo<ValidationState>,
    table_focused: State<bool>,
    edit_focused: State<bool>,
    env: Env,
}

impl Editor {
    fn validation(&self) -> ValidationState {
        self.validation
            .with(Clone::clone)
            .unwrap_or(ValidationState::Valid)
    }

    /// Writes a valid edit and returns to the table; keeps an invalid one
    /// open with its message. An empty name gets its own message.
    fn commit(&self) {
        let Some(edit) = self.edit.get_untracked().ok().flatten() else {
            return;
        };
        let result = if edit.column == 0 && edit.text.trim().is_empty() {
            Err(name_required(&self.env))
        } else {
            edit.commit(&mut *self.model.model.borrow_mut())
        };
        match result {
            Ok(()) => self.close(),
            Err(error) => self.error.set(Some(error)),
        }
    }

    fn close(&self) {
        self.edit.set(None);
        self.error.set(None);
        self.table_focused.set(true);
    }

    /// The editor of the open edit, in a cell of `size`, named after
    /// `column`. A bool column edits with a checkbox that Space toggles.
    fn field(self, column: usize, size: Size) -> impl View {
        let edit = self.edit.get_untracked().ok().flatten();
        let boolean = edit.is_some_and(|edit| edit.checked().is_some());
        let focused = self.edit_focused.binding();
        let editor = if boolean {
            Either::Left(self.checkbox(column))
        } else {
            Either::Right(self.input(column))
        };
        editor
            .focused(focused)
            .frame(Frame::new().width(size.width).height(size.height))
    }

    /// The text field of a text or number column. A number column takes
    /// only the keys that can make a number.
    fn input(self, column: usize) -> impl View {
        let (built, cancel, shown, typed, committed) = (
            self.clone(),
            self.clone(),
            self.clone(),
            self.clone(),
            self.clone(),
        );
        let cancel: Rc<dyn Fn(Key)> = Rc::new(move |_| cancel.close());
        Configured::custom(ControlKind::Input, move |env| {
            let title = column_title(env, column);
            let name = env.text("cell-edit.editing", &[("column", &title)]);
            let filter = built
                .edit
                .get_untracked()
                .ok()
                .flatten()
                .map(|edit| edit.filter())
                .unwrap_or_default();
            let mut input = controls::Input::new(name, "").with_filter(filter);
            input.set_validation(built.validation());
            Handle::keyed(input, &[Key::Escape], cancel.clone())
        })
        .value(move || {
            ControlValue::Text(shown.edit.get().map(|edit| edit.text).unwrap_or_default())
        })
        .on_message(move |message| {
            if let ControlMessage::Changed(ControlValue::Text(text)) = message {
                typed.edit.update(|edit| {
                    if let Some(edit) = edit {
                        edit.set_text(text.clone());
                    }
                });
                typed.error.set(None);
            }
        })
        .on_activate(move || committed.commit())
    }

    /// The checkbox of a bool column: Space toggles, Return commits, and
    /// Escape cancels.
    fn checkbox(self, column: usize) -> impl View {
        let (keys, shown, toggled) = (self.clone(), self.clone(), self.clone());
        let keys: Rc<dyn Fn(Key)> = Rc::new(move |key| {
            if key == Key::Enter {
                keys.commit();
            } else {
                keys.close();
            }
        });
        Configured::custom(ControlKind::Checkbox, move |env| {
            let title = column_title(env, column);
            let name = env.text("cell-edit.editing", &[("column", &title)]);
            Handle::keyed(
                controls::Checkbox::new(name, false),
                &[Key::Escape, Key::Enter],
                keys.clone(),
            )
        })
        .value(move || {
            let checked = shown.edit.get().and_then(|edit| edit.checked());
            ControlValue::Bool(checked.unwrap_or(false))
        })
        .on_message(move |message| {
            if let ControlMessage::Changed(ControlValue::Bool(checked)) = message {
                toggled.edit.update(|edit| {
                    if let Some(edit) = edit {
                        edit.set_checked(*checked);
                    }
                });
                toggled.error.set(None);
            }
        })
    }
}

/// A validation message under a field.
fn problem(text: String) -> impl View {
    Text::new(text)
        .text_style(TextPreset::Small)
        .foreground(ColorRole::Destructive)
}

pub fn cell_edit(_: &PageCx) -> AnyView {
    CellEditPage.into_any()
}

// ---------------------------------------------------------------------------
// Data mapper

/// The mapped columns.
const MAPPED: [usize; 3] = [0, 1, 2];

/// The error of an empty product name in `env`.
fn name_required(env: &Env) -> ValidationError {
    ValidationError::Message(env.text("data-mapper.name_required", &[]))
}

/// Parses the text of mapped column `column` in `env`'s locale. A name
/// loses its surrounding spaces.
fn parse_mapped(env: &Env, column: usize, text: &str) -> Result<Value, ValidationError> {
    let locale = env.locale();
    let (text, message) = (text.trim(), |id: &str| {
        ValidationError::Message(env.text(id, &[]))
    });
    match column {
        1 => locale
            .parse_integer(text)
            .ok()
            .filter(|quantity| *quantity >= 0)
            .map(Value::Int)
            .ok_or_else(|| message("data-mapper.whole_number")),
        2 => locale
            .parse_decimal(text)
            .ok()
            .filter(|price| *price >= 0.0)
            .map(Value::Float)
            .ok_or_else(|| message("data-mapper.price")),
        _ if text.is_empty() => Err(name_required(env)),
        _ => Ok(Value::Text(text.to_owned())),
    }
}

/// The form of the data mapper page: the mapper, its model, the errors
/// of the current row, and a version that changes with the mapper.
#[derive(Clone)]
struct Form {
    model: SharedModel<TreeStore<Product>>,
    mapper: Rc<RefCell<DataMapper>>,
    version: State<u64>,
    errors: State<Vec<(usize, ValidationError)>>,
    env: Env,
}

impl Form {
    /// Reports a change of the mapper: the version, and the errors of the
    /// current row.
    fn changed(&self) {
        let mapper = self.mapper.borrow();
        let errors = MAPPED
            .iter()
            .filter_map(|column| Some((*column, mapper.error(*column)?.clone())))
            .collect();
        self.errors.set(errors);
        self.version.update(|version| *version += 1);
    }

    /// The text of mapped column `column`: pending text, or the model's.
    fn value(&self, column: usize) -> String {
        self.version.get();
        self.mapper
            .borrow()
            .values(&*self.model.model.borrow())
            .into_iter()
            .find(|(mapped, _)| *mapped == column)
            .map(|(_, text)| text)
            .unwrap_or_default()
    }

    /// Keeps `text` pending, and checks a field with an error again.
    fn edit(&self, column: usize, text: &str) {
        {
            let mut mapper = self.mapper.borrow_mut();
            mapper.edit(&*self.model.model.borrow(), column, text);
            mapper.check(column, |column, text| parse_mapped(&self.env, column, text));
        }
        self.changed();
    }

    /// Whether mapped column `column` of the current row has unsaved
    /// text.
    fn pending(&self, column: usize) -> bool {
        self.version.get();
        self.mapper.borrow().is_pending(column)
    }

    fn error(&self, column: usize) -> Option<ValidationError> {
        self.errors
            .get()
            .into_iter()
            .find(|(mapped, _)| *mapped == column)
            .map(|(_, error)| error)
    }

    /// Writes the pending fields; invalid ones keep their messages.
    fn submit(&self) {
        // The mapper keeps the errors with their fields.
        let _ = self
            .mapper
            .borrow_mut()
            .submit(&mut *self.model.model.borrow_mut(), |column, text| {
                parse_mapped(&self.env, column, text)
            });
        self.changed();
    }

    /// Binds the form to the row at `row`.
    fn follow(&self, row: Option<usize>) {
        let key = row.and_then(|row| self.model.keys(&[row]).first().copied());
        if self.mapper.borrow_mut().set_current(key) {
            self.changed();
        }
    }

    /// The caption, field, and message of mapped column `column`.
    fn field(self, column: usize) -> impl View {
        let (titled, named, built, shown, edited, submitted, checked) = (
            self.clone(),
            self.clone(),
            self.clone(),
            self.clone(),
            self.clone(),
            self.clone(),
            self.clone(),
        );
        VStack::new((
            Text::new(move || {
                let label = column_title(&titled.env, column);
                if titled.pending(column) {
                    titled.env.text("data-mapper.unsaved", &[("label", &label)])
                } else {
                    label
                }
            })
            .text_style(TextPreset::Small)
            .foreground(ColorRole::MutedForeground),
            Input::new(
                move || column_title(&named.env, column),
                Binding::new(
                    move || shown.value(column),
                    move |text: String| edited.edit(column, &text),
                ),
            )
            .with(move |mut input| {
                if let Some(error) = built.error(column) {
                    input.set_validation(ValidationState::Invalid(error));
                }
                input
            })
            .on_activate(move || submitted.submit())
            .frame(Frame::new().width(260.0)),
            Dynamic::new(move || {
                checked
                    .error(column)
                    .map(|error| problem(error.message(&checked.env.localizer())))
            }),
        ))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S1)
    }
}

/// A product list with a form bound to the selected row. Edits stay
/// pending per row until Submit writes the valid ones.
struct DataMapperPage;

impl View for DataMapperPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let model = SharedModel::new(products(cx.env(), 20));
        let first = model.model.borrow().key(None, 0);
        let mut mapper = DataMapper::new(MAPPED.to_vec()).with_locale(cx.env().locale());
        mapper.set_current(first);
        let form = Form {
            model: model.clone(),
            mapper: Rc::new(RefCell::new(mapper)),
            version: cx.state(0),
            errors: cx.state(Vec::new()),
            env: cx.env().clone(),
        };
        // Submits change cells, never rows, so the list never builds
        // again for them. The list starts on the bound row.
        let revision = cx.state(0_u64);
        let selected = cx.state(first.into_iter().collect::<Vec<_>>());
        let version = form.version;
        let (shown, named, following, submitted) =
            (model.clone(), model.clone(), form.clone(), form.clone());
        let list = rows(
            ControlKind::ListView,
            &model,
            revision,
            selected,
            move |env| {
                let mut view = item_view(
                    &env.text("header-view.products", &[]),
                    SemanticRole::List,
                    &shown,
                    TextItemDelegate::default(),
                );
                view.set_selection_mode(SelectionMode::Single);
                Handle::new(view)
            },
        )
        .on_message(move |message| {
            if let ControlMessage::Changed(ControlValue::Indices(rows)) = message {
                following.follow(rows.first().copied());
            }
        })
        .accessibility_value(move || {
            // A submit changes the cells of the selected row.
            version.get();
            first_selected(&named, &selected.get())
        });
        HStack::new((
            list.frame(
                Frame::new()
                    .width(240.0)
                    .max_height(f64::INFINITY)
                    .alignment(Alignment::FILL),
            ),
            VStack::new((
                form.clone().field(MAPPED[0]),
                form.clone().field(MAPPED[1]),
                form.clone().field(MAPPED[2]),
                Button::new(t!("data-mapper.submit"), move || submitted.submit()),
            ))
            .alignment(HorizontalAlignment::Leading)
            .spacing(Space::S4),
            Spacer::new(),
        ))
        .alignment(VerticalAlignment::Top)
        .spacing(Space::S6)
        .frame(fill())
    }
}

pub fn data_mapper(_: &PageCx) -> AnyView {
    DataMapperPage.into_any()
}

#[cfg(test)]
mod tests {
    use super::super::tests::{press, set_value};
    use crate::ui::pages::tests::Harness;
    use choscorui::controls::ShortcutModifiers;
    use choscorui::core::{
        Key, NodeId, Point, PointerButton, PointerEvent, Rect, SemanticAction, SemanticRole,
    };

    fn press_key(page: &mut Harness, key: Key) {
        press(page, key, ShortcutModifiers::default());
    }

    #[test]
    fn header_view_sorts_the_products_by_the_focused_column() {
        let mut page = Harness::new("header-view");
        assert!(page.act("Name", SemanticAction::Activate));
        assert_eq!(page.value("Name").as_deref(), Some("ascending"));
        assert!(page.act("Products", SemanticAction::Focus));
        press_key(&mut page, Key::Home);
        let first = page.value("Products").unwrap_or_default();
        assert!(first.starts_with("Bánh mì, "), "{first}");
        assert!(page.act("Name", SemanticAction::Activate));
        assert_eq!(page.value("Name").as_deref(), Some("descending"));
        // The caption and the total row describe the table.
        let table = page.node("Products");
        assert_eq!(
            page.host.semantic(table).unwrap().description,
            "Quantities and prices of the products on sale. Total, 1440"
        );
    }

    #[test]
    fn cell_edit_rejects_text_in_a_number_column_and_commits_a_number() {
        let mut page = Harness::new("cell-edit");
        assert!(page.act("Editable products", SemanticAction::Focus));
        press_key(&mut page, Key::ArrowDown);
        press_key(&mut page, Key::Enter);
        assert_eq!(page.value("Editing Quantity").as_deref(), Some("0"));
        // The number column refuses letters.
        set_value(&mut page, "Editing Quantity", "many");
        assert_eq!(page.value("Editing Quantity").as_deref(), Some("0"));
        set_value(&mut page, "Editing Quantity", "");
        press_key(&mut page, Key::Enter);
        assert!(page.host.find("Enter a number").is_some());
        let field = page.node("Editing Quantity");
        assert!(page.host.semantic(field).unwrap().state.invalid);
        assert_eq!(
            page.value("Editable products").as_deref(),
            Some("Editing Quantity: ")
        );
        set_value(&mut page, "Editing Quantity", "12");
        press_key(&mut page, Key::Enter);
        assert!(page.host.find("Editing Quantity").is_none());
        let row = page.value("Editable products").unwrap_or_default();
        assert_eq!(row, "Cà phê sữa đá, 12, 1.50, Yes");
    }

    /// The bounds of the semantic node `node`.
    fn bounds(page: &mut Harness, node: NodeId) -> Rect {
        let (snapshot, _) = page.host.semantics();
        snapshot
            .into_iter()
            .find(|entry| entry.id == node)
            .map(|entry| entry.bounds)
            .unwrap_or_else(|| panic!("no node {node:?}"))
    }

    #[test]
    fn cell_edit_picks_a_column_by_click_and_edits_over_the_cell() {
        let mut page = Harness::new("cell-edit");
        let table = page.node("Editable products");
        let area = bounds(&mut page, table);
        // A click on the second row in the price column picks the column.
        let at = Point::new(area.origin.x + 200.0 + 100.0 + 10.0, area.origin.y + 36.0);
        let button = PointerButton::Primary;
        let none = ShortcutModifiers::default();
        page.host.pointer(
            PointerEvent::Down {
                position: at,
                button,
            },
            none,
        );
        page.host.pointer(
            PointerEvent::Up {
                position: at,
                button,
            },
            none,
        );
        page.host.layout();
        assert!(page.host.find("Selected column “Price” · Return edits, Left/Right or a click change the column").is_some());
        press_key(&mut page, Key::Enter);
        // The editor covers the price cell of the second row.
        let editor = page.node("Editing Price");
        let cell = bounds(&mut page, editor);
        assert_eq!(cell.origin.x, area.origin.x + 300.0);
        assert_eq!(cell.origin.y, area.origin.y + 24.0);
        // Blur keeps invalid text open with its message.
        set_value(&mut page, "Editing Price", "");
        page.host.focus(Some(table));
        page.host.layout();
        assert!(page.host.find("Editing Price").is_some());
        assert!(page.host.find("Enter a number").is_some());
        // Blur commits valid text, in the format that the table shows.
        let editor = page.node("Editing Price");
        page.host.focus(Some(editor));
        set_value(&mut page, "Editing Price", "2.25");
        page.host.focus(Some(table));
        page.host.layout();
        assert!(page.host.find("Editing Price").is_none());
        let row = page.value("Editable products").unwrap_or_default();
        assert_eq!(row, "Bánh mì, 7, 2.25, Yes");
    }

    #[test]
    fn cell_edit_moves_between_columns_and_cancels_with_escape() {
        let mut page = Harness::new("cell-edit");
        assert!(page.act("Editable products", SemanticAction::Focus));
        press_key(&mut page, Key::ArrowDown);
        press_key(&mut page, Key::ArrowRight);
        assert!(
            page.host
                .find("Selected column “Price” · Return edits, Left/Right or a click change the column")
                .is_some()
        );
        press_key(&mut page, Key::Enter);
        assert_eq!(page.value("Editing Price").as_deref(), Some("1.50"));
        press_key(&mut page, Key::Escape);
        assert!(page.host.find("Editing Price").is_none());
    }

    /// The text field named `name`; its caption has the same name.
    fn field(page: &mut Harness, name: &str) -> NodeId {
        let (snapshot, _) = page.host.semantics();
        snapshot
            .into_iter()
            .find(|entry| entry.node.name == name && entry.node.role == SemanticRole::TextField)
            .map(|entry| entry.id)
            .unwrap_or_else(|| panic!("no field named {name:?}"))
    }

    fn type_into(page: &mut Harness, name: &str, text: &str) {
        let node = field(page, name);
        assert!(
            page.host
                .perform(node, SemanticAction::SetValue, Some(text))
        );
        page.host.layout();
    }

    #[test]
    fn data_mapper_keeps_invalid_text_pending_until_it_parses() {
        let mut page = Harness::new("data-mapper");
        // The list starts on the bound row.
        assert_eq!(
            page.value("Products").as_deref(),
            Some("Cà phê sữa đá, 0, 1.50, Yes")
        );
        let name = field(&mut page, "Name");
        let value = page.host.semantic(name).unwrap().value;
        assert_eq!(value.as_deref(), Some("Cà phê sữa đá"));
        type_into(&mut page, "Quantity", "lots");
        assert!(page.host.find("Quantity · unsaved changes").is_some());
        assert!(page.host.find("Name · unsaved changes").is_none());
        assert!(page.act("Submit", SemanticAction::Activate));
        assert!(page.host.find("Enter a whole number").is_some());
        let quantity = field(&mut page, "Quantity");
        assert!(page.host.semantic(quantity).unwrap().state.invalid);
        // The error stays with its text on another row and back.
        assert!(page.act("Products", SemanticAction::Focus));
        press_key(&mut page, Key::ArrowDown);
        assert!(page.host.find("Enter a whole number").is_none());
        press_key(&mut page, Key::ArrowUp);
        assert!(page.host.find("Enter a whole number").is_some());
        // An edit checks the field again.
        type_into(&mut page, "Quantity", "5");
        assert!(page.host.find("Enter a whole number").is_none());
        type_into(&mut page, "Name", "  Trà đá  ");
        assert!(page.act("Submit", SemanticAction::Activate));
        assert!(page.host.find("Quantity · unsaved changes").is_none());
        // Text equal to the saved value is not unsaved.
        type_into(&mut page, "Quantity", "5");
        assert!(page.host.find("Quantity · unsaved changes").is_none());
        assert_eq!(
            page.value("Products").as_deref(),
            Some("Trà đá, 5, 1.50, Yes")
        );
    }
}
