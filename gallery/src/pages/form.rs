//! Form components mounted through `ControlHost`: switches, toggles,
//! selects, text inputs, code inputs, and keyboard keys.

use crate::page::{Env, Page};
use crate::pages::hosted::{HostedPage, Row};
use choscorui::controls::{
    AddonAlign, CalendarMode, ControlSize, CustomControl, DatePicker, FieldOrientation, FieldPart,
    FieldText, Input, InputGroup, InputGroupAddon, InputOtp, Kbd, KbdVariant, ListEntry,
    Orientation, Select, Switch, TextField, Textarea, Toggle, ToggleGroup, ToggleItem,
    ToggleVariant, ValidationState,
};
use choscorui::core::{CivilDate, Icon, NodeId, Tree};

fn boxed(control: impl CustomControl + 'static) -> Box<dyn CustomControl> {
    Box::new(control)
}

pub fn switch(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows: Vec<Row> = vec![
        (
            "Default size",
            vec![
                boxed(Switch::new("Airplane mode", false)),
                boxed(Switch::new("Wi-Fi", true)),
            ],
        ),
        (
            "Small size",
            vec![
                boxed(Switch::new("Bluetooth", false).with_size(ControlSize::Sm)),
                boxed(Switch::new("Hotspot", true).with_size(ControlSize::Sm)),
            ],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

pub fn toggle(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows = [ToggleVariant::Default, ToggleVariant::Outline]
        .into_iter()
        .map(|variant| {
            let caption = match variant {
                ToggleVariant::Default => "Default variant",
                ToggleVariant::Outline => "Outline variant",
            };
            let controls = [ControlSize::Sm, ControlSize::Default, ControlSize::Lg]
                .into_iter()
                .zip([
                    ("Bold", Icon::Bold),
                    ("Italic", Icon::Italic),
                    ("Underline", Icon::Underline),
                ])
                .enumerate()
                .map(|(index, (size, (label, icon)))| {
                    boxed(
                        Toggle::new(label, index == 1)
                            .with_icon(icon)
                            .with_variant(variant)
                            .with_size(size),
                    )
                })
                .collect();
            (caption, controls)
        })
        .collect();
    HostedPage::build(tree, parent, rows)
}

fn alignment() -> Vec<ToggleItem> {
    vec![
        ToggleItem::new("Start").with_icon(Icon::TextAlignStart),
        ToggleItem::new("Center").with_icon(Icon::TextAlignCenter),
        ToggleItem::new("End").with_icon(Icon::TextAlignEnd),
    ]
}

pub fn toggle_group(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let mut single =
        ToggleGroup::new("Alignment", alignment(), false).with_variant(ToggleVariant::Outline);
    single.set_selected(&[0]);
    let mut multiple = ToggleGroup::new(
        "Text style",
        vec![
            ToggleItem::new("Bold").with_icon(Icon::Bold),
            ToggleItem::new("Italic").with_icon(Icon::Italic),
            ToggleItem::new("Underline").with_icon(Icon::Underline),
            ToggleItem::new("Strikethrough").disabled(),
        ],
        true,
    )
    .with_spacing(4.0);
    multiple.set_selected(&[0, 2]);
    let rows: Vec<Row> = vec![
        ("Single selection, joined", vec![boxed(single)]),
        ("Multiple selection, spaced", vec![boxed(multiple)]),
        (
            "Small and vertical",
            vec![
                boxed(ToggleGroup::new("Small", alignment(), false).with_size(ControlSize::Sm)),
                boxed(
                    ToggleGroup::new("Vertical", alignment(), false)
                        .with_variant(ToggleVariant::Outline)
                        .with_orientation(Orientation::Vertical),
                ),
            ],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

fn fruits() -> Vec<ListEntry> {
    vec![
        ListEntry::group("Fruits"),
        ListEntry::option("Apple"),
        ListEntry::option("Banana"),
        ListEntry::option("Blueberry"),
        ListEntry::disabled("Grapes"),
        ListEntry::option("Pineapple"),
        ListEntry::Separator,
        ListEntry::group("Vegetables"),
        ListEntry::option("Carrot"),
        ListEntry::option("Leek"),
    ]
}

pub fn select(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let mut chosen = Select::new("Fruit", fruits()).with_placeholder("Select a fruit");
    chosen.set_selected(Some(2));
    let mut invalid = Select::new("Required fruit", fruits()).with_placeholder("Select a fruit");
    invalid.set_invalid(true);
    let zones = [
        "Eastern Time",
        "Central Time",
        "Mountain Time",
        "Pacific Time",
        "Alaska Time",
        "Hawaii Time",
        "Greenwich Mean Time",
        "Central European Time",
        "Eastern European Time",
        "India Standard Time",
        "China Standard Time",
        "Japan Standard Time",
    ];
    let rows: Vec<Row> = vec![
        (
            "Default size",
            vec![
                boxed(Select::new("Fruit", fruits()).with_placeholder("Select a fruit")),
                boxed(chosen),
            ],
        ),
        (
            "Small size",
            vec![boxed(
                Select::new("Small fruit", fruits())
                    .with_placeholder("Select a fruit")
                    .with_size(ControlSize::Sm),
            )],
        ),
        ("Invalid", vec![boxed(invalid)]),
        (
            "Native look",
            vec![boxed(
                Select::native(
                    "Status",
                    vec![
                        ListEntry::option("Todo"),
                        ListEntry::option("In progress"),
                        ListEntry::option("Done"),
                    ],
                )
                .with_placeholder("Select a status"),
            )],
        ),
        (
            "Scrolling list",
            vec![boxed(
                Select::new(
                    "Time zone",
                    zones.iter().map(|zone| ListEntry::option(*zone)).collect(),
                )
                .with_placeholder("Select a time zone"),
            )],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

pub fn input(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let mut invalid = Input::new("Username", "a");
    invalid.set_validation(ValidationState::Invalid(
        "Use at least three characters".into(),
    ));
    let rows: Vec<Row> = vec![
        (
            "Text",
            vec![boxed(
                Input::new("Email", "").with_placeholder("name@example.com"),
            )],
        ),
        (
            "Password",
            vec![boxed(Input::password("Password", "hunter2"))],
        ),
        ("Invalid", vec![boxed(invalid)]),
    ];
    HostedPage::build(tree, parent, rows)
}

pub fn input_group(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows: Vec<Row> = vec![
        (
            "Inline addons",
            vec![boxed(InputGroup::new(
                TextField::new("Search", "").with_placeholder("Search…"),
                vec![
                    InputGroupAddon::icon(AddonAlign::InlineStart, Icon::Search),
                    InputGroupAddon::text(AddonAlign::InlineEnd, "12 results"),
                ],
            ))],
        ),
        (
            "Text and buttons",
            vec![
                boxed(InputGroup::new(
                    TextField::new("Website", "example"),
                    vec![
                        InputGroupAddon::text(AddonAlign::InlineStart, "https://"),
                        InputGroupAddon::text(AddonAlign::InlineEnd, ".com"),
                    ],
                )),
                boxed(InputGroup::new(
                    TextField::new("Link", "https://choscor.com"),
                    vec![
                        InputGroupAddon::button(
                            AddonAlign::InlineEnd,
                            "Copy",
                            Some(Icon::Copy),
                            ControlSize::IconXs,
                        ),
                        InputGroupAddon::button(
                            AddonAlign::InlineEnd,
                            "Share",
                            None,
                            ControlSize::Xs,
                        ),
                    ],
                )),
            ],
        ),
        (
            "Block addons",
            vec![boxed(InputGroup::new(
                TextField::new("Message", "").with_placeholder("Ask anything"),
                vec![
                    InputGroupAddon::text(AddonAlign::BlockStart, "To: Design team"),
                    InputGroupAddon::button(
                        AddonAlign::BlockEnd,
                        "Send",
                        Some(Icon::Send),
                        ControlSize::Sm,
                    ),
                ],
            ))],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

pub fn textarea(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let mut invalid = Textarea::new("Bio", "");
    invalid.set_invalid(true);
    let rows: Vec<Row> = vec![
        (
            "Grows with its text",
            vec![boxed(
                Textarea::new("Message", "").with_placeholder("Type your message here."),
            )],
        ),
        (
            "Invalid",
            vec![boxed(invalid.with_placeholder("Tell us about yourself"))],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

pub fn input_otp(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let mut invalid = InputOtp::new("Invalid code", 6);
    invalid.set_invalid(true);
    let rows: Vec<Row> = vec![
        ("Six digits", vec![boxed(InputOtp::new("Code", 6))]),
        (
            "Groups with a separator",
            vec![boxed(
                InputOtp::new("Grouped code", 6).with_groups(vec![3, 3]),
            )],
        ),
        (
            "Letters and digits",
            vec![boxed(
                InputOtp::new("Recovery code", 6)
                    .with_groups(vec![2, 2, 2])
                    .alphanumeric(),
            )],
        ),
        ("Invalid", vec![boxed(invalid)]),
    ];
    HostedPage::build(tree, parent, rows)
}

pub fn field(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let text = |part, text: &str| boxed(FieldText::new(part, text));
    let mut invalid = Input::new("Username", "a");
    invalid.set_validation(ValidationState::Invalid(
        "Use at least three characters".into(),
    ));
    let rows: Vec<(Row, Option<FieldOrientation>)> = vec![
        (
            (
                "Field set",
                vec![
                    text(FieldPart::Legend, "Payment method"),
                    text(
                        FieldPart::Description,
                        "All transactions are secure and encrypted.",
                    ),
                    text(FieldPart::Label, "Name on card"),
                    boxed(Input::new("Name on card", "").with_placeholder("Evil Rabbit")),
                ],
            ),
            Some(FieldOrientation::Vertical),
        ),
        (
            (
                "Vertical field",
                vec![
                    text(FieldPart::Label, "Email"),
                    boxed(Input::new("Email", "").with_placeholder("name@example.com")),
                    text(
                        FieldPart::Description,
                        "We never share your email with anyone.",
                    ),
                ],
            ),
            Some(FieldOrientation::Vertical),
        ),
        (
            (
                "Horizontal field",
                vec![
                    boxed(Switch::new("Marketing emails", true)),
                    text(
                        FieldPart::Description,
                        "Receive emails about new products and features.",
                    ),
                ],
            ),
            Some(FieldOrientation::Horizontal),
        ),
        (
            (
                "Responsive field",
                vec![
                    text(FieldPart::Label, "Display name"),
                    boxed(Input::new("Display name", "")),
                ],
            ),
            Some(FieldOrientation::Responsive),
        ),
        (
            (
                "Invalid field",
                vec![
                    text(FieldPart::Label, "Username"),
                    boxed(invalid),
                    text(FieldPart::Error, "Use at least three characters."),
                ],
            ),
            Some(FieldOrientation::Vertical),
        ),
        (
            (
                "Separator",
                vec![text(FieldPart::Separator, "Or continue with")],
            ),
            Some(FieldOrientation::Vertical),
        ),
    ];
    HostedPage::build_fields(tree, parent, rows)
}

pub fn date_picker(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let today = CivilDate::new(2026, 10, 12).expect("the demo date is valid");
    let picker = |name: &str, mode| {
        DatePicker::new(name, env.locale.clone(), today)
            .with_mode(mode)
            .with_placeholder("Pick a date")
            .with_today(today)
    };
    let mut chosen = picker("Due date", CalendarMode::Single);
    chosen.set_dates(&[today]);
    let rows: Vec<Row> = vec![
        (
            "Single date",
            vec![
                boxed(picker("Start date", CalendarMode::Single)),
                boxed(chosen),
            ],
        ),
        (
            "Date range",
            vec![boxed(picker("Stay", CalendarMode::Range))],
        ),
        (
            "Several dates",
            vec![boxed(picker("Meeting days", CalendarMode::Multiple))],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

pub fn kbd(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let keys = |keys: &[&str]| {
        keys.iter()
            .map(|key| (*key).to_string())
            .collect::<Vec<_>>()
    };
    let rows: Vec<Row> = vec![
        (
            "Keys",
            vec![
                boxed(Kbd::new("Esc")),
                boxed(Kbd::new("⇧")),
                boxed(Kbd::group(keys(&["⌘", "K"]))),
                boxed(Kbd::group(keys(&["Ctrl", "Shift", "P"]))),
            ],
        ),
        (
            "Inverted",
            vec![
                boxed(Kbd::new("Enter").with_variant(KbdVariant::Inverted)),
                boxed(Kbd::group(keys(&["⌥", "⌫"])).with_variant(KbdVariant::Inverted)),
            ],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

#[cfg(test)]
mod tests {
    use crate::pages::hosted::HostedPage;
    use crate::pages::tests::Harness;
    use choscorui::core::{Key, NodeId, SemanticAction, SemanticRole};

    fn checked(h: &mut Harness, node: NodeId) -> Option<bool> {
        let page = h.get::<HostedPage>();
        page.host
            .get(node)
            .and_then(|control| control.semantics().state.checked)
    }

    #[test]
    fn switch_page_flips_through_assistive_actions() {
        let mut h = Harness::new("switch");
        let wifi = h.node("Wi-Fi", Some(SemanticRole::Checkbox));
        assert_eq!(checked(&mut h, wifi), Some(true));
        assert!(h.act(wifi, SemanticAction::Activate));
        assert_eq!(checked(&mut h, wifi), Some(false));
    }

    #[test]
    fn toggle_group_page_takes_keys_while_focused() {
        let mut h = Harness::new("toggle-group");
        let group = h.node("Alignment", Some(SemanticRole::RadioGroup));
        assert!(h.act(group, SemanticAction::Focus));
        assert!(h.press(Key::ArrowRight));
        assert!(h.press(Key::Space));
        let page = h.get::<HostedPage>();
        let value = page
            .host
            .get(group)
            .and_then(|control| control.semantics().value);
        assert_eq!(value.as_deref(), Some("Center"));
        assert!(
            !h.press(Key::Tab),
            "Tab leaves the page's focus order alone"
        );
    }
}
