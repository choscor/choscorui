//! Headless tests of the form view and the presentation modifiers: the
//! form layout in both directions, tooltips, context and dropdown menus,
//! and toasts.

use choscorui_compose::Runtime;
use choscorui_compose::view::{
    Button, Cx, Env, Form, FormRow, MenuChoice, State, Text, VStack, View, ViewError, ViewExt,
    ViewHost,
};
use choscorui_controls::{EstimatedText, ShortcutModifiers, Toast, ToastPosition};
use choscorui_core::{
    AccessibilityPreferences, Catalog, DesignTokens, Key, KeyEvent, Locale, NodeId, Point,
    PointerButton, PointerEvent, Rect, SemanticAction, SemanticRole, Size,
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

// ---------------------------------------------------------------------------
// Context menu

struct Editor;

impl View for Editor {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
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
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
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
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let toasts = cx.env().toasts().expect("a toaster is above");
        Button::new("Direct", move || {
            toasts.push(Toast::new("Direct toast"));
        })
    }
}

struct Saver;

impl View for Saver {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
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
        fn body(&self, cx: &mut Cx<'_>) -> impl View {
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
