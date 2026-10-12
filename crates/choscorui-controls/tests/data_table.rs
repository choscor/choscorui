//! DataTable through the control host.

mod hosted;

use choscorui_controls::{ControlMessage, ControlValue, DataTable};
use choscorui_core::{
    ItemData, Key, KeyEvent, LayoutDirection, ListStore, Point, Role, SemanticAction, SemanticRole,
    Value,
};
use hosted::{Harness, texts};

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
    assert_eq!(h.semantics().description, "Filter emails...");
    assert!(tab(&mut h));
    assert_eq!(h.semantics().description, "Columns");
    assert!(tab(&mut h));
    // The grid starts on the header checkbox; move to the Amount header.
    h.key(Key::End);
    assert_eq!(h.semantics().description, "Amount");
    h.key(Key::Enter);
    h.key(Key::Enter);
    h.key(Key::ArrowDown);
    assert_eq!(
        h.semantics().description,
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
    assert_eq!(h.semantics().description, "Next", "Previous is disabled");
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
