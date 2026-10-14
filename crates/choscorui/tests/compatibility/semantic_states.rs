//! Hosted controls in states that the catalog samples do not reach: the
//! child parts with their roles and states, for example a checked and a
//! disabled menu item, an open submenu, a sorted column, and today in a
//! calendar. Each fixture mounts left to right, takes the keyboard focus,
//! opens when it can, and then plays its keys. The catalog samples stay
//! small because the other golden files use them too.

use super::{mount_hosted, write_parts};
use choscorui::controls::{
    AddonAlign, Alert, AlertVariant, Bubble, ButtonVariant, Calendar, CommandId, CommandState,
    ContextMenu, ControlSize, CustomControl, DataTable, DatePicker, Dialog, DropdownMenu,
    EstimatedText, HostEnv, InputGroup, InputGroupAddon, MenuCheck, MenuItem, Menubar, Message,
    Question, Questionnaire, QuestionnaireLabels, Resizable, SheetSide, SplitOrientation, Styles,
    TextField,
};
use choscorui::core::{
    CivilDate, DesignTokens, Icon, InputModality, Key, KeyEvent, LayoutDirection, Locale, Rect,
    Size, ThemeResolver, Tree, TreeStore,
};
use std::fmt::Write;

/// Builds one control in a tree.
type Build = fn(&mut Tree) -> Box<dyn CustomControl>;

/// A fixture: its name, how to build it, the keys it plays, and a width
/// that replaces its natural width. A resizable is as wide as its pane
/// minimums by nature, which leaves no room to move its handle.
type Fixture = (&'static str, Build, &'static [Key], Option<f64>);

/// The menu of the menu fixtures: a checked check box item, a disabled
/// item, and a submenu. The commands get their state from the control.
fn menu() -> Vec<MenuItem> {
    vec![
        MenuItem::action("Show grid", CommandId(1)).with_check(MenuCheck::Checkbox),
        MenuItem::action("Paste", CommandId(2)),
        MenuItem::submenu("Share", vec![MenuItem::action("Email", CommandId(3))]),
    ]
}

const CHECKED: CommandState = CommandState {
    enabled: true,
    checked: true,
};

const DISABLED: CommandState = CommandState {
    enabled: false,
    checked: false,
};

/// The keys that move from no highlight to the submenu, past the
/// disabled item, and open it.
const TO_SUBMENU: &[Key] = &[Key::ArrowDown, Key::ArrowDown, Key::ArrowRight];

/// The keys that move from the first item to the submenu, past the
/// disabled item, and open it: an open popup highlights its first item.
const OPEN_TO_SUBMENU: &[Key] = &[Key::ArrowDown, Key::ArrowRight];

fn labels() -> QuestionnaireLabels {
    QuestionnaireLabels {
        previous: "Previous".into(),
        skip: "Skip".into(),
        next: "Next".into(),
        submit: "Submit".into(),
        required: "Required".into(),
    }
}

fn date(year: i32, month: u8, day: u8) -> CivilDate {
    CivilDate::new(year, month, day).unwrap()
}

fn fixtures() -> Vec<Fixture> {
    vec![
        (
            "dialog with buttons",
            |_| {
                let dialog = Dialog::new("Edit", "Edit profile", "Make changes here.")
                    .with_action("Cancel", ButtonVariant::Outline)
                    .with_action("Save", ButtonVariant::Default)
                    .with_close("Close");
                Box::new(dialog)
            },
            &[],
            None,
        ),
        (
            "sheet with buttons",
            |_| {
                let sheet =
                    Dialog::sheet(SheetSide::Right, "Open", "Edit profile", "Make changes.")
                        .with_action("Save", ButtonVariant::Default)
                        .with_close("Close");
                Box::new(sheet)
            },
            &[],
            None,
        ),
        (
            "drawer with buttons",
            |_| {
                // A drawer has no close button: the label shows no part.
                let drawer = Dialog::drawer(SheetSide::Bottom, "Open", "Move goal", "Set a goal.")
                    .with_action("Submit", ButtonVariant::Default)
                    .with_action("Cancel", ButtonVariant::Outline)
                    .with_close("Close");
                Box::new(drawer)
            },
            &[],
            None,
        ),
        (
            "destructive alert",
            |_| {
                let alert = Alert::new("Error", "Your session has expired.")
                    .with_variant(AlertVariant::Destructive)
                    .with_action("Sign in", ButtonVariant::Outline);
                Box::new(alert)
            },
            &[],
            None,
        ),
        (
            "dropdown-menu with states",
            |_| {
                let mut menu = DropdownMenu::new(menu()).hosted("Edit");
                menu.set_command(CommandId(1), CHECKED).unwrap();
                menu.set_command(CommandId(2), DISABLED).unwrap();
                Box::new(menu)
            },
            TO_SUBMENU,
            None,
        ),
        (
            "context-menu with states",
            |_| {
                let mut menu = ContextMenu::new("Area", "Right click here", menu());
                menu.set_command(CommandId(1), CHECKED).unwrap();
                menu.set_command(CommandId(2), DISABLED).unwrap();
                Box::new(menu)
            },
            OPEN_TO_SUBMENU,
            None,
        ),
        (
            "menubar with states",
            |_| {
                let mut bar = Menubar::new("Menu", vec![("View".to_string(), menu())]);
                bar.set_command(CommandId(1), CHECKED).unwrap();
                bar.set_command(CommandId(2), DISABLED).unwrap();
                Box::new(bar)
            },
            OPEN_TO_SUBMENU,
            None,
        ),
        (
            "calendar with today",
            |_| {
                let calendar = Calendar::new(Locale::parse("en-US").unwrap(), date(2024, 5, 15))
                    .with_today(date(2024, 5, 2))
                    .with_name("Due");
                Box::new(calendar)
            },
            &[],
            None,
        ),
        (
            "date-picker with today",
            |_| {
                let locale = Locale::parse("en").unwrap();
                let picker = DatePicker::new("Due date", locale, date(2026, 10, 12))
                    .with_today(date(2026, 10, 1));
                Box::new(picker)
            },
            &[],
            None,
        ),
        (
            "data-table sorted",
            |_| {
                let mut store = TreeStore::new(1);
                store.set_headers(["Name"]);
                for name in ["Chi", "Ana"] {
                    store.push(None, name.to_owned()).unwrap();
                }
                Box::new(DataTable::new(
                    "People",
                    Box::new(store),
                    "Previous",
                    "Next",
                ))
            },
            &[Key::ArrowRight, Key::Space],
            None,
        ),
        (
            "input-group with button",
            |_| {
                let addons = vec![
                    InputGroupAddon::icon(AddonAlign::InlineStart, Icon::Search),
                    InputGroupAddon::button(AddonAlign::InlineEnd, "Search", None, ControlSize::Xs),
                ];
                Box::new(InputGroup::new(TextField::new("Query", ""), addons))
            },
            &[],
            None,
        ),
        (
            "message with reactions",
            |_| {
                let bubble = Bubble::new("Ship it?").with_reactions("👍 2");
                Box::new(Message::new("Ana", vec![bubble]))
            },
            &[],
            None,
        ),
        (
            "questionnaire multiple",
            |_| {
                let question = Question::multiple("Pick any", vec!["A".into(), "B".into()]);
                Box::new(Questionnaire::new("Survey", vec![question], labels()))
            },
            &[Key::Space],
            None,
        ),
        (
            "resizable",
            |_| {
                let orientation = SplitOrientation::Horizontal;
                Box::new(Resizable::hosted("Sidebar", orientation, 40.0, 60.0, 4.0, 120.0).unwrap())
            },
            &[],
            Some(300.0),
        ),
    ]
}

/// Writes each fixture: the control, its parts, its parts while open,
/// and its parts after its keys.
pub fn record(out: &mut String) {
    let themes = ThemeResolver::new(DesignTokens::light());
    let styles = Styles::new();
    let direction = LayoutDirection::Ltr;
    for (name, build, keys, width) in fixtures() {
        let (mut tree, mut host, node) = mount_hosted(build, &themes, &styles, direction);
        let env = HostEnv::new(&themes, &styles, &EstimatedText).direction(direction);
        if let Some(width) = width {
            let bounds = tree.bounds(node).unwrap();
            let size = Size::new(width, bounds.size.height);
            tree.set_bounds(node, Rect::new(bounds.origin, size))
                .unwrap();
        }
        tree.set_input_modality(InputModality::Keyboard);
        host.move_focus(&mut tree, Some(node));
        host.arrange(&tree, &env);
        let semantic = host.get(node).unwrap().semantics();
        writeln!(
            out,
            "{name}: role {:?} name {:?} value {:?} desc {:?} state {:?}",
            semantic.role, semantic.name, semantic.value, semantic.description, semantic.state
        )
        .unwrap();
        write_parts(out, &host.parts(node));
        if host.set_open(&mut tree, node, true) {
            host.arrange(&tree, &env);
            // The opening motion ends before the keys.
            host.advance(&mut tree, 10.0);
            host.arrange(&tree, &env);
            writeln!(out, "  open:").unwrap();
            write_parts(out, &host.parts(node));
        }
        if !keys.is_empty() {
            for key in keys {
                host.dispatch_key(
                    &mut tree,
                    KeyEvent::Down {
                        key: *key,
                        repeat: false,
                    },
                );
                host.dispatch_key(&mut tree, KeyEvent::Up { key: *key });
                host.arrange(&tree, &env);
            }
            writeln!(out, "  after {keys:?}:").unwrap();
            write_parts(out, &host.parts(node));
        }
    }
}
