//! The built-in views, editors, menus, dialogs, shell controls, and
//! containers through the control host: input and messages, assistive
//! actions, values from the application, the disabled state, and semantics.

mod hosted;

use choscorui_controls::{
    AlertDialog, Calendar, CalendarMode, CanvasView, ColorPicker, ColumnBrowser, CommandId,
    CommandState, ContainerError, ControlMessage, ControlValue, CustomControl, DateTimeEdit,
    DateTimeKind, DialogButton, DialogButtonRole, DockArea, DockLayout, DockPanel, DockTarget,
    DropdownMenu, EstimatedText, GroupBox, HeaderColumn, HeaderView, ItemView, MeasureContext,
    MenuItem, PanelId, Resizable, ScrollArea, ShortcutModifiers, SplitOrientation,
    StackedContainer, StatusBar, Styles, TextItemDelegate, Toolbar, ToolbarItem,
};
use choscorui_core::{
    CivilDate, CivilDateTime, CivilTime, Color, Constraints, DesignTokens, DisplayCommand,
    DisplayList, ItemShape, ItemStyle, Key, LayoutDirection, Locale, Point, PointerButton,
    PointerEvent, Rect, Scene, SceneItem, ScrollViewport, SemanticAction, SemanticRole, Size,
    StringListModel, TreeStore, WheelUnit,
};
use hosted::{Harness, rect, texts};
use std::time::Duration;

fn date(year: i32, month: u8, day: u8) -> CivilDate {
    CivilDate::new(year, month, day).unwrap()
}

fn press(h: &mut Harness, event: PointerEvent) {
    h.host.dispatch_pointer(&mut h.tree, event);
}

fn set_value(h: &mut Harness, value: ControlValue) -> bool {
    h.host.set_value(&mut h.tree, h.node, &value)
}

fn rows(count: usize) -> ItemView {
    let model = StringListModel::from_strings((0..count).map(|row| format!("Row {row}")));
    ItemView::new("Rows", SemanticRole::List, 20.0)
        .with_model(Box::new(model), Box::new(TextItemDelegate::default()))
}

/// The center of visible row `row` of a 20 pt list placed at (10, 10).
fn row_point(row: usize) -> Point {
    Point::new(50.0, 10.0 + row as f64 * 20.0 + 10.0)
}

#[test]
fn a_hosted_item_view_paints_only_the_visible_rows_of_a_large_model() {
    let mut h = Harness::new(rows(100_000));
    let painted = texts(&h.paint());
    assert!(painted.len() <= 15, "painted {} rows", painted.len());
    assert_eq!(painted.first().map(String::as_str), Some("Row 0"));
    h.focus();
    h.key(Key::End);
    assert_eq!(
        h.messages(),
        vec![ControlMessage::Changed(ControlValue::Indices(vec![99_999]))]
    );
    let painted = texts(&h.paint());
    assert!(painted.len() <= 15, "painted {} rows", painted.len());
    assert_eq!(painted.last().map(String::as_str), Some("Row 99999"));
}

#[test]
fn a_hosted_item_view_selects_activates_and_scrolls() {
    let mut h = Harness::new(rows(50));
    assert_eq!(h.semantics().role, SemanticRole::List);
    assert_eq!(h.semantics().name, "Rows");
    h.double_click(row_point(3));
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::Changed(ControlValue::Indices(vec![3])),
            ControlMessage::ItemActivated(3),
            ControlMessage::Changed(ControlValue::Indices(vec![4])),
            ControlMessage::ItemActivated(4),
        ]
    );
    press(
        &mut h,
        PointerEvent::Wheel {
            position: row_point(0),
            delta: Point::new(0.0, 5.0),
            unit: WheelUnit::Lines,
        },
    );
    assert!(h.messages().is_empty());
    assert_eq!(texts(&h.paint()).first().map(String::as_str), Some("Row 3"));
}

#[test]
fn a_hosted_item_view_with_styled_rows_takes_the_list_row_height() {
    let mut h = Harness::new(rows(50).with_styled_rows());
    // The `ListView` row recipe is 32 pt high, so this point is in row 3,
    // not in row 5 of 20 pt rows.
    h.click(Point::new(50.0, 10.0 + 3.0 * 32.0 + 16.0));
    assert_eq!(
        h.messages(),
        vec![ControlMessage::Changed(ControlValue::Indices(vec![3]))]
    );
}

#[test]
fn a_hosted_item_view_takes_a_selection_and_focus_action() {
    let mut h = Harness::new(rows(20));
    assert!(set_value(&mut h, ControlValue::Indices(vec![1, 2])));
    assert!(!set_value(&mut h, ControlValue::Indices(vec![20])));
    assert!(!set_value(&mut h, ControlValue::Bool(true)));
    assert!(h.messages().is_empty());
    assert!(h.perform(SemanticAction::Focus, None));
    assert_eq!(h.tree.focus(), Some(h.node));
    // The current item is the last one set, so ArrowDown moves past it.
    h.key(Key::ArrowDown);
    assert_eq!(
        h.messages(),
        vec![ControlMessage::Changed(ControlValue::Indices(vec![3]))]
    );
    h.set_enabled(false);
    h.click(row_point(5));
    h.key(Key::ArrowDown);
    assert!(!h.perform(SemanticAction::Focus, None));
    assert!(h.messages().is_empty());
}

/// A table of `count` 20 pt rows in a 100 pt and an 80 pt column, with a
/// total row and a caption.
fn stock(count: usize) -> ItemView {
    let model = StringListModel::from_strings((0..count).map(|row| format!("Item {row}")));
    let mut view = ItemView::new("Stock", SemanticRole::Table, 20.0)
        .with_model(Box::new(model), Box::new(TextItemDelegate::table()))
        .with_footer(vec![vec!["Total".into(), count.to_string()]])
        .with_caption("Items on hand");
    assert!(view.set_column_widths(vec![100.0, 80.0]));
    view
}

/// The baseline of the text run `label` in `list`.
fn baseline(list: &DisplayList, label: &str) -> Point {
    list.commands()
        .iter()
        .find_map(|command| match command {
            DisplayCommand::DrawText { text, baseline, .. } if text == label => Some(*baseline),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no text {label}"))
}

#[test]
fn a_hosted_table_paints_its_footer_under_the_rows_and_then_its_caption() {
    let h = Harness::new(stock(3));
    let semantics = h.semantics();
    assert_eq!(semantics.role, SemanticRole::Table);
    assert_eq!(semantics.description, "Items on hand. Total, 3");
    let list = h.paint();
    // Three 20 pt rows from y = 10 put the footer row at y = 70.
    let (total, count) = (baseline(&list, "Total"), baseline(&list, "3"));
    assert!(total.y > 70.0 && total.y < 90.0, "{total:?}");
    assert_eq!(total.y, count.y);
    assert!(count.x >= 110.0, "the count is in the second column");
    let caption = baseline(&list, "Items on hand");
    assert!(caption.y > 90.0);
    assert!(caption.x > total.x, "the caption is centered");
    let muted = DesignTokens::light().color(choscorui_core::ColorRole::Muted);
    assert!(list.commands().iter().any(|command| matches!(
        command,
        DisplayCommand::FillRect { color, .. } if *color == muted.with_alpha(0.5)
    )));
}

#[test]
fn a_hosted_table_footer_stays_under_a_full_viewport_and_takes_no_selection() {
    let mut h = Harness::new(stock(50));
    let list = h.paint();
    let total = baseline(&list, "Total");
    let caption = baseline(&list, "Items on hand");
    assert!(total.y < caption.y && caption.y < h.bounds().max_y());
    // The rows clip to the viewport, which stops above the footer row.
    let clips: Vec<f64> = list
        .commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::PushClip(rect) => Some(rect.max_y()),
            _ => None,
        })
        .collect();
    assert!(
        clips
            .iter()
            .any(|bottom| *bottom < total.y && *bottom > total.y - 20.0),
        "{clips:?} {total:?}"
    );
    h.click(Point::new(total.x, total.y - 4.0));
    assert!(h.messages().is_empty());
}

fn header() -> HeaderView {
    HeaderView::new(vec![
        HeaderColumn {
            title: "Name".into(),
            width: 100.0,
            min_width: 40.0,
        },
        HeaderColumn {
            title: "Size".into(),
            width: 80.0,
            min_width: 40.0,
        },
    ])
}

#[test]
fn a_hosted_header_sorts_resizes_and_moves_between_columns() {
    let mut h = Harness::new(header());
    assert_eq!(h.bounds().size.width, 180.0);
    let y = h.bounds().center().y;
    h.click(Point::new(60.0, y));
    assert_eq!(h.semantics().role, SemanticRole::ColumnHeader);
    assert_eq!(h.semantics().value.as_deref(), Some("ascending"));
    h.click(Point::new(60.0, y));
    assert_eq!(h.semantics().value.as_deref(), Some("descending"));
    let edge = Point::new(110.0, y);
    press(
        &mut h,
        PointerEvent::Down {
            position: edge,
            button: PointerButton::Primary,
        },
    );
    press(
        &mut h,
        PointerEvent::Move {
            position: Point::new(130.0, y),
        },
    );
    press(
        &mut h,
        PointerEvent::Up {
            position: Point::new(130.0, y),
            button: PointerButton::Primary,
        },
    );
    assert_eq!(h.semantics().description, "120 points wide");
    h.focus();
    h.key(Key::ArrowRight);
    assert_eq!(h.semantics().name, "Size");
    h.key(Key::Enter);
    assert!(h.perform(SemanticAction::Activate, None));
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::ItemActivated(0),
            ControlMessage::ItemActivated(0),
            ControlMessage::Changed(ControlValue::Index(Some(0))),
            ControlMessage::ItemActivated(1),
            ControlMessage::ItemActivated(1),
        ]
    );
    assert!(!set_value(&mut h, ControlValue::Index(Some(0))));
    h.set_enabled(false);
    h.click(Point::new(60.0, y));
    h.key(Key::Enter);
    assert!(!h.perform(SemanticAction::Activate, None));
    assert!(h.messages().is_empty());
}

fn files() -> ColumnBrowser {
    ColumnBrowser::new()
}

fn file_model() -> TreeStore<String> {
    let mut store = TreeStore::new(1);
    let documents = store.push(None, "Documents".to_owned()).unwrap();
    store.push(Some(documents), "Report".to_owned()).unwrap();
    store.push(None, "Music".to_owned()).unwrap();
    store
}

#[test]
fn a_hosted_column_browser_reports_its_path() {
    let mut h = Harness::new(files().hosted("Files", Box::new(file_model())));
    assert_eq!(h.semantics().role, SemanticRole::Tree);
    assert_eq!(h.semantics().name, "Files");
    let origin = h.bounds().origin;
    h.click(Point::new(origin.x + 20.0, origin.y + 10.0));
    assert_eq!(texts(&h.paint()), vec!["Documents", "Music", "Report"]);
    h.focus();
    h.key(Key::ArrowRight);
    h.key(Key::ArrowDown);
    h.key(Key::ArrowLeft);
    h.key(Key::ArrowDown);
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::Changed(ControlValue::Indices(vec![0])),
            ControlMessage::Changed(ControlValue::Indices(vec![0, 0])),
            ControlMessage::Changed(ControlValue::Indices(vec![0])),
            ControlMessage::Changed(ControlValue::Indices(vec![1])),
        ]
    );
    assert!(set_value(&mut h, ControlValue::Indices(vec![0, 0])));
    assert!(!set_value(&mut h, ControlValue::Indices(vec![5])));
    assert!(h.messages().is_empty());
    assert_eq!(h.semantics().value.as_deref(), Some("Report"));
    assert!(h.perform(SemanticAction::Focus, None));
    h.set_enabled(false);
    h.key(Key::ArrowLeft);
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_column_browser_mirrors_its_keys_right_to_left() {
    let mut h = Harness::with_direction(
        files().hosted("Files", Box::new(file_model())),
        LayoutDirection::Rtl,
    );
    h.focus();
    h.key(Key::ArrowDown);
    h.key(Key::ArrowLeft);
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::Changed(ControlValue::Indices(vec![0])),
            ControlMessage::Changed(ControlValue::Indices(vec![0, 0])),
        ]
    );
}

fn en() -> Locale {
    Locale::parse("en-US").unwrap()
}

#[test]
fn a_hosted_calendar_moves_its_selection_and_leaves_choosing_to_the_application() {
    let calendar = Calendar::new(en(), date(2024, 5, 15))
        .with_name("Due")
        .with_today(date(2024, 5, 1));
    let mut h = Harness::new(calendar);
    assert_eq!(h.semantics().role, SemanticRole::Table);
    assert_eq!(h.semantics().name, "Due");
    assert_eq!(h.semantics().description, "May 2024");
    h.focus();
    h.key(Key::ArrowRight);
    h.key(Key::Enter);
    assert!(h.perform(SemanticAction::Focus, None));
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::Changed(ControlValue::Date(Some(date(2024, 5, 16)))),
            ControlMessage::Activated,
        ]
    );
    h.click(h.bounds().center());
    let messages = h.messages();
    assert!(
        matches!(
            messages.as_slice(),
            [ControlMessage::Changed(ControlValue::Date(Some(day)))] if *day != date(2024, 5, 16)
        ),
        "{messages:?}"
    );
    assert!(set_value(
        &mut h,
        ControlValue::Date(Some(date(2024, 7, 4)))
    ));
    assert!(!set_value(&mut h, ControlValue::Dates(Vec::new())));
    assert!(h.messages().is_empty());
    assert_eq!(h.semantics().description, "July 2024");
    h.set_enabled(false);
    h.key(Key::ArrowRight);
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_multiple_calendar_chooses_days() {
    let calendar = Calendar::new(en(), date(2024, 5, 15)).with_mode(CalendarMode::Multiple);
    let mut h = Harness::new(calendar);
    h.focus();
    h.key(Key::Space);
    h.key(Key::ArrowRight);
    assert_eq!(
        h.messages(),
        vec![ControlMessage::Changed(ControlValue::Dates(vec![date(
            2024, 5, 15
        )]))]
    );
    let dates = vec![date(2024, 5, 1), date(2024, 5, 2)];
    assert!(set_value(&mut h, ControlValue::Dates(dates)));
    assert!(!set_value(
        &mut h,
        ControlValue::Date(Some(date(2024, 5, 1)))
    ));
    assert!(h.messages().is_empty());
    assert_eq!(
        h.semantics().value.as_deref(),
        Some("May 1, 2024, May 2, 2024")
    );
}

fn datetime(year: i32, month: u8, day: u8, hour: u8, minute: u8) -> CivilDateTime {
    CivilDateTime {
        date: date(year, month, day),
        time: CivilTime::new(hour, minute, 0).unwrap(),
    }
}

#[test]
fn a_hosted_date_edit_steps_types_and_takes_a_date() {
    let edit = DateTimeEdit::new(
        "Start",
        en(),
        DateTimeKind::Date,
        datetime(2024, 5, 15, 0, 0),
    );
    let mut h = Harness::new(edit);
    assert_eq!(h.semantics().role, SemanticRole::SpinButton);
    assert_eq!(h.semantics().name, "Start");
    h.focus();
    h.key(Key::ArrowUp);
    assert!(h.text("1"));
    assert!(h.text("2"));
    assert!(h.perform(SemanticAction::Decrement, None));
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::Changed(ControlValue::Date(Some(date(2024, 6, 15)))),
            ControlMessage::Changed(ControlValue::Date(Some(date(2024, 1, 15)))),
            ControlMessage::Changed(ControlValue::Date(Some(date(2024, 12, 15)))),
            ControlMessage::Changed(ControlValue::Date(Some(date(2024, 12, 14)))),
        ]
    );
    let steppers = h.bounds().max_x() - 4.0;
    h.click(Point::new(steppers, h.bounds().origin.y + 4.0));
    assert_eq!(
        h.messages(),
        vec![ControlMessage::Changed(ControlValue::Date(Some(date(
            2024, 12, 15
        ))))]
    );
    assert!(set_value(
        &mut h,
        ControlValue::Date(Some(date(2025, 2, 3)))
    ));
    assert!(!set_value(&mut h, ControlValue::Text("10:30".into())));
    assert!(h.messages().is_empty());
    assert_eq!(h.semantics().value.as_deref(), Some("February 3, 2025"));
    h.set_enabled(false);
    h.key(Key::ArrowUp);
    assert!(!h.text("1"));
    assert!(!h.perform(SemanticAction::Increment, None));
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_time_edit_reports_iso_text() {
    let edit = DateTimeEdit::new(
        "Alarm",
        en(),
        DateTimeKind::Time,
        datetime(2024, 5, 15, 7, 0),
    );
    let mut h = Harness::new(edit);
    assert!(set_value(&mut h, ControlValue::Text("09:30".into())));
    assert!(!set_value(&mut h, ControlValue::Text("late".into())));
    assert!(h.messages().is_empty());
    h.focus();
    h.key(Key::ArrowUp);
    assert_eq!(
        h.messages(),
        vec![ControlMessage::Changed(ControlValue::Text(
            "10:30:00".into()
        ))]
    );
    let edit = DateTimeEdit::new(
        "Meeting",
        en(),
        DateTimeKind::DateTime,
        datetime(2024, 5, 15, 7, 0),
    );
    let mut h = Harness::new(edit);
    assert!(set_value(
        &mut h,
        ControlValue::Text("2025-01-02T08:15".into())
    ));
    assert_eq!(
        h.semantics().value.as_deref(),
        Some("January 2, 2025 8:15 AM")
    );
}

const COPY: CommandId = CommandId(1);
const PASTE: CommandId = CommandId(2);
const SHARE: CommandId = CommandId(3);

fn edit_menu() -> DropdownMenu {
    DropdownMenu::new(vec![
        MenuItem::action("Copy", COPY),
        MenuItem::action("Paste", PASTE),
        MenuItem::submenu("More", vec![MenuItem::action("Share", SHARE)]),
    ])
}

/// A point inside the painted text `label`.
fn text_point(list: &DisplayList, label: &str) -> Point {
    list.commands()
        .iter()
        .find_map(|command| match command {
            DisplayCommand::DrawText { baseline, text, .. } if text == label => {
                Some(Point::new(baseline.x + 1.0, baseline.y - 2.0))
            }
            _ => None,
        })
        .unwrap()
}

#[test]
fn a_hosted_dropdown_menu_runs_commands_by_key_pointer_and_action() {
    let mut h = Harness::new(edit_menu().hosted("Edit"));
    assert_eq!(h.semantics().role, SemanticRole::Menu);
    assert_eq!(h.semantics().name, "Edit");
    assert_eq!(texts(&h.paint()), vec!["Copy", "Paste", "More"]);

    h.focus();
    h.key(Key::ArrowDown);
    assert_eq!(h.semantics().description, "Copy");
    h.key(Key::Enter);
    // The submenu opens beside its row and floats over other controls.
    h.key(Key::End);
    h.key(Key::ArrowRight);
    assert!(h.host.get(h.node).unwrap().overlay().is_some());
    assert!(texts(&h.paint()).contains(&"Share".to_string()));
    h.key(Key::Enter);
    assert!(h.host.get(h.node).unwrap().overlay().is_none());
    h.key(Key::ArrowDown);
    assert!(h.perform(SemanticAction::Activate, None));
    h.key(Key::Escape);
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::Command(COPY),
            ControlMessage::Command(SHARE),
            ControlMessage::Command(COPY),
            ControlMessage::Dismissed,
        ]
    );

    let paste = text_point(&h.paint(), "Paste");
    h.click(paste);
    assert_eq!(h.messages(), vec![ControlMessage::Command(PASTE)]);

    h.set_enabled(false);
    h.click(paste);
    h.key(Key::Enter);
    assert!(h.messages().is_empty());
    assert!(h.semantics().actions.contains(&SemanticAction::Focus));
}

#[test]
fn a_hosted_dropdown_menu_skips_disabled_commands() {
    let mut menu = edit_menu().hosted("Edit");
    menu.set_command(
        COPY,
        CommandState {
            enabled: false,
            checked: false,
        },
    )
    .unwrap();
    let mut h = Harness::new(menu);
    h.focus();
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert_eq!(h.messages(), vec![ControlMessage::Command(PASTE)]);
}

fn delete_alert() -> AlertDialog {
    AlertDialog::new(
        "Delete file?",
        "This cannot be undone.",
        vec![
            DialogButton::new("Cancel", DialogButtonRole::Reject),
            DialogButton::new("Delete", DialogButtonRole::Destructive),
            DialogButton::new("Keep", DialogButtonRole::Accept),
        ],
    )
}

fn open(h: &mut Harness) {
    assert!(h.host.set_open(&mut h.tree, h.node, true));
}

/// Paints after the open animation ends, without moving the host time.
fn settled_paint(h: &mut Harness) -> DisplayList {
    h.time += 10.0;
    let list = h.paint();
    h.time -= 10.0;
    list
}

#[test]
fn a_hosted_alert_is_modal_and_reports_its_buttons() {
    let mut h = Harness::new(delete_alert().hosted());
    assert!(h.semantics().state.hidden);
    assert!(h.host.modal(&h.tree).is_none());
    assert!(!h.perform(SemanticAction::Dismiss, None));
    assert!(texts(&h.paint()).is_empty());

    open(&mut h);
    assert_eq!(h.host.modal(&h.tree), Some(h.node));
    let semantics = h.semantics();
    assert_eq!(semantics.role, SemanticRole::Dialog);
    assert_eq!(semantics.name, "Delete file?");
    assert_eq!(semantics.actions, vec![SemanticAction::Dismiss]);
    let painted = settled_paint(&mut h);
    assert_eq!(
        texts(&painted),
        vec![
            "Delete file?",
            "This cannot be undone.",
            "Cancel",
            "Delete",
            "Keep"
        ]
    );
    // Enter never triggers the destructive button.
    h.key(Key::Enter);
    assert!(h.host.modal(&h.tree).is_none());
    open(&mut h);
    h.key(Key::Escape);
    open(&mut h);
    assert!(h.perform(SemanticAction::Dismiss, None));
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::ItemActivated(2),
            ControlMessage::OpenChanged(false),
            ControlMessage::ItemActivated(0),
            ControlMessage::OpenChanged(false),
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed,
        ]
    );

    open(&mut h);
    // A press outside the panel goes to the modal alert and does nothing.
    h.click(Point::new(2.0, 2.0));
    assert!(h.messages().is_empty());
    h.click(text_point(&painted, "Delete"));
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::ItemActivated(1),
            ControlMessage::OpenChanged(false),
        ]
    );
}

#[test]
fn a_hosted_alert_moves_focus_between_its_buttons() {
    let mut h = Harness::new(delete_alert().hosted());
    open(&mut h);
    // Focus starts on the default button, the last one. Shift+Tab moves
    // back to Delete, and Tab wraps from Keep to Cancel.
    h.modifiers(ShortcutModifiers {
        shift: true,
        ..ShortcutModifiers::default()
    });
    h.key(Key::Tab);
    h.modifiers(ShortcutModifiers::default());
    h.key(Key::Space);
    open(&mut h);
    h.key(Key::Tab);
    h.key(Key::Space);
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::ItemActivated(1),
            ControlMessage::OpenChanged(false),
            ControlMessage::ItemActivated(0),
            ControlMessage::OpenChanged(false),
        ]
    );
    open(&mut h);
    h.set_enabled(false);
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ],
        "disabling closes the open alert and reports it"
    );
    assert!(h.host.modal(&h.tree).is_none());
    h.key(Key::Enter);
    assert!(h.messages().is_empty());
}

fn red_picker() -> ColorPicker {
    ColorPicker::new(Color::rgba8(255, 0, 0, 255)).with_name("Accent")
}

#[test]
fn a_hosted_color_picker_edits_channels_and_takes_hex_values() {
    let mut h = Harness::new(red_picker());
    let semantics = h.semantics();
    assert_eq!(semantics.role, SemanticRole::Group);
    assert_eq!(semantics.name, "Accent");
    assert_eq!(semantics.value.as_deref(), Some("#FF0000"));

    h.focus();
    // ArrowDown moves to the saturation channel.
    h.key(Key::ArrowDown);
    h.key(Key::Home);
    h.key(Key::End);
    h.key(Key::Enter);
    assert!(h.perform(SemanticAction::SetValue, Some("#00FF00")));
    assert!(!h.perform(SemanticAction::SetValue, Some("green")));
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::Changed(ControlValue::Text("#FFFFFF".into())),
            ControlMessage::Changed(ControlValue::Text("#FF0000".into())),
            ControlMessage::Activated,
            ControlMessage::Changed(ControlValue::Text("#00FF00".into())),
        ]
    );

    assert!(
        h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Text("#0000FF".into()))
    );
    assert!(
        !h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Bool(true))
    );
    assert_eq!(h.semantics().value.as_deref(), Some("#0000FF"));
    assert!(h.messages().is_empty());

    // A press at the start of the last row drags the opacity to zero.
    let bounds = h.bounds();
    let gap = DesignTokens::light().spacing * 2.0;
    h.click(Point::new(bounds.origin.x, bounds.max_y() - gap - 1.0));
    assert_eq!(
        h.messages(),
        vec![ControlMessage::Changed(ControlValue::Text(
            "#0000FF00".into()
        ))]
    );

    h.set_enabled(false);
    h.key(Key::End);
    assert!(!h.perform(SemanticAction::SetValue, Some("#FFFFFF")));
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_color_picker_mirrors_its_arrow_keys() {
    let mut rtl = Harness::with_direction(red_picker(), LayoutDirection::Rtl);
    rtl.focus();
    // The hue is at its start: only the forward key moves it.
    rtl.key(Key::ArrowRight);
    assert!(rtl.messages().is_empty());
    rtl.key(Key::ArrowLeft);
    assert_eq!(rtl.messages().len(), 1);
}

fn edit_toolbar() -> Toolbar {
    Toolbar::new(
        "Edit",
        vec![
            ToolbarItem::command("copy", "Copy", COPY),
            ToolbarItem::separator("gap"),
            ToolbarItem::command("paste", "Paste", PASTE),
            ToolbarItem::command("share", "Share", SHARE),
        ],
    )
}

#[test]
fn a_hosted_toolbar_runs_commands_by_key_and_pointer() {
    let toolbar = edit_toolbar();
    let mut h = Harness::new(toolbar.clone().hosted());
    assert_eq!(h.semantics().role, SemanticRole::Toolbar);
    assert_eq!(h.semantics().name, "Edit");
    assert_eq!(texts(&h.paint()), vec!["Copy", "Paste", "Share"]);

    // Focus entering the toolbar goes to the first command.
    h.focus();
    h.key(Key::Enter);
    h.key(Key::ArrowRight);
    h.key(Key::Space);
    h.key(Key::End);
    h.key(Key::Enter);
    let layout = toolbar.layout(h.bounds(), LayoutDirection::Ltr);
    let (_, paste) = layout.items[2];
    h.click(paste.center());
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::Command(COPY),
            ControlMessage::Command(PASTE),
            ControlMessage::Command(SHARE),
            ControlMessage::Command(PASTE),
        ]
    );

    h.set_enabled(false);
    h.click(paste.center());
    h.key(Key::Enter);
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_toolbar_skips_disabled_commands_and_mirrors_keys() {
    let mut toolbar = edit_toolbar().hosted();
    toolbar
        .set_command(
            PASTE,
            CommandState {
                enabled: false,
                checked: false,
            },
        )
        .unwrap();
    let mut h = Harness::with_direction(toolbar, LayoutDirection::Rtl);
    h.focus();
    // ArrowLeft is next in a right-to-left toolbar.
    h.key(Key::ArrowLeft);
    h.key(Key::Enter);
    assert_eq!(h.messages(), vec![ControlMessage::Command(SHARE)]);
}

#[test]
fn a_hosted_toolbar_opens_its_overflow_menu() {
    let mut h = Harness::new(edit_toolbar().hosted());
    let bounds = h.bounds();
    h.place(rect(
        bounds.origin.x,
        bounds.origin.y,
        bounds.size.width / 2.0,
        bounds.size.height,
    ));
    assert_eq!(h.semantics().state.expanded, Some(false));
    let layout = edit_toolbar().layout(h.bounds(), LayoutDirection::Ltr);
    let chevron: Rect = layout.overflow_button.unwrap();
    h.click(chevron.center());
    assert_eq!(h.messages(), vec![ControlMessage::OpenChanged(true)]);
    assert_eq!(h.semantics().state.expanded, Some(true));
    let painted = settled_paint(&mut h);
    let share = text_point(&painted, "Share");
    h.click(share);
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::Command(SHARE),
            ControlMessage::OpenChanged(false),
        ]
    );
    assert!(h.host.set_open(&mut h.tree, h.node, true));
    assert!(h.host.get(h.node).unwrap().overlay().is_some());
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_status_bar_expires_its_message_on_the_host_timer() {
    let mut bar = StatusBar::new();
    bar.set_default_text("Ready");
    bar.set_section("line", "Ln 1", 60.0);
    bar.show_message("Saved", Some(Duration::from_secs(2)), Duration::ZERO);
    let mut h = Harness::new(bar);
    assert!(!h.host.get(h.node).unwrap().focusable());
    let semantics = h.semantics();
    assert_eq!(semantics.role, SemanticRole::StatusBar);
    assert_eq!(semantics.value.as_deref(), Some("Saved, Ln 1"));
    assert_eq!(texts(&h.paint()), vec!["Saved", "Ln 1"]);

    assert_eq!(h.host.next_deadline(&h.tree), Some(2.0));
    assert!(!h.host.advance(&mut h.tree, 1.0));
    assert!(h.host.advance(&mut h.tree, 2.0));
    assert_eq!(h.semantics().value.as_deref(), Some("Ready, Ln 1"));
    assert_eq!(h.host.next_deadline(&h.tree), None);

    assert!(h.perform(SemanticAction::Focus, None));
    assert_eq!(h.tree.focus(), Some(h.node));
    assert!(h.messages().is_empty());
    h.set_enabled(false);
    assert!(!h.perform(SemanticAction::Focus, None));
}

/// Arranges `control` in `bounds` as a host would.
fn arrange(control: &mut dyn CustomControl, bounds: Rect, direction: LayoutDirection) {
    let (tokens, styles) = (DesignTokens::light(), Styles::new());
    let cx = MeasureContext {
        constraints: Constraints::tight(bounds.size),
        viewport: bounds,
        direction,
        tokens: &tokens,
        styles: &styles,
        text: &EstimatedText,
    };
    control.arrange(&cx, bounds);
}

fn wheel(h: &mut Harness, at: Point, dy: f64) {
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Wheel {
            position: at,
            delta: Point::new(0.0, dy),
            unit: WheelUnit::Points,
        },
    );
}

fn pointer(h: &mut Harness, event: PointerEvent) {
    h.host.dispatch_pointer(&mut h.tree, event);
}

#[test]
fn a_hosted_resizable_moves_its_handle_by_key_and_action() {
    let resizable = Resizable::hosted(
        "Sidebar",
        SplitOrientation::Horizontal,
        50.0,
        50.0,
        4.0,
        100.0,
    )
    .unwrap();
    let mut h = Harness::new(resizable);
    h.place(rect(0.0, 0.0, 300.0, 100.0));
    assert_eq!(h.semantics().role, SemanticRole::Slider);
    assert_eq!(h.semantics().name, "Sidebar");
    h.focus();
    h.key(Key::ArrowRight);
    h.key(Key::End);
    h.key(Key::Home);
    assert!(h.perform(SemanticAction::Increment, None));
    assert_eq!(
        h.messages(),
        [110.0, 246.0, 50.0, 60.0]
            .map(|value| ControlMessage::Changed(ControlValue::Number(value)))
    );
    assert!(
        h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Number(80.0))
    );
    assert_eq!(h.semantics().value.as_deref(), Some("80"));
    for bad in [10.0, 500.0, f64::NAN] {
        assert!(
            !h.host
                .set_value(&mut h.tree, h.node, &ControlValue::Number(bad))
        );
    }
    assert!(h.messages().is_empty());
    h.set_enabled(false);
    h.key(Key::ArrowRight);
    assert!(!h.perform(SemanticAction::Increment, None));
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_resizable_mirrors_in_right_to_left_layouts() {
    let resizable = || {
        Resizable::hosted(
            "Sidebar",
            SplitOrientation::Horizontal,
            50.0,
            50.0,
            4.0,
            100.0,
        )
        .unwrap()
    };
    let mut h = Harness::with_direction(resizable(), LayoutDirection::Rtl);
    h.place(rect(0.0, 0.0, 300.0, 100.0));
    h.focus();
    h.key(Key::ArrowLeft);
    assert_eq!(
        h.messages(),
        vec![ControlMessage::Changed(ControlValue::Number(110.0))]
    );

    let mut control = resizable();
    let bounds = rect(0.0, 0.0, 300.0, 100.0);
    arrange(&mut control, bounds, LayoutDirection::Ltr);
    let (first, handle, second) = control.panes();
    assert_eq!(first, rect(0.0, 0.0, 100.0, 100.0));
    assert_eq!(handle, rect(100.0, 0.0, 4.0, 100.0));
    assert_eq!(second, rect(104.0, 0.0, 196.0, 100.0));
    arrange(&mut control, bounds, LayoutDirection::Rtl);
    let (first, handle, _) = control.panes();
    assert_eq!(first, rect(200.0, 0.0, 100.0, 100.0));
    assert_eq!(handle, rect(196.0, 0.0, 4.0, 100.0));
    assert!(matches!(
        Resizable::hosted("Bad", SplitOrientation::Vertical, 50.0, 50.0, 0.0, 100.0),
        Err(ContainerError::InvalidGeometry)
    ));
}

#[test]
fn a_hosted_group_box_paints_its_title_and_reports_its_content_area() {
    let mut h = Harness::new(GroupBox::hosted("Options"));
    h.place(rect(10.0, 10.0, 200.0, 120.0));
    assert_eq!(h.semantics().role, SemanticRole::Group);
    assert_eq!(h.semantics().name, "Options");
    assert!(!h.host.get(h.node).unwrap().focusable());
    assert_eq!(texts(&h.paint()), vec!["Options"]);
    h.click(Point::new(50.0, 50.0));
    assert!(h.messages().is_empty());

    let mut control = GroupBox::hosted("Options");
    let bounds = rect(10.0, 10.0, 200.0, 120.0);
    arrange(&mut control, bounds, LayoutDirection::Ltr);
    let content = control.content_rect();
    assert!(content.origin.y > bounds.origin.y);
    assert!(content.max_y() < bounds.max_y() && content.size.width < bounds.size.width);
}

#[test]
fn a_hosted_stack_takes_its_page_from_the_application() {
    let mut h = Harness::new(StackedContainer::hosted("Pages", 3, 0).unwrap());
    assert_eq!(h.semantics().value.as_deref(), Some("1"));
    assert!(!h.host.get(h.node).unwrap().focusable());
    assert!(
        h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Index(Some(1)))
    );
    assert_eq!(h.semantics().value.as_deref(), Some("2"));
    for bad in [ControlValue::Index(Some(3)), ControlValue::Index(None)] {
        assert!(!h.host.set_value(&mut h.tree, h.node, &bad));
    }
    assert!(h.messages().is_empty());
    assert!(matches!(
        StackedContainer::hosted("Pages", 0, 0),
        Err(ContainerError::Empty)
    ));
    assert!(matches!(
        StackedContainer::hosted("Pages", 2, 2),
        Err(ContainerError::InvalidPosition)
    ));
}

/// A harness with a hosted scroll area over a 100×100 viewport node and
/// 100×1000 content with 20-point lines.
fn scroll_area() -> (Harness, choscorui_core::NodeId) {
    let mut h = Harness::new(StackedContainer::hosted("Placeholder", 1, 0).unwrap());
    let root = h.tree.root();
    let viewport = h.tree.attach(root, rect(0.0, 0.0, 100.0, 100.0)).unwrap();
    let content = h
        .tree
        .attach(viewport, rect(0.0, 0.0, 100.0, 1000.0))
        .unwrap();
    let scroll = ScrollViewport::new(
        viewport,
        content,
        Size::new(100.0, 1000.0),
        Size::new(10.0, 20.0),
    )
    .unwrap();
    let placeholder = h.node;
    h.host.unmount(&mut h.tree, placeholder);
    let area = ScrollArea::new(scroll).with_name("Log");
    h.node = h.host.mount(&mut h.tree, root, Box::new(area)).unwrap();
    h.place(rect(0.0, 0.0, 100.0, 100.0));
    (h, content)
}

#[test]
fn a_hosted_scroll_area_scrolls_by_wheel_key_drag_and_action() {
    let (mut h, content) = scroll_area();
    let top = |h: &Harness| h.tree.bounds(content).unwrap().origin.y;
    assert_eq!(h.semantics().role, SemanticRole::ScrollArea);
    assert_eq!(h.semantics().name, "Log");
    wheel(&mut h, Point::new(50.0, 50.0), 40.0);
    assert_eq!(top(&h), -40.0);
    assert_eq!(h.semantics().value.as_deref(), Some("4%"));
    h.focus();
    h.key(Key::PageDown);
    assert_eq!(top(&h), -120.0);
    assert!(h.perform(SemanticAction::Increment, None));
    assert_eq!(top(&h), -200.0);
    assert!(h.perform(SemanticAction::Decrement, None));
    assert_eq!(top(&h), -120.0);
    // The thumb is 20 points long; dragging it halfway scrolls halfway.
    h.key(Key::Home);
    pointer(
        &mut h,
        PointerEvent::Down {
            position: Point::new(95.0, 5.0),
            button: PointerButton::Primary,
        },
    );
    pointer(
        &mut h,
        PointerEvent::Move {
            position: Point::new(95.0, 45.0),
        },
    );
    pointer(
        &mut h,
        PointerEvent::Up {
            position: Point::new(95.0, 45.0),
            button: PointerButton::Primary,
        },
    );
    assert_eq!(top(&h), -450.0);
    assert_eq!(h.tree.pointer_capture(), None);
    assert!(
        !h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Number(0.0))
    );
    assert!(h.messages().is_empty());
    h.set_enabled(false);
    wheel(&mut h, Point::new(50.0, 50.0), 40.0);
    h.key(Key::PageDown);
    assert!(!h.perform(SemanticAction::Increment, None));
    assert_eq!(top(&h), -450.0);
}

const FILES: PanelId = PanelId(1);
const CONSOLE: PanelId = PanelId(3);
const SEARCH: PanelId = PanelId(4);

fn dock() -> DockLayout {
    let mut dock = DockLayout::new(24.0);
    let left = DockTarget::Tab {
        area: DockArea::Left,
        group: 0,
    };
    let bottom = DockTarget::Tab {
        area: DockArea::Bottom,
        group: 0,
    };
    dock.add(FILES, DockPanel::new("files", "Files"), left)
        .unwrap();
    let mut console = DockPanel::new("console", "Console");
    console.closable = false;
    dock.add(CONSOLE, console, bottom).unwrap();
    dock.add(SEARCH, DockPanel::new("search", "Search"), bottom)
        .unwrap();
    dock
}

#[test]
fn a_hosted_dock_selects_tabs_resizes_areas_and_closes_panels() {
    let bounds = rect(0.0, 0.0, 800.0, 600.0);
    let arrangement = dock().arrange(bounds);
    let bottom = arrangement
        .groups
        .iter()
        .find(|group| group.area == DockArea::Bottom)
        .unwrap()
        .clone();
    let left_splitter = arrangement
        .splitters
        .iter()
        .find(|(area, _)| *area == DockArea::Left)
        .unwrap()
        .1;
    let mut h = Harness::new(dock());
    h.place(bounds);
    let console = bottom.tab_rect(0, false).center();
    let search = bottom.tab_rect(1, false).center();
    h.click(console);
    assert_eq!(h.semantics().role, SemanticRole::TabPanel);
    assert_eq!(h.semantics().name, "Console");
    assert!(!h.perform(SemanticAction::Dismiss, None), "not closable");
    h.focus();
    h.key(Key::ArrowRight);
    assert_eq!(h.semantics().name, "Search");
    assert!(h.perform(SemanticAction::Dismiss, None));
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::ItemActivated(CONSOLE.0 as usize),
            ControlMessage::ItemActivated(SEARCH.0 as usize),
            ControlMessage::Dismissed,
        ]
    );
    assert_eq!(h.semantics().name, "Console");

    h.click(left_splitter.center());
    assert_eq!(h.semantics().role, SemanticRole::Slider);
    h.key(Key::ArrowRight);
    assert!(h.perform(SemanticAction::Decrement, None));
    assert_eq!(
        h.messages(),
        [250.0, 240.0].map(|value| ControlMessage::Changed(ControlValue::Number(value)))
    );
    assert!(
        !h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Number(300.0))
    );
    assert!(texts(&h.paint()).contains(&"Files".to_owned()));

    h.set_enabled(false);
    h.click(search);
    h.key(Key::ArrowRight);
    assert!(h.messages().is_empty());
}

#[test]
fn a_hosted_canvas_selects_activates_deletes_and_zooms() {
    let mut scene = Scene::new();
    let ids: Vec<_> = (0..3)
        .map(|i| {
            scene
                .add(
                    SceneItem::new(
                        ItemShape::Rect(rect(i as f64 * 100.0, 0.0, 50.0, 50.0)),
                        ItemStyle::filled(Color::rgba8(0, 128, 0, 255)),
                    )
                    .named(format!("Box {i}"), SemanticRole::Button),
                )
                .unwrap()
        })
        .collect();
    let canvas = CanvasView::new("Diagram", rect(0.0, 0.0, 400.0, 300.0)).hosted(scene);
    let mut h = Harness::new(canvas);
    assert_eq!(h.bounds(), rect(10.0, 10.0, 400.0, 300.0));
    assert_eq!(h.semantics().role, SemanticRole::List);
    assert_eq!(h.semantics().name, "Diagram");
    h.click(Point::new(135.0, 35.0));
    h.focus();
    h.key(Key::Enter);
    h.key(Key::Delete);
    let index = |id: choscorui_core::SceneItemId| id.0 as usize;
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::Changed(ControlValue::Indices(vec![index(ids[1])])),
            ControlMessage::ItemActivated(index(ids[1])),
            ControlMessage::Changed(ControlValue::Indices(Vec::new())),
        ]
    );
    let take = |h: &mut Harness, ids: Vec<usize>| {
        h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Indices(ids))
    };
    assert!(take(&mut h, vec![index(ids[0])]), "a live item");
    assert!(!take(&mut h, vec![index(ids[1])]), "a deleted item");
    assert!(h.messages().is_empty());
    h.modifiers(ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    });
    wheel(&mut h, Point::new(110.0, 110.0), -100.0);
    assert_ne!(h.semantics().description, "Zoom 100%");
    h.modifiers(ShortcutModifiers::default());

    h.set_enabled(false);
    h.click(Point::new(235.0, 35.0));
    h.key(Key::Escape);
    assert!(h.messages().is_empty());
}

#[test]
fn a_double_click_on_a_resizable_handle_collapses_and_restores_the_pane() {
    let resizable = Resizable::hosted(
        "Sidebar",
        SplitOrientation::Horizontal,
        50.0,
        50.0,
        4.0,
        100.0,
    )
    .unwrap();
    let mut h = Harness::new(resizable);
    h.place(rect(0.0, 0.0, 300.0, 100.0));
    let handle = Point::new(102.0, 50.0);
    h.double_click(handle);
    assert_eq!(h.semantics().value.as_deref(), Some("50"), "collapsed");
    let handle = Point::new(52.0, 50.0);
    h.double_click(handle);
    assert_eq!(h.semantics().value.as_deref(), Some("100"), "restored");
    assert_eq!(
        h.messages(),
        [50.0, 100.0].map(|value| ControlMessage::Changed(ControlValue::Number(value)))
    );
}

#[test]
fn a_double_click_on_a_dock_splitter_collapses_and_restores_the_area() {
    let bounds = rect(0.0, 0.0, 800.0, 600.0);
    let splitter = |dock: &DockLayout| {
        dock.arrange(bounds)
            .splitters
            .iter()
            .find(|(area, _)| *area == DockArea::Left)
            .unwrap()
            .1
            .center()
    };
    let mut h = Harness::new(dock());
    h.place(bounds);
    let open = splitter(&dock());
    h.double_click(open);
    let mut collapsed = dock();
    collapsed.set_extent(DockArea::Left, 80.0);
    h.double_click(splitter(&collapsed));
    assert_eq!(
        h.messages(),
        [80.0, 240.0].map(|value| ControlMessage::Changed(ControlValue::Number(value)))
    );
}
