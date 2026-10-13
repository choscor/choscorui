//! Headless tests of the form field views: orientations, the responsive
//! switch on resize, right-to-left mirroring, error semantics, and the
//! gaps of field sets and groups.

use choscorui_compose::Runtime;
use choscorui_compose::view::{
    Field, FieldContent, FieldGroup, FieldSeparator, FieldSet, FieldTitle, Input, Switch, VStack,
    View, ViewExt, ViewHost,
};
use choscorui_controls::{EstimatedText, FieldOrientation, ShortcutModifiers};
use choscorui_core::{
    Catalog, Color, ColorRole, DesignTokens, DisplayCommand, Locale, NodeId, PointerButton,
    PointerEvent, Rect, SemanticAction, SemanticRelation, SemanticRole, Size,
};
use std::rc::Rc;

/// A host `width` points wide whose locale has the language `tag`.
fn host(view: impl View, width: f64, tag: &str) -> ViewHost {
    let mut catalog = Catalog::new();
    let locale = Locale::parse(tag).unwrap();
    catalog.insert(&locale, "email", "Email");
    let mut host = ViewHost::new(
        Size::new(width, 800.0),
        DesignTokens::light(),
        choscorui_compose::view::Env::new(Rc::new(catalog), locale),
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

/// A field labelled "Email" around an input named "address", with help
/// text.
fn email(orientation: FieldOrientation) -> impl View {
    struct Email(FieldOrientation);
    impl View for Email {
        fn body(&self, cx: &mut choscorui_compose::view::Cx) -> impl View {
            let text = cx.state(String::new());
            Field::new(
                choscorui_compose::t!("email"),
                Input::new("address", text.binding()),
            )
            .orientation(self.0)
            .description("We never share it")
        }
    }
    Email(orientation)
}

#[test]
fn a_vertical_field_stacks_its_parts_with_the_field_gap() {
    let mut host = host(email(FieldOrientation::Vertical), 600.0, "en");
    let label = bounds(&mut host, "Email");
    let input = bounds(&mut host, "address");
    let help = bounds(&mut host, "We never share it");
    assert_eq!(input.min_y(), label.max_y() + 12.0, "the 12 pt field gap");
    assert_eq!(help.min_y(), input.max_y() + 12.0);
    assert_eq!(label.min_x(), input.min_x());
    assert_eq!(input.size.width, 600.0, "each part fills the field");
}

#[test]
fn a_horizontal_field_puts_the_label_beside_the_other_parts() {
    let mut host = host(email(FieldOrientation::Horizontal), 600.0, "en");
    let label = bounds(&mut host, "Email");
    let input = bounds(&mut host, "address");
    let help = bounds(&mut host, "We never share it");
    assert_eq!(label.min_x(), 0.0);
    assert_eq!(input.min_x(), label.max_x() + 12.0);
    assert_eq!(input.max_x(), 600.0);
    assert_eq!(help.min_y(), input.max_y() + 4.0, "the content gap");
    assert_eq!(label.min_y(), input.min_y(), "two parts align to the top");
}

#[test]
fn a_responsive_field_follows_the_window_width() {
    let mut host = host(email(FieldOrientation::Responsive), 600.0, "en");
    let (label, input) = (bounds(&mut host, "Email"), bounds(&mut host, "address"));
    assert_eq!(
        input.min_x(),
        label.max_x() + 12.0,
        "side by side at 600 pt"
    );

    host.resize(Size::new(400.0, 400.0));
    host.layout();
    let (label, input) = (bounds(&mut host, "Email"), bounds(&mut host, "address"));
    assert_eq!(input.min_y(), label.max_y() + 12.0, "stacked at 400 pt");
    assert_eq!(input.size.width, 400.0);

    host.resize(Size::new(448.0, 400.0));
    host.layout();
    let (label, input) = (bounds(&mut host, "Email"), bounds(&mut host, "address"));
    assert_eq!(
        input.min_x(),
        label.max_x() + 12.0,
        "side by side at 448 pt"
    );
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn a_horizontal_field_mirrors_in_right_to_left_layouts() {
    let mut host = host(email(FieldOrientation::Horizontal), 600.0, "ar");
    let label = bounds(&mut host, "Email");
    let input = bounds(&mut host, "address");
    assert_eq!(label.max_x(), 600.0, "the label is at the right");
    assert_eq!(input.max_x(), label.min_x() - 12.0);
    assert_eq!(input.min_x(), 0.0);
}

/// A field whose error shows while `invalid` is true.
struct Username;

impl View for Username {
    fn body(&self, cx: &mut choscorui_compose::view::Cx) -> impl View {
        let name = cx.state(String::from("x"));
        let invalid = cx.state(true);
        VStack::new((
            Field::new("Username", Input::new("name", name.binding()))
                .description("Pick a name")
                .error("Too short", move || invalid.get()),
            Switch::new("Invalid", invalid.binding()),
        ))
    }
}

#[test]
fn an_error_shows_and_marks_the_control_invalid() {
    let mut host = host(Username, 600.0, "en");
    let input = node(&mut host, "name");
    let semantic = host.semantic(input).unwrap();
    assert!(semantic.state.invalid);
    // The error does not replace the help text: both describe the input.
    assert_eq!(semantic.description, "Too short\nPick a name");
    let label = node(&mut host, "Username");
    let help = node(&mut host, "Pick a name");
    assert!(
        semantic
            .relations
            .contains(&SemanticRelation::LabelledBy(label))
    );
    assert!(
        semantic
            .relations
            .contains(&SemanticRelation::DescribedBy(help))
    );
    let error = node(&mut host, "Too short");
    assert!(
        semantic
            .relations
            .contains(&SemanticRelation::DescribedBy(error))
    );
    let error_node = host.semantic(error).unwrap();
    assert_eq!(error_node.role, SemanticRole::StaticText);
    assert!(error_node.state.invalid);
    assert_eq!(
        bounds(&mut host, "Too short").min_y(),
        bounds(&mut host, "Pick a name").max_y() + 12.0
    );

    // The switch clears the error: the text goes, and the control is
    // valid with the description again.
    let switch = node(&mut host, "Invalid");
    assert!(host.perform(switch, choscorui_core::SemanticAction::Activate, None));
    host.layout();
    assert_eq!(host.take_errors(), Vec::new());
    assert_eq!(host.find("Too short"), None);
    let semantic = host.semantic(input).unwrap();
    assert!(!semantic.state.invalid);
    assert_eq!(semantic.description, "Pick a name");
    assert_eq!(
        semantic.relations,
        [
            SemanticRelation::LabelledBy(label),
            SemanticRelation::DescribedBy(help)
        ]
    );
}

/// The color of the painted text `text`.
fn text_color(host: &mut ViewHost, text: &str) -> Option<Color> {
    host.paint()
        .commands()
        .iter()
        .find_map(|command| match command {
            DisplayCommand::DrawText {
                text: drawn, color, ..
            } if drawn == text => Some(*color),
            _ => None,
        })
}

/// True when the paint strokes an edge in `color`.
fn strokes(host: &mut ViewHost, color: Color) -> bool {
    host.paint().commands().iter().any(|command| {
        matches!(command, DisplayCommand::StrokeRoundedRect { color: stroke, .. } if *stroke == color)
    })
}

#[test]
fn an_error_gives_the_control_the_invalid_look_and_the_label_the_destructive_color() {
    let mut host = host(Username, 600.0, "en");
    let tokens = DesignTokens::light();
    let destructive = tokens.color(ColorRole::Destructive);
    let input = node(&mut host, "name");
    let label = node(&mut host, "Username");
    let state = |host: &ViewHost, node| host.controls().visual_state(host.tree(), node);
    assert!(state(&host, input).invalid());
    assert!(state(&host, label).invalid());
    assert_eq!(text_color(&mut host, "Username"), Some(destructive));
    assert!(strokes(&mut host, destructive), "the destructive edge");

    let switch = node(&mut host, "Invalid");
    assert!(host.perform(switch, SemanticAction::Activate, None));
    host.layout();
    assert!(!state(&host, input).invalid());
    assert_eq!(
        text_color(&mut host, "Username"),
        Some(tokens.color(ColorRole::Foreground))
    );
    assert!(!strokes(&mut host, destructive));
}

/// A required switch in a field with an error.
struct Consent;

impl View for Consent {
    fn body(&self, cx: &mut choscorui_compose::view::Cx) -> impl View {
        let on = cx.state(false);
        Field::new("Consent", Switch::new("Share data", on.binding()))
            .error("Required", move || !on.get())
    }
}

#[test]
fn a_field_around_a_switch_shows_the_error_and_keeps_the_switch_look() {
    let mut host = host(Consent, 600.0, "en");
    let destructive = DesignTokens::light().color(ColorRole::Destructive);
    assert_eq!(text_color(&mut host, "Consent"), Some(destructive));
    assert!(host.find("Required").is_some());
    assert!(
        !strokes(&mut host, destructive),
        "the switch stays the same"
    );
}

#[test]
fn a_click_on_the_label_focuses_the_control_or_toggles_a_switch() {
    let mut host = host(email(FieldOrientation::Vertical), 600.0, "en");
    let click = |host: &mut ViewHost, at: Rect| {
        let position = at.center();
        let button = PointerButton::Primary;
        let modifiers = ShortcutModifiers::default();
        host.pointer(PointerEvent::Down { position, button }, modifiers);
        host.pointer(PointerEvent::Up { position, button }, modifiers);
        host.layout();
    };
    let label = bounds(&mut host, "Email");
    click(&mut host, label);
    assert_eq!(host.tree().focus(), Some(node(&mut host, "address")));

    let mut consent = self::host(Consent, 600.0, "en");
    let switch = node(&mut consent, "Share data");
    let label = bounds(&mut consent, "Consent");
    click(&mut consent, label);
    assert_eq!(consent.tree().focus(), Some(switch));
    assert_eq!(consent.semantic(switch).unwrap().state.checked, Some(true));
    assert_eq!(consent.find("Required"), None, "the binding took the value");
}

#[test]
fn a_disabled_field_disables_its_control_and_dims_its_label() {
    struct Off;
    impl View for Off {
        fn body(&self, cx: &mut choscorui_compose::view::Cx) -> impl View {
            let text = cx.state(String::new());
            Field::new("Email", Input::new("address", text.binding())).disabled(true)
        }
    }
    let mut host = host(Off, 600.0, "en");
    let input = node(&mut host, "address");
    let label = node(&mut host, "Email");
    assert!(host.semantic(input).unwrap().state.disabled);
    assert!(!host.controls().is_enabled(label));
    let state = host.controls().visual_state(host.tree(), label);
    assert!(state.disabled(), "the label paints with half opacity");
}

#[test]
fn sets_and_groups_stack_with_their_recipe_gaps() {
    struct Payment;
    impl View for Payment {
        fn body(&self, cx: &mut choscorui_compose::view::Cx) -> impl View {
            let (card, owner) = (cx.state(String::new()), cx.state(String::new()));
            FieldSet::new(
                "Payment",
                FieldGroup::new((
                    Field::new("Card", Input::new("card", card.binding())),
                    FieldSeparator::new().text("or"),
                    Field::new("Owner", Input::new("owner", owner.binding())),
                    FieldContent::new((FieldTitle::new("Billing"), FieldTitle::new("Details"))),
                )),
            )
            .description("All transactions are secure")
        }
    }
    let mut host = host(Payment, 600.0, "en");
    // The set is a group named by its legend, before the legend text.
    let set = node(&mut host, "Payment");
    assert_eq!(host.semantic(set).unwrap().role, SemanticRole::Group);
    let legend = host.tree().children(set).unwrap()[0];
    let legend = host.tree().bounds(legend).unwrap();
    let description = bounds(&mut host, "All transactions are secure");
    assert_eq!(description.min_y(), legend.max_y() + 24.0, "the set gap");
    let card = bounds(&mut host, "Card");
    assert_eq!(card.min_y(), description.max_y() + 24.0);
    let separator = bounds(&mut host, "or");
    let card_input = bounds(&mut host, "card");
    assert_eq!(
        separator.min_y(),
        card_input.max_y() + 28.0,
        "the group gap"
    );
    assert_eq!(bounds(&mut host, "Owner").min_y(), separator.max_y() + 28.0);
    let title = bounds(&mut host, "Billing");
    assert_eq!(
        bounds(&mut host, "Details").min_y(),
        title.max_y() + 4.0,
        "the content gap"
    );
}
