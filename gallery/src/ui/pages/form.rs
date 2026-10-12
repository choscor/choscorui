//! Form components: switches, toggles, selects, text inputs, code inputs,
//! fields, date pickers, range sliders, and keyboard keys.

use crate::ui::page::{Build, PageCx, column, row, section, stateful};
use choscorui::compose::view::{
    AnyView, Configured, Cx, DatePicker, FieldText, Frame, HStack, HorizontalAlignment, Input,
    InputOtp, Kbd, RangeSlider, Select, Switch, Textarea, Toggle, ToggleGroup, VStack, View,
    ViewExt,
};
use choscorui::controls::{
    self, AddonAlign, CalendarMode, ControlKind, ControlSize, FieldPart, InputGroup,
    InputGroupAddon, KbdVariant, ListEntry, Orientation, TextField, ToggleItem, ToggleVariant,
    ValidationState,
};
use choscorui::core::{CivilDate, Icon, Space};

fn switch(cx: &mut Cx<'_>, label: &'static str, on: bool, size: ControlSize) -> impl View {
    Switch::new(label, cx.state(on).binding()).with(move |switch| switch.with_size(size))
}

pub fn switch_page(_: &PageCx) -> AnyView {
    stateful(|cx| {
        column((
            section(
                "Default size",
                row((
                    switch(cx, "Airplane mode", false, ControlSize::Default),
                    switch(cx, "Wi-Fi", true, ControlSize::Default),
                )),
            ),
            section(
                "Small size",
                row((
                    switch(cx, "Bluetooth", false, ControlSize::Sm),
                    switch(cx, "Hotspot", true, ControlSize::Sm),
                )),
            ),
        ))
    })
}

pub fn toggle(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let line = |cx: &mut Cx<'_>, variant: ToggleVariant| {
            let toggles: Vec<_> = [ControlSize::Sm, ControlSize::Default, ControlSize::Lg]
                .into_iter()
                .zip([
                    ("Bold", Icon::Bold),
                    ("Italic", Icon::Italic),
                    ("Underline", Icon::Underline),
                ])
                .enumerate()
                .map(|(index, (size, (label, icon)))| {
                    Toggle::new(label, cx.state(index == 1).binding()).with(move |toggle| {
                        toggle.with_icon(icon).with_variant(variant).with_size(size)
                    })
                })
                .collect();
            row(toggles)
        };
        column((
            section("Default variant", line(cx, ToggleVariant::Default)),
            section("Outline variant", line(cx, ToggleVariant::Outline)),
        ))
    })
}

fn alignment() -> Vec<ToggleItem> {
    vec![
        ToggleItem::new("Start").with_icon(Icon::TextAlignStart),
        ToggleItem::new("Center").with_icon(Icon::TextAlignCenter),
        ToggleItem::new("End").with_icon(Icon::TextAlignEnd),
    ]
}

pub fn toggle_group(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let single = ToggleGroup::single("Alignment", alignment(), cx.state(Some(0)).binding())
            .with(|group| group.with_variant(ToggleVariant::Outline));
        let multiple = ToggleGroup::multiple(
            "Text style",
            vec![
                ToggleItem::new("Bold").with_icon(Icon::Bold),
                ToggleItem::new("Italic").with_icon(Icon::Italic),
                ToggleItem::new("Underline").with_icon(Icon::Underline),
                ToggleItem::new("Strikethrough").disabled(),
            ],
            cx.state(vec![0, 2]).binding(),
        )
        .with(|group| group.with_spacing(4.0));
        let small = ToggleGroup::single("Small", alignment(), cx.state(None).binding())
            .with(|group| group.with_size(ControlSize::Sm));
        let vertical =
            ToggleGroup::single("Vertical", alignment(), cx.state(None).binding()).with(|group| {
                group
                    .with_variant(ToggleVariant::Outline)
                    .with_orientation(Orientation::Vertical)
            });
        column((
            section("Single selection, joined", single),
            section("Multiple selection, spaced", multiple),
            section("Small and vertical", row((small, vertical))),
        ))
    })
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

const ZONES: [&str; 12] = [
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

pub fn select(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let fruit = |cx: &mut Cx<'_>, name: &'static str, selected: Option<usize>| {
            Select::new(name, fruits(), cx.state(selected).binding())
                .with(|select| select.with_placeholder("Select a fruit"))
        };
        let native = Configured::custom(ControlKind::NativeSelect, |_| {
            controls::Select::native(
                "Status",
                vec![
                    ListEntry::option("Todo"),
                    ListEntry::option("In progress"),
                    ListEntry::option("Done"),
                ],
            )
            .with_placeholder("Select a status")
        });
        let zones = Select::new(
            "Time zone",
            ZONES
                .iter()
                .map(|zone| ListEntry::option(*zone))
                .collect::<Vec<_>>(),
            cx.state(None).binding(),
        )
        .with(|select| select.with_placeholder("Select a time zone"));
        column((
            section(
                "Default size",
                row((fruit(cx, "Fruit", None), fruit(cx, "Fruit", Some(2)))),
            ),
            section(
                "Small size",
                fruit(cx, "Small fruit", None).with(|select| select.with_size(ControlSize::Sm)),
            ),
            section(
                "Invalid",
                fruit(cx, "Required fruit", None).with(|mut select| {
                    select.set_invalid(true);
                    select
                }),
            ),
            section("Native look", native),
            section("Scrolling list", zones),
        ))
    })
}

/// The invalid username sample of the input and field pages.
fn invalid_username(cx: &mut Cx<'_>) -> impl View {
    Input::new("Username", cx.state("a".to_owned()).binding()).with(|mut input| {
        input.set_validation(ValidationState::Invalid(
            "Use at least three characters".into(),
        ));
        input
    })
}

/// An empty input with `placeholder`.
fn input(cx: &mut Cx<'_>, name: &'static str, placeholder: &'static str) -> impl View {
    Input::new(name, cx.state(String::new()).binding())
        .with(move |input| input.with_placeholder(placeholder))
}

pub fn input_page(_: &PageCx) -> AnyView {
    stateful(|cx| {
        column((
            section("Text", input(cx, "Email", "name@example.com")),
            section(
                "Password",
                Input::password("Password", cx.state("hunter2".to_owned()).binding()),
            ),
            section("Invalid", invalid_username(cx)),
        ))
    })
}

/// A text field with addons.
fn input_group(field: impl Fn() -> TextField + 'static, addons: Vec<InputGroupAddon>) -> impl View {
    Configured::custom(ControlKind::InputGroup, move |_| {
        InputGroup::new(field(), addons.clone())
    })
}

pub fn input_group_page(_: &PageCx) -> AnyView {
    column((
        section(
            "Inline addons",
            input_group(
                || TextField::new("Search", "").with_placeholder("Search…"),
                vec![
                    InputGroupAddon::icon(AddonAlign::InlineStart, Icon::Search),
                    InputGroupAddon::text(AddonAlign::InlineEnd, "12 results"),
                ],
            ),
        ),
        section(
            "Text and buttons",
            row((
                input_group(
                    || TextField::new("Website", "example"),
                    vec![
                        InputGroupAddon::text(AddonAlign::InlineStart, "https://"),
                        InputGroupAddon::text(AddonAlign::InlineEnd, ".com"),
                    ],
                ),
                input_group(
                    || TextField::new("Link", "https://choscor.com"),
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
                ),
            )),
        ),
        section(
            "Block addons",
            input_group(
                || TextField::new("Message", "").with_placeholder("Ask anything"),
                vec![
                    InputGroupAddon::text(AddonAlign::BlockStart, "To: Design team"),
                    InputGroupAddon::button(
                        AddonAlign::BlockEnd,
                        "Send",
                        Some(Icon::Send),
                        ControlSize::Sm,
                    ),
                ],
            ),
        ),
    ))
}

pub fn textarea(_: &PageCx) -> AnyView {
    stateful(|cx| {
        column((
            section(
                "Grows with its text",
                Textarea::new("Message", cx.state(String::new()).binding())
                    .with(|textarea| textarea.with_placeholder("Type your message here.")),
            ),
            section(
                "Invalid",
                Textarea::new("Bio", cx.state(String::new()).binding()).with(|mut textarea| {
                    textarea.set_invalid(true);
                    textarea.with_placeholder("Tell us about yourself")
                }),
            ),
        ))
    })
}

pub fn input_otp(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let code = |cx: &mut Cx<'_>, name: &'static str| {
            InputOtp::new(name, 6, cx.state(String::new()).binding())
        };
        column((
            section("Six digits", code(cx, "Code")),
            section(
                "Groups with a separator",
                code(cx, "Grouped code").with(|otp| otp.with_groups(vec![3, 3])),
            ),
            section(
                "Letters and digits",
                code(cx, "Recovery code").with(|otp| otp.with_groups(vec![2, 2, 2]).alphanumeric()),
            ),
            section(
                "Invalid",
                code(cx, "Invalid code").with(|mut otp| {
                    otp.set_invalid(true);
                    otp
                }),
            ),
        ))
    })
}

/// The widest a field gets; its parts wrap to it.
const FIELD_WIDTH: f64 = 480.0;

/// Parts stacked with the field's item gap, each filling the field width.
fn stacked(parts: impl View) -> impl View {
    VStack::new(parts)
        .alignment(HorizontalAlignment::Fill)
        .spacing(Space::S3)
        .frame(Frame::new().max_width(FIELD_WIDTH))
}

/// A lead part with the other parts stacked beside it, as a horizontal
/// field places them.
fn beside(lead: impl View, rest: impl View) -> impl View {
    HStack::new((
        lead,
        // The column takes the rest of the width, and its text wraps.
        VStack::new(rest)
            .alignment(HorizontalAlignment::Fill)
            .spacing(Space::S1)
            .layout_priority(1.0),
    ))
    .spacing(Space::S3)
    .frame(Frame::new().max_width(FIELD_WIDTH))
}

pub fn field(_: &PageCx) -> AnyView {
    stateful(|cx| {
        column((
            section(
                "Field set",
                stacked((
                    FieldText::new(FieldPart::Legend, "Payment method"),
                    FieldText::new(
                        FieldPart::Description,
                        "All transactions are secure and encrypted.",
                    ),
                    FieldText::new(FieldPart::Label, "Name on card"),
                    input(cx, "Name on card", "Evil Rabbit"),
                )),
            ),
            section(
                "Vertical field",
                stacked((
                    FieldText::new(FieldPart::Label, "Email"),
                    input(cx, "Email", "name@example.com"),
                    FieldText::new(
                        FieldPart::Description,
                        "We never share your email with anyone.",
                    ),
                )),
            ),
            section(
                "Horizontal field",
                beside(
                    switch(cx, "Marketing emails", true, ControlSize::Default),
                    FieldText::new(
                        FieldPart::Description,
                        "Receive emails about new products and features.",
                    ),
                ),
            ),
            section(
                "Responsive field",
                beside(
                    FieldText::new(FieldPart::Label, "Display name"),
                    Input::new("Display name", cx.state(String::new()).binding()),
                ),
            ),
            section(
                "Invalid field",
                stacked((
                    FieldText::new(FieldPart::Label, "Username"),
                    invalid_username(cx),
                    FieldText::new(FieldPart::Error, "Use at least three characters."),
                )),
            ),
            section(
                "Separator",
                stacked(FieldText::new(FieldPart::Separator, "Or continue with")),
            ),
        ))
    })
}

pub fn date_picker(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let today = CivilDate::new(2026, 10, 12).expect("the demo date is valid");
        let single = |cx: &mut Cx<'_>, name: &'static str, date: Option<CivilDate>| {
            DatePicker::new(name, today, cx.state(date).binding())
                .with(move |picker| picker.with_placeholder("Pick a date").with_today(today))
        };
        let dates = |cx: &mut Cx<'_>, name: &'static str, mode: CalendarMode| {
            DatePicker::dates(name, today, mode, cx.state(Vec::new()).binding())
                .with(move |picker| picker.with_placeholder("Pick a date").with_today(today))
        };
        column((
            section(
                "Single date",
                row((
                    single(cx, "Start date", None),
                    single(cx, "Due date", Some(today)),
                )),
            ),
            section("Date range", dates(cx, "Stay", CalendarMode::Range)),
            section(
                "Several dates",
                dates(cx, "Meeting days", CalendarMode::Multiple),
            ),
        ))
    })
}

pub fn range_slider(_: &PageCx) -> AnyView {
    stateful(|cx| {
        column((
            section(
                "Horizontal",
                RangeSlider::new("Price", 0.0..=100.0, 5.0, cx.state((25.0, 75.0)).binding()),
            ),
            section(
                "Vertical",
                RangeSlider::new(
                    "Temperature",
                    0.0..=40.0,
                    1.0,
                    cx.state((18.0, 24.0)).binding(),
                )
                .with(|slider| slider.with_orientation(Orientation::Vertical)),
            ),
        ))
    })
}

/// Key caps for a combination of keys.
fn keys(keys: &'static [&'static str]) -> Configured<controls::Kbd> {
    Configured::custom(ControlKind::Kbd, move |_| {
        controls::Kbd::group(keys.iter().map(|key| (*key).to_owned()))
    })
}

pub fn kbd(_: &PageCx) -> AnyView {
    let inverted = |kbd: controls::Kbd| kbd.with_variant(KbdVariant::Inverted);
    column((
        section(
            "Keys",
            row((
                Kbd::new("Esc"),
                Kbd::new("⇧"),
                keys(&["⌘", "K"]),
                keys(&["Ctrl", "Shift", "P"]),
            )),
        ),
        section(
            "Inverted",
            row((
                Kbd::new("Enter").with(inverted),
                keys(&["⌥", "⌫"]).with(inverted),
            )),
        ),
    ))
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "switch" => switch_page,
        "toggle" => toggle,
        "toggle-group" => toggle_group,
        "select" => select,
        "date-picker" => date_picker,
        "input" => input_page,
        "input-group" => input_group_page,
        "textarea" => textarea,
        "field" => field,
        "input-otp" => input_otp,
        "kbd" => kbd,
        "range-slider" => range_slider,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::controls::ShortcutModifiers;
    use choscorui::core::{Key, KeyEvent, SemanticAction};

    fn checked(page: &mut Harness, name: &str) -> Option<bool> {
        let node = page.node(name);
        page.host.semantic(node).and_then(|node| node.state.checked)
    }

    fn press(page: &mut Harness, key: Key) -> bool {
        let handled = page.host.key(
            KeyEvent::Down { key, repeat: false },
            ShortcutModifiers::default(),
        );
        page.host.layout();
        handled
    }

    #[test]
    fn switch_page_flips_through_assistive_actions() {
        let mut page = Harness::new("switch");
        assert_eq!(checked(&mut page, "Wi-Fi"), Some(true));
        assert_eq!(checked(&mut page, "Airplane mode"), Some(false));
        assert!(page.act("Wi-Fi", SemanticAction::Activate));
        assert_eq!(checked(&mut page, "Wi-Fi"), Some(false));
    }

    #[test]
    fn toggle_group_page_takes_keys_while_focused() {
        let mut page = Harness::new("toggle-group");
        assert_eq!(page.value("Alignment").as_deref(), Some("Start"));
        assert!(page.act("Alignment", SemanticAction::Focus));
        assert!(press(&mut page, Key::ArrowRight));
        assert!(press(&mut page, Key::Space));
        assert_eq!(page.value("Alignment").as_deref(), Some("Center"));
    }

    #[test]
    fn invalid_samples_report_their_state() {
        let mut page = Harness::new("input");
        let username = page.node("Username");
        let node = page.host.semantic(username).unwrap();
        assert!(node.state.invalid);
        assert_eq!(node.value.as_deref(), Some("a"));
        assert!(page.host.find("Password").is_some());
    }

    #[test]
    fn selects_show_their_starting_choice() {
        let mut page = Harness::new("select");
        assert!(page.host.find("Time zone").is_some());
        assert!(page.host.find("Status").is_some());
        let required = page.node("Required fruit");
        assert!(page.host.semantic(required).unwrap().state.invalid);
    }
}
