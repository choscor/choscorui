//! `set_value` and `set_open` from the application on every hosted
//! control that reports a value or opens a popup, dialog, or disclosure.
//! A value or an open state set this way changes semantics and emits no
//! message.

mod hosted;

use choscorui_controls::{
    Accordion, AddonAlign, Breadcrumb, CalendarMode, Carousel, Collapsible, Command, CommandId,
    CommandItem, ContextMenu, ControlValue, DataTable, DatePicker, Dialog, Input, InputGroup,
    InputGroupAddon, InputOtp, ListEntry, MenuItem, Menubar, NavigationLink, NavigationMenu,
    Pagination, Popover, Question, Questionnaire, QuestionnaireLabels, RangeSlider, Select,
    Sidebar, SidebarCollapse, SidebarItem, Switch, TextField, Textarea, Toggle, ToggleGroup,
    ToggleItem,
};
use choscorui_core::{CivilDate, Icon, ItemData, ListStore, Locale, Role, SemanticRole, Value};
use hosted::Harness;

/// Sets `value` and checks that no message is emitted.
fn take(h: &mut Harness, value: ControlValue) -> bool {
    let taken = h.host.set_value(&mut h.tree, h.node, &value);
    assert!(h.messages().is_empty(), "set_value emits no message");
    taken
}

/// Opens or closes the control and checks that no message is emitted.
fn open(h: &mut Harness, open: bool) -> bool {
    let taken = h.host.set_open(&mut h.tree, h.node, open);
    assert!(h.messages().is_empty(), "set_open emits no message");
    taken
}

fn value(h: &Harness) -> Option<String> {
    h.semantics().value
}

fn expanded(h: &Harness) -> Option<bool> {
    h.semantics().state.expanded
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

#[test]
fn switch_takes_a_bool() {
    let mut h = Harness::new(Switch::new("Airplane mode", false));
    assert!(take(&mut h, ControlValue::Bool(true)));
    assert_eq!(h.semantics().state.checked, Some(true));
    assert!(!take(&mut h, ControlValue::Number(1.0)));
    assert!(!open(&mut h, true));
    assert_eq!(h.semantics().state.checked, Some(true));
}

#[test]
fn toggle_takes_a_bool() {
    let mut h = Harness::new(Toggle::new("Bold", false));
    assert!(take(&mut h, ControlValue::Bool(true)));
    assert_eq!(h.semantics().state.checked, Some(true));
    assert!(!take(&mut h, ControlValue::Index(Some(0))));
}

#[test]
fn toggle_group_takes_indices_that_fit() {
    let items = || {
        ["Bold", "Italic", "Underline"]
            .map(ToggleItem::new)
            .to_vec()
    };
    let mut h = Harness::new(ToggleGroup::new("Format", items(), true));
    assert!(take(&mut h, ControlValue::Indices(vec![0, 2])));
    assert_eq!(value(&h).as_deref(), Some("Bold, Underline"));
    assert!(!take(&mut h, ControlValue::Indices(vec![3])));
    assert!(!take(&mut h, ControlValue::Bool(true)));
    assert_eq!(value(&h).as_deref(), Some("Bold, Underline"));

    let mut h = Harness::new(ToggleGroup::new("Align", items(), false));
    assert!(!take(&mut h, ControlValue::Indices(vec![0, 1])));
    assert!(take(&mut h, ControlValue::Indices(vec![1])));
    assert_eq!(value(&h).as_deref(), Some("Italic"));
    assert!(take(&mut h, ControlValue::Indices(Vec::new())));
    assert_eq!(value(&h).as_deref(), Some(""));
}

fn fruits() -> Select {
    Select::new(
        "Fruit",
        vec![
            ListEntry::group("Fruits"),
            ListEntry::option("Apple"),
            ListEntry::option("Banana"),
        ],
    )
    .with_placeholder("Select a fruit")
}

#[test]
fn select_takes_an_option_index_and_opens() {
    let mut h = Harness::new(fruits());
    assert!(take(&mut h, ControlValue::Index(Some(2))));
    assert_eq!(value(&h).as_deref(), Some("Banana"));
    assert!(
        !take(&mut h, ControlValue::Index(Some(0))),
        "a group header"
    );
    assert!(!take(&mut h, ControlValue::Index(Some(9))));
    assert!(!take(&mut h, ControlValue::Text("Apple".into())));
    assert_eq!(value(&h).as_deref(), Some("Banana"));
    assert!(take(&mut h, ControlValue::Index(None)));
    assert_eq!(
        value(&h).as_deref(),
        Some(""),
        "no choice has an empty value"
    );

    assert!(open(&mut h, true));
    assert_eq!(expanded(&h), Some(true));
    assert!(open(&mut h, false));
    assert_eq!(expanded(&h), Some(false));
    h.set_enabled(false);
    assert!(!open(&mut h, true));
    assert_eq!(expanded(&h), Some(false));
}

fn date(day: u8) -> CivilDate {
    CivilDate::new(2026, 10, day).unwrap()
}

fn picker(mode: CalendarMode) -> DatePicker {
    DatePicker::new("Due date", Locale::parse("en").unwrap(), date(1))
        .with_mode(mode)
        .with_placeholder("Pick a date")
}

/// The trigger value of a picker of `mode` with `dates` chosen.
fn picker_value(mode: CalendarMode, dates: &[CivilDate]) -> Option<String> {
    let mut expected = picker(mode);
    expected.set_dates(dates);
    Harness::new(expected).semantics().value
}

#[test]
fn date_picker_takes_dates_of_its_mode_and_opens() {
    let mut h = Harness::new(picker(CalendarMode::Single));
    assert!(take(&mut h, ControlValue::Date(Some(date(14)))));
    assert_eq!(value(&h), picker_value(CalendarMode::Single, &[date(14)]));
    assert!(!take(&mut h, ControlValue::Dates(vec![date(12)])));
    assert!(take(&mut h, ControlValue::Date(None)));
    assert_eq!(value(&h).as_deref(), Some("Pick a date"));

    let mut h = Harness::new(picker(CalendarMode::Range));
    assert!(take(&mut h, ControlValue::Dates(vec![date(12), date(16)])));
    let range = picker_value(CalendarMode::Range, &[date(12), date(16)]);
    assert_eq!(value(&h), range);
    assert!(!take(&mut h, ControlValue::Dates(vec![date(16), date(12)])));
    assert!(!take(
        &mut h,
        ControlValue::Dates(vec![date(1), date(2), date(3)])
    ));
    assert!(!take(&mut h, ControlValue::Date(Some(date(3)))));
    assert_eq!(value(&h), range);

    let mut h = Harness::new(picker(CalendarMode::Multiple));
    assert!(take(&mut h, ControlValue::Dates(vec![date(3), date(9)])));
    assert_eq!(
        value(&h),
        picker_value(CalendarMode::Multiple, &[date(3), date(9)])
    );

    assert!(open(&mut h, true));
    assert_eq!(expanded(&h), Some(true));
    assert!(open(&mut h, false));
    assert_eq!(expanded(&h), Some(false));
}

#[test]
fn text_inputs_take_text() {
    let mut h = Harness::new(Input::new("Email", ""));
    assert!(take(&mut h, ControlValue::Text("ada@example.com".into())));
    assert_eq!(value(&h).as_deref(), Some("ada@example.com"));
    assert!(!take(&mut h, ControlValue::Number(1.0)));
    assert!(!open(&mut h, true));
    h.set_enabled(false);
    assert!(take(&mut h, ControlValue::Text("off".into())));
    assert_eq!(value(&h).as_deref(), Some("off"));

    let group = InputGroup::new(
        TextField::new("Search", ""),
        vec![InputGroupAddon::icon(AddonAlign::InlineStart, Icon::Search)],
    );
    let mut h = Harness::new(group);
    assert!(take(&mut h, ControlValue::Text("tokens".into())));
    assert_eq!(value(&h).as_deref(), Some("tokens"));
    assert!(!take(&mut h, ControlValue::Bool(true)));

    let mut h = Harness::new(Textarea::new("Notes", ""));
    assert!(take(&mut h, ControlValue::Text("one\ntwo".into())));
    assert_eq!(value(&h).as_deref(), Some("one\ntwo"));
    assert!(!take(&mut h, ControlValue::Index(None)));
    assert_eq!(value(&h).as_deref(), Some("one\ntwo"));
}

#[test]
fn input_otp_takes_text_that_fits() {
    let mut h = Harness::new(InputOtp::new("Code", 6));
    assert!(take(&mut h, ControlValue::Text("123".into())));
    assert_eq!(value(&h).as_deref(), Some("123"));
    assert!(!take(&mut h, ControlValue::Text("12a".into())));
    assert!(!take(&mut h, ControlValue::Text("1234567".into())));
    assert!(!take(&mut h, ControlValue::Number(123.0)));
    assert_eq!(value(&h).as_deref(), Some("123"));
}

#[test]
fn range_slider_clamps_and_snaps_a_range() {
    let mut h = Harness::new(RangeSlider::new("Price", 0.0, 100.0, 1.0, 20.0, 80.0));
    assert!(take(&mut h, ControlValue::Range(10.4, 120.0)));
    assert_eq!(value(&h).as_deref(), Some("10 – 100"));
    assert!(!take(&mut h, ControlValue::Range(50.0, 40.0)));
    assert!(!take(&mut h, ControlValue::Range(f64::NAN, 40.0)));
    assert!(!take(&mut h, ControlValue::Number(30.0)));
    assert_eq!(value(&h).as_deref(), Some("10 – 100"));
}

#[test]
fn collapsible_opens() {
    let mut h = Harness::new(Collapsible::new(
        "Repositories",
        "Toggle",
        "first",
        vec!["second".into()],
    ));
    assert!(open(&mut h, true));
    assert_eq!(expanded(&h), Some(true));
    assert!(open(&mut h, false));
    assert_eq!(expanded(&h), Some(false));
    assert!(!take(&mut h, ControlValue::Bool(true)));
}

fn faq() -> Vec<(String, String)> {
    [("Is it accessible?", "Yes."), ("Is it styled?", "Yes.")]
        .map(|(title, text)| (title.to_string(), text.to_string()))
        .to_vec()
}

#[test]
fn accordion_takes_the_open_items() {
    let mut h = Harness::new(Accordion::new("FAQ", faq()));
    assert!(take(&mut h, ControlValue::Indices(vec![1])));
    assert_eq!(value(&h).as_deref(), Some("Is it styled?"));
    assert!(!take(&mut h, ControlValue::Indices(vec![0, 1])), "single");
    assert!(!take(&mut h, ControlValue::Indices(vec![2])));
    assert!(!take(&mut h, ControlValue::Index(Some(0))));
    assert_eq!(value(&h).as_deref(), Some("Is it styled?"));
    assert!(!open(&mut h, true));

    let mut h = Harness::new(Accordion::new("FAQ", faq()).multiple());
    assert!(take(&mut h, ControlValue::Indices(vec![0, 1])));
    assert_eq!(
        value(&h).as_deref(),
        Some("Is it accessible?, Is it styled?")
    );
    assert!(take(&mut h, ControlValue::Indices(Vec::new())));
    assert_eq!(value(&h), None);
}

#[test]
fn popover_opens() {
    let mut h = Harness::new(Popover::new("Open", "Title", "Text"));
    assert!(open(&mut h, true));
    assert_eq!(expanded(&h), Some(true));
    assert!(open(&mut h, false));
    assert_eq!(expanded(&h), Some(false));
    assert!(!take(&mut h, ControlValue::Bool(true)));
    h.set_enabled(false);
    assert!(!open(&mut h, true));
}

#[test]
fn dialog_opens() {
    let mut h = Harness::new(Dialog::new("Edit profile", "Edit profile", "Make changes."));
    assert!(open(&mut h, true));
    assert_eq!(h.semantics().role, SemanticRole::Dialog);
    assert!(open(&mut h, false));
    assert_eq!(h.semantics().role, SemanticRole::Button);
    assert!(!take(&mut h, ControlValue::Bool(true)));
}

fn commands() -> Vec<(String, Vec<CommandItem>)> {
    vec![(
        "Suggestions".to_string(),
        vec![CommandItem::new("Calendar"), CommandItem::new("Calculator")],
    )]
}

#[test]
fn command_takes_the_search_and_its_dialog_opens() {
    let mut h = Harness::new(Command::new("Commands", "Search", commands()));
    assert!(take(&mut h, ControlValue::Text("calc".into())));
    assert_eq!(value(&h).as_deref(), Some("calc"));
    assert!(!take(&mut h, ControlValue::Index(Some(0))));
    assert!(!open(&mut h, true), "an inline palette has nothing to open");

    let mut h = Harness::new(Command::dialog(
        "Open palette",
        "Commands",
        "Search",
        commands(),
    ));
    assert!(open(&mut h, true));
    assert_eq!(h.semantics().role, SemanticRole::ComboBox);
    assert!(take(&mut h, ControlValue::Text("cal".into())));
    assert_eq!(value(&h).as_deref(), Some("cal"));
    assert!(open(&mut h, false));
    assert_eq!(h.semantics().role, SemanticRole::Button);
    assert!(open(&mut h, true));
    assert_eq!(value(&h).as_deref(), Some(""), "opening clears the search");
}

fn menu_items() -> Vec<MenuItem> {
    vec![
        MenuItem::action("Back", CommandId(1)),
        MenuItem::action("Forward", CommandId(2)),
    ]
}

#[test]
fn menus_open() {
    let mut h = Harness::new(ContextMenu::new(
        "Page actions",
        "Right click",
        menu_items(),
    ));
    assert!(open(&mut h, true));
    assert_eq!(expanded(&h), Some(true));
    assert!(open(&mut h, false));
    assert_eq!(expanded(&h), Some(false));
    assert!(!take(&mut h, ControlValue::Index(Some(0))));

    let mut h = Harness::new(Menubar::new(
        "Main menu",
        vec![
            ("File".to_string(), menu_items()),
            ("Edit".to_string(), menu_items()),
        ],
    ));
    assert!(open(&mut h, true));
    assert_eq!(expanded(&h), Some(true));
    assert_eq!(h.selected_part(), "File");
    assert!(open(&mut h, false));
    assert_eq!(expanded(&h), Some(false));
    h.set_enabled(false);
    assert!(!open(&mut h, true));
}

#[test]
fn breadcrumb_opens_its_ellipsis_menu() {
    let trail = || strings(&["Home", "Components", "Docs", "Breadcrumb"]);
    let mut h = Harness::new(Breadcrumb::new("Breadcrumb", trail()).with_collapsed(1..3));
    assert!(open(&mut h, true));
    assert_eq!(expanded(&h), Some(true));
    assert!(open(&mut h, false));
    assert_eq!(expanded(&h), Some(false));
    assert!(!take(&mut h, ControlValue::Index(Some(0))));

    let mut h = Harness::new(Breadcrumb::new("Breadcrumb", trail()));
    assert!(!open(&mut h, true), "nothing is collapsed");
    assert!(!open(&mut h, false));
}

#[test]
fn navigation_menu_opens_the_focused_trigger() {
    let mut h = Harness::new(NavigationMenu::new(
        "Site",
        vec![
            (
                "Getting started".to_string(),
                vec![NavigationLink::new("Introduction", "Re-usable components.")],
            ),
            ("Docs".to_string(), Vec::new()),
        ],
    ));
    assert!(open(&mut h, true));
    assert_eq!(expanded(&h), Some(true));
    assert!(open(&mut h, false));
    assert_eq!(expanded(&h), Some(false));
    assert!(!take(&mut h, ControlValue::Index(Some(0))));

    let mut h = Harness::new(NavigationMenu::new(
        "Site",
        vec![("Docs".to_string(), Vec::new())],
    ));
    assert!(!open(&mut h, true), "no trigger has links");
    assert!(!open(&mut h, false));
}

#[test]
fn pagination_takes_a_page() {
    let mut h = Harness::new(Pagination::new("Pages", "Previous", "Next", 10, 0));
    assert!(take(&mut h, ControlValue::Index(Some(4))));
    assert_eq!(value(&h).as_deref(), Some("5"));
    assert!(!take(&mut h, ControlValue::Index(Some(10))));
    assert!(!take(&mut h, ControlValue::Index(None)));
    assert!(!take(&mut h, ControlValue::Number(2.0)));
    assert_eq!(value(&h).as_deref(), Some("5"));
}

fn sidebar() -> Sidebar {
    Sidebar::new(
        "Navigation",
        "Toggle sidebar",
        vec![(
            "Application".to_string(),
            vec![
                SidebarItem::new("Inbox", Icon::Inbox),
                SidebarItem::new("Calendar", Icon::Calendar),
                SidebarItem::new("Search", Icon::Search),
            ],
        )],
    )
}

#[test]
fn sidebar_takes_an_item_and_collapses() {
    let mut h = Harness::new(sidebar());
    assert!(take(&mut h, ControlValue::Index(Some(2))));
    assert_eq!(value(&h).as_deref(), Some("Search"));
    assert!(!take(&mut h, ControlValue::Index(Some(3))));
    assert!(!take(&mut h, ControlValue::Indices(vec![0])));
    assert_eq!(value(&h).as_deref(), Some("Search"));
    assert!(take(&mut h, ControlValue::Index(None)));
    assert_eq!(value(&h), None);

    assert!(open(&mut h, false));
    assert_eq!(expanded(&h), Some(false));
    assert!(open(&mut h, true));
    assert_eq!(expanded(&h), Some(true));

    let mut h = Harness::new(sidebar().with_collapse(SidebarCollapse::None));
    assert!(!open(&mut h, false), "the sidebar does not collapse");
    assert_eq!(expanded(&h), Some(true));
}

#[test]
fn carousel_takes_a_slide() {
    let slides = strings(&["One", "Two", "Three"]);
    let mut h = Harness::new(Carousel::new("Gallery", "Previous", "Next", slides));
    assert!(take(&mut h, ControlValue::Index(Some(2))));
    assert_eq!(value(&h).as_deref(), Some("Three"));
    assert!(!take(&mut h, ControlValue::Index(Some(3))));
    assert!(!take(&mut h, ControlValue::Index(None)));
    assert!(!take(&mut h, ControlValue::Bool(true)));
    assert_eq!(value(&h).as_deref(), Some("Three"));
}

#[test]
fn questionnaire_takes_the_question_the_picks_and_the_answer() {
    let mut h = Harness::new(Questionnaire::new(
        "Survey",
        vec![
            Question::choice("Which plan fits?", strings(&["Free", "Pro", "Team"])),
            Question::multiple("Which features matter?", strings(&["Speed", "Price"])),
            Question::freeform("Anything else?", "Type here"),
        ],
        QuestionnaireLabels {
            previous: "Previous".into(),
            skip: "Skip".into(),
            next: "Next".into(),
            submit: "Submit".into(),
            required: "Pick an answer.".into(),
        },
    ));
    assert!(take(&mut h, ControlValue::Indices(vec![1])));
    let parts = h.parts();
    let pro = &parts.parts()[1];
    assert_eq!(
        (pro.node.name.as_str(), pro.node.state.checked),
        ("Pro", Some(true))
    );
    assert!(!take(&mut h, ControlValue::Indices(vec![0, 1])), "single");
    assert!(!take(&mut h, ControlValue::Indices(vec![3])));
    assert!(!take(&mut h, ControlValue::Text("Hi".into())), "a choice");

    assert!(take(&mut h, ControlValue::Index(Some(1))));
    assert_eq!(value(&h).as_deref(), Some("Which features matter?"));
    assert!(take(&mut h, ControlValue::Indices(vec![1, 0])));
    assert_eq!(h.focused_part(), "Speed");

    assert!(take(&mut h, ControlValue::Index(Some(2))));
    assert_eq!(value(&h).as_deref(), Some("Anything else?"));
    assert!(take(&mut h, ControlValue::Text("Dark mode".into())));
    let answer = h.focused_node().and_then(|node| node.value);
    assert_eq!(answer.as_deref(), Some("Dark mode"));
    assert!(!take(&mut h, ControlValue::Indices(vec![0])), "freeform");
    assert!(!take(&mut h, ControlValue::Index(Some(3))));
    assert_eq!(value(&h).as_deref(), Some("Anything else?"));
}

struct Payment(&'static str, &'static str);

impl ItemData for Payment {
    fn data(&self, column: usize, role: Role) -> Value {
        match (column, role) {
            (0, Role::Display | Role::Sort) => Value::Text(self.0.into()),
            (1, Role::Display | Role::Sort) => Value::Text(self.1.into()),
            _ => Value::None,
        }
    }
}

fn payments() -> ListStore<Payment> {
    let mut store = ListStore::new(2);
    store.set_headers(["Status", "Email"]);
    store
        .insert(
            None,
            0,
            vec![
                Payment("success", "ken99@example.com"),
                Payment("failed", "abe45@example.com"),
                Payment("success", "silas22@example.com"),
            ],
        )
        .unwrap();
    store
}

#[test]
fn data_table_takes_rows_and_a_filter_and_opens_its_column_menu() {
    let table = DataTable::new("Payments", Box::new(payments()), "Previous", "Next")
        .with_filter("Filter emails...", Some(1))
        .with_column_menu("Columns")
        .with_status(|selected, total| format!("{selected} of {total}"));
    let mut h = Harness::new(table);
    assert!(take(&mut h, ControlValue::Indices(vec![0, 2])));
    assert_eq!(value(&h).as_deref(), Some("2 of 3"));
    assert!(!take(&mut h, ControlValue::Indices(vec![3])));
    assert!(!take(&mut h, ControlValue::Index(Some(0))));
    assert_eq!(value(&h).as_deref(), Some("2 of 3"));
    assert!(take(&mut h, ControlValue::Text("ken".into())));
    assert_eq!(
        value(&h).as_deref(),
        Some("1 of 1"),
        "only rows in the filter count"
    );

    assert!(open(&mut h, true));
    assert_eq!(expanded(&h), Some(true));
    assert!(open(&mut h, false));
    assert_eq!(expanded(&h), Some(false));

    let mut h = Harness::new(DataTable::new(
        "Payments",
        Box::new(payments()),
        "Previous",
        "Next",
    ));
    assert!(!take(&mut h, ControlValue::Text("ken".into())), "no filter");
    assert!(!open(&mut h, true), "no column menu");
}
