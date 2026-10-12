//! Every hosted control with sample content, shared by the contract tests
//! and the facade's golden files.

use choscorui_controls::{
    Accordion, AddonAlign, Alert, AspectRatio, Attachment, AttachmentState, Avatar, AvatarGroup,
    Badge, Breadcrumb, Bubble, Card, Carousel, Chart, ChartKind, ChartSeries, Collapsible, Command,
    CommandId, CommandItem, ContextMenu, Conversation, CustomControl, DataTable, DatePicker,
    Dialog, Empty, FieldPart, FieldText, Input, InputGroup, InputGroupAddon, InputOtp, Item,
    ItemGroup, Kbd, ListEntry, Marker, MenuItem, Menubar, Message, MessageScroller, NavigationLink,
    NavigationMenu, Pagination, Popover, Question, Questionnaire, QuestionnaireLabels, RangeSlider,
    Select, Sidebar, SidebarItem, Skeleton, Spinner, Switch, TextField, Textarea, Toast, Toaster,
    Toasts, Toggle, ToggleGroup, ToggleItem,
};
use choscorui_core::{CivilDate, Icon, Locale, StringListModel};

pub type Factory = fn() -> Box<dyn CustomControl>;

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
pub fn catalog() -> Vec<(&'static str, Factory)> {
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
