//! Contracts that every hosted control keeps. A recipe changes only how a
//! control paints: under another recipe, the same keys bring the same
//! messages, focus, and semantics. Reduced motion finishes every
//! animation at once.

mod hosted;

use choscorui_controls::{
    Accordion, AddonAlign, Alert, AspectRatio, Attachment, AttachmentState, Avatar, AvatarGroup,
    Badge, Breadcrumb, Bubble, Card, Carousel, Chart, ChartKind, ChartSeries, Collapsible, Command,
    CommandId, CommandItem, ContextMenu, ControlKind, ControlMessage, Conversation, Corners,
    CustomControl, DataTable, DatePicker, DefaultRecipe, Dialog, Empty, FieldPart, FieldText,
    Input, InputGroup, InputGroupAddon, InputOtp, Item, ItemGroup, Kbd, ListEntry, Marker,
    MenuItem, Menubar, Message, MessageScroller, NavigationLink, NavigationMenu, Pagination,
    Popover, Question, Questionnaire, QuestionnaireLabels, RangeSlider, Recipe, ResolvedStyle,
    Select, Sidebar, SidebarItem, Skeleton, Spinner, StyleQuery, Styles, Switch, TextField,
    Textarea, Toast, Toaster, Toasts, Toggle, ToggleGroup, ToggleItem,
};
use choscorui_core::{
    CivilDate, Color, DesignTokens, Icon, Key, KeyEvent, LayoutDirection, Locale, RecipeSet,
    SemanticNode, ShadowLayers, StringListModel, ThemeResolver,
};
use hosted::Harness;
use std::rc::Rc;

type Factory = fn() -> Box<dyn CustomControl>;

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

fn menu() -> Vec<MenuItem> {
    vec![
        MenuItem::action("Back", CommandId(1)),
        MenuItem::action("Forward", CommandId(2)),
    ]
}

/// Every hosted control, with sample content.
fn catalog() -> Vec<(&'static str, Factory)> {
    vec![
        ("switch", || Box::new(Switch::new("Airplane mode", false))),
        ("toggle", || Box::new(Toggle::new("Bold", false))),
        ("toggle-group", || {
            Box::new(ToggleGroup::new(
                "Alignment",
                vec![ToggleItem::new("Left"), ToggleItem::new("Right")],
                false,
            ))
        }),
        ("kbd", || Box::new(Kbd::new("⌘K"))),
        ("select", || {
            Box::new(Select::new(
                "Status",
                vec![ListEntry::option("Todo"), ListEntry::option("Done")],
            ))
        }),
        ("input", || Box::new(Input::new("Email", ""))),
        ("input-group", || {
            Box::new(InputGroup::new(
                TextField::new("Search", ""),
                vec![InputGroupAddon::icon(AddonAlign::InlineStart, Icon::Search)],
            ))
        }),
        ("textarea", || Box::new(Textarea::new("Message", ""))),
        ("input-otp", || Box::new(InputOtp::new("Code", 6))),
        ("field-text", || {
            Box::new(FieldText::new(FieldPart::Description, "We never share it."))
        }),
        ("date-picker", || {
            Box::new(DatePicker::new(
                "Due date",
                Locale::parse("en").unwrap(),
                CivilDate::new(2026, 10, 12).unwrap(),
            ))
        }),
        ("range-slider", || {
            Box::new(RangeSlider::new("Price", 0.0, 100.0, 1.0, 20.0, 80.0))
        }),
        ("context-menu", || {
            Box::new(ContextMenu::new("Area", "Right click here", menu()))
        }),
        ("menubar", || {
            Box::new(Menubar::new("Menu", vec![("File".to_string(), menu())]))
        }),
        ("popover", || {
            Box::new(Popover::new("Open", "Dimensions", "Set the layer size."))
        }),
        ("hover-card", || {
            Box::new(Popover::hover_card(
                "@nextjs",
                "Next.js",
                "The React framework.",
            ))
        }),
        ("dialog", || {
            Box::new(Dialog::new("Edit", "Edit profile", "Make changes here."))
        }),
        ("command", || {
            Box::new(Command::new(
                "Commands",
                "Type a command",
                vec![(
                    "Suggestions".to_string(),
                    vec![CommandItem::new("Calendar"), CommandItem::new("Search")],
                )],
            ))
        }),
        ("breadcrumb", || {
            Box::new(Breadcrumb::new(
                "Path",
                strings(&["Home", "Docs", "Button"]),
            ))
        }),
        ("pagination", || {
            Box::new(Pagination::new("Pages", "Previous", "Next", 10, 2))
        }),
        ("navigation-menu", || {
            Box::new(NavigationMenu::new(
                "Site",
                vec![(
                    "Docs".to_string(),
                    vec![NavigationLink::new("Introduction", "Start here.")],
                )],
            ))
        }),
        ("sidebar", || {
            Box::new(Sidebar::new(
                "Navigation",
                "Toggle sidebar",
                vec![(
                    "Application".to_string(),
                    vec![
                        SidebarItem::new("Inbox", Icon::Inbox),
                        SidebarItem::new("Calendar", Icon::Calendar),
                    ],
                )],
            ))
        }),
        ("card", || {
            Box::new(
                Card::new("Login", "Enter your email.")
                    .with_footer("Sign in", choscorui_controls::ButtonVariant::Default),
            )
        }),
        ("badge", || Box::new(Badge::new("New").as_link())),
        ("avatar", || Box::new(Avatar::new("Ana Lee", "AL"))),
        ("avatar-group", || {
            Box::new(AvatarGroup::new(
                "Team",
                vec![Avatar::new("Ana Lee", "AL"), Avatar::new("Bao Tran", "BT")],
            ))
        }),
        ("carousel", || {
            Box::new(Carousel::new(
                "Slides",
                "Previous",
                "Next",
                strings(&["1", "2", "3"]),
            ))
        }),
        ("data-table", || {
            let model = StringListModel::from_strings(["Ana", "Bao", "Chi"]);
            Box::new(DataTable::new(
                "People",
                Box::new(model),
                "Previous",
                "Next",
            ))
        }),
        ("chart", || {
            Box::new(Chart::new(
                "Visitors",
                ChartKind::Bar,
                strings(&["Jan", "Feb"]),
                vec![ChartSeries::new("Desktop", vec![186.0, 305.0])],
            ))
        }),
        ("alert", || {
            Box::new(
                Alert::new("Heads up!", "You can add components.")
                    .with_action("Undo", choscorui_controls::ButtonVariant::Outline),
            )
        }),
        ("spinner", || Box::new(Spinner::new("Loading"))),
        ("skeleton", || {
            Box::new(Skeleton::new("Loading", 200.0, 16.0))
        }),
        ("empty", || {
            Box::new(
                Empty::new("No projects", "Create one.")
                    .with_action("Create", choscorui_controls::ButtonVariant::Default),
            )
        }),
        ("toaster", || {
            let toasts = Toasts::new();
            toasts.push(Toast::new("Event created").with_action("Undo"));
            Box::new(Toaster::new("Notifications", toasts, "Close"))
        }),
        ("aspect-ratio", || {
            Box::new(AspectRatio::new("Photo", 16.0 / 9.0))
        }),
        ("collapsible", || {
            Box::new(Collapsible::new(
                "Starred",
                "Toggle",
                "first",
                strings(&["second"]),
            ))
        }),
        ("accordion", || {
            Box::new(Accordion::new(
                "FAQ",
                pairs(&[("One", "First."), ("Two", "Second.")]),
            ))
        }),
        ("item-group", || {
            Box::new(ItemGroup::new(
                "Settings",
                vec![
                    Item::new("Profile").as_link(),
                    Item::new("Billing")
                        .with_action("Open", choscorui_controls::ButtonVariant::Outline),
                ],
            ))
        }),
        ("message", || {
            Box::new(Message::new("Ana", vec![Bubble::new("Hello!")]).with_footer("Read"))
        }),
        ("attachment", || {
            Box::new(
                Attachment::new("report.pdf")
                    .with_state(AttachmentState::Processing)
                    .with_remove("Remove"),
            )
        }),
        ("marker", || Box::new(Marker::new("Today"))),
        ("message-scroller", || {
            let conversation = Conversation::new();
            conversation.push(Message::new("Ana", vec![Bubble::new("Hi")]));
            Box::new(MessageScroller::new("Chat", conversation))
        }),
        ("questionnaire", || {
            Box::new(Questionnaire::new(
                "Survey",
                vec![
                    Question::choice("Pick one", strings(&["A", "B"])),
                    Question::freeform("Why?", "Type here"),
                ],
                QuestionnaireLabels {
                    previous: "Previous".into(),
                    skip: "Skip".into(),
                    next: "Next".into(),
                    submit: "Submit".into(),
                    required: "Required".into(),
                },
            ))
        }),
    ]
}

/// The default look with other geometry and colors: square corners, more
/// padding and gap, taller boxes, and no shadows.
struct Reshaped;

impl Recipe for Reshaped {
    fn resolve(&self, query: &StyleQuery, tokens: &DesignTokens) -> Option<ResolvedStyle> {
        let mut style = DefaultRecipe.resolve(query, tokens)?;
        style.radius = Corners::ZERO;
        style.shadow = ShadowLayers::NONE;
        style.padding.inline_start += 3.0;
        style.padding.inline_end += 3.0;
        style.padding.block_start += 2.0;
        style.padding.block_end += 2.0;
        style.gap += 4.0;
        style.min_height += 6.0;
        if style.background.alpha > 0 {
            style.background = Color::rgba8(40, 140, 230, 255);
        }
        Some(style)
    }
}

fn reshaped() -> Styles {
    let mut styles = Styles::new();
    for kind in ControlKind::ALL {
        styles.set_recipe(RecipeSet::DEFAULT, *kind, Rc::new(Reshaped));
    }
    styles
}

/// The keys that each control receives, in order.
const SCRIPT: [Key; 9] = [
    Key::ArrowDown,
    Key::ArrowRight,
    Key::Space,
    Key::Tab,
    Key::Enter,
    Key::ArrowUp,
    Key::Escape,
    Key::End,
    Key::Tab,
];

/// What a control reports after each step of the script.
#[derive(Debug, PartialEq)]
struct Step {
    key: Key,
    handled: bool,
    messages: Vec<ControlMessage>,
    focused: bool,
    semantics: SemanticNode,
    animating: bool,
}

/// Mounts the control with `styles` and `tokens`, focuses it, and plays
/// the script.
fn play(factory: Factory, styles: Styles, tokens: DesignTokens) -> (Harness, Vec<Step>) {
    let themes = ThemeResolver::new(tokens);
    let mut h = Harness::mount(factory(), themes, styles, LayoutDirection::Ltr);
    let _ = h.host.advance(&mut h.tree, 0.0);
    let bounds = h.bounds();
    h.place(bounds);
    let mut steps = Vec::new();
    if h.host.get(h.node).unwrap().focusable() {
        h.focus();
    }
    for key in SCRIPT {
        let handled = h
            .host
            .dispatch_key(&mut h.tree, KeyEvent::Down { key, repeat: false });
        let _ = h.host.dispatch_key(&mut h.tree, KeyEvent::Up { key });
        let bounds = h.bounds();
        h.place(bounds);
        steps.push(Step {
            key,
            handled,
            messages: h.messages(),
            focused: h.tree.focus() == Some(h.node),
            semantics: h.semantics(),
            animating: h.host.animating(&h.tree, h.time),
        });
    }
    (h, steps)
}

#[test]
fn another_recipe_keeps_messages_focus_and_semantics() {
    for (name, factory) in catalog() {
        let (_, default) = play(factory, Styles::new(), DesignTokens::light());
        let (_, swapped) = play(factory, reshaped(), DesignTokens::light());
        assert_eq!(default.len(), swapped.len(), "{name}");
        for (expected, actual) in default.iter().zip(&swapped) {
            assert_eq!(expected, actual, "{name}: {:?}", expected.key);
        }
    }
}

#[test]
fn reduced_motion_finishes_every_animation_at_once() {
    let mut tokens = DesignTokens::light();
    tokens.motion.reduced = true;
    for (name, factory) in catalog() {
        let (h, steps) = play(factory, Styles::new(), tokens);
        assert!(
            !h.host.animating(&h.tree, h.time),
            "{name} animates at rest"
        );
        for step in &steps {
            assert!(!step.animating, "{name} animates after {:?}", step.key);
        }
    }
}
