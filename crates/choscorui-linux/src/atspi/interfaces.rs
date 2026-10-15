//! The interfaces that a node has by its role and its semantics:
//! `Action`, `Text`, `EditableText`, `Value`, `Selection`, and `Table`.

use super::handler::{
    ACTION, Call, EDITABLE_TEXT, SELECTION, TABLE, TEXT, VALUE, convert, count, extents_value,
    node_interfaces,
};
use super::model::{Target, WindowModel};
use super::roles;
use super::text::{self, Unit};
use super::tree::Node;
use crate::dbus::{Reply, Value};
use choscorui_core::{SemanticAction, SemanticRole};

/// The properties of `interface` on node `node`, or `None` when the node
/// does not have the interface.
pub fn properties(
    call: &Call<'_>,
    window: &WindowModel,
    node_id: u64,
    interface: &str,
) -> Option<Vec<(&'static str, Value)>> {
    let node = window.node(node_id)?;
    if !node_interfaces(node).contains(&interface) {
        return None;
    }
    let semantic = &node.entry.node;
    match interface {
        ACTION => Some(vec![("NActions", Value::from(count(actions(node).len())))]),
        TEXT => {
            let content = text::content(&node.entry).unwrap_or_default();
            Some(vec![
                (
                    "CharacterCount",
                    Value::from(count(text::char_count(content))),
                ),
                ("CaretOffset", Value::from(count(caret(node)))),
            ])
        }
        VALUE => {
            let range = semantic.state.range?;
            Some(vec![
                ("MinimumValue", Value::from(range.minimum)),
                ("MaximumValue", Value::from(range.maximum)),
                ("MinimumIncrement", Value::from(range.step)),
                ("CurrentValue", Value::from(range.value)),
                (
                    "Text",
                    Value::from(semantic.value.clone().unwrap_or_default()),
                ),
            ])
        }
        SELECTION => Some(vec![(
            "NSelectedChildren",
            Value::from(count(selected(window, node).len())),
        )]),
        TABLE => {
            let table = Table::new(window, node);
            Some(vec![
                ("NRows", Value::from(count(table.row_count()))),
                ("NColumns", Value::from(count(table.columns))),
                ("Caption", call.null()),
                ("Summary", call.null()),
                (
                    "NSelectedRows",
                    Value::from(count(table.selected_rows().len())),
                ),
                ("NSelectedColumns", Value::from(0i32)),
            ])
        }
        EDITABLE_TEXT => Some(Vec::new()),
        _ => None,
    }
}

/// Answers a method of the role interfaces on node `node_id`.
pub fn answer(
    call: &Call<'_>,
    window: &mut WindowModel,
    node_id: u64,
    interface: &str,
    member: &str,
) -> Reply {
    let Some(node) = window.node(node_id) else {
        return Reply::unknown_method(call.message);
    };
    if !node_interfaces(node).contains(&interface) {
        return Reply::unknown_method(call.message);
    }
    match interface {
        ACTION => action(call, window, node_id, member),
        TEXT => text_method(call, window, node_id, member),
        EDITABLE_TEXT => editable(call, window, node_id, member),
        SELECTION => selection(call, window, node_id, member),
        TABLE => table(call, window, node_id, member),
        _ => Reply::unknown_method(call.message),
    }
}

/// The declared actions that the `Action` interface lists.
fn actions(node: &Node) -> Vec<(SemanticAction, &'static str)> {
    node.entry
        .node
        .actions
        .iter()
        .filter_map(|&action| roles::action_name(action).map(|name| (action, name)))
        .collect()
}

fn action(call: &Call<'_>, window: &mut WindowModel, node_id: u64, member: &str) -> Reply {
    let Some(node) = window.node(node_id) else {
        return Reply::unknown_method(call.message);
    };
    let list = actions(node);
    let chosen = usize::try_from(call.int(0))
        .ok()
        .and_then(|index| list.get(index).copied());
    let name = chosen.map_or("", |(_, name)| name);
    let values = match member {
        "GetName" | "GetLocalizedName" => vec![Value::from(name)],
        "GetDescription" => vec![Value::from(roles::action_description(name))],
        "GetKeyBinding" => vec![Value::from("")],
        "GetActions" => vec![Value::array(
            "(sss)",
            list.iter()
                .map(|(_, name)| {
                    Value::Struct(vec![
                        Value::from(*name),
                        Value::from(roles::action_description(name)),
                        Value::from(""),
                    ])
                })
                .collect(),
        )],
        "DoAction" => {
            vec![Value::from(chosen.is_some_and(|(action, _)| {
                window.request(node_id, action, None)
            }))]
        }
        _ => return Reply::unknown_method(call.message),
    };
    Reply::Return(values)
}

/// The caret: the end of the selection, in characters.
fn caret(node: &Node) -> usize {
    text::selection(&node.entry).map_or(0, |range| range.end)
}

/// `(s, i, i)`: the text of `range` and its bounds.
fn text_range(content: &str, range: std::ops::Range<usize>) -> Vec<Value> {
    vec![
        Value::from(text::slice(content, range.clone())),
        Value::from(count(range.start)),
        Value::from(count(range.end)),
    ]
}

fn text_method(call: &Call<'_>, window: &WindowModel, node_id: u64, member: &str) -> Reply {
    let Some(node) = window.node(node_id) else {
        return Reply::unknown_method(call.message);
    };
    let content = text::content(&node.entry).unwrap_or_default();
    let length = text::char_count(content);
    let offset = |index: usize| usize::try_from(call.int(index)).unwrap_or(0).min(length);
    let unit_at = |offset: usize, unit: Unit| text::unit_at(content, offset, unit);
    let parent = node
        .parent
        .and_then(|parent| window.node(parent))
        .map_or(window.bounds(), |parent| parent.entry.bounds);
    let empty_attributes = || Value::Dict {
        key: "s".to_owned(),
        value: "s".to_owned(),
        entries: Vec::new(),
    };
    let values = match member {
        "GetText" => {
            let end = usize::try_from(call.int(1)).map_or(length, |end| end.min(length));
            vec![Value::from(text::slice(content, offset(0)..end))]
        }
        "GetTextAtOffset" => text_range(
            content,
            unit_at(offset(0), Unit::from_boundary(call.uint(1))),
        ),
        "GetStringAtOffset" => text_range(
            content,
            unit_at(offset(0), Unit::from_granularity(call.uint(1))),
        ),
        "GetTextBeforeOffset" => {
            let unit = Unit::from_boundary(call.uint(1));
            let at = unit_at(offset(0), unit);
            let before = if at.start == 0 {
                0..0
            } else {
                unit_at(at.start - 1, unit)
            };
            text_range(content, before)
        }
        "GetTextAfterOffset" => {
            let unit = Unit::from_boundary(call.uint(1));
            let at = unit_at(offset(0), unit);
            text_range(content, unit_at(at.end, unit))
        }
        "GetCharacterAtOffset" => {
            let character = content.chars().nth(offset(0)).map_or(0, |c| c as i32);
            vec![Value::from(character)]
        }
        "GetAttributeValue" => vec![Value::from("")],
        "GetAttributes" | "GetAttributeRun" => vec![
            empty_attributes(),
            Value::from(0i32),
            Value::from(count(length)),
        ],
        "GetDefaultAttributes" | "GetDefaultAttributeSet" => vec![empty_attributes()],
        "GetCharacterExtents" => extents_value(convert(
            window,
            node.entry.bounds,
            Some(parent),
            call.uint(1),
        )),
        "GetRangeExtents" => extents_value(convert(
            window,
            node.entry.bounds,
            Some(parent),
            call.uint(2),
        )),
        "GetOffsetAtPoint" => vec![Value::from(-1i32)],
        "GetNSelections" => {
            let selected = text::selection(&node.entry).is_some_and(|range| !range.is_empty());
            vec![Value::from(i32::from(selected))]
        }
        "GetSelection" => {
            let range = text::selection(&node.entry)
                .filter(|range| !range.is_empty() && call.int(0) == 0)
                .unwrap_or(0..0);
            vec![
                Value::from(count(range.start)),
                Value::from(count(range.end)),
            ]
        }
        "GetBoundedRanges" => vec![Value::array("(iisv)", Vec::new())],
        "SetCaretOffset"
        | "AddSelection"
        | "RemoveSelection"
        | "SetSelection"
        | "ScrollSubstringTo"
        | "ScrollSubstringToPoint" => vec![Value::from(false)],
        _ => return Reply::unknown_method(call.message),
    };
    Reply::Return(values)
}

fn editable(call: &Call<'_>, window: &mut WindowModel, node_id: u64, member: &str) -> Reply {
    let Some(node) = window.node(node_id) else {
        return Reply::unknown_method(call.message);
    };
    let content = text::content(&node.entry).unwrap_or_default().to_owned();
    let length = text::char_count(&content);
    let position = |index: usize| usize::try_from(call.int(index)).unwrap_or(0).min(length);
    let new_text = match member {
        "SetTextContents" => Some(call.text(0)),
        "InsertText" => {
            let at = position(0);
            let inserted: String = match usize::try_from(call.int(2)) {
                Ok(limit) => call.text(1).chars().take(limit).collect(),
                Err(_) => call.text(1),
            };
            Some(format!(
                "{}{inserted}{}",
                text::slice(&content, 0..at),
                text::slice(&content, at..length)
            ))
        }
        "DeleteText" => {
            let (start, end) = (position(0), position(1));
            Some(format!(
                "{}{}",
                text::slice(&content, 0..start.min(end)),
                text::slice(&content, end.max(start)..length)
            ))
        }
        "CopyText" => return Reply::Return(Vec::new()),
        "CutText" | "PasteText" => None,
        _ => return Reply::unknown_method(call.message),
    };
    let done = new_text
        .is_some_and(|new_text| window.request(node_id, SemanticAction::SetValue, Some(new_text)));
    Reply::Return(vec![Value::from(done)])
}

/// The selected children of a container.
fn selected(window: &WindowModel, node: &Node) -> Vec<u64> {
    node.children
        .iter()
        .copied()
        .filter(|&child| {
            window
                .node(child)
                .is_some_and(|child| child.entry.node.state.selected == Some(true))
        })
        .collect()
}

fn selection(call: &Call<'_>, window: &mut WindowModel, node_id: u64, member: &str) -> Reply {
    let Some(node) = window.node(node_id) else {
        return Reply::unknown_method(call.message);
    };
    let chosen = selected(window, node);
    let index = usize::try_from(call.int(0)).ok();
    let child = index.and_then(|index| node.children.get(index).copied());
    let values = match member {
        "GetSelectedChild" => {
            let window_id = window_id_of(call);
            let selected = index.and_then(|index| chosen.get(index).copied());
            vec![selected.map_or_else(
                || call.null(),
                |selected| call.reference(Target::Node(window_id, selected)),
            )]
        }
        "IsChildSelected" => vec![Value::from(
            child.is_some_and(|child| chosen.contains(&child)),
        )],
        "SelectChild" => {
            vec![Value::from(child.is_some_and(|child| {
                window.request(child, SemanticAction::Activate, None)
            }))]
        }
        "DeselectSelectedChild" | "DeselectChild" | "SelectAll" | "ClearSelection" => {
            vec![Value::from(false)]
        }
        _ => return Reply::unknown_method(call.message),
    };
    Reply::Return(values)
}

/// The window number in the path of the call.
fn window_id_of(call: &Call<'_>) -> u32 {
    match call.message.path().as_deref().and_then(Target::parse) {
        Some(Target::Node(window, _) | Target::Window(window)) => window,
        _ => 0,
    }
}

/// The rows and the columns of a table node.
struct Table {
    /// The realized rows and their row numbers.
    rows: Vec<(usize, u64)>,
    headers: Vec<u64>,
    columns: usize,
    item_count: Option<usize>,
    selected: Vec<usize>,
    cells: Vec<Vec<u64>>,
}

impl Table {
    fn new(window: &WindowModel, node: &Node) -> Self {
        let mut rows = Vec::new();
        let mut headers = Vec::new();
        let mut cells = Vec::new();
        let mut selected = Vec::new();
        let mut collect_headers = |children: &[u64]| {
            for &child in children {
                if window
                    .node(child)
                    .is_some_and(|child| child.entry.node.role == SemanticRole::ColumnHeader)
                {
                    headers.push(child);
                }
            }
        };
        collect_headers(&node.children);
        for &child in &node.children {
            let Some(row) = window.node(child) else {
                continue;
            };
            if row.entry.node.role != SemanticRole::Row {
                continue;
            }
            let number = row.entry.item_index.unwrap_or(rows.len());
            if row.children.iter().all(|&cell| {
                window
                    .node(cell)
                    .is_some_and(|cell| cell.entry.node.role == SemanticRole::ColumnHeader)
            }) && !row.children.is_empty()
            {
                collect_headers(&row.children);
                continue;
            }
            if row.entry.node.state.selected == Some(true) {
                selected.push(number);
            }
            rows.push((number, child));
            cells.push(row.children.clone());
        }
        let collection = node.entry.collection;
        let widest = cells.iter().map(Vec::len).max().unwrap_or(0);
        Self {
            columns: collection
                .and_then(|collection| collection.column_count)
                .unwrap_or(widest.max(headers.len())),
            item_count: collection.map(|collection| collection.item_count),
            rows,
            headers,
            selected,
            cells,
        }
    }

    fn row_count(&self) -> usize {
        self.item_count.unwrap_or(self.rows.len())
    }

    fn selected_rows(&self) -> &[usize] {
        &self.selected
    }

    fn cell(&self, row: usize, column: usize) -> Option<u64> {
        let position = self.rows.iter().position(|&(number, _)| number == row)?;
        self.cells.get(position)?.get(column).copied()
    }
}

fn table(call: &Call<'_>, window: &WindowModel, node_id: u64, member: &str) -> Reply {
    let Some(node) = window.node(node_id) else {
        return Reply::unknown_method(call.message);
    };
    let table = Table::new(window, node);
    let window_id = window_id_of(call);
    let reference = |node: Option<u64>| {
        node.map_or_else(
            || call.null(),
            |node| call.reference(Target::Node(window_id, node)),
        )
    };
    let index = |position: usize| usize::try_from(call.int(position)).unwrap_or(0);
    let columns = table.columns.max(1);
    let values = match member {
        "GetAccessibleAt" => vec![reference(table.cell(index(0), index(1)))],
        "GetIndexAt" => vec![Value::from(count(index(0) * columns + index(1)))],
        "GetRowAtIndex" => vec![Value::from(count(index(0) / columns))],
        "GetColumnAtIndex" => vec![Value::from(count(index(0) % columns))],
        "GetRowDescription" => vec![Value::from("")],
        "GetColumnDescription" => {
            let header = table.headers.get(index(0)).and_then(|&id| window.node(id));
            vec![Value::from(header.map_or_else(String::new, |header| {
                header.entry.node.name.clone()
            }))]
        }
        "GetRowExtentAt" | "GetColumnExtentAt" => vec![Value::from(1i32)],
        "GetRowHeader" => vec![call.null()],
        "GetColumnHeader" => vec![reference(table.headers.get(index(0)).copied())],
        "GetSelectedRows" => vec![Value::array(
            "i",
            table
                .selected_rows()
                .iter()
                .map(|&row| Value::from(count(row)))
                .collect(),
        )],
        "GetSelectedColumns" => vec![Value::array("i", Vec::new())],
        "IsRowSelected" | "IsSelected" => {
            vec![Value::from(table.selected_rows().contains(&index(0)))]
        }
        "IsColumnSelected" => vec![Value::from(false)],
        "AddRowSelection"
        | "AddColumnSelection"
        | "RemoveRowSelection"
        | "RemoveColumnSelection" => vec![Value::from(false)],
        "GetRowColumnExtentsAtIndex" => {
            let (row, column) = (index(0) / columns, index(0) % columns);
            vec![
                Value::from(table.cell(row, column).is_some()),
                Value::from(count(row)),
                Value::from(count(column)),
                Value::from(1i32),
                Value::from(1i32),
                Value::from(table.selected_rows().contains(&row)),
            ]
        }
        _ => return Reply::unknown_method(call.message),
    };
    Reply::Return(values)
}
