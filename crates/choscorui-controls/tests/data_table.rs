//! DataTable through the control host.

mod hosted;

use choscorui_controls::{ControlMessage, ControlValue, DataTable, ShortcutModifiers};
use choscorui_core::{
    Icon, ItemData, Key, KeyEvent, LayoutDirection, ListStore, Point, Role, SemanticAction,
    SemanticRole, Value,
};
use hosted::{Harness, draws_icon, texts};
use std::cell::Cell;
use std::rc::Rc;

struct Payment(&'static str, &'static str, f64);

impl ItemData for Payment {
    fn data(&self, column: usize, role: Role) -> Value {
        match (column, role) {
            (0, Role::Display | Role::Sort) => Value::Text(self.0.into()),
            (1, Role::Display | Role::Sort) => Value::Text(self.1.into()),
            (2, Role::Display) => Value::Text(format!("${:.2}", self.2)),
            (2, Role::Sort) => Value::Float(self.2),
            _ => Value::None,
        }
    }
}

fn payments() -> ListStore<Payment> {
    let mut store = ListStore::new(3);
    store.set_headers(["Status", "Email", "Amount"]);
    store
        .insert(
            None,
            0,
            vec![
                Payment("success", "ken99@example.com", 316.0),
                Payment("success", "abe45@example.com", 242.0),
                Payment("processing", "monserrat44@example.com", 837.0),
                Payment("success", "silas22@example.com", 874.0),
                Payment("failed", "carmella@example.com", 721.0),
            ],
        )
        .unwrap();
    store
}

fn table() -> DataTable {
    DataTable::new("Payments", Box::new(payments()), "Previous", "Next")
        .with_filter("Filter emails...", Some(1))
        .with_column_menu("Columns")
        .with_empty("No results.")
        .with_status(|selected, total| format!("{selected} of {total} row(s) selected."))
        .with_page_size(3)
}

fn tab(h: &mut Harness) -> bool {
    h.host.dispatch_key(
        &mut h.tree,
        KeyEvent::Down {
            key: Key::Tab,
            repeat: false,
        },
    )
}

#[test]
fn the_table_pages_rows_and_reports_its_status() {
    let mut h = Harness::new(table());
    assert_eq!(h.measure().width, 640.0);
    let painted = texts(&h.paint());
    for text in [
        "Status",
        "Email",
        "Amount",
        "ken99@example.com",
        "$837.00",
        "Columns",
    ] {
        assert!(painted.contains(&text.to_string()), "{text}");
    }
    assert!(
        !painted.contains(&"silas22@example.com".to_string()),
        "page 2"
    );
    let node = h.semantics();
    assert_eq!(node.role, SemanticRole::Table);
    assert_eq!(node.value.as_deref(), Some("0 of 5 row(s) selected."));
    assert!(h.perform(SemanticAction::Increment, None));
    assert!(texts(&h.paint()).contains(&"silas22@example.com".to_string()));
    assert!(!h.perform(SemanticAction::Increment, None), "the last page");
}

#[test]
fn keys_walk_the_tab_stops_and_the_grid() {
    let mut h = Harness::new(table());
    h.focus();
    assert_eq!(h.focused_part(), "Filter emails...");
    assert!(tab(&mut h));
    assert_eq!(h.focused_part(), "Columns");
    assert!(tab(&mut h));
    // The grid starts on the header checkbox; move to the Amount header.
    h.key(Key::End);
    assert_eq!(h.focused_part(), "Amount");
    h.key(Key::Enter);
    h.key(Key::Enter);
    h.key(Key::ArrowDown);
    assert_eq!(h.focused_part(), "$874.00");
    let parts = h.parts();
    let cell = &parts.parts()[parts.focused().unwrap()];
    assert_eq!(
        parts.parts()[cell.parent.unwrap()].node.name,
        "success, silas22@example.com, $874.00",
        "descending by amount"
    );
    h.key(Key::Space);
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Indices(vec![3]))]
    );
    h.key(Key::ArrowUp);
    h.key(Key::Home);
    h.key(Key::Space);
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Indices(vec![
            2, 3, 4
        ]))],
        "the header checkbox selects the page"
    );
    assert!(tab(&mut h));
    assert_eq!(h.focused_part(), "Next", "Previous is disabled");
    assert!(!tab(&mut h));
}

#[test]
fn the_filter_narrows_rows_and_shows_the_empty_text() {
    let mut h = Harness::new(table());
    h.focus();
    assert!(h.text("ab"));
    let painted = texts(&h.paint());
    assert!(painted.contains(&"abe45@example.com".to_string()));
    assert!(!painted.contains(&"ken99@example.com".to_string()));
    assert_eq!(
        h.semantics().value.as_deref(),
        Some("0 of 1 row(s) selected.")
    );
    assert!(h.perform(SemanticAction::SetValue, Some("zzz")));
    assert!(texts(&h.paint()).contains(&"No results.".to_string()));
}

#[test]
fn the_column_menu_hides_columns() {
    let mut h = Harness::new(table());
    h.focus();
    assert!(tab(&mut h));
    h.key(Key::Enter);
    assert_eq!(h.semantics().state.expanded, Some(true));
    // The menu highlights Status first.
    h.key(Key::Enter);
    h.set_time(1.0);
    let bounds = h.bounds();
    h.place(bounds);
    let painted = texts(&h.paint());
    assert!(!painted.contains(&"Status".to_string()));
    assert!(!painted.contains(&"processing".to_string()));
    assert_eq!(h.semantics().state.expanded, Some(false));
}

#[test]
fn clicks_sort_and_select_and_rtl_mirrors() {
    let mut h = Harness::new(table());
    let bounds = h.bounds();
    // Toolbar 36 + gap 16, then a 1 pt border and the 40 pt header.
    let header_y = bounds.origin.y + 36.0 + 16.0 + 1.0 + 20.0;
    let first_row_y = header_y + 20.0 + 18.0;
    let email_x = bounds.origin.x + 1.0 + 32.0 + 220.0;
    h.click(Point::new(email_x, header_y));
    h.click(Point::new(bounds.origin.x + 17.0, first_row_y));
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Indices(vec![1]))],
        "abe45 sorts first"
    );

    let mut rtl = Harness::with_direction(table(), LayoutDirection::Rtl);
    let bounds = rtl.bounds();
    rtl.click(Point::new(bounds.max_x() - 17.0, first_row_y));
    assert_eq!(
        rtl.messages(),
        [ControlMessage::Changed(ControlValue::Indices(vec![0]))],
        "the checkboxes are at the right"
    );
}

fn select(h: &mut Harness, rows: Vec<usize>) {
    let node = h.node;
    assert!(
        h.host
            .set_value(&mut h.tree, node, &ControlValue::Indices(rows))
    );
}

#[test]
fn the_status_counts_only_selected_rows_inside_the_filter() {
    let mut h = Harness::new(table());
    select(&mut h, vec![0, 1]);
    assert_eq!(
        h.semantics().value.as_deref(),
        Some("2 of 5 row(s) selected.")
    );
    h.focus();
    assert!(h.text("ab"));
    assert_eq!(
        h.semantics().value.as_deref(),
        Some("1 of 1 row(s) selected."),
        "ken99 is selected but the filter hides it"
    );
    assert!(
        texts(&h.paint()).contains(&"1 of 1 row(s) selected.".to_string()),
        "the painted status counts the same rows"
    );
}

/// A row that counts the model reads of every row.
struct Counted(usize, Rc<Cell<usize>>);

impl ItemData for Counted {
    fn data(&self, _: usize, role: Role) -> Value {
        self.1.set(self.1.get() + 1);
        match role {
            Role::Display | Role::Sort => Value::Text(format!("row {}", self.0)),
            _ => Value::None,
        }
    }
}

#[test]
fn a_selection_change_in_a_large_table_does_not_read_every_row() {
    let reads = Rc::new(Cell::new(0));
    let mut store = ListStore::new(1);
    store.set_headers(["Name"]);
    let rows: Vec<Counted> = (0..100_000)
        .map(|row| Counted(row, reads.clone()))
        .collect();
    store.insert(None, 0, rows).unwrap();
    let mut h = Harness::new(
        DataTable::new("Rows", Box::new(store), "Previous", "Next")
            .with_filter("Filter", None)
            .with_status(|selected, total| format!("{selected} of {total}")),
    );
    h.focus();
    // "row 1" matches 11,111 rows.
    assert!(h.text("row 1"));
    assert_eq!(h.semantics().value.as_deref(), Some("0 of 11111"));
    assert!(tab(&mut h));
    h.key(Key::ArrowDown);
    reads.set(0);
    h.key(Key::Space);
    for _ in 0..4 {
        h.key(Key::ArrowDown);
    }
    let shift = ShortcutModifiers {
        shift: true,
        ..ShortcutModifiers::default()
    };
    h.chord(shift, Key::Space);
    assert_eq!(h.semantics().value.as_deref(), Some("5 of 11111"));
    h.key(Key::Space);
    assert_eq!(h.semantics().value.as_deref(), Some("4 of 11111"));
    assert!(
        reads.get() < 100,
        "{} reads: a click reads the changed rows, not the model",
        reads.get()
    );
}

#[test]
fn sortable_headers_show_a_neutral_icon_until_they_sort() {
    let mut h = Harness::new(table());
    let list = h.paint();
    assert!(draws_icon(&list, Icon::ChevronsUpDown), "the neutral icon");
    assert!(!draws_icon(&list, Icon::ArrowUp));
    h.focus();
    tab(&mut h);
    tab(&mut h);
    h.key(Key::End);
    h.key(Key::Enter);
    let list = h.paint();
    assert!(draws_icon(&list, Icon::ArrowUp), "the sort column");
    assert!(
        draws_icon(&list, Icon::ChevronsUpDown),
        "the other columns stay neutral"
    );
}

#[test]
fn a_sort_change_returns_to_the_first_page() {
    let mut h = Harness::new(table());
    assert!(h.perform(SemanticAction::Increment, None));
    h.focus();
    tab(&mut h);
    tab(&mut h);
    h.key(Key::End);
    h.key(Key::Enter);
    assert!(
        texts(&h.paint()).contains(&"$242.00".to_string()),
        "the smallest amount is on the first page"
    );
    assert!(!h.perform(SemanticAction::Decrement, None), "first page");
}

#[test]
fn the_last_visible_column_does_not_hide() {
    let mut h = Harness::new(
        DataTable::new("Payments", Box::new(payments()), "Previous", "Next")
            .with_column_menu("Columns")
            .with_hidden(0)
            .with_hidden(1),
    );
    h.focus();
    h.key(Key::Enter);
    assert_eq!(h.semantics().state.expanded, Some(true));
    // Amount is the only visible column, so its menu item is disabled.
    for _ in 0..2 {
        h.key(Key::ArrowDown);
    }
    h.key(Key::Enter);
    assert!(texts(&h.paint()).contains(&"$316.00".to_string()));
}

#[test]
fn hiding_the_sort_column_clears_the_sort() {
    let mut h = Harness::new(table());
    h.focus();
    tab(&mut h);
    tab(&mut h);
    h.key(Key::ArrowRight);
    h.key(Key::Enter);
    assert_eq!(h.focused_part(), "Status");
    assert!(texts(&h.paint()).contains(&"carmella@example.com".to_string()));
    h.chord(
        ShortcutModifiers {
            shift: true,
            ..ShortcutModifiers::default()
        },
        Key::Tab,
    );
    assert_eq!(h.focused_part(), "Columns");
    h.key(Key::Enter);
    h.key(Key::Enter);
    h.set_time(1.0);
    let bounds = h.bounds();
    h.place(bounds);
    let painted = texts(&h.paint());
    assert!(!painted.contains(&"Status".to_string()));
    assert!(
        !painted.contains(&"carmella@example.com".to_string()),
        "the rows return to the source order"
    );
}

#[test]
fn a_click_on_an_empty_area_keeps_the_inner_focus() {
    let mut h = Harness::new(table());
    h.focus();
    tab(&mut h);
    assert_eq!(h.focused_part(), "Columns");
    h.host.move_focus(&mut h.tree, None);
    let bounds = h.bounds();
    // The gap between the toolbar and the table.
    h.click(Point::new(bounds.center().x, bounds.origin.y + 36.0 + 8.0));
    assert_eq!(h.focused_part(), "Columns");
}

#[test]
fn numbers_use_the_locale_format() {
    struct Count(i64);
    impl ItemData for Count {
        fn data(&self, _: usize, role: Role) -> Value {
            match role {
                Role::Display | Role::Sort => Value::Int(self.0),
                _ => Value::None,
            }
        }
    }
    let mut store = ListStore::new(1);
    store.set_headers(["Count"]);
    store.insert(None, 0, vec![Count(1280)]).unwrap();
    let h = Harness::new(DataTable::new("Counts", Box::new(store), "P", "N"));
    assert!(texts(&h.paint()).contains(&"1,280".to_string()));
}

#[test]
fn shift_extends_and_page_keys_and_command_a_reach_every_row() {
    let mut h = Harness::new(table());
    h.focus();
    assert!(tab(&mut h));
    assert!(tab(&mut h));
    h.key(Key::ArrowDown);
    h.key(Key::Space);
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Indices(vec![0]))]
    );
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    let shift = ShortcutModifiers {
        shift: true,
        ..ShortcutModifiers::default()
    };
    h.chord(shift, Key::Space);
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Indices(vec![
            0, 1, 2
        ]))],
        "Shift adds the range from the last toggled row"
    );
    h.key(Key::PageDown);
    assert!(texts(&h.paint()).contains(&"silas22@example.com".to_string()));
    h.key(Key::PageUp);
    assert!(texts(&h.paint()).contains(&"ken99@example.com".to_string()));
    let command = ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    };
    h.chord(command, Key::Character('a'));
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Indices(vec![
            0, 1, 2, 3, 4
        ]))],
        "Command+A selects the rows on every page"
    );
}
