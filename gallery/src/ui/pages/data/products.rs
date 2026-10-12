//! The typed product table and the pages built on it: a sortable,
//! resizable header, cell editing, and a form mapped to the selected row.

use super::hosting::{Handle, SharedModel};
use super::{fill, first_selected, item_view, rows};
use crate::ui::page::PageCx;
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
            3 if display => Value::Text(if self.in_stock { "Yes" } else { "No" }.into()),
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

const COLUMNS: [&str; 4] = ["Name", "Quantity", "Price", "In stock"];

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

fn products(count: usize) -> TreeStore<Product> {
    let mut store = TreeStore::new(4);
    store.set_headers(COLUMNS);
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
        quantity: ((i * 7) % 50) as i64,
        price: 1.5 + (i * 13 % 40) as f64 * 0.75,
        in_stock: i % 4 != 3,
    });
    store.insert(None, 0, rows).expect("root insert is valid");
    store
}

fn product_header() -> Rc<RefCell<HeaderView>> {
    Rc::new(RefCell::new(HeaderView::new(
        COLUMNS
            .iter()
            .zip([200.0, 100.0, 100.0, 90.0])
            .map(|(title, width)| HeaderColumn {
                title: (*title).to_owned(),
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

/// A sortable, resizable table header over 60 products.
struct HeaderViewPage;

impl View for HeaderViewPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let model = SharedModel::proxied(products(60));
        let header = product_header();
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
        let list = rows(ControlKind::Table, &model, revision, selected, move |_| {
            let mut view = item_view(
                "Products",
                SemanticRole::Table,
                &shown,
                TextItemDelegate::table(),
            );
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
        let model = SharedModel::new(products(30));
        let header = product_header();
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
                "Editable products",
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
        .accessibility_value(move || match edit.get() {
            Some(edit) => format!("Editing {}: {}", COLUMNS[edit.column], edit.text),
            None => first_selected(&named, &selected.get()),
        });
        let editing = cx.memo(move || edit.get().is_some());
        let validation = cx.memo(move || match error.get() {
            Some(message) => ValidationState::Invalid(message),
            None => edit
                .get()
                .map_or(ValidationState::Valid, |edit| edit.validation()),
        });
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
                    Either::Right(
                        Text::new(move || {
                            format!(
                                "Editing column “{}” · Return edits, Left/Right change column",
                                COLUMNS[column.get()]
                            )
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
        let name = format!("Editing {}", COLUMNS[column]);
        let (built, cancel, shown, typed, committed, message) = (
            self.clone(),
            self.clone(),
            self.clone(),
            self.clone(),
            self.clone(),
            self.clone(),
        );
        let cancel: Rc<dyn Fn(Key)> = Rc::new(move |_| cancel.close());
        let field = Configured::custom(ControlKind::Input, move |_| {
            let mut input = controls::Input::new(name.clone(), "");
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

const MAPPED: [(usize, &str); 3] = [(0, "Name"), (1, "Quantity"), (2, "Price")];

/// Parses the text of mapped column `column` in `env`'s locale.
fn parse_mapped(env: &Env, column: usize, text: &str) -> Result<Value, String> {
    let locale = env.locale();
    match column {
        1 => locale
            .parse_integer(text.trim())
            .map(Value::Int)
            .map_err(|_| "Enter a whole number".into()),
        2 => locale
            .parse_decimal(text.trim())
            .map(Value::Float)
            .map_err(|_| "Enter a price".into()),
        _ if text.trim().is_empty() => Err("A name is required".into()),
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
    fn field(self, column: usize, label: &'static str) -> impl View {
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
                if titled.pending() {
                    format!("{label} · unsaved changes")
                } else {
                    label.to_owned()
                }
            })
            .text_style(TextPreset::Small)
            .foreground(ColorRole::MutedForeground),
            Configured::custom(ControlKind::Input, move |_| {
                let mut input = controls::Input::new(label, "");
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
        let model = SharedModel::new(products(20));
        let mut mapper = DataMapper::new(MAPPED.iter().map(|(column, _)| *column).collect());
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
            move |_| {
                Handle::new(item_view(
                    "Products",
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
                form.clone().field(MAPPED[0].0, MAPPED[0].1),
                form.clone().field(MAPPED[1].0, MAPPED[1].1),
                form.clone().field(MAPPED[2].0, MAPPED[2].1),
                Button::new("Submit", move || submitted.submit()),
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
