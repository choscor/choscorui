//! Headless tests of the views over control models: the focus frame, the
//! input dialog, the font dialog, the button group, the sidebar shortcut,
//! and the invalid state of form views.

use choscorui_compose::Runtime;
use choscorui_compose::view::{
    ButtonGroup, Checkbox, Cx, Env, FocusFrame, FontDialog, Frame, HStack, InputDialog, Sidebar,
    Text, VStack, View, ViewExt, ViewHost,
};
use choscorui_controls::{
    ButtonGroupMember, ButtonVariant, CommandId, EstimatedText, InputKind, InputValue,
    ShortcutModifiers, SidebarItem, TextMeasure,
};
use choscorui_core::{
    Catalog, DesignTokens, DisplayCommand, DisplayList, FontSlant, FontWeight, Icon, Key, KeyEvent,
    Locale, Point, PointerButton, PointerEvent, Rect, SemanticAction, Size, TextRun, TextStyle,
    TypeSpec,
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
    // The frame draws the ring outside the clip, and the button draws none.
    assert_eq!(depths, vec![0], "rings at clip depths");
    assert!(press(&mut host, Key::Tab));
    let second = bounds(&mut host, "Second");
    let list = host.paint();
    assert!(rings_around(&list, second, width).contains(&0));
    assert!(!rings_around(&list, rect, width).contains(&0));
}

#[test]
fn the_focus_frame_moves_the_ring_with_the_focused_node() {
    let width = DesignTokens::light().ring_width;
    let moved = choscorui_compose::view::Button::new("Moved", || {})
        .frame(Frame::size(80.0, 30.0))
        .offset(Point::new(10.0, 5.0));
    let mut host = host(FocusFrame::new(VStack::new(moved).clip_shape()));
    let node = node(&mut host, "Moved");
    host.focus(Some(node));
    host.layout();
    let rect = bounds(&mut host, "Moved");
    let list = host.paint();
    let commands = list.commands();
    let ring = rect.inset(-width / 2.0);
    let at = commands
        .iter()
        .rposition(|command| {
            matches!(command, DisplayCommand::StrokeRoundedRect { rect, .. } if *rect == ring)
        })
        .expect("a ring");
    let offset = choscorui_core::Transform::translate(10.0, 5.0);
    assert_eq!(commands[at - 1], DisplayCommand::PushTransform(offset));
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

/// An "Open" button for an input dialog of `kind`, which records the
/// values it accepts, and a readout of its text.
struct Asking {
    kind: InputKind,
    text: &'static str,
    accepted: Rc<RefCell<Vec<InputValue>>>,
}

impl View for Asking {
    fn body(&self, cx: &mut Cx) -> impl View {
        let text = cx.state(self.text.to_owned());
        let open = cx.state(false);
        let accepted = self.accepted.clone();
        VStack::new((
            choscorui_compose::view::Button::new("Open", move || open.set(true)),
            InputDialog::new(
                ("Ask", "Value", "OK"),
                self.kind.clone(),
                text.binding(),
                open.binding(),
                move |value| accepted.borrow_mut().push(value),
            ),
            Text::new(move || format!("[{}]", text.get())),
        ))
    }
}

/// A host with an `Asking` view for `kind` and `text`, and its log.
fn asking(kind: InputKind, text: &'static str) -> (ViewHost, Rc<RefCell<Vec<InputValue>>>) {
    let accepted = Rc::new(RefCell::new(Vec::new()));
    let view = Asking {
        kind,
        text,
        accepted: accepted.clone(),
    };
    (host(view), accepted)
}

/// Focuses "Open" and activates it, so that the dialog opens.
fn open_dialog(host: &mut ViewHost) -> choscorui_core::NodeId {
    let open = node(host, "Open");
    host.focus(Some(open));
    perform(host, "Open", SemanticAction::Activate, None);
    open
}

#[test]
fn the_input_dialog_is_modal_and_accepts_only_valid_values() {
    let lines = InputKind::Integer {
        minimum: 1,
        maximum: 500,
    };
    let (mut host, accepted) = asking(lines, "42");
    assert!(host.find("Value").is_none(), "closed until presented");
    let open = open_dialog(&mut host);
    let field = node(&mut host, "Value");
    assert_eq!(host.tree().focus(), Some(field), "focus moves inside");
    perform(&mut host, "Value", SemanticAction::SetValue, Some("501"));
    assert!(disabled(&mut host, "OK"), "out of range");
    let error = host.semantic(field).unwrap().description;
    assert!(error.contains("Must be at most 500"), "{error}");
    assert!(
        host.find("Must be at most 500").is_none(),
        "the visible message is not read a second time"
    );
    press(&mut host, Key::Enter);
    assert!(
        accepted.borrow().is_empty(),
        "Return keeps an invalid value"
    );

    perform(&mut host, "Value", SemanticAction::SetValue, Some("7"));
    assert!(!disabled(&mut host, "OK"));
    press(&mut host, Key::Enter);
    assert_eq!(*accepted.borrow(), vec![InputValue::Integer(7)]);
    assert!(host.find("Value").is_none(), "accept closes the dialog");
    assert_eq!(host.tree().focus(), Some(open), "focus returns");

    open_dialog(&mut host);
    press(&mut host, Key::Escape);
    assert!(host.find("Value").is_none(), "Escape cancels");
    assert_eq!(host.tree().focus(), Some(open));
    open_dialog(&mut host);
    perform(&mut host, "Cancel", SemanticAction::Activate, None);
    assert!(host.find("Value").is_none(), "Cancel closes");
    assert_eq!(accepted.borrow().len(), 1);
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn the_input_dialog_trims_text_and_chooses_with_a_select() {
    let (mut host, accepted) = asking(InputKind::Text { allow_empty: false }, "  Notes ");
    open_dialog(&mut host);
    perform(&mut host, "OK", SemanticAction::Activate, None);
    assert_eq!(*accepted.borrow(), vec![InputValue::Text("Notes".into())]);
    assert!(
        host.find("[Notes]").is_some(),
        "the text is the trimmed value"
    );

    let sizes = InputKind::Choice(vec!["Small".into(), "Medium".into(), "Large".into()]);
    let (mut host, accepted) = asking(sizes, "Medium");
    open_dialog(&mut host);
    let select = node(&mut host, "Value");
    let node = host.semantic(select).unwrap();
    assert_eq!(node.role, choscorui_core::SemanticRole::ComboBox);
    assert_eq!(node.value.as_deref(), Some("Medium"));
    perform(&mut host, "Value", SemanticAction::SetValue, Some("Large"));
    perform(&mut host, "OK", SemanticAction::Activate, None);
    assert_eq!(*accepted.borrow(), vec![InputValue::Choice(2)]);
    assert!(host.find("[Large]").is_some());
    assert_eq!(host.take_errors(), Vec::new());
}

// ---------------------------------------------------------------------------
// Font dialog

/// A font dialog over three families, with a readout of the style.
struct Fonts;

impl View for Fonts {
    fn body(&self, cx: &mut Cx) -> impl View {
        let style = cx.state(TextStyle::new("Georgia", 22.0));
        let families = ["Helvetica", "Georgia", "Menlo"].map(String::from).to_vec();
        VStack::new((
            FontDialog::new(
                "Sample",
                ("Font", "Size", "Bold", "Italic"),
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

/// Measures Menlo twice as wide as the estimate, so that a test can see
/// which family measured the text.
struct WideMenlo;

impl TextMeasure for WideMenlo {
    fn width(&self, text: &str, spec: &TypeSpec) -> f64 {
        EstimatedText.width(text, spec)
    }

    fn run_width(&self, text: &str, run: &TextRun<'_>) -> f64 {
        let scale = if run.family == "Menlo" { 2.0 } else { 1.0 };
        scale * EstimatedText.run_width(text, run)
    }
}

/// A narrow medium-weight font dialog with a long sample and a readout.
struct NarrowFonts;

impl View for NarrowFonts {
    fn body(&self, cx: &mut Cx) -> impl View {
        let style = cx.state(TextStyle::new("Georgia", 16.0).with_weight(FontWeight::MEDIUM));
        let families = ["Georgia", "Menlo"].map(String::from).to_vec();
        let labels = ("Family", "Size", "Bold", "Italic");
        let sample = "Sphinx of black quartz, judge my vow";
        VStack::new((
            FontDialog::new(sample, labels, families, style.binding())
                .frame(Frame::new().width(260.0)),
            Text::new(move || {
                style
                    .with(|style| format!("{} {} {:?}", style.size, style.weight.0, style.slant))
                    .unwrap_or_default()
            }),
            choscorui_compose::view::Button::new("Use Menlo", move || {
                style.update(|style| style.family = "Menlo".into())
            }),
        ))
    }
}

/// The sample lines that the preview draws, with their slant.
fn sample_lines(host: &mut ViewHost) -> Vec<(String, FontSlant)> {
    host.paint()
        .commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::DrawText {
                text,
                font_family,
                font_slant,
                ..
            } if font_family == "Georgia" || font_family == "Menlo" => {
                Some((text.clone(), *font_slant))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn the_font_preview_draws_italic_and_wraps_in_the_chosen_family() {
    let mut host = ViewHost::new(
        Size::new(600.0, 400.0),
        DesignTokens::light(),
        Env::new(Rc::new(Catalog::new()), Locale::parse("en").unwrap()),
        Runtime::new(),
        Rc::new(WideMenlo),
    );
    host.mount(NarrowFonts);
    host.layout();
    let lines = sample_lines(&mut host);
    assert!(lines.len() > 1, "the sample wraps: {lines:?}");
    assert!(lines.iter().all(|(_, slant)| *slant == FontSlant::Upright));
    perform(&mut host, "Italic", SemanticAction::Activate, None);
    assert!(host.find("16 500 Italic").is_some());
    let lines = sample_lines(&mut host);
    assert!(lines.iter().all(|(_, slant)| *slant == FontSlant::Italic));
    // Menlo measures wider here, so its preview needs more lines.
    let preview = node(&mut host, "Sphinx of black quartz, judge my vow");
    let georgia = host.tree().bounds(preview).unwrap();
    perform(&mut host, "Use Menlo", SemanticAction::Activate, None);
    let menlo = host.tree().bounds(preview).unwrap();
    assert!(
        menlo.size.height > georgia.size.height,
        "{menlo:?} {georgia:?}"
    );
    assert!(menlo.size.width <= 260.0);
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn the_font_dialog_restores_the_weight_keeps_fractions_and_wraps_its_row() {
    let mut host = host(NarrowFonts);
    assert!(host.find("Family").is_some(), "the label names the chooser");
    perform(&mut host, "Bold", SemanticAction::Activate, None);
    assert!(host.find("16 700 Upright").is_some());
    perform(&mut host, "Bold", SemanticAction::Activate, None);
    assert!(
        host.find("16 500 Upright").is_some(),
        "bold off restores 500"
    );
    perform(&mut host, "Size", SemanticAction::SetValue, Some("16.5"));
    assert!(host.find("16.5 500 Upright").is_some());
    let family = bounds(&mut host, "Family");
    let italic = bounds(&mut host, "Italic");
    assert!(italic.min_y() >= family.max_y(), "the row wraps");
    assert_eq!(host.take_errors(), Vec::new());
}

// ---------------------------------------------------------------------------
// Sidebar shortcut

/// A sidebar beside a button, with the sidebar's open state shown as text.
struct Shell;

impl View for Shell {
    fn body(&self, cx: &mut Cx) -> impl View {
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

/// A checkbox whose invalid state follows a state value, and a button that
/// clears it.
struct Consent;

impl View for Consent {
    fn body(&self, cx: &mut Cx) -> impl View {
        let on = cx.state(true);
        let invalid = cx.state(true);
        VStack::new((
            Checkbox::new("Consent", on.binding())
                .with(move |checkbox| checkbox.with_invalid(invalid.get())),
            choscorui_compose::view::Button::new("Fix", move || invalid.set(false)),
        ))
    }
}

#[test]
fn a_form_view_shows_the_invalid_state_and_follows_its_change() {
    let mut host = host(Consent);
    let checkbox = node(&mut host, "Consent");
    let semantic = host.semantic(checkbox).unwrap();
    assert!(semantic.state.invalid);
    assert_eq!(semantic.state.checked, Some(true));
    perform(&mut host, "Fix", SemanticAction::Activate, None);
    let semantic = host.semantic(checkbox).unwrap();
    assert!(!semantic.state.invalid);
    assert_eq!(semantic.state.checked, Some(true), "the value stays");
    assert_eq!(host.take_errors(), Vec::new());
}

// ---------------------------------------------------------------------------
// Button group

/// A button, a separator, and a toggle, with a readout of the runs and
/// the checked members.
struct Mail;

impl View for Mail {
    fn body(&self, cx: &mut Cx) -> impl View {
        let (runs, checked) = (cx.state(0_usize), cx.state(Vec::new()));
        let group = choscorui_controls::ButtonGroup::from_members(
            vec![
                ButtonGroupMember::Button(CommandId(1), ButtonVariant::Outline),
                ButtonGroupMember::Separator,
                ButtonGroupMember::Toggle(CommandId(2)),
            ],
            false,
        );
        let labels = ["Archive", "", "Pin"].map(String::from).to_vec();
        VStack::new((
            ButtonGroup::new("Mail", group, labels, checked.binding())
                .on_item(move |index| runs.update(|runs| *runs += index + 1)),
            Text::new(move || format!("runs {} checked {:?}", runs.get(), checked.get())),
        ))
    }
}

#[test]
fn the_button_group_view_runs_buttons_and_binds_toggles() {
    let mut host = host(Mail);
    perform(&mut host, "Archive", SemanticAction::Activate, None);
    assert!(host.find("runs 1 checked []").is_some());
    let group = node(&mut host, "Archive");
    host.focus(Some(group));
    press(&mut host, Key::ArrowRight);
    perform(&mut host, "Pin", SemanticAction::Activate, None);
    assert!(host.find("runs 1 checked [2]").is_some());
    assert_eq!(host.take_errors(), Vec::new());
}
