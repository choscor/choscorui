//! Headless tests of the views for hosted controls that had only the
//! `Configured::custom` form before. Each view keeps the semantics and
//! the focus order of the configured form of its control, and runs its
//! action or edits its binding on the control's activation keys.

use choscorui_compose::Runtime;
use choscorui_compose::view::{
    AlertDialog, AnyView, AvatarGroup, Calendar, Checkbox, Command, CommandLink, Configured, Cx,
    DropdownMenu, Env, FontChooser, GroupBox, Kbd, NumericDisplay, ScrollBar, StackedContainer,
    State, Toaster, ToolBox, ToolButton, View, ViewHost,
};
use choscorui_controls as controls;
use choscorui_controls::{
    Avatar, CalendarMode, CommandId, CommandItem, ControlKind, ControlValue, DialogButton,
    DialogButtonRole, EstimatedText, MenuItem, NumberBase, ScrollOrientation, ShortcutModifiers,
    Toast, Toasts,
};
use choscorui_core::{
    Catalog, CivilDate, DesignTokens, Key, KeyEvent, Locale, NodeId, SemanticAction, SemanticEntry,
    Size,
};
use std::cell::Cell;
use std::rc::Rc;

/// A component whose body `build` makes.
struct Screen<F>(F);

impl<F: Fn(&mut Cx) -> AnyView + 'static> View for Screen<F> {
    fn body(&self, cx: &mut Cx) -> impl View {
        (self.0)(cx)
    }
}

fn host(build: impl Fn(&mut Cx) -> AnyView + 'static) -> ViewHost {
    let env = Env::new(Rc::new(Catalog::new()), Locale::parse("en").unwrap());
    let mut host = ViewHost::new(
        Size::new(600.0, 400.0),
        DesignTokens::light(),
        env,
        Runtime::new(),
        Rc::new(EstimatedText),
    );
    host.mount(Screen(build));
    host.layout();
    assert_eq!(host.take_errors(), Vec::new());
    host
}

/// The semantic entries of the host, then its tab order.
fn snapshot(host: &mut ViewHost) -> (Vec<SemanticEntry>, Vec<NodeId>) {
    let entries = host.semantics().0;
    let mut order = Vec::new();
    while let Some(node) = host.focus_next(false) {
        if order.contains(&node) {
            break;
        }
        order.push(node);
    }
    host.focus(None);
    (entries, order)
}

/// Checks that the view of `view` has the semantics and the focus order
/// of the configured form that `configured` makes.
fn same_as_configured(
    view: impl Fn(&mut Cx) -> AnyView + 'static,
    configured: impl Fn(&mut Cx) -> AnyView + 'static,
) {
    let (mut view, mut configured) = (host(view), host(configured));
    let (view, configured) = (snapshot(&mut view), snapshot(&mut configured));
    assert!(!view.0.is_empty(), "the view publishes semantics");
    assert_eq!(view, configured);
}

/// A slot that hands a component's state to the test.
type Slot<T> = Rc<Cell<Option<State<T>>>>;

fn slot<T>() -> Slot<T> {
    Rc::new(Cell::new(None))
}

fn take<T>(slot: &Slot<T>) -> State<T> {
    slot.get().expect("the screen mounted")
}

/// Presses and releases `key`. Space activates on the release.
fn press(host: &mut ViewHost, key: Key) {
    host.key(
        KeyEvent::Down { key, repeat: false },
        ShortcutModifiers::default(),
    );
    host.key(KeyEvent::Up { key }, ShortcutModifiers::default());
    host.flush();
}

fn people() -> Vec<Avatar> {
    vec![Avatar::new("Ana Lee", "AL"), Avatar::new("Bao Tran", "BT")]
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[test]
fn an_avatar_group_keeps_the_configured_semantics() {
    same_as_configured(
        |_| {
            AvatarGroup::new("Team", people())
                .with(|group| group.with_count("+3"))
                .into_any()
        },
        |_| {
            Configured::custom(ControlKind::Avatar, |_| {
                controls::AvatarGroup::new("Team", people()).with_count("+3")
            })
            .into_any()
        },
    );
}

#[test]
fn a_stacked_container_shows_the_current_page() {
    same_as_configured(
        |_| StackedContainer::new("Pages", 3, 1_usize).into_any(),
        |_| {
            Configured::try_custom(ControlKind::Card, |_| {
                controls::StackedContainer::hosted("Pages", 3, 0)
            })
            .value(ControlValue::Index(Some(1)))
            .into_any()
        },
    );
    let page = slot();
    let shared = page.clone();
    let mut host = host(move |cx| {
        let current = cx.state(0_usize);
        shared.set(Some(current));
        StackedContainer::new("Pages", 3, current).into_any()
    });
    let node = host.find("Pages").unwrap();
    let before = host.semantic(node).unwrap();
    take(&page).set(2);
    host.flush();
    assert_ne!(host.semantic(node).unwrap(), before);
}

#[test]
fn a_group_box_keeps_the_configured_semantics() {
    same_as_configured(
        |_| GroupBox::new("Shipping").into_any(),
        |_| {
            Configured::custom(ControlKind::Card, |_| {
                controls::GroupBox::hosted("Shipping")
            })
            .into_any()
        },
    );
}

#[test]
fn a_tool_box_opens_a_page_from_the_keyboard() {
    same_as_configured(
        |cx| {
            let open = cx.state(0_usize);
            ToolBox::new(strings(&["General", "Advanced"]), open.binding()).into_any()
        },
        |_| {
            Configured::custom(ControlKind::Accordion, |_| {
                controls::ToolBox::new(strings(&["General", "Advanced"]))
            })
            .value(ControlValue::Index(Some(0)))
            .into_any()
        },
    );
    let open = slot();
    let shared = open.clone();
    let mut host = host(move |cx| {
        let page = cx.state(0_usize);
        shared.set(Some(page));
        ToolBox::new(strings(&["General", "Advanced"]), page.binding()).into_any()
    });
    host.focus_next(false);
    press(&mut host, Key::ArrowDown);
    press(&mut host, Key::Space);
    assert_eq!(take(&open).get(), 1);
}

#[test]
fn a_scroll_bar_edits_its_offset() {
    let configured = |_: &mut Cx| {
        Configured::custom(ControlKind::ScrollArea, |_| {
            let mut bar = controls::ScrollBar::new("Log", ScrollOrientation::Vertical);
            bar.set_line_step(20.0);
            bar.set_range(1000.0, 200.0, 200.0);
            bar
        })
        .value(ControlValue::Number(0.0))
        .into_any()
    };
    let view = |cx: &mut Cx| {
        let top = cx.state(0.0);
        ScrollBar::new(
            "Log",
            ScrollOrientation::Vertical,
            1000.0,
            200.0,
            top.binding(),
        )
        .line_step(20.0)
        .into_any()
    };
    same_as_configured(view, configured);
    let offset = slot();
    let shared = offset.clone();
    let mut host = host(move |cx| {
        let top = cx.state(0.0);
        shared.set(Some(top));
        ScrollBar::new(
            "Log",
            ScrollOrientation::Vertical,
            1000.0,
            200.0,
            top.binding(),
        )
        .line_step(20.0)
        .into_any()
    });
    let bar = host.find("Log").unwrap();
    host.perform(bar, SemanticAction::Increment, None);
    host.flush();
    assert!(take(&offset).get() > 0.0);
}

#[test]
fn a_toaster_keeps_the_configured_semantics() {
    let queue = || {
        let toasts = Toasts::new();
        toasts.push(Toast::new("Event created").with_action("Undo"));
        toasts
    };
    same_as_configured(
        move |_| {
            Toaster::new("Notifications", queue(), "Close")
                .position(controls::ToastPosition::BottomEnd)
                .into_any()
        },
        move |_| {
            Configured::custom(ControlKind::Toast, move |_| {
                controls::Toaster::new("Notifications", queue(), "Close")
                    .with_position(controls::ToastPosition::BottomEnd)
            })
            .into_any()
        },
    );
}

#[test]
fn a_tool_button_runs_its_action_and_edits_its_check_state() {
    same_as_configured(
        |cx| {
            let bold = cx.state(false);
            ToolButton::new("Bold", || {})
                .checked(bold.binding())
                .into_any()
        },
        |_| {
            Configured::custom(ControlKind::Toggle, |_| {
                let mut button = controls::ToolButton::new("Bold", CommandId(0));
                button.checkable = true;
                button.hosted()
            })
            .value(ControlValue::Bool(false))
            .into_any()
        },
    );
    let ran = Rc::new(Cell::new(0));
    let count = ran.clone();
    let checked = slot();
    let shared = checked.clone();
    let mut host = host(move |cx| {
        let bold = cx.state(false);
        shared.set(Some(bold));
        let count = count.clone();
        AnyView::new((
            ToolButton::new("Cut", move || count.set(count.get() + 1)),
            ToolButton::new("Bold", || {}).checked(bold.binding()),
        ))
    });
    let cut = host.find("Cut").unwrap();
    host.perform(cut, SemanticAction::Activate, None);
    assert_eq!(ran.get(), 1);
    let bold = host.find("Bold").unwrap();
    host.focus(Some(bold));
    press(&mut host, Key::Space);
    assert!(take(&checked).get());
}

#[test]
fn a_command_link_runs_its_action() {
    same_as_configured(
        |_| CommandLink::new("Install", "Add the app", || {}).into_any(),
        |_| {
            Configured::custom(ControlKind::Item, |_| {
                controls::CommandLink::new("Install", "Add the app").hosted(CommandId(0))
            })
            .into_any()
        },
    );
    let ran = Rc::new(Cell::new(false));
    let seen = ran.clone();
    let mut host = host(move |_| {
        let seen = seen.clone();
        CommandLink::new("Install", "Add the app", move || seen.set(true)).into_any()
    });
    let link = host.find("Install").unwrap();
    host.focus(Some(link));
    press(&mut host, Key::Enter);
    assert!(ran.get());
}

fn buttons() -> Vec<DialogButton> {
    vec![
        DialogButton::new("Cancel", DialogButtonRole::Reject),
        DialogButton::new("Delete", DialogButtonRole::Accept),
    ]
}

#[test]
fn an_alert_dialog_reports_the_chosen_button_and_closes() {
    same_as_configured(
        |cx| {
            let open = cx.state(true);
            AlertDialog::new(
                "Delete?",
                "You cannot undo this.",
                buttons(),
                open.binding(),
            )
            .detail("One file")
            .into_any()
        },
        |cx| {
            let open = cx.state(true);
            Configured::custom(ControlKind::AlertDialog, |_| {
                controls::AlertDialog::new("Delete?", "You cannot undo this.", buttons())
                    .hosted()
                    .with_detail("One file")
            })
            .open(open.binding())
            .into_any()
        },
    );
    let chose = Rc::new(Cell::new(None));
    let seen = chose.clone();
    let open = slot();
    let shared = open.clone();
    let mut host = host(move |cx| {
        let presented = cx.state(true);
        shared.set(Some(presented));
        let seen = seen.clone();
        AlertDialog::new(
            "Delete?",
            "You cannot undo this.",
            buttons(),
            presented.binding(),
        )
        .on_item(move |index| seen.set(Some(index)))
        .into_any()
    });
    host.advance(1.0);
    press(&mut host, Key::Enter);
    assert_eq!(chose.get(), Some(1));
    assert!(!take(&open).get());
}

fn menu() -> Vec<MenuItem> {
    vec![
        MenuItem::action("Copy", CommandId(1)),
        MenuItem::action("Paste", CommandId(2)),
    ]
}

#[test]
fn a_dropdown_menu_reports_the_chosen_command() {
    same_as_configured(
        |_| DropdownMenu::new("Edit", menu()).into_any(),
        |_| {
            Configured::custom(ControlKind::Menu, |_| {
                controls::DropdownMenu::new(menu()).hosted("Edit")
            })
            .into_any()
        },
    );
    let ran = Rc::new(Cell::new(None));
    let seen = ran.clone();
    let mut host = host(move |_| {
        let seen = seen.clone();
        DropdownMenu::new("Edit", menu())
            .on_command(move |command| seen.set(Some(command)))
            .into_any()
    });
    let trigger = host.find("Edit").unwrap();
    host.focus(Some(trigger));
    press(&mut host, Key::Enter);
    host.advance(1.0);
    press(&mut host, Key::ArrowDown);
    press(&mut host, Key::Enter);
    assert!(ran.get().is_some());
}

#[test]
fn a_font_chooser_edits_the_chosen_family() {
    let families = || strings(&["Georgia", "Menlo"]);
    same_as_configured(
        move |cx| {
            let chosen = cx.state(Some(0_usize));
            FontChooser::new(families(), "Helvetica", chosen.binding()).into_any()
        },
        move |_| {
            Configured::custom(ControlKind::Combobox, move |_| {
                controls::FontChooser::new(families(), "Georgia", "Helvetica")
            })
            .value(ControlValue::Index(Some(0)))
            .into_any()
        },
    );
    let chosen = slot();
    let shared = chosen.clone();
    let mut host = host(move |cx| {
        let family = cx.state(Some(0_usize));
        shared.set(Some(family));
        FontChooser::new(families(), "Helvetica", family.binding()).into_any()
    });
    let node = host.find("Font").unwrap();
    assert!(host.perform(node, SemanticAction::SetValue, Some("Menlo")));
    host.flush();
    assert_eq!(take(&chosen).get(), Some(1));
}

#[test]
fn a_counter_steps_its_value_from_the_keyboard() {
    let value = slot();
    let shared = value.clone();
    let mut host = host(move |cx| {
        let count = cx.state(7_i64);
        shared.set(Some(count));
        NumericDisplay::counter("Count", 8, NumberBase::Decimal, count.binding()).into_any()
    });
    let node = host.find("Count").unwrap();
    assert_eq!(host.focus_next(false), Some(node));
    press(&mut host, Key::ArrowUp);
    assert_eq!(take(&value).get(), 8);
    press(&mut host, Key::PageUp);
    assert_eq!(take(&value).get(), 80);
    press(&mut host, Key::ArrowLeft);
    assert_eq!(take(&value).get(), 79);
    take(&value).set(5);
    host.flush();
    assert_eq!(host.semantic(node).unwrap().value.as_deref(), Some("5"));
    let mut plain =
        self::host(|_| NumericDisplay::new("Count", 8, NumberBase::Decimal, 7_i64).into_any());
    assert_eq!(plain.focus_next(false), None);
}

#[test]
fn key_caps_for_a_combination_keep_the_configured_semantics() {
    same_as_configured(
        |_| Kbd::keys(strings(&["⌘", "K"])).into_any(),
        |_| {
            Configured::custom(ControlKind::Kbd, |_| {
                controls::Kbd::group(strings(&["⌘", "K"]))
            })
            .into_any()
        },
    );
}

#[test]
fn a_mixed_checkbox_edits_an_optional_value() {
    same_as_configured(
        |cx| {
            let all: State<Option<bool>> = cx.state(None);
            Checkbox::mixed("Select all", all.binding()).into_any()
        },
        |_| {
            Configured::custom(ControlKind::Checkbox, |_| {
                controls::Checkbox::new("Select all", false)
            })
            .value(ControlValue::Mixed(None))
            .into_any()
        },
    );
    let checked = slot();
    let shared = checked.clone();
    let mut host = host(move |cx| {
        let all: State<Option<bool>> = cx.state(None);
        shared.set(Some(all));
        Checkbox::mixed("Select all", all.binding()).into_any()
    });
    let node = host.find("Select all").unwrap();
    assert_eq!(host.semantic(node).unwrap().state.checked, None);
    host.perform(node, SemanticAction::Activate, None);
    host.flush();
    assert!(take(&checked).get().is_some());
}

fn commands() -> Vec<(String, Vec<CommandItem>)> {
    vec![(
        "Suggestions".to_owned(),
        vec![CommandItem::new("Calendar"), CommandItem::new("Search")],
    )]
}

#[test]
fn a_command_dialog_keeps_the_configured_semantics() {
    same_as_configured(
        |_| Command::dialog("Open", "Commands", "Type a command", commands()).into_any(),
        |_| {
            Configured::custom(ControlKind::Command, |_| {
                controls::Command::dialog("Open", "Commands", "Type a command", commands())
            })
            .into_any()
        },
    );
}

#[test]
fn a_calendar_of_several_days_edits_its_dates() {
    let day = |day| CivilDate::new(2026, 10, day).unwrap();
    let dates = slot();
    let shared = dates.clone();
    let mut host = host(move |cx| {
        let chosen = cx.state(vec![day(8)]);
        shared.set(Some(chosen));
        Calendar::dates(day(1), CalendarMode::Multiple, chosen.binding())
            .with(|calendar| calendar.with_name("Meetings"))
            .into_any()
    });
    let node = host.find("Meetings").unwrap();
    host.focus(Some(node));
    press(&mut host, Key::ArrowRight);
    press(&mut host, Key::Enter);
    assert_eq!(take(&dates).get(), vec![day(8), day(9)]);
    take(&dates).set(vec![day(20)]);
    host.flush();
    press(&mut host, Key::Enter);
    assert_eq!(take(&dates).get(), Vec::new());
    assert_eq!(host.take_errors(), Vec::new());
}
