//! View constructors for model views and rows: tables, table headers,
//! column browsers, and item groups. Like the views in `widgets`, each
//! takes its content, a `Binding` where the control edits a value, and an
//! action where it fires one.

use super::View;
use super::configured::Configured;
use super::control::ControlView;
use super::env::Env;
use super::host::Cx;
use super::state::{Binding, IntoLive};
use super::text::TextContent;
use super::widgets::{Items, as_indices, label, text};
use choscorui_controls as controls;
use choscorui_controls::{
    ControlContext, ControlEvent, ControlKind, ControlMessage, ControlValue, CustomControl,
    HeaderColumn, MeasureContext, PaintContext,
};
use choscorui_core::{
    EventReply, ItemModel, Measured, Rect, SemanticAction, SemanticNode, SemanticRole, SortOrder,
};
use std::cell::RefCell;
use std::rc::Rc;

impl Configured<controls::ItemView> {
    /// The rows of the model that `model` makes as a table, one cell for
    /// each column; `selection` edits the positions of the selected rows.
    /// Like [`List::new`](super::List), the table paints only the rows in
    /// view. Add footer rows and a caption with `.with` and
    /// `ItemView::with_footer` and `ItemView::with_caption`.
    pub fn table<M>(
        name: impl IntoLive<TextContent, M>,
        model: impl Fn() -> Box<dyn ItemModel> + 'static,
        selection: Binding<Vec<usize>>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Table, move |env| {
            controls::ItemView::new(text(&name, env), SemanticRole::Table, 1.0)
                .with_model(model(), Box::new(controls::TextItemDelegate::table()))
                .with_styled_rows()
        })
        .bind(selection, ControlValue::Indices, as_indices)
    }
}

/// A sort of a table header: a column and its order.
type Sort = Option<(usize, SortOrder)>;

/// The sort as the value that a [`HeaderLeaf`] takes: no indices for no
/// sort, otherwise the column and 0 for ascending or 1 for descending.
fn to_sort(sort: Sort) -> ControlValue {
    ControlValue::Indices(match sort {
        None => Vec::new(),
        Some((column, SortOrder::Ascending)) => vec![column, 0],
        Some((column, SortOrder::Descending)) => vec![column, 1],
    })
}

/// What the header had after the user's last input.
#[derive(Default)]
struct HeaderState {
    sort: Sort,
    widths: Vec<f64>,
}

/// A table header that writes its sort and widths to `state` after each
/// input, so the view's handlers can read them.
struct HeaderLeaf {
    header: controls::HeaderView,
    state: Rc<RefCell<HeaderState>>,
}

impl HeaderLeaf {
    fn record(&self) {
        let mut state = self.state.borrow_mut();
        state.sort = self.header.sort();
        state.widths = self.header.widths();
    }
}

impl CustomControl for HeaderLeaf {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        self.header.measure(cx)
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.header.arrange(cx, bounds);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let reply = self.header.event(cx, event);
        self.record();
        reply
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        CustomControl::paint(&self.header, cx);
    }

    fn semantics(&self) -> SemanticNode {
        CustomControl::semantics(&self.header)
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        let performed = self.header.perform(cx, action, value);
        self.record();
        performed
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.header.set_enabled(enabled);
    }

    /// Takes the sort that [`to_sort`] makes.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let sort = match value {
            ControlValue::Indices(parts) => match parts.as_slice() {
                [] => None,
                [column, 0] => Some((*column, SortOrder::Ascending)),
                [column, 1] => Some((*column, SortOrder::Descending)),
                _ => return false,
            },
            _ => return false,
        };
        self.header.restore_sort(sort);
        self.record();
        self.header.sort() == sort
    }
}

/// A table header over `columns`. A click or Space sorts by a column:
/// ascending, descending, then no sort. `sort` edits the sort. Dragging a
/// column edge, or `+` and `-`, resizes the column and runs `.on_resize`
/// with the new widths. The header keeps its widths when the columns
/// are made again, for example for a new locale.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::HeaderColumn;
/// struct Columns;
///
/// impl View for Columns {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let sort = cx.state(None);
///         let columns = vec![HeaderColumn { title: "Name".into(), width: 160.0, min_width: 60.0 }];
///         HeaderView::new(columns, sort.binding()).on_resize(|widths| println!("{widths:?}"))
///     }
/// }
/// ```
pub struct HeaderView {
    columns: Rc<dyn Items<HeaderColumn>>,
    sort: Binding<Sort>,
    on_resize: Option<Rc<dyn Fn(Vec<f64>)>>,
}

impl HeaderView {
    /// A header of `columns`; `sort` edits the sort column and order.
    pub fn new(columns: impl Items<HeaderColumn>, sort: Binding<Sort>) -> Self {
        Self {
            columns: Rc::new(columns),
            sort,
            on_resize: None,
        }
    }

    /// Runs `action` with the column widths after the user resizes one.
    #[must_use]
    pub fn on_resize(mut self, action: impl Fn(Vec<f64>) + 'static) -> Self {
        self.on_resize = Some(Rc::new(action));
        self
    }
}

impl View for HeaderView {
    fn mount(self, cx: &mut Cx) {
        let Self {
            columns,
            sort,
            on_resize,
        } = self;
        let state: Rc<RefCell<HeaderState>> = Rc::default();
        let built = state.clone();
        ControlView::build(move |env: &Env| {
            let mut header = controls::HeaderView::new(columns.items(env));
            header.restore_widths(&built.borrow().widths);
            Box::new(HeaderLeaf {
                header,
                state: built.clone(),
            })
        })
        .kind(ControlKind::Table)
        .value(move || to_sort(sort.get()))
        .on_message(move |message| match message {
            ControlMessage::ItemActivated(_) => sort.set(state.borrow().sort),
            ControlMessage::Changed(ControlValue::Index(_)) => {
                if let Some(action) = &on_resize {
                    action(state.borrow().widths.clone());
                }
            }
            _ => {}
        })
        .mount(cx);
    }
}

/// Columns of a tree model, one level each: Right opens a row's
/// children, and Left goes back. `path` edits the row chosen in each
/// column, from the first.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_core::{ItemModel, StringListModel};
/// struct Files;
///
/// impl View for Files {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let path = cx.state(Vec::new());
///         let model = |_: &Env| Box::new(StringListModel::from_strings(["Documents", "Music"])) as Box<dyn ItemModel>;
///         ColumnBrowser::new("Files", model, path.binding())
///     }
/// }
/// ```
pub type ColumnBrowser = Configured<controls::HostedColumnBrowser>;

impl Configured<controls::HostedColumnBrowser> {
    /// A column browser named `name` of the tree model that `model` makes
    /// in the view's environment.
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        model: impl Fn(&Env) -> Box<dyn ItemModel> + 'static,
        path: Binding<Vec<usize>>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::ListView, move |env| {
            controls::ColumnBrowser::new().hosted(text(&name, env), model(env))
        })
        .bind(path, ControlValue::Indices, as_indices)
    }
}

/// Rows of items, each with a title and optional media, description,
/// header, footer, and action. A link row reports its row index through
/// `.on_item`, and an action button its row and action index through
/// `.on_action`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls as controls;
/// let rows = vec![controls::Item::new("Profile").with_description("Name and photo")];
/// let group = ItemGroup::new("Settings", rows).on_item(|index| println!("row {index}"));
/// ```
pub type ItemGroup = Configured<controls::ItemGroup>;

impl Configured<controls::ItemGroup> {
    /// An item group named `name` of `items`.
    pub fn new<M>(name: impl IntoLive<TextContent, M>, items: impl Items<controls::Item>) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Item, move |env| {
            controls::ItemGroup::new(text(&name, env), items.items(env))
        })
    }

    /// Runs `action` with the row index and the action index of an
    /// action button that the user activates.
    #[must_use]
    pub fn on_action(self, action: impl Fn(usize, usize) + 'static) -> Self {
        self.on_message(move |message| {
            if let ControlMessage::ItemAction {
                item,
                action: index,
            } = message
            {
                action(*item, *index);
            }
        })
    }
}
