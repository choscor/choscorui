//! The typed product table and the pages built on it: a sortable,
//! resizable header, cell editing, and a form mapped to the selected row.

use super::hosting::{Handle, SharedModel};
use super::{fill, first_selected, item_view, rows};
use crate::ui::page::PageCx;
use choscorui::compose::t;
use choscorui::compose::view::{
    Alignment, AnyView, Button, Configured, Cx, Dynamic, Either, Env, Frame, HStack,
    HorizontalAlignment, Spacer, State, Text, VStack, VerticalAlignment, View, ViewExt,
};
use choscorui::controls::{
    self, CellEdit, ControlKind, ControlMessage, ControlValue, DataMapper, DefaultEditorFactory,
    HeaderColumn, HeaderView, TextItemDelegate, ValidationState,
};
use choscorui::core::{
    ColorRole, ItemData, ItemModel, Key, LayoutDirection, ModelError, Role, SemanticRole, Space,
    TextPreset, TreeStore, Value,
};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub struct Product {
    pub name: String,
    pub quantity: i64,
    pub price: f64,
    pub in_stock: bool,
    /// What the in-stock column shows when `in_stock` is true, and when
    /// it is false.
    pub stock_text: Rc<[String; 2]>,
}

impl ItemData for Product {
    fn data(&self, column: usize, role: Role) -> Value {
        let display = matches!(role, Role::Display | Role::Accessibility);
        match column {
            0 if display || matches!(role, Role::Edit | Role::Sort) => {
                Value::Text(self.name.clone())
            }
            1 if display => Value::Text(self.quantity.to_string()),
            1 if matches!(role, Role::Edit | Role::Sort) => Value::Int(self.quantity),
            2 if display => Value::Text(format!("{:.2}", self.price)),
            2 if matches!(role, Role::Edit | Role::Sort) => Value::Float(self.price),
            3 if display => Value::Text(self.stock_text[usize::from(!self.in_stock)].clone()),
            3 if matches!(role, Role::Edit | Role::Sort) => Value::Bool(self.in_stock),
            _ => Value::None,
        }
    }

    fn set_data(&mut self, column: usize, _role: Role, value: Value) -> Result<(), ModelError> {
        match (column, value) {
            (0, Value::Text(name)) if !name.trim().is_empty() => self.name = name,
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
    let stock_text = Rc::new([
        env.text("header-view.yes", &[]),
        env.text("header-view.no", &[]),
    ]);
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
        stock_text: stock_text.clone(),
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
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
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

/// A product table edited in place: Left and Right choose the column,
/// Return (or a double click) edits, Return commits, Escape cancels.
struct CellEditPage;

impl View for CellEditPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let model = SharedModel::new(products(cx.env(), 30));
        let header = product_header(cx.env());
        let widths = cx.state(header.borrow().widths());
        // Edits change cells, never rows, so the view never builds again
        // for them.
        let revision = cx.state(0_u64);
        let selected = cx.state(Vec::new());
        let column = cx.state(1_usize);
        let edit = cx.state(None::<CellEdit>);
        let error = cx.state(None::<String>);
        let table_focused = cx.state(false);
        let edit_focused = cx.state(false);
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
            );
            error.set(None);
            let started_any = started.is_some();
            edit.set(started);
            if started_any {
                edit_focused.set(true);
            }
        };
        let bar = header_bar(header, widths, |_| {});
        let (shown, named) = (model.clone(), model.clone());
        let table = rows(ControlKind::Table, &model, revision, selected, move |env| {
            let mut view = item_view(
                &env.text("cell-edit.products", &[]),
                SemanticRole::Table,
                &shown,
                TextItemDelegate::table(),
            );
            view.set_column_widths(widths.get());
            let rtl = env.direction() == LayoutDirection::Rtl;
            Handle::keyed(
                view,
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
        })
        .focused(table_focused.binding())
        .on_message(move |message| match message {
            ControlMessage::ItemActivated(_) => begin(),
            ControlMessage::Changed(_) => {
                edit.set(None);
                error.set(None);
            }
            _ => {}
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
        let editing = cx.memo(move || edit.get().is_some());
        let validation = cx.memo(move || match error.get() {
            Some(message) => ValidationState::Invalid(message),
            None => edit
                .get()
                .map_or(ValidationState::Valid, |edit| edit.validation()),
        });
        let hint_env = cx.env().clone();
        let editor = Editor {
            model,
            edit,
            error,
            validation,
            table_focused,
            edit_focused,
        };
        VStack::new((
            bar,
            table.frame(fill()),
            Dynamic::new(move || {
                if editing.get() {
                    Either::Left(editor.clone().field())
                } else {
                    let env = hint_env.clone();
                    Either::Right(
                        Text::new(move || {
                            let title = column_title(&env, column.get());
                            env.text("cell-edit.hint", &[("column", &title)])
                        })
                        .text_style(TextPreset::Small)
                        .foreground(ColorRole::MutedForeground),
                    )
                }
            }),
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
    error: State<Option<String>>,
    validation: choscorui::compose::view::Memo<ValidationState>,
    table_focused: State<bool>,
    edit_focused: State<bool>,
}

impl Editor {
    fn validation(&self) -> ValidationState {
        self.validation
            .with(Clone::clone)
            .unwrap_or(ValidationState::Valid)
    }

    /// Writes a valid edit and returns to the table; keeps an invalid one
    /// open with its message.
    fn commit(&self) {
        let Some(edit) = self.edit.get() else {
            return;
        };
        match edit.commit(&mut *self.model.model.borrow_mut()) {
            Ok(()) => self.close(),
            Err(message) => self.error.set(Some(message)),
        }
    }

    fn close(&self) {
        self.edit.set(None);
        self.error.set(None);
        self.table_focused.set(true);
    }

    /// The field for the open edit, named after its column, with the
    /// validation message under it.
    fn field(self) -> impl View {
        let column = self
            .edit
            .get_untracked()
            .ok()
            .flatten()
            .map_or(0, |edit| edit.column);
        let (built, cancel, shown, typed, committed, message) = (
            self.clone(),
            self.clone(),
            self.clone(),
            self.clone(),
            self.clone(),
            self.clone(),
        );
        let cancel: Rc<dyn Fn(Key)> = Rc::new(move |_| cancel.close());
        let field = Configured::custom(ControlKind::Input, move |env| {
            let title = column_title(env, column);
            let name = env.text("cell-edit.editing", &[("column", &title)]);
            let mut input = controls::Input::new(name, "");
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
        .focused(self.edit_focused.binding());
        VStack::new((
            field.frame(Frame::new().width(260.0)),
            Dynamic::new(move || match message.validation() {
                ValidationState::Invalid(text) => Some(problem(text)),
                _ => None,
            }),
        ))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S1)
        .padding(Space::S2)
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

/// Parses the text of mapped column `column` in `env`'s locale.
fn parse_mapped(env: &Env, column: usize, text: &str) -> Result<Value, String> {
    let locale = env.locale();
    match column {
        1 => locale
            .parse_integer(text.trim())
            .map(Value::Int)
            .map_err(|_| env.text("data-mapper.whole_number", &[])),
        2 => locale
            .parse_decimal(text.trim())
            .map(Value::Float)
            .map_err(|_| env.text("data-mapper.price", &[])),
        _ if text.trim().is_empty() => Err(env.text("data-mapper.name_required", &[])),
        _ => Ok(Value::Text(text.to_owned())),
    }
}

/// The form of the data mapper page: the mapper, its model, the errors
/// of the last submit, and a version that changes with the mapper.
#[derive(Clone)]
struct Form {
    model: SharedModel<TreeStore<Product>>,
    mapper: Rc<RefCell<DataMapper>>,
    version: State<u64>,
    errors: State<Vec<(usize, String)>>,
    env: Env,
}

impl Form {
    fn changed(&self) {
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

    fn edit(&self, column: usize, text: &str) {
        self.mapper.borrow_mut().edit(column, text);
        self.changed();
    }

    /// Whether the current row has unsaved text.
    fn pending(&self) -> bool {
        self.version.get();
        let mapper = self.mapper.borrow();
        mapper.current().is_some_and(|key| mapper.has_pending(key))
    }

    fn error(&self, column: usize) -> Option<String> {
        self.errors
            .get()
            .into_iter()
            .find(|(mapped, _)| *mapped == column)
            .map(|(_, message)| message)
    }

    /// Writes the pending fields; invalid ones keep their messages.
    fn submit(&self) {
        let result = self
            .mapper
            .borrow_mut()
            .submit(&mut *self.model.model.borrow_mut(), |column, text| {
                parse_mapped(&self.env, column, text)
            });
        self.errors.set(result.err().unwrap_or_default());
        self.changed();
    }

    /// Binds the form to the row at `row`.
    fn follow(&self, row: Option<usize>) {
        let key = row.and_then(|row| self.model.keys(&[row]).first().copied());
        if self.mapper.borrow_mut().set_current(key) {
            self.errors.set(Vec::new());
            self.changed();
        }
    }

    /// The caption, field, and message of mapped column `column`.
    fn field(self, column: usize) -> impl View {
        let (titled, built, shown, edited, submitted, checked) = (
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
                if titled.pending() {
                    titled.env.text("data-mapper.unsaved", &[("label", &label)])
                } else {
                    label
                }
            })
            .text_style(TextPreset::Small)
            .foreground(ColorRole::MutedForeground),
            Configured::custom(ControlKind::Input, move |env| {
                let mut input = controls::Input::new(column_title(env, column), "");
                if let Some(message) = built.error(column) {
                    input.set_validation(ValidationState::Invalid(message));
                }
                input
            })
            .value(move || ControlValue::Text(shown.value(column)))
            .on_message(move |message| {
                if let ControlMessage::Changed(ControlValue::Text(text)) = message {
                    edited.edit(column, text);
                }
            })
            .on_activate(move || submitted.submit())
            .frame(Frame::new().width(260.0)),
            Dynamic::new(move || checked.error(column).map(problem)),
        ))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S1)
    }
}

/// A product list with a form bound to the selected row. Edits stay
/// pending per row until Submit writes the valid ones.
struct DataMapperPage;

impl View for DataMapperPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let model = SharedModel::new(products(cx.env(), 20));
        let mut mapper = DataMapper::new(MAPPED.to_vec());
        mapper.set_current(model.model.borrow().key(None, 0));
        let form = Form {
            model: model.clone(),
            mapper: Rc::new(RefCell::new(mapper)),
            version: cx.state(0),
            errors: cx.state(Vec::new()),
            env: cx.env().clone(),
        };
        // Submits change cells, never rows, so the list never builds
        // again for them.
        let revision = cx.state(0_u64);
        let selected = cx.state(Vec::new());
        let (shown, named, following, submitted) =
            (model.clone(), model.clone(), form.clone(), form.clone());
        let list = rows(
            ControlKind::ListView,
            &model,
            revision,
            selected,
            move |env| {
                Handle::new(item_view(
                    &env.text("header-view.products", &[]),
                    SemanticRole::List,
                    &shown,
                    TextItemDelegate::default(),
                ))
            },
        )
        .on_message(move |message| {
            if let ControlMessage::Changed(ControlValue::Indices(rows)) = message {
                following.follow(rows.first().copied());
            }
        })
        .accessibility_value(move || first_selected(&named, &selected.get()));
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
    use choscorui::core::{Key, NodeId, SemanticAction, SemanticRole};

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
        set_value(&mut page, "Editing Quantity", "many");
        press_key(&mut page, Key::Enter);
        assert!(page.host.find("Enter a whole number").is_some());
        let field = page.node("Editing Quantity");
        assert!(page.host.semantic(field).unwrap().state.invalid);
        assert_eq!(
            page.value("Editable products").as_deref(),
            Some("Editing Quantity: many")
        );
        set_value(&mut page, "Editing Quantity", "12");
        press_key(&mut page, Key::Enter);
        assert!(page.host.find("Editing Quantity").is_none());
        let row = page.value("Editable products").unwrap_or_default();
        assert_eq!(row, "Cà phê sữa đá, 12, 1.50, Yes");
    }

    #[test]
    fn cell_edit_moves_between_columns_and_cancels_with_escape() {
        let mut page = Harness::new("cell-edit");
        assert!(page.act("Editable products", SemanticAction::Focus));
        press_key(&mut page, Key::ArrowDown);
        press_key(&mut page, Key::ArrowRight);
        assert!(
            page.host
                .find("Editing column “Price” · Return edits, Left/Right change column")
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
        let name = field(&mut page, "Name");
        let value = page.host.semantic(name).unwrap().value;
        assert_eq!(value.as_deref(), Some("Cà phê sữa đá"));
        type_into(&mut page, "Quantity", "lots");
        assert!(page.host.find("Quantity · unsaved changes").is_some());
        assert!(page.act("Submit", SemanticAction::Activate));
        assert!(page.host.find("Enter a whole number").is_some());
        let quantity = field(&mut page, "Quantity");
        assert!(page.host.semantic(quantity).unwrap().state.invalid);
        type_into(&mut page, "Quantity", "5");
        assert!(page.act("Submit", SemanticAction::Activate));
        assert!(page.host.find("Enter a whole number").is_none());
        assert!(page.host.find("Quantity · unsaved changes").is_none());
        assert!(page.act("Products", SemanticAction::Focus));
        press_key(&mut page, Key::ArrowDown);
        assert_eq!(
            page.value("Products").as_deref(),
            Some("Cà phê sữa đá, 5, 1.50, Yes")
        );
    }
}
