//! Headless tests of the form view and the presentation modifiers: the
//! form layout in both directions, tooltips, context and dropdown menus,
//! toasts, dialog and drawer panels, and hover cards.

use choscorui_compose::view::{
    Button, Cx, Env, Form, FormRow, MenuChoice, State, Text, VStack, View, ViewError, ViewExt,
    ViewHost,
};
use choscorui_compose::{Property, Runtime, t};
use choscorui_controls::{EstimatedText, SheetSide, ShortcutModifiers, Toast, ToastPosition};
use choscorui_core::{
    AccessibilityPreferences, Catalog, DesignTokens, DisplayCommand, DisplayList, Key, KeyEvent,
    Locale, NodeId, Point, PointerButton, PointerEvent, Rect, SemanticAction, SemanticRelation,
    SemanticRole, Size,
};
use std::rc::Rc;

fn host(view: impl View) -> ViewHost {
    host_in(view, "en")
}

/// A host whose locale has the language `tag`.
fn host_in(view: impl View, tag: &str) -> ViewHost {
    let locale = Locale::parse(tag).unwrap();
    let mut host = ViewHost::new(
        Size::new(600.0, 400.0),
        DesignTokens::light(),
        Env::new(Rc::new(Catalog::new()), locale),
        Runtime::new(),
        Rc::new(EstimatedText),
    );
    host.mount(view);
    host.layout();
    assert_eq!(host.take_errors(), Vec::new());
    host
}

fn node(host: &mut ViewHost, name: &str) -> NodeId {
    host.find(name)
        .unwrap_or_else(|| panic!("no node named {name}"))
}

fn bounds(host: &mut ViewHost, name: &str) -> Rect {
    let node = node(host, name);
    host.tree().bounds(node).unwrap()
}

fn press(host: &mut ViewHost, key: Key) -> bool {
    let handled = host.key(
        KeyEvent::Down { key, repeat: false },
        ShortcutModifiers::default(),
    );
    host.layout();
    handled
}

fn pointer(host: &mut ViewHost, event: PointerEvent) {
    host.pointer(event, ShortcutModifiers::default());
    host.layout();
}

/// Whether a node named `text` shows.
fn shown(host: &mut ViewHost, text: &str) -> bool {
    host.find(text).is_some()
}

// ---------------------------------------------------------------------------
// Form

fn signup() -> impl View {
    Form::new((
        FormRow::new("Name", Button::new("first field", || {})),
        FormRow::new("Email address", Button::new("second field", || {})),
    ))
}

#[test]
fn form_puts_labels_against_their_fields() {
    let mut host = host(signup());
    let (name, email) = (
        bounds(&mut host, "Name"),
        bounds(&mut host, "Email address"),
    );
    let (first, second) = (
        bounds(&mut host, "first field"),
        bounds(&mut host, "second field"),
    );
    // Labels end at one edge, beside the fields, which share a column.
    assert!(
        (name.max_x() - email.max_x()).abs() < 0.5,
        "{name:?} {email:?}"
    );
    assert!(name.size.width < email.size.width);
    assert!((first.min_x() - second.min_x()).abs() < 0.5);
    assert!(first.min_x() > email.max_x());
    // One row per pair, top to bottom.
    assert!(second.min_y() > first.max_y());
    assert!(email.min_y() > name.max_y());
    assert!(first.min_y() < name.max_y() && name.min_y() < first.max_y());
    // Focus goes through the fields in row order.
    assert_eq!(host.focus_next(false), Some(node(&mut host, "first field")));
    assert_eq!(
        host.focus_next(false),
        Some(node(&mut host, "second field"))
    );
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn form_mirrors_in_right_to_left_layouts() {
    let mut ltr = host(signup());
    let mut rtl = host_in(signup(), "ar");
    let (name, email) = (bounds(&mut rtl, "Name"), bounds(&mut rtl, "Email address"));
    let field = bounds(&mut rtl, "first field");
    assert!(
        (name.min_x() - email.min_x()).abs() < 0.5,
        "{name:?} {email:?}"
    );
    assert!(field.max_x() < email.min_x(), "the labels sit at the right");
    let ltr_field = bounds(&mut ltr, "first field");
    let width = ltr.size().width;
    assert!((ltr_field.min_x() - (width - field.max_x())).abs() < 0.5);
    assert_eq!(rtl.focus_next(false), Some(node(&mut rtl, "first field")));
}

#[test]
fn form_labels_follow_the_label_alignment() {
    let form = Form::new((
        FormRow::new("Name", Button::new("first field", || {})),
        FormRow::new("Email address", Button::new("second field", || {})),
    ))
    .label_alignment(choscorui_compose::view::HorizontalAlignment::Leading);
    let mut host = host(form);
    let (name, email) = (
        bounds(&mut host, "Name"),
        bounds(&mut host, "Email address"),
    );
    assert!((name.min_x() - email.min_x()).abs() < 0.5);
}

// ---------------------------------------------------------------------------
// Tooltip

fn toolbar() -> impl View {
    VStack::new((
        Button::new("Save", || {}).tooltip("Save the file"),
        Button::new("Other", || {}),
    ))
}

/// The tooltip wrapper of the node named `name`.
fn tip_of(host: &mut ViewHost, name: &str) -> NodeId {
    let node = node(host, name);
    host.tree().parent(node).unwrap()
}

fn tip_shows(host: &ViewHost, wrapper: NodeId) -> bool {
    host.controls()
        .get(wrapper)
        .is_some_and(|control| control.overlay().is_some())
}

#[test]
fn a_hover_change_eases_in_a_view_and_then_stops_asking_for_frames() {
    let mut host = host(toolbar());
    host.advance(1.0);
    host.paint();
    let center = bounds(&mut host, "Other").center();
    pointer(&mut host, PointerEvent::Move { position: center });
    let start = host.paint();
    assert!(host.needs_frame(), "the hover color eases in");
    host.advance(1.075);
    let middle = host.paint();
    assert!(host.needs_frame());
    assert_ne!(middle, start);
    host.advance(1.15);
    assert!(host.needs_frame(), "the last frame paints the hover style");
    let end = host.paint();
    assert_ne!(end, middle);
    host.advance(1.2);
    assert!(!host.needs_frame(), "an idle view asks for no frames");
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn tooltip_shows_after_the_hover_delay_and_hides_on_exit() {
    let mut host = host(toolbar());
    let save = node(&mut host, "Save");
    assert_eq!(
        host.semantic(save).unwrap().description,
        "Save the file",
        "the tooltip describes its view"
    );
    let wrapper = tip_of(&mut host, "Save");
    let center = bounds(&mut host, "Save").center();
    host.advance(10.0);
    pointer(&mut host, PointerEvent::Move { position: center });
    assert!(!tip_shows(&host, wrapper));
    let due = host
        .next_deadline()
        .expect("the tooltip waits for its delay");
    assert!((due - 10.6).abs() < 1e-6, "due at {due}");
    host.advance(10.3);
    assert!(!tip_shows(&host, wrapper));
    host.advance(10.6);
    host.layout();
    assert!(tip_shows(&host, wrapper));
    let bubble = host.controls().get(wrapper).unwrap().overlay().unwrap();
    let save_bounds = bounds(&mut host, "Save");
    assert!(
        bubble.max_y() <= save_bounds.min_y() || bubble.min_y() >= save_bounds.max_y(),
        "the bubble does not cover its view: {bubble:?}"
    );
    // The pointer leaves the view.
    pointer(
        &mut host,
        PointerEvent::Move {
            position: Point::new(5.0, 395.0),
        },
    );
    assert!(!tip_shows(&host, wrapper));
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn tooltip_shows_on_keyboard_focus_and_hides_on_escape() {
    let mut host = host(toolbar());
    let wrapper = tip_of(&mut host, "Save");
    host.advance(20.0);
    assert_eq!(host.focus_next(false), Some(node(&mut host, "Save")));
    assert!(!tip_shows(&host, wrapper));
    host.advance(20.6);
    host.layout();
    assert!(tip_shows(&host, wrapper));
    assert!(press(&mut host, Key::Escape), "Escape closes the tooltip");
    assert!(!tip_shows(&host, wrapper));
    // A pointer press also hides it, and it does not show again until
    // focus or the pointer moves on.
    host.advance(30.0);
    host.focus_next(false);
    host.focus_next(true);
    host.advance(31.0);
    host.layout();
    assert!(tip_shows(&host, wrapper));
    let center = bounds(&mut host, "Save").center();
    pointer(
        &mut host,
        PointerEvent::Down {
            position: center,
            button: PointerButton::Primary,
        },
    );
    assert!(!tip_shows(&host, wrapper));
    host.advance(40.0);
    assert!(!tip_shows(&host, wrapper));
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn escape_hides_a_hovered_tooltip_while_focus_is_elsewhere() {
    let mut host = host(toolbar());
    let wrapper = tip_of(&mut host, "Save");
    let other = node(&mut host, "Other");
    host.focus(Some(other));
    let center = bounds(&mut host, "Save").center();
    host.advance(10.0);
    pointer(&mut host, PointerEvent::Move { position: center });
    host.advance(10.6);
    host.layout();
    assert!(tip_shows(&host, wrapper));
    assert!(press(&mut host, Key::Escape), "Escape closes the tooltip");
    assert!(!tip_shows(&host, wrapper));
    assert_eq!(host.tree().focus(), Some(other), "focus stays");
    assert_eq!(host.take_errors(), Vec::new());
}

// ---------------------------------------------------------------------------
// Context menu

struct Editor;

impl View for Editor {
    fn body(&self, cx: &mut Cx) -> impl View {
        let chosen: State<String> = cx.state(String::from("none"));
        VStack::new((
            Button::new("Target", || {}).context_menu(vec![
                MenuChoice::action("Copy", move || chosen.set("copied".into())).shortcut("⌘C"),
                MenuChoice::separator(),
                MenuChoice::action("Delete", move || chosen.set("deleted".into())).destructive(),
            ]),
            Text::new(move || format!("chosen {}", chosen.get())),
        ))
    }
}

/// The focused node, which must be a menu.
fn focused_menu(host: &mut ViewHost) -> NodeId {
    let focus = host.tree().focus().expect("focus is in the menu");
    assert_eq!(host.semantic(focus).unwrap().role, SemanticRole::Menu);
    focus
}

#[test]
fn context_menu_opens_at_a_secondary_click_and_runs_an_action() {
    let mut host = host(Editor);
    let target = node(&mut host, "Target");
    host.focus(Some(target));
    let at = Point::new(
        bounds(&mut host, "Target").min_x() + 10.0,
        bounds(&mut host, "Target").center().y,
    );
    pointer(
        &mut host,
        PointerEvent::Down {
            position: at,
            button: PointerButton::Secondary,
        },
    );
    let menu = focused_menu(&mut host);
    let panel = host.tree().bounds(menu).unwrap();
    assert!((panel.min_x() - at.x).abs() < 0.5, "{panel:?} at {at:?}");
    assert!((panel.min_y() - at.y).abs() < 0.5, "{panel:?} at {at:?}");
    assert_eq!(host.semantic(menu).unwrap().name, "Target");
    press(&mut host, Key::ArrowDown);
    press(&mut host, Key::Enter);
    assert!(shown(&mut host, "chosen copied"));
    assert_eq!(host.tree().focus(), Some(target), "focus returns");
    assert!(host.semantic(menu).is_none(), "the menu closed");
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn context_menu_closes_on_escape_and_opens_for_show_menu() {
    let mut host = host(Editor);
    let target = node(&mut host, "Target");
    host.focus(Some(target));
    assert!(
        host.semantic(target)
            .unwrap()
            .actions
            .contains(&SemanticAction::ShowMenu)
    );
    assert!(host.perform(target, SemanticAction::ShowMenu, None));
    host.layout();
    let menu = focused_menu(&mut host);
    let panel = host.tree().bounds(menu).unwrap();
    let center = bounds(&mut host, "Target").center();
    assert!((panel.min_x() - center.x).abs() < 0.5);
    assert_eq!(host.semantic(menu).unwrap().description, "Copy");
    // Tab stays in the menu.
    host.focus_next(false);
    assert_eq!(host.tree().focus(), Some(menu));
    assert!(press(&mut host, Key::Escape));
    assert!(host.semantic(menu).is_none(), "Escape closes the menu");
    assert_eq!(host.tree().focus(), Some(target), "focus returns");
    assert!(shown(&mut host, "chosen none"));
    // A Control-click opens it too, and a press outside closes it.
    host.pointer(
        PointerEvent::Down {
            position: center,
            button: PointerButton::Primary,
        },
        ShortcutModifiers {
            control: true,
            ..ShortcutModifiers::default()
        },
    );
    host.layout();
    let menu = focused_menu(&mut host);
    pointer(
        &mut host,
        PointerEvent::Down {
            position: Point::new(595.0, 395.0),
            button: PointerButton::Primary,
        },
    );
    assert!(host.semantic(menu).is_none());
    assert_eq!(host.take_errors(), Vec::new());
}

fn press_with(host: &mut ViewHost, key: Key, modifiers: ShortcutModifiers) -> bool {
    let handled = host.key(KeyEvent::Down { key, repeat: false }, modifiers);
    host.layout();
    handled
}

const SHIFT: ShortcutModifiers = ShortcutModifiers {
    shift: true,
    control: false,
    option: false,
    command: false,
};

#[test]
fn context_menu_opens_from_the_keyboard_with_the_first_row_highlighted() {
    let mut host = host(Editor);
    let target = node(&mut host, "Target");
    host.focus(Some(target));
    assert!(
        !press(&mut host, Key::F(10)),
        "F10 alone is not the open key"
    );
    assert_eq!(host.tree().focus(), Some(target));
    assert!(press_with(&mut host, Key::F(10), SHIFT));
    let menu = focused_menu(&mut host);
    let panel = host.tree().bounds(menu).unwrap();
    let center = bounds(&mut host, "Target").center();
    assert!(
        (panel.min_x() - center.x).abs() < 0.5,
        "{panel:?} at {center:?}"
    );
    assert!(
        (panel.min_y() - center.y).abs() < 0.5,
        "{panel:?} at {center:?}"
    );
    assert_eq!(host.semantic(menu).unwrap().description, "Copy");
    press(&mut host, Key::Enter);
    assert!(shown(&mut host, "chosen copied"));
    assert_eq!(host.tree().focus(), Some(target), "focus returns");
    // The Menu key opens it too.
    assert!(press(&mut host, Key::ContextMenu));
    let menu = focused_menu(&mut host);
    assert_eq!(host.semantic(menu).unwrap().description, "Copy");
    press(&mut host, Key::Escape);
    // A secondary click after a keyboard open highlights no row.
    pointer(
        &mut host,
        PointerEvent::Down {
            position: center,
            button: PointerButton::Secondary,
        },
    );
    let menu = focused_menu(&mut host);
    press(&mut host, Key::ArrowDown);
    assert_eq!(host.semantic(menu).unwrap().description, "Copy");
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn the_keyboard_opens_the_innermost_context_menu_of_the_focus() {
    let view = VStack::new((
        Button::new("Inner", || {}).context_menu(vec![MenuChoice::action("Inner item", || {})]),
        Button::new("Outer area", || {}),
    ))
    .context_menu(vec![MenuChoice::action("Outer item", || {})]);
    let mut host = host(view);
    let inner = node(&mut host, "Inner");
    host.focus(Some(inner));
    press(&mut host, Key::ContextMenu);
    let menu = focused_menu(&mut host);
    assert_eq!(host.semantic(menu).unwrap().description, "Inner item");
    press(&mut host, Key::Escape);
    let outer = node(&mut host, "Outer area");
    host.focus(Some(outer));
    press_with(&mut host, Key::F(10), SHIFT);
    let menu = focused_menu(&mut host);
    assert_eq!(host.semantic(menu).unwrap().description, "Outer item");
    let panel = host.tree().bounds(menu).unwrap();
    let center = bounds(&mut host, "Outer area").center();
    assert!(
        (panel.min_x() - center.x).abs() < 0.5,
        "at the focused view"
    );
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn context_menu_follows_the_environment_disabled_state() {
    let mut host = host(Editor.disabled(true));
    let target = node(&mut host, "Target");
    let center = bounds(&mut host, "Target").center();
    pointer(
        &mut host,
        PointerEvent::Down {
            position: center,
            button: PointerButton::Secondary,
        },
    );
    assert_eq!(host.tree().focus(), None, "a disabled view opens no menu");
    assert!(
        !host
            .semantic(target)
            .unwrap()
            .actions
            .contains(&SemanticAction::ShowMenu)
    );
}

#[test]
fn nested_context_menus_open_the_innermost_one() {
    let view = VStack::new((
        Button::new("Inner", || {}).context_menu(vec![MenuChoice::action("Inner item", || {})]),
        Button::new("Outer area", || {}),
    ))
    .context_menu(vec![MenuChoice::action("Outer item", || {})]);
    let mut host = host(view);
    let inner = bounds(&mut host, "Inner").center();
    pointer(
        &mut host,
        PointerEvent::Down {
            position: inner,
            button: PointerButton::Secondary,
        },
    );
    let menu = focused_menu(&mut host);
    press(&mut host, Key::ArrowDown);
    assert_eq!(host.semantic(menu).unwrap().description, "Inner item");
    press(&mut host, Key::Escape);
    let outer = bounds(&mut host, "Outer area").center();
    pointer(
        &mut host,
        PointerEvent::Down {
            position: outer,
            button: PointerButton::Secondary,
        },
    );
    let menu = focused_menu(&mut host);
    press(&mut host, Key::ArrowDown);
    assert_eq!(host.semantic(menu).unwrap().description, "Outer item");
    assert_eq!(host.take_errors(), Vec::new());
}

// ---------------------------------------------------------------------------
// Dropdown menu

struct Options;

impl View for Options {
    fn body(&self, cx: &mut Cx) -> impl View {
        let open = cx.state(false);
        let renamed = cx.state(0);
        VStack::new((
            Button::new("Options", move || open.set(true)).dropdown_menu(
                open.binding(),
                vec![
                    MenuChoice::action("Rename", move || renamed.update(|n| *n += 1)),
                    MenuChoice::action("Archive", || {}).disabled(true),
                    MenuChoice::submenu("Move to", vec![MenuChoice::action("Inbox", || {})]),
                ],
            ),
            Text::new(move || format!("open {}", open.get())),
            Text::new(move || format!("renamed {}", renamed.get())),
        ))
    }
}

#[test]
fn dropdown_menu_follows_its_binding() {
    let mut host = host(Options);
    let options = node(&mut host, "Options");
    host.focus(Some(options));
    assert!(host.perform(options, SemanticAction::Activate, None));
    host.layout();
    assert!(shown(&mut host, "open true"));
    let menu = focused_menu(&mut host);
    let panel = host.tree().bounds(menu).unwrap();
    let button = bounds(&mut host, "Options");
    assert!(
        panel.min_y() >= button.max_y(),
        "the menu sits below the view"
    );
    // The keys skip the disabled action.
    press(&mut host, Key::ArrowDown);
    press(&mut host, Key::ArrowDown);
    let highlighted = host.semantic(menu).unwrap().description;
    assert_eq!(highlighted, "Move to");
    press(&mut host, Key::ArrowUp);
    press(&mut host, Key::Enter);
    assert!(shown(&mut host, "renamed 1"));
    assert!(
        shown(&mut host, "open false"),
        "a choice clears the binding"
    );
    assert_eq!(host.tree().focus(), Some(options));
    // Escape clears the binding as well.
    host.perform(options, SemanticAction::Activate, None);
    host.layout();
    focused_menu(&mut host);
    press(&mut host, Key::Escape);
    assert!(shown(&mut host, "open false"));
    assert_eq!(host.tree().focus(), Some(options));
    assert_eq!(host.take_errors(), Vec::new());
}

// ---------------------------------------------------------------------------
// Toast

/// Posts a toast through the environment's queue.
struct Poster;

impl View for Poster {
    fn body(&self, cx: &mut Cx) -> impl View {
        let toasts = cx.env().toasts().expect("a toaster is above");
        Button::new("Direct", move || {
            toasts.push(Toast::new("Direct toast"));
        })
    }
}

struct Saver;

impl View for Saver {
    fn body(&self, cx: &mut Cx) -> impl View {
        let posted = cx.state(false);
        VStack::new((
            Button::new("Post", move || posted.set(true))
                .toast(posted.binding(), |_env| Toast::new("Saved")),
            Poster,
        ))
        .toaster(ToastPosition::BottomEnd, "Notifications", "Close")
    }
}

/// The text of the front toast.
fn front_toast(host: &mut ViewHost) -> Option<String> {
    let stack = node(host, "Notifications");
    host.semantic(stack).unwrap().value
}

/// Advances the host time from `from` through each deadline up to
/// `until`.
fn run_until(host: &mut ViewHost, from: f64, until: f64) {
    let mut now = from;
    while let Some(due) = host.next_deadline().filter(|due| *due <= until) {
        now = due.max(now);
        host.advance(now);
    }
    host.advance(until);
    host.layout();
}

#[test]
fn toast_shows_and_closes_by_itself() {
    let mut host = host(Saver);
    host.advance(1.0);
    let post = node(&mut host, "Post");
    assert!(host.perform(post, SemanticAction::Activate, None));
    run_until(&mut host, 1.0, 1.0);
    assert_eq!(front_toast(&mut host).as_deref(), Some("Saved"));
    // The toast is still there just before its time ends, and goes after
    // its closing motion.
    run_until(&mut host, 1.0, 4.9);
    assert_eq!(front_toast(&mut host).as_deref(), Some("Saved"));
    run_until(&mut host, 4.9, 6.0);
    assert_eq!(front_toast(&mut host), None);
    // A descendant posts through the environment.
    let direct = node(&mut host, "Direct");
    assert!(host.perform(direct, SemanticAction::Activate, None));
    run_until(&mut host, 6.0, 6.0);
    assert_eq!(front_toast(&mut host).as_deref(), Some("Direct toast"));
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn toast_closes_at_its_time_with_reduced_motion() {
    let mut host = host(Saver);
    host.set_accessibility(AccessibilityPreferences {
        reduced_motion: true,
        ..AccessibilityPreferences::default()
    });
    host.layout();
    host.advance(1.0);
    let post = node(&mut host, "Post");
    host.perform(post, SemanticAction::Activate, None);
    run_until(&mut host, 1.0, 1.0);
    assert_eq!(front_toast(&mut host).as_deref(), Some("Saved"));
    let due = host.next_deadline().expect("the toast has a timer");
    assert!((due - 5.0).abs() < 1e-6, "due at {due}");
    host.advance(due);
    host.layout();
    assert_eq!(front_toast(&mut host), None, "no closing animation");
}

#[test]
fn toast_without_a_toaster_reports_an_error() {
    struct Lonely;
    impl View for Lonely {
        fn body(&self, cx: &mut Cx) -> impl View {
            let posted = cx.state(false);
            Button::new("Post", move || posted.set(true))
                .toast(posted.binding(), |_env| Toast::new("Saved"))
        }
    }
    let mut host = host(Lonely);
    let post = node(&mut host, "Post");
    host.perform(post, SemanticAction::Activate, None);
    assert_eq!(host.take_errors(), vec![ViewError::NoToaster]);
}

// ---------------------------------------------------------------------------
// Dialog, sheet, and drawer panels

/// The opacity at which the first text `text` paints in `list`, or `None`
/// when it does not paint.
fn text_opacity(list: &DisplayList, text: &str) -> Option<f64> {
    let mut stack = vec![1.0];
    for command in list.commands() {
        match command {
            DisplayCommand::PushOpacity(opacity) => {
                let current = stack.last().copied().unwrap_or(1.0);
                stack.push(current * opacity);
            }
            DisplayCommand::PopOpacity => {
                stack.pop();
            }
            DisplayCommand::DrawText { text: drawn, .. } if drawn == text => {
                return stack.last().copied();
            }
            _ => {}
        }
    }
    None
}

/// A button that opens a dialog with a close button.
struct Confirm;

impl View for Confirm {
    fn body(&self, cx: &mut Cx) -> impl View {
        let open = cx.state(false);
        VStack::new((
            Button::new("Open", move || open.set(true)),
            Text::new(move || format!("open {}", open.get())),
        ))
        .dialog(open.binding(), t!("confirm.title"), move || {
            VStack::new((
                Text::new("Body"),
                Button::new("Close", move || open.set(false)),
            ))
        })
    }
}

fn catalog() -> Catalog {
    let (en, vi) = (Locale::parse("en").unwrap(), Locale::parse("vi").unwrap());
    let mut catalog = Catalog::new();
    catalog.insert(&en, "confirm.title", "Delete file");
    catalog.insert(&vi, "confirm.title", "Xóa tệp");
    catalog
}

/// A host of `view` whose locale `locale` changes at run time.
fn live_host(view: impl View, locale: &Property<Locale>, runtime: Runtime) -> ViewHost {
    let live = locale.clone();
    let mut host = ViewHost::new(
        Size::new(600.0, 400.0),
        DesignTokens::light(),
        Env::new(Rc::new(catalog()), move || live.get()),
        runtime,
        Rc::new(EstimatedText),
    );
    host.mount(view);
    host.layout();
    assert_eq!(host.take_errors(), Vec::new());
    host
}

fn host_with_catalog(view: impl View) -> ViewHost {
    let runtime = Runtime::new();
    let locale = runtime.property(Locale::parse("en").unwrap());
    live_host(view, &locale, runtime)
}

fn activate(host: &mut ViewHost, name: &str) {
    let node = node(host, name);
    assert!(host.perform(node, SemanticAction::Activate, None));
    host.layout();
}

#[test]
fn dialog_is_named_by_its_title_and_follows_the_locale() {
    let runtime = Runtime::new();
    let locale = runtime.property(Locale::parse("en").unwrap());
    let mut host = live_host(Confirm, &locale, runtime);
    activate(&mut host, "Open");
    let panel = node(&mut host, "Delete file");
    assert_eq!(host.semantic(panel).unwrap().role, SemanticRole::Dialog);
    let body = node(&mut host, "Body");
    let (snapshot, _) = host.semantics();
    let entry = snapshot.iter().find(|entry| entry.id == body).unwrap();
    assert_eq!(entry.parent, Some(panel), "the dialog holds its content");
    locale.set(Locale::parse("vi").unwrap());
    host.flush();
    assert_eq!(host.find("Xóa tệp"), Some(panel), "the same node renames");
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn dialog_animates_in_and_out_with_the_host_time() {
    let mut host = host_with_catalog(Confirm);
    host.advance(1.0);
    let open = node(&mut host, "Open");
    host.focus(Some(open));
    activate(&mut host, "Open");
    assert_eq!(text_opacity(&host.paint(), "Body"), Some(0.0));
    host.advance(1.05);
    let half = text_opacity(&host.paint(), "Body").unwrap();
    assert!(half > 0.0 && half < 1.0, "half way: {half}");
    assert!(host.needs_frame(), "the motion asks for frames");
    host.advance(1.2);
    assert_eq!(text_opacity(&host.paint(), "Body"), Some(1.0));
    assert!(!host.needs_frame());
    // Closing hides the dialog from assistive technology and returns
    // focus at once, and the panel fades out before it unmounts.
    host.advance(2.0);
    let body = node(&mut host, "Body");
    activate(&mut host, "Close");
    assert!(shown(&mut host, "open false"));
    assert!(!shown(&mut host, "Body"));
    assert_eq!(host.tree().focus(), Some(open));
    assert!(host.tree().contains(body));
    assert_eq!(text_opacity(&host.paint(), "Body"), Some(1.0));
    host.advance(2.05);
    let half = text_opacity(&host.paint(), "Body").unwrap();
    assert!(half > 0.0 && half < 1.0, "half way out: {half}");
    let due = host.next_deadline().expect("the close has an end");
    assert!((due - 2.1).abs() < 1e-6, "due at {due}");
    host.advance(due);
    host.layout();
    assert!(!host.tree().contains(body), "the panel unmounted");
    assert_eq!(text_opacity(&host.paint(), "Body"), None);
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn a_closing_dialog_lets_presses_through() {
    let mut host = host_with_catalog(Confirm);
    host.advance(1.0);
    activate(&mut host, "Open");
    let open = bounds(&mut host, "Open").center();
    press(&mut host, Key::Escape);
    assert!(shown(&mut host, "open false"));
    for event in [
        PointerEvent::Down {
            position: open,
            button: PointerButton::Primary,
        },
        PointerEvent::Up {
            position: open,
            button: PointerButton::Primary,
        },
    ] {
        pointer(&mut host, event);
    }
    assert!(shown(&mut host, "open true"), "the press opened it again");
    assert!(shown(&mut host, "Body"));
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn a_dialog_takes_focus_closes_on_command_period_and_gives_focus_back() {
    let mut host = host_with_catalog(Confirm);
    let open = node(&mut host, "Open");
    host.focus(Some(open));
    activate(&mut host, "Open");
    let panel = node(&mut host, "Delete file");
    let focus = host.tree().focus().expect("the dialog takes focus");
    assert!(host.tree().is_within(focus, panel), "focus moves into it");
    let command = ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    };
    assert!(press_with(&mut host, Key::Character('.'), command));
    assert!(shown(&mut host, "open false"), "Command-period dismisses");
    assert_eq!(host.tree().focus(), Some(open), "focus goes back");
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn dialog_shows_and_closes_at_once_with_reduced_motion() {
    let mut host = host_with_catalog(Confirm);
    host.set_accessibility(AccessibilityPreferences {
        reduced_motion: true,
        ..AccessibilityPreferences::default()
    });
    host.advance(1.0);
    activate(&mut host, "Open");
    assert_eq!(text_opacity(&host.paint(), "Body"), Some(1.0));
    assert!(!host.needs_frame());
    let body = node(&mut host, "Body");
    activate(&mut host, "Close");
    assert!(!host.tree().contains(body), "no closing motion");
    assert_eq!(host.next_deadline(), None);
    assert_eq!(host.take_errors(), Vec::new());
}

/// A drawer at the leading edge.
struct Filters;

impl View for Filters {
    fn body(&self, cx: &mut Cx) -> impl View {
        let open = cx.state(true);
        VStack::new((
            Button::new("Show filters", move || open.set(true)),
            Text::new(move || format!("open {}", open.get())),
        ))
        .drawer(open.binding(), SheetSide::Left, "Filters", || {
            Text::new("Only unread messages")
        })
    }
}

fn drag(host: &mut ViewHost, from: Point, to: Point) {
    for event in [
        PointerEvent::Down {
            position: from,
            button: PointerButton::Primary,
        },
        PointerEvent::Move { position: to },
        PointerEvent::Up {
            position: to,
            button: PointerButton::Primary,
        },
    ] {
        pointer(host, event);
    }
}

#[test]
fn drawer_sits_at_its_side_and_a_drag_dismisses_it() {
    let mut host = host(Filters);
    host.advance(1.0);
    let panel = bounds(&mut host, "Filters");
    assert_eq!(panel.min_x(), 0.0, "the drawer sits at the leading edge");
    assert_eq!(panel.size.height, 400.0);
    let start = bounds(&mut host, "Only unread messages").center();
    // A short drag springs back.
    let short = Point::new(start.x - panel.size.width * 0.1, start.y);
    drag(&mut host, start, short);
    assert!(shown(&mut host, "open true"));
    // The panel follows the drag toward its edge.
    pointer(
        &mut host,
        PointerEvent::Down {
            position: start,
            button: PointerButton::Primary,
        },
    );
    let far = Point::new(start.x - panel.size.width * 0.5, start.y);
    pointer(&mut host, PointerEvent::Move { position: far });
    let moved = host.paint().commands().iter().any(|command| {
        matches!(command, DisplayCommand::PushTransform(transform)
            if (transform.tx + panel.size.width * 0.5).abs() < 1e-6)
    });
    assert!(moved, "the panel moves with the pointer");
    pointer(
        &mut host,
        PointerEvent::Up {
            position: far,
            button: PointerButton::Primary,
        },
    );
    assert!(shown(&mut host, "open false"), "a long drag dismisses");
    assert!(!shown(&mut host, "Filters"));
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn drawer_sides_mirror_in_right_to_left_layouts() {
    let mut host = host_in(Filters, "ar");
    let panel = bounds(&mut host, "Filters");
    assert_eq!(panel.max_x(), 600.0, "the leading edge is at the right");
    let start = bounds(&mut host, "Only unread messages").center();
    drag(
        &mut host,
        start,
        Point::new(start.x + panel.size.width * 0.5, start.y),
    );
    assert!(shown(&mut host, "open false"));
}

// ---------------------------------------------------------------------------
// Tooltip semantics and motion

/// Hovers the view named `name` from host time `from` until its tooltip
/// shows.
fn hover_until_shown(host: &mut ViewHost, name: &str, from: f64) {
    let center = bounds(host, name).center();
    host.advance(from);
    pointer(host, PointerEvent::Move { position: center });
    host.advance(from + 0.6);
    host.layout();
}

fn leave(host: &mut ViewHost) {
    pointer(
        host,
        PointerEvent::Move {
            position: Point::new(5.0, 395.0),
        },
    );
}

/// The published tooltip nodes.
fn tooltip_entries(host: &mut ViewHost) -> Vec<choscorui_core::SemanticEntry> {
    let (snapshot, _) = host.semantics();
    snapshot
        .into_iter()
        .filter(|entry| entry.node.role == SemanticRole::Tooltip)
        .collect()
}

#[test]
fn tooltip_publishes_its_node_while_it_shows() {
    let mut host = host(toolbar());
    let save = node(&mut host, "Save");
    assert!(tooltip_entries(&mut host).is_empty());
    hover_until_shown(&mut host, "Save", 10.0);
    let tips = tooltip_entries(&mut host);
    assert_eq!(tips.len(), 1, "{tips:?}");
    assert_eq!(tips[0].node.name, "Save the file");
    assert_eq!(
        tips[0].node.relations,
        vec![SemanticRelation::DescribedBy(save)]
    );
    leave(&mut host);
    assert!(tooltip_entries(&mut host).is_empty());
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn tooltip_fades_in_and_out_with_the_host_time() {
    let mut host = host(toolbar());
    hover_until_shown(&mut host, "Save", 10.0);
    assert_eq!(text_opacity(&host.paint(), "Save the file"), None);
    host.advance(10.65);
    let half = text_opacity(&host.paint(), "Save the file").unwrap();
    assert!(half > 0.0 && half < 1.0, "half way: {half}");
    host.advance(10.8);
    assert_eq!(text_opacity(&host.paint(), "Save the file"), Some(1.0));
    host.advance(11.0);
    leave(&mut host);
    host.advance(11.05);
    let half = text_opacity(&host.paint(), "Save the file").unwrap();
    assert!(half > 0.0 && half < 1.0, "half way out: {half}");
    assert!(host.needs_frame());
    host.advance(11.2);
    assert_eq!(text_opacity(&host.paint(), "Save the file"), None);
}

#[test]
fn tooltip_shows_and_hides_at_once_with_reduced_motion() {
    let mut host = host(toolbar());
    host.set_accessibility(AccessibilityPreferences {
        reduced_motion: true,
        ..AccessibilityPreferences::default()
    });
    hover_until_shown(&mut host, "Save", 10.0);
    assert_eq!(text_opacity(&host.paint(), "Save the file"), Some(1.0));
    leave(&mut host);
    assert_eq!(text_opacity(&host.paint(), "Save the file"), None);
}

// ---------------------------------------------------------------------------
// Checkbox and radio menu rows

struct ViewMenu;

impl View for ViewMenu {
    fn body(&self, cx: &mut Cx) -> impl View {
        let open = cx.state(false);
        let bookmarks = cx.state(false);
        let zoom = cx.state(0_u8);
        VStack::new((
            Button::new("View", move || open.set(true)).dropdown_menu(
                open.binding(),
                vec![
                    MenuChoice::checkbox("Show bookmarks", bookmarks.binding()),
                    MenuChoice::separator(),
                    MenuChoice::radio("Small", zoom.binding(), 1),
                    MenuChoice::radio("Large", zoom.binding(), 2),
                ],
            ),
            Text::new(move || format!("open {}", open.get())),
            Text::new(move || format!("bookmarks {}", bookmarks.get())),
            Text::new(move || format!("zoom {}", zoom.get())),
        ))
    }
}

/// Opens the menu of the view named `name` at host time `time`.
fn open_menu(host: &mut ViewHost, name: &str, time: f64) -> NodeId {
    host.advance(time);
    activate(host, name);
    host.advance(time + 1.0);
    focused_menu(host)
}

/// The vertical centers of the radio dots: 8 pt round fills.
fn dots(list: &DisplayList) -> Vec<f64> {
    list.commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::FillRoundedRect { rect, .. } if rect.size.width == 8.0 => {
                Some(rect.center().y)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn checkbox_and_radio_rows_set_their_bindings() {
    let mut host = host(ViewMenu);
    let view = node(&mut host, "View");
    host.focus(Some(view));
    open_menu(&mut host, "View", 1.0);
    assert!(dots(&host.paint()).is_empty(), "no radio row is chosen");
    press(&mut host, Key::ArrowDown);
    press(&mut host, Key::Enter);
    assert!(shown(&mut host, "bookmarks true"));
    assert!(
        shown(&mut host, "open false"),
        "a check row closes the menu"
    );
    assert_eq!(host.tree().focus(), Some(view));
    // The checkbox flips back.
    open_menu(&mut host, "View", 3.0);
    press(&mut host, Key::ArrowDown);
    press(&mut host, Key::Enter);
    assert!(shown(&mut host, "bookmarks false"));
    // A radio row sets the selection, and its dot shows.
    open_menu(&mut host, "View", 5.0);
    press(&mut host, Key::End);
    press(&mut host, Key::Enter);
    assert!(shown(&mut host, "zoom 2"));
    open_menu(&mut host, "View", 7.0);
    let large = dots(&host.paint());
    assert_eq!(large.len(), 1);
    press(&mut host, Key::End);
    press(&mut host, Key::ArrowUp);
    press(&mut host, Key::Enter);
    assert!(shown(&mut host, "zoom 1"));
    open_menu(&mut host, "View", 9.0);
    let small = dots(&host.paint());
    assert_eq!(small.len(), 1, "one radio row of the group is chosen");
    assert!(small[0] < large[0], "the dot moved one row up");
    assert_eq!(host.take_errors(), Vec::new());
}

// ---------------------------------------------------------------------------
// Hover card

const CARD_TEXT: &str = "The React Framework";

/// A link with a hover card that holds text and a button.
fn profile() -> impl View {
    VStack::new((
        Button::new("@nextjs", || {})
            .hover_card(|| VStack::new((Text::new(CARD_TEXT), Button::new("Follow", || {})))),
        Button::new("Other", || {}),
    ))
}

/// Rests the pointer on the view named `name` from host time `from`
/// until its hover card opens.
fn rest_until_open(host: &mut ViewHost, name: &str, from: f64) {
    let center = bounds(host, name).center();
    host.advance(from);
    pointer(host, PointerEvent::Move { position: center });
    host.advance(from + 0.7);
    host.layout();
}

#[test]
fn hover_card_opens_after_the_hover_delay_and_closes_after_the_pointer_leaves() {
    let mut host = host(profile());
    let center = bounds(&mut host, "@nextjs").center();
    host.advance(10.0);
    pointer(&mut host, PointerEvent::Move { position: center });
    assert!(!shown(&mut host, CARD_TEXT));
    let due = host.next_deadline().expect("the card waits for its delay");
    assert!((due - 10.7).abs() < 1e-6, "due at {due}");
    host.advance(10.4);
    assert!(!shown(&mut host, CARD_TEXT));
    host.advance(10.7);
    host.layout();
    assert!(shown(&mut host, CARD_TEXT));
    assert_eq!(host.tree().focus(), None, "the card takes no focus");
    let anchor = bounds(&mut host, "@nextjs");
    let card = bounds(&mut host, CARD_TEXT);
    assert!(card.min_y() >= anchor.max_y(), "below its view: {card:?}");
    // The pointer moves onto the card, which keeps it open.
    pointer(
        &mut host,
        PointerEvent::Move {
            position: card.center(),
        },
    );
    host.advance(12.0);
    assert!(shown(&mut host, CARD_TEXT));
    assert_eq!(host.next_deadline(), None);
    // The pointer leaves the view and the card.
    leave(&mut host);
    assert!(
        shown(&mut host, CARD_TEXT),
        "the card waits before it closes"
    );
    let due = host.next_deadline().expect("the card waits to close");
    assert!((due - 12.3).abs() < 1e-6, "due at {due}");
    host.advance(due);
    host.layout();
    assert!(!shown(&mut host, CARD_TEXT));
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn hover_card_closes_on_a_press_outside() {
    let mut host = host(profile());
    rest_until_open(&mut host, "@nextjs", 10.0);
    assert!(shown(&mut host, CARD_TEXT));
    pointer(
        &mut host,
        PointerEvent::Down {
            position: Point::new(590.0, 395.0),
            button: PointerButton::Primary,
        },
    );
    assert!(!shown(&mut host, CARD_TEXT));
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn hover_card_opens_on_keyboard_focus_and_closes_on_escape_and_blur() {
    let mut host = host(profile());
    let (link, other) = (node(&mut host, "@nextjs"), node(&mut host, "Other"));
    host.advance(20.0);
    assert_eq!(host.focus_next(false), Some(link));
    assert!(!shown(&mut host, CARD_TEXT));
    host.advance(20.7);
    host.layout();
    assert!(shown(&mut host, CARD_TEXT));
    // Tab skips the card's content, and leaving the view closes the card
    // at once.
    assert_eq!(host.focus_next(false), Some(other));
    host.layout();
    assert!(!shown(&mut host, CARD_TEXT));
    // Back in the view, it opens again; Escape closes it and stops.
    host.advance(30.0);
    assert_eq!(host.focus_next(true), Some(link));
    host.advance(30.7);
    host.layout();
    assert!(shown(&mut host, CARD_TEXT));
    assert!(press(&mut host, Key::Escape), "Escape closes the card");
    assert!(!shown(&mut host, CARD_TEXT));
    assert_eq!(host.tree().focus(), Some(link), "focus stays on the view");
    host.advance(32.0);
    host.layout();
    assert!(!shown(&mut host, CARD_TEXT), "it does not open again");
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn escape_closes_a_hovered_card_while_focus_is_elsewhere() {
    let mut host = host(profile());
    let other = node(&mut host, "Other");
    host.focus(Some(other));
    rest_until_open(&mut host, "@nextjs", 10.0);
    assert!(shown(&mut host, CARD_TEXT));
    assert!(press(&mut host, Key::Escape), "Escape closes the card");
    assert!(!shown(&mut host, CARD_TEXT));
    assert_eq!(host.tree().focus(), Some(other), "focus stays");
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn hover_card_fades_in_and_shows_at_once_with_reduced_motion() {
    let mut host = host(profile());
    rest_until_open(&mut host, "@nextjs", 10.0);
    assert_eq!(text_opacity(&host.paint(), CARD_TEXT), Some(0.0));
    assert!(host.needs_frame(), "the card fades in");

    let mut host = host_in(profile(), "en");
    host.set_accessibility(AccessibilityPreferences {
        reduced_motion: true,
        ..AccessibilityPreferences::default()
    });
    rest_until_open(&mut host, "@nextjs", 10.0);
    assert_eq!(text_opacity(&host.paint(), CARD_TEXT), Some(1.0));
    let text = node(&mut host, CARD_TEXT);
    leave(&mut host);
    host.advance(11.0);
    host.layout();
    assert!(!host.tree().contains(text), "no closing motion");
    assert_eq!(host.next_deadline(), None);
    assert_eq!(host.take_errors(), Vec::new());
}
