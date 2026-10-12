//! Headless tests of the views over control models: the focus frame, the
//! input dialog, the font dialog, the sidebar shortcut, and the invalid
//! state of form views.

use choscorui_compose::Runtime;
use choscorui_compose::view::{
    Cx, Env, FocusFrame, FontDialog, Frame, HStack, InputDialog, Sidebar, Switch, Text, VStack,
    View, ViewExt, ViewHost,
};
use choscorui_controls::{EstimatedText, InputKind, InputValue, ShortcutModifiers, SidebarItem};
use choscorui_core::{
    Catalog, DesignTokens, DisplayCommand, DisplayList, Icon, Key, KeyEvent, Locale, Point,
    PointerButton, PointerEvent, Rect, SemanticAction, Size, TextStyle,
};
use std::cell::RefCell;
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

fn node(host: &mut ViewHost, name: &str) -> choscorui_core::NodeId {
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

fn perform(host: &mut ViewHost, name: &str, action: SemanticAction, value: Option<&str>) {
    let node = node(host, name);
    assert!(host.perform(node, action, value), "{name}: {action:?}");
    host.layout();
}

fn disabled(host: &mut ViewHost, name: &str) -> bool {
    let node = node(host, name);
    host.semantic(node).unwrap().state.disabled
}

fn value(host: &mut ViewHost, name: &str) -> Option<String> {
    let node = node(host, name);
    host.semantic(node).unwrap().value
}

// ---------------------------------------------------------------------------
// Focus frame

/// Two buttons that fill a clipping panel, in a focus frame.
fn framed() -> impl View {
    let button = |name: &'static str| {
        choscorui_compose::view::Button::new(name, || {}).frame(Frame::size(80.0, 30.0))
    };
    VStack::new(FocusFrame::new(
        HStack::new((button("First"), button("Second")))
            .spacing(0.0)
            .frame(Frame::size(160.0, 30.0))
            .clip_shape(),
    ))
    .padding(20.0)
}

/// The clip depth of each focus ring around `rect` in `list`.
fn rings_around(list: &DisplayList, rect: Rect, width: f64) -> Vec<usize> {
    let ring = rect.inset(-width / 2.0);
    let mut depth = 0usize;
    let mut found = Vec::new();
    for command in list.commands() {
        match command {
            DisplayCommand::PushClip(_) | DisplayCommand::PushClipPath(_) => depth += 1,
            DisplayCommand::PopClip => depth -= 1,
            DisplayCommand::StrokeRoundedRect {
                rect, width: line, ..
            } if *rect == ring && *line == width => found.push(depth),
            _ => {}
        }
    }
    found
}

#[test]
fn the_focus_frame_draws_the_ring_outside_the_clip() {
    let width = DesignTokens::light().ring_width;
    let mut host = host(framed());
    let first = node(&mut host, "First");
    host.focus(Some(first));
    host.layout();
    let rect = bounds(&mut host, "First");
    let list = host.paint();
    let depths = rings_around(&list, rect, width);
    // The button draws its own ring inside the clip; the frame draws one
    // outside it.
    assert!(depths.contains(&0), "rings at clip depths {depths:?}");
    assert!(press(&mut host, Key::Tab));
    let second = bounds(&mut host, "Second");
    let list = host.paint();
    assert!(rings_around(&list, second, width).contains(&0));
    assert!(!rings_around(&list, rect, width).contains(&0));
}

#[test]
fn the_focus_frame_shows_only_keyboard_focus() {
    let width = DesignTokens::light().ring_width;
    let mut host = host(framed());
    let rect = bounds(&mut host, "Second");
    host.pointer(
        PointerEvent::Down {
            position: Point::new(rect.min_x() + 10.0, rect.min_y() + 10.0),
            button: PointerButton::Primary,
        },
        ShortcutModifiers::default(),
    );
    host.layout();
    let second = node(&mut host, "Second");
    assert_eq!(host.tree().focus(), Some(second));
    let list = host.paint();
    assert!(rings_around(&list, rect, width).is_empty());
}

#[test]
fn the_focus_frame_follows_the_mirrored_layout() {
    let width = DesignTokens::light().ring_width;
    let mut host = host_in(framed(), "ar");
    let first = node(&mut host, "First");
    host.focus(Some(first));
    host.layout();
    let rect = bounds(&mut host, "First");
    assert!(rect.min_x() > bounds(&mut host, "Second").min_x());
    let list = host.paint();
    assert!(rings_around(&list, rect, width).contains(&0));
}

// ---------------------------------------------------------------------------
// Input dialog

/// A "Go to line" dialog that records the values it accepts.
struct GoToLine {
    accepted: Rc<RefCell<Vec<InputValue>>>,
}

impl View for GoToLine {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let text = cx.state("42".to_owned());
        let accepted = self.accepted.clone();
        InputDialog::new(
            "Go to line",
            "Line",
            "OK",
            InputKind::Integer {
                minimum: 1,
                maximum: 500,
            },
            text.binding(),
            move |value| accepted.borrow_mut().push(value),
        )
    }
}

#[test]
fn the_input_dialog_accepts_only_valid_values() {
    let accepted = Rc::new(RefCell::new(Vec::new()));
    let mut host = host(GoToLine {
        accepted: accepted.clone(),
    });
    assert!(host.find("Go to line").is_some(), "the group has the title");
    assert!(!disabled(&mut host, "OK"));
    perform(&mut host, "Line", SemanticAction::SetValue, Some("501"));
    assert!(disabled(&mut host, "OK"), "out of range");
    assert!(host.find("Enter a number from 1 to 500").is_some());
    let field = node(&mut host, "Line");
    host.focus(Some(field));
    press(&mut host, Key::Enter);
    assert!(
        accepted.borrow().is_empty(),
        "Return keeps an invalid value"
    );

    perform(&mut host, "Line", SemanticAction::SetValue, Some("forty"));
    assert!(disabled(&mut host, "OK"));
    perform(&mut host, "Line", SemanticAction::SetValue, Some("7"));
    assert!(!disabled(&mut host, "OK"));
    assert!(host.find("Enter a number from 1 to 500").is_none());
    press(&mut host, Key::Enter);
    assert_eq!(*accepted.borrow(), vec![InputValue::Integer(7)]);
    perform(&mut host, "OK", SemanticAction::Activate, None);
    assert_eq!(accepted.borrow().len(), 2);
    assert_eq!(host.take_errors(), Vec::new());
}

// ---------------------------------------------------------------------------
// Font dialog

/// A font dialog over three families, with a readout of the style.
struct Fonts;

impl View for Fonts {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let style = cx.state(TextStyle::new("Georgia", 22.0));
        let families = ["Helvetica", "Georgia", "Menlo"].map(String::from).to_vec();
        VStack::new((
            FontDialog::new(
                "Sample",
                "Size",
                "Bold",
                "Italic",
                families,
                style.binding(),
            ),
            Text::new(move || {
                style
                    .with(|style| format!("{} {} {}", style.family, style.size, style.weight.0))
                    .unwrap_or_default()
            }),
            choscorui_compose::view::Button::new("Use Menlo", move || {
                style.update(|style| style.family = "Menlo".into())
            }),
        ))
    }
}

#[test]
fn the_font_dialog_edits_the_style() {
    let mut host = host(Fonts);
    assert!(host.find("Georgia 22 400").is_some());
    assert_eq!(value(&mut host, "Sample").as_deref(), Some("Georgia"));
    perform(&mut host, "Size", SemanticAction::Increment, None);
    assert!(host.find("Georgia 23 400").is_some());
    perform(&mut host, "Bold", SemanticAction::Activate, None);
    assert!(host.find("Georgia 23 700").is_some());
    let bold = node(&mut host, "Bold");
    assert_eq!(host.semantic(bold).unwrap().state.checked, Some(true));
    perform(&mut host, "Font", SemanticAction::SetValue, Some("Menlo"));
    assert!(host.find("Menlo 23 700").is_some());
    assert_eq!(value(&mut host, "Sample").as_deref(), Some("Menlo"));
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn a_missing_family_falls_back_and_stays_in_the_field() {
    let mut host = host(Fonts);
    perform(
        &mut host,
        "Font",
        SemanticAction::SetValue,
        Some("Comic Neue"),
    );
    assert!(host.find("Helvetica 22 400").is_some(), "the fallback");
    assert_eq!(value(&mut host, "Sample").as_deref(), Some("Helvetica"));
    let font = node(&mut host, "Font");
    let chooser = host.semantic(font).unwrap();
    assert!(chooser.state.invalid, "the family is missing");
    assert_eq!(chooser.value.as_deref(), Some("Comic Neue"));
    // A change of the binding shows in the field.
    perform(&mut host, "Use Menlo", SemanticAction::Activate, None);
    let chooser = host.semantic(font).unwrap();
    assert!(!chooser.state.invalid);
    assert_eq!(chooser.value.as_deref(), Some("Menlo"));
    assert_eq!(value(&mut host, "Sample").as_deref(), Some("Menlo"));
    assert_eq!(host.take_errors(), Vec::new());
}

// ---------------------------------------------------------------------------
// Sidebar shortcut

/// A sidebar beside a button, with the sidebar's open state shown as text.
struct Shell;

impl View for Shell {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let open = cx.state(true);
        let selected = cx.state(None);
        HStack::new((
            Sidebar::new(
                "Navigation",
                "Toggle sidebar",
                vec![(
                    "Application".to_string(),
                    vec![SidebarItem::new("Inbox", Icon::Inbox)],
                )],
                selected.binding(),
            )
            .open(open.binding()),
            choscorui_compose::view::Button::new("Save", || {}),
            Text::new(move || format!("open {}", open.get())),
        ))
    }
}

#[test]
fn command_b_toggles_the_sidebar_view_while_another_view_has_focus() {
    let mut host = ViewHost::new(
        Size::new(1000.0, 400.0),
        DesignTokens::light(),
        Env::new(Rc::new(Catalog::new()), Locale::parse("en").unwrap()),
        Runtime::new(),
        Rc::new(EstimatedText),
    );
    host.mount(Shell);
    host.layout();
    let save = node(&mut host, "Save");
    host.focus(Some(save));
    let command = ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    };
    let key = KeyEvent::Down {
        key: Key::Character('b'),
        repeat: false,
    };
    assert!(host.key(key, command));
    host.layout();
    assert!(host.find("open false").is_some());
    assert_eq!(host.tree().focus(), Some(save), "focus stays on the button");
    assert!(host.key(key, command));
    host.layout();
    assert!(host.find("open true").is_some());
    assert_eq!(host.take_errors(), Vec::new());
}

// ---------------------------------------------------------------------------
// Invalid state

/// A switch whose invalid state follows a state value, and a button that
/// clears it.
struct Consent;

impl View for Consent {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let on = cx.state(true);
        let invalid = cx.state(true);
        VStack::new((
            Switch::new("Consent", on.binding())
                .with(move |switch| switch.with_invalid(invalid.get())),
            choscorui_compose::view::Button::new("Fix", move || invalid.set(false)),
        ))
    }
}

#[test]
fn a_form_view_shows_the_invalid_state_and_follows_its_change() {
    let mut host = host(Consent);
    let switch = node(&mut host, "Consent");
    let semantic = host.semantic(switch).unwrap();
    assert!(semantic.state.invalid);
    assert_eq!(semantic.state.checked, Some(true));
    perform(&mut host, "Fix", SemanticAction::Activate, None);
    let semantic = host.semantic(switch).unwrap();
    assert!(!semantic.state.invalid);
    assert_eq!(semantic.state.checked, Some(true), "the value stays");
    assert_eq!(host.take_errors(), Vec::new());
}
