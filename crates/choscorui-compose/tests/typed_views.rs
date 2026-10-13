//! Headless tests of the typed control views: each mounts, binds its
//! value in both directions where it edits one, runs its action where it
//! fires one, and takes its text from the catalog.

use choscorui_compose::Runtime;
use choscorui_compose::view::{
    AnyView, ColorPicker, ColumnBrowser, Configured, Cx, DateTimeEdit, DecimalSpinBox, Dial, Dock,
    Env, Frame, HeaderView, Input, InputGroup, ItemGroup, KeySequenceEdit, Label, List, Message,
    MessageScroller, NumericDisplay, Questionnaire, Select, SizeGrip, State, StatusBar, Toolbar,
    VStack, View, ViewExt, ViewHost,
};
use choscorui_controls::{
    AddonAlign, Bubble, CommandId, ControlKind, DateTimeKind, DockLayout, DockPanel, DockTarget,
    EstimatedText, HeaderColumn, InputGroupAddon, InputMask, Item, ListEntry, NumberBase, PanelId,
    Question, QuestionnaireLabels, ShortcutModifiers, StatusSection, Toggle, ToolbarArea,
    ToolbarItem,
};
use choscorui_core::{
    Catalog, CivilDate, CivilDateTime, CivilTime, DesignTokens, ItemModel, Key, KeyEvent, Locale,
    NodeId, Point, PointerButton, PointerEvent, SemanticAction, Size, SortOrder, StringListModel,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// A component whose body `build` makes.
struct Screen<F>(F);

impl<F: Fn(&mut Cx) -> AnyView + 'static> View for Screen<F> {
    fn body(&self, cx: &mut Cx) -> impl View {
        (self.0)(cx)
    }
}

fn en() -> Locale {
    Locale::parse("en").unwrap()
}

fn host_with(env: Env, runtime: Runtime, build: impl Fn(&mut Cx) -> AnyView + 'static) -> ViewHost {
    let mut host = ViewHost::new(
        Size::new(400.0, 300.0),
        DesignTokens::light(),
        env,
        runtime,
        Rc::new(EstimatedText),
    );
    host.mount(Screen(build));
    host.layout();
    assert_eq!(host.take_errors(), Vec::new());
    host
}

fn host(build: impl Fn(&mut Cx) -> AnyView + 'static) -> ViewHost {
    host_with(
        Env::new(Rc::new(Catalog::new()), en()),
        Runtime::new(),
        build,
    )
}

/// A slot that hands a component's state to the test.
type Slot<T> = Rc<Cell<Option<State<T>>>>;

fn slot<T>() -> Slot<T> {
    Rc::new(Cell::new(None))
}

/// The state in `slot`.
fn take<T>(slot: &Slot<T>) -> State<T> {
    slot.get().expect("the screen mounted")
}

fn press(host: &mut ViewHost, key: Key) {
    host.key(
        KeyEvent::Down { key, repeat: false },
        ShortcutModifiers::default(),
    );
}

fn value(host: &mut ViewHost, node: NodeId) -> Option<String> {
    host.semantic(node).unwrap().value
}

#[test]
fn a_label_follows_the_locale_on_the_same_node() {
    let runtime = Runtime::new();
    let locale = runtime.property(en());
    let vi = Locale::parse("vi").unwrap();
    let mut catalog = Catalog::new();
    catalog.insert(&en(), "email", "Email");
    catalog.insert(&vi, "email", "Thư điện tử");
    let live = locale.clone();
    let env = Env::new(Rc::new(catalog), move || live.get());
    let mut host = host_with(env, runtime, |_| {
        Label::new(choscorui_compose::t!("email")).into_any()
    });
    let node = host.find("Email").unwrap();
    locale.set(vi);
    host.flush();
    assert_eq!(host.find("Thư điện tử"), Some(node));
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn a_native_select_binds_both_ways() {
    let chosen = slot();
    let shared = chosen.clone();
    let mut host = host(move |cx| {
        let selected = cx.state(None);
        shared.set(Some(selected));
        let entries = vec![ListEntry::option("Todo"), ListEntry::option("Done")];
        Select::native("Status", entries, selected.binding()).into_any()
    });
    let node = host.find("Status").unwrap();
    assert!(host.perform(node, SemanticAction::SetValue, Some("Done")));
    host.flush();
    assert_eq!(take(&chosen).get(), Some(1));
    take(&chosen).set(Some(0));
    host.flush();
    assert_eq!(value(&mut host, node).as_deref(), Some("Todo"));
}

#[test]
fn an_input_group_binds_its_text_both_ways() {
    let text = slot();
    let shared = text.clone();
    let mut host = host(move |cx| {
        let value = cx.state(String::new());
        shared.set(Some(value));
        let addons = vec![InputGroupAddon::text(AddonAlign::InlineStart, "https://")];
        InputGroup::new("Website", addons, value.binding()).into_any()
    });
    let node = host.find("Website").unwrap();
    assert!(host.perform(node, SemanticAction::SetValue, Some("choscor")));
    host.flush();
    assert_eq!(take(&text).get(), "choscor");
    take(&text).set("example".to_owned());
    host.flush();
    assert_eq!(value(&mut host, node).as_deref(), Some("example"));
}

#[test]
fn a_masked_input_keeps_typed_characters_and_shows_literals() {
    let raw = slot();
    let shared = raw.clone();
    let mut host = host(move |cx| {
        let raw = cx.state(String::new());
        shared.set(Some(raw));
        let mask = InputMask::parse("(999) 999-9999").unwrap();
        Input::masked("Phone", mask, raw.binding()).into_any()
    });
    let node = host.find("Phone").unwrap();
    assert!(host.perform(node, SemanticAction::SetValue, Some("0905")));
    host.flush();
    assert_eq!(take(&raw).get(), "(090) 5", "the value keeps the literals");
    assert_eq!(value(&mut host, node).as_deref(), Some("(090) 5"));
    // Deleting the shown literal removes the typed digit before it.
    assert!(host.perform(node, SemanticAction::SetValue, Some("(090) ")));
    host.flush();
    assert_eq!(take(&raw).get(), "(090) ");
    take(&raw).set("1234567890".to_owned());
    host.flush();
    assert_eq!(value(&mut host, node).as_deref(), Some("(123) 456-7890"));
}

#[test]
fn a_decimal_spin_box_binds_both_ways() {
    let number = slot();
    let shared = number.clone();
    let mut host = host(move |cx| {
        let value = cx.state(1.5);
        shared.set(Some(value));
        DecimalSpinBox::new("Price", 0.0..=10.0, 0.25, 2, value.binding()).into_any()
    });
    let node = host.find("Price").unwrap();
    assert_eq!(value(&mut host, node).as_deref(), Some("1.50"));
    host.perform(node, SemanticAction::Increment, None);
    host.flush();
    assert_eq!(take(&number).get(), 1.75);
    take(&number).set(3.0);
    host.flush();
    assert_eq!(value(&mut host, node).as_deref(), Some("3.00"));
}

#[test]
fn a_date_time_edit_binds_both_ways_and_keeps_the_other_part() {
    let date = CivilDate::new(2026, 10, 12).unwrap();
    let at = move |hour| CivilDateTime {
        date,
        time: CivilTime::new(hour, 30, 0).unwrap(),
    };
    let stamp = slot();
    let shared = stamp.clone();
    let mut host = host(move |cx| {
        let value = cx.state(at(9));
        shared.set(Some(value));
        DateTimeEdit::new("Start", DateTimeKind::Time, value.binding()).into_any()
    });
    let node = host.find("Start").unwrap();
    host.focus(Some(node));
    host.perform(node, SemanticAction::Increment, None);
    host.flush();
    assert_eq!(take(&stamp).try_get().unwrap(), at(10));
    take(&stamp).set(at(14));
    host.flush();
    assert_eq!(value(&mut host, node).as_deref(), Some("2:30 PM"));
    assert_eq!(take(&stamp).try_get().unwrap().date, date);
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn a_key_sequence_edit_binds_both_ways() {
    let sequence = slot();
    let shared = sequence.clone();
    let mut host = host(move |cx| {
        let value = cx.state(String::new());
        shared.set(Some(value));
        KeySequenceEdit::new("Shortcut", 1, value.binding()).into_any()
    });
    let node = host.find("Shortcut").unwrap();
    host.focus(Some(node));
    assert!(!host.captures_keys());
    host.perform(node, SemanticAction::Activate, None);
    // While it records, the window gives it the menu key equivalents.
    assert!(host.captures_keys());
    let command = ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    };
    host.key(
        KeyEvent::Down {
            key: Key::Character('k'),
            repeat: false,
        },
        command,
    );
    host.flush();
    assert!(!host.captures_keys());
    let recorded = take(&sequence).get();
    assert!(recorded.contains('K'), "{recorded}");
    take(&sequence).set("⇧⌘S".to_owned());
    host.flush();
    assert_eq!(value(&mut host, node).as_deref(), Some("⇧⌘S"));
}

#[test]
fn a_color_picker_binds_its_hex_both_ways() {
    let hex = slot();
    let shared = hex.clone();
    let mut host = host(move |cx| {
        let value = cx.state("#00FF00".to_owned());
        shared.set(Some(value));
        ColorPicker::new("Color", value.binding()).into_any()
    });
    let node = host.find("Color").unwrap();
    assert!(host.perform(node, SemanticAction::SetValue, Some("#FF0000")));
    host.flush();
    assert_eq!(take(&hex).get(), "#FF0000");
    take(&hex).set("#0000FF".to_owned());
    host.flush();
    assert_eq!(take(&hex).get(), "#0000FF");
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn a_dial_binds_both_ways() {
    let level = slot();
    let shared = level.clone();
    let mut host = host(move |cx| {
        let value = cx.state(30);
        shared.set(Some(value));
        Dial::new("Gain", value.binding()).range(0..=100).into_any()
    });
    let node = host.find("Gain").unwrap();
    assert_eq!(value(&mut host, node).as_deref(), Some("30"));
    host.perform(node, SemanticAction::Increment, None);
    host.flush();
    assert_eq!(take(&level).get(), 31);
    take(&level).set(80);
    host.flush();
    assert_eq!(value(&mut host, node).as_deref(), Some("80"));
}

#[test]
fn a_numeric_display_shows_its_live_value() {
    let number = slot();
    let shared = number.clone();
    let mut host = host(move |cx| {
        let value = cx.state(2026_i64);
        shared.set(Some(value));
        NumericDisplay::new("Counter", 6, NumberBase::Decimal, value).into_any()
    });
    let node = host.find("Counter").unwrap();
    assert_eq!(value(&mut host, node).as_deref(), Some("2026"));
    take(&number).set(42);
    host.flush();
    assert_eq!(value(&mut host, node).as_deref(), Some("42"));
}

fn rows() -> Box<dyn ItemModel> {
    Box::new(StringListModel::from_strings(["Apples", "Pears", "Plums"]))
}

#[test]
fn a_table_binds_its_selection_both_ways() {
    let selection = slot();
    let shared = selection.clone();
    let mut host = host(move |cx| {
        let selected = cx.state(Vec::new());
        shared.set(Some(selected));
        List::table("Fruit", rows, selected.binding()).into_any()
    });
    let node = host.find("Fruit").unwrap();
    host.focus(Some(node));
    press(&mut host, Key::ArrowDown);
    host.flush();
    assert!(!take(&selection).get().is_empty());
    take(&selection).set(vec![2]);
    host.flush();
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn a_header_binds_its_sort_and_reports_resizes() {
    let sorted = slot();
    let shared = sorted.clone();
    let widths = Rc::new(RefCell::new(Vec::new()));
    let resized = widths.clone();
    let mut host = host(move |cx| {
        let sort = cx.state(None);
        shared.set(Some(sort));
        let columns = vec![
            HeaderColumn {
                title: "Name".into(),
                width: 120.0,
                min_width: 40.0,
            },
            HeaderColumn {
                title: "Size".into(),
                width: 80.0,
                min_width: 40.0,
            },
        ];
        let resized = resized.clone();
        VStack::new(
            HeaderView::new(columns, sort.binding())
                .on_resize(move |widths| *resized.borrow_mut() = widths),
        )
        .into_any()
    });
    let node = host.find("Name").unwrap();
    host.perform(node, SemanticAction::Activate, None);
    host.flush();
    assert_eq!(take(&sorted).get(), Some((0, SortOrder::Ascending)));
    host.perform(node, SemanticAction::Activate, None);
    host.flush();
    assert_eq!(take(&sorted).get(), Some((0, SortOrder::Descending)));
    take(&sorted).set(Some((0, SortOrder::Ascending)));
    host.flush();
    assert_eq!(value(&mut host, node).as_deref(), Some("ascending"));
    host.focus(Some(node));
    press(&mut host, Key::Character('+'));
    host.flush();
    assert_eq!(*widths.borrow(), vec![130.0, 80.0]);
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn a_column_browser_binds_its_path_both_ways() {
    let path = slot();
    let shared = path.clone();
    let mut host = host(move |cx| {
        let rows_path = cx.state(vec![0_usize]);
        shared.set(Some(rows_path));
        ColumnBrowser::new("Places", |_: &Env| rows(), rows_path.binding()).into_any()
    });
    let node = host.find("Places").unwrap();
    host.focus(Some(node));
    press(&mut host, Key::ArrowDown);
    host.flush();
    assert_eq!(take(&path).get(), vec![1]);
    take(&path).set(vec![2]);
    host.flush();
    press(&mut host, Key::ArrowUp);
    host.flush();
    assert_eq!(take(&path).get(), vec![1]);
}

#[test]
fn an_item_group_reports_an_activated_row() {
    let activated = Rc::new(Cell::new(None));
    let reported = activated.clone();
    let mut host = host(move |_| {
        let reported = reported.clone();
        ItemGroup::new(
            "Inbox",
            vec![Item::new("Ana").with_action("Open", Default::default())],
        )
        .on_item(move |index| reported.set(Some(index)))
        .into_any()
    });
    let node = host.find("Inbox").unwrap();
    host.perform(node, SemanticAction::Activate, None);
    assert_eq!(activated.get(), Some(0));
}

#[test]
fn chat_views_mount_with_their_content() {
    let mut host = host(|_| {
        let message = |text: &str| {
            choscorui_controls::Message::new("Ana", vec![Bubble::new(text.to_owned())])
        };
        VStack::new((
            Message::new("Ana", vec![Bubble::new("Lunch?")]),
            MessageScroller::new("Chat", vec![message("Hi"), message("Bye")])
                .frame(Frame::new().width(300.0).height(160.0)),
        ))
        .into_any()
    });
    host.advance(0.0);
    host.layout();
    assert!(host.find("Chat").is_some());
    assert_eq!(host.take_errors(), Vec::new());
}

fn labels(_: &Env) -> QuestionnaireLabels {
    QuestionnaireLabels {
        previous: "Previous".into(),
        skip: "Skip".into(),
        next: "Next".into(),
        submit: "Submit".into(),
        required: "Required".into(),
    }
}

#[test]
fn a_questionnaire_binds_its_question_and_runs_submit() {
    let question = slot();
    let shared = question.clone();
    let submitted = Rc::new(Cell::new(false));
    let ran = submitted.clone();
    let mut host = host(move |cx| {
        let current = cx.state(0_usize);
        shared.set(Some(current));
        let ran = ran.clone();
        let questions = vec![
            Question::choice("Color", vec!["Red".into(), "Blue".into()]),
            Question::choice("Size", vec!["S".into(), "L".into()]),
        ];
        Questionnaire::new("Survey", questions, labels, current.binding())
            .on_activate(move || ran.set(true))
            .into_any()
    });
    let node = host.find("Survey").unwrap();
    assert_eq!(value(&mut host, node).as_deref(), Some("Color"));
    host.focus(Some(node));
    // Tab moves from the answer to the buttons; Next shows question 2.
    for _ in 0..4 {
        if host.semantic(node).unwrap().description == "Next" {
            break;
        }
        press(&mut host, Key::Tab);
    }
    host.perform(node, SemanticAction::Activate, None);
    host.flush();
    assert_eq!(take(&question).get(), 1);
    take(&question).set(0);
    host.flush();
    assert_eq!(value(&mut host, node).as_deref(), Some("Color"));
    take(&question).set(1);
    host.flush();
    for _ in 0..4 {
        if host.semantic(node).unwrap().description == "Submit" {
            break;
        }
        press(&mut host, Key::Tab);
    }
    host.perform(node, SemanticAction::Activate, None);
    assert!(submitted.get());
}

#[test]
fn a_toolbar_runs_its_command_and_a_status_bar_shows_its_text() {
    const SAVE: CommandId = CommandId(7);
    let ran = Rc::new(Cell::new(None));
    let reported = ran.clone();
    let mut host = host(move |_| {
        let reported = reported.clone();
        VStack::new((
            Toolbar::new(
                "Editor",
                ToolbarArea::Top,
                vec![ToolbarItem::command("save", "Save", SAVE)],
            )
            .on_command(move |command| reported.set(Some(command))),
            StatusBar::new(
                "Ready",
                vec![StatusSection {
                    key: "line".into(),
                    text: "Ln 1".into(),
                    width: 60.0,
                }],
            ),
        ))
        .into_any()
    });
    let toolbar = host.find("Editor").unwrap();
    host.focus(Some(toolbar));
    press(&mut host, Key::Home);
    press(&mut host, Key::Enter);
    assert_eq!(ran.get(), Some(SAVE));
    let (snapshot, _) = host.semantics();
    assert!(
        snapshot.iter().any(|entry| entry
            .node
            .value
            .as_deref()
            .is_some_and(|value| value.contains("Ready"))),
        "the status bar reads its message"
    );
}

#[test]
fn a_dock_mounts_its_layout_and_reports_a_closed_panel() {
    let closed = Rc::new(Cell::new(false));
    let ran = closed.clone();
    let mut host = host(move |_| {
        let ran = ran.clone();
        Dock::new(|_| {
            let mut dock = DockLayout::new(24.0);
            let target = DockTarget::Tab {
                area: choscorui_controls::DockArea::Left,
                group: 0,
            };
            let _ = dock.add(PanelId(1), DockPanel::new("files", "Files"), target);
            dock
        })
        .on_dismiss(move || ran.set(true))
        .into_any()
    });
    let node = host.find("Files").unwrap();
    host.perform(node, SemanticAction::Dismiss, None);
    assert!(closed.get());
}

/// The leaf nodes under `node`.
fn leaves(host: &ViewHost, node: NodeId, found: &mut Vec<NodeId>) {
    match host.tree().children(node) {
        Some(children) if !children.is_empty() => {
            for child in children.iter().copied() {
                leaves(host, child, found);
            }
        }
        _ => found.push(node),
    }
}

#[test]
fn a_size_grip_reports_the_dragged_size() {
    let size = Rc::new(Cell::new(None));
    let reported = size.clone();
    let mut host = host(move |_| {
        let reported = reported.clone();
        SizeGrip::new(
            Size::new(100.0, 100.0),
            Size::new(1000.0, 1000.0),
            move |size| reported.set(Some(size)),
        )
        .into_any()
    });
    let mut found = Vec::new();
    leaves(&host, host.tree().root(), &mut found);
    let grip = host.tree().bounds(found[0]).unwrap();
    let start = grip.center();
    host.pointer(
        PointerEvent::Down {
            position: start,
            button: PointerButton::Primary,
        },
        ShortcutModifiers::default(),
    );
    host.pointer(
        PointerEvent::Move {
            position: Point::new(start.x + 20.0, start.y + 10.0),
        },
        ShortcutModifiers::default(),
    );
    assert_eq!(size.get(), Some(Size::new(420.0, 310.0)));
}

#[test]
fn bind_connects_a_custom_configured_control_both_ways() {
    let on = slot();
    let shared = on.clone();
    let mut host = host(move |cx| {
        let starred = cx.state(false);
        shared.set(Some(starred));
        Configured::custom(ControlKind::Toggle, |_| Toggle::new("Star", false))
            .bind(
                starred.binding(),
                choscorui_controls::ControlValue::Bool,
                |value| match value {
                    choscorui_controls::ControlValue::Bool(on) => Some(*on),
                    _ => None,
                },
            )
            .into_any()
    });
    let node = host.find("Star").unwrap();
    host.perform(node, SemanticAction::Activate, None);
    host.flush();
    assert!(take(&on).get());
    take(&on).set(false);
    host.flush();
    assert_eq!(host.semantic(node).unwrap().state.checked, Some(false));
}
