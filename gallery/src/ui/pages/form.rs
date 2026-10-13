//! Form components: switches, toggles, selects, native selects, text
//! inputs, code inputs, fields, date pickers, range sliders, and keyboard
//! keys.

use crate::ui::page::{Build, PageCx, column, row, section, size_key, stateful};
use choscorui::compose::t;
use choscorui::compose::view::{
    AnyView, Cx, DatePicker, Env, Field, FieldGroup, FieldSeparator, FieldSet, Frame,
    HorizontalAlignment, Input, InputGroup, InputOtp, Kbd, RangeSlider, Select, Switch, Textarea,
    Toggle, ToggleGroup, VStack, View, ViewExt,
};
use choscorui::controls::{
    self, AddonAlign, CalendarMode, ControlSize, FieldOrientation, InputGroupAddon, KbdVariant,
    ListEntry, Orientation, ToggleItem, ToggleVariant, ValidationState,
};
use choscorui::core::{CivilDate, Icon, Space};

/// The catalog string `key` in `env`.
fn text(env: &Env, key: &str) -> String {
    env.text(key, &[])
}

fn switch(cx: &mut Cx, key: &'static str, on: bool, size: ControlSize) -> Switch {
    Switch::new(t!(key), cx.state(on).binding()).with(move |switch| switch.with_size(size))
}

pub fn switch_page(_: &PageCx) -> AnyView {
    stateful(|cx| {
        column((
            section(
                t!("switch.default"),
                row((
                    switch(cx, "switch.airplane", false, ControlSize::Default),
                    switch(cx, "switch.wifi", true, ControlSize::Default),
                )),
            ),
            section(
                t!("switch.small"),
                row((
                    switch(cx, "switch.bluetooth", false, ControlSize::Sm),
                    switch(cx, "switch.hotspot", true, ControlSize::Sm),
                )),
            ),
        ))
    })
}

/// The toggle variants with the catalog keys of their names.
const TOGGLE_VARIANTS: [(ToggleVariant, &str); 2] = [
    (ToggleVariant::Default, "toggle.default"),
    (ToggleVariant::Outline, "toggle.outline"),
];

/// The sizes of a toggle and of a toggle group.
const TOGGLE_SIZES: [ControlSize; 3] = [ControlSize::Sm, ControlSize::Default, ControlSize::Lg];

pub fn toggle(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let line = |cx: &mut Cx, variant: ToggleVariant| {
            let toggles: Vec<_> = TOGGLE_SIZES
                .into_iter()
                .zip([
                    ("toggle.bold", Icon::Bold),
                    ("toggle.italic", Icon::Italic),
                    ("toggle.underline", Icon::Underline),
                ])
                .enumerate()
                .map(|(index, (size, (key, icon)))| {
                    Toggle::new(t!(key), cx.state(index == 1).binding()).with(move |toggle| {
                        toggle.with_icon(icon).with_variant(variant).with_size(size)
                    })
                })
                .collect();
            row(toggles)
        };
        let lines = TOGGLE_VARIANTS
            .into_iter()
            .map(|(variant, key)| section(t!(key), line(cx, variant)).into_any())
            .collect::<Vec<_>>();
        column(lines)
    })
}

fn alignment(env: &Env) -> Vec<ToggleItem> {
    vec![
        ToggleItem::new(text(env, "toggle-group.start")).with_icon(Icon::TextAlignStart),
        ToggleItem::new(text(env, "toggle-group.center")).with_icon(Icon::TextAlignCenter),
        ToggleItem::new(text(env, "toggle-group.end")).with_icon(Icon::TextAlignEnd),
    ]
}

pub fn toggle_group(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let single = ToggleGroup::single(
            t!("toggle-group.alignment"),
            alignment,
            cx.state(Some(0)).binding(),
        )
        .with(|group| group.with_variant(ToggleVariant::Outline));
        let multiple = ToggleGroup::multiple(
            t!("toggle-group.text_style"),
            |env: &Env| {
                vec![
                    ToggleItem::new(text(env, "toggle.bold")).with_icon(Icon::Bold),
                    ToggleItem::new(text(env, "toggle.italic")).with_icon(Icon::Italic),
                    ToggleItem::new(text(env, "toggle.underline")).with_icon(Icon::Underline),
                    ToggleItem::new(text(env, "toggle-group.strikethrough")).disabled(),
                ]
            },
            cx.state(vec![0, 2]).binding(),
        )
        .with(|group| group.with_spacing(4.0));
        let small = ToggleGroup::single(
            t!("toggle-group.small"),
            alignment,
            cx.state(None).binding(),
        )
        .with(|group| group.with_size(ControlSize::Sm));
        let vertical = ToggleGroup::single(
            t!("toggle-group.vertical"),
            alignment,
            cx.state(None).binding(),
        )
        .with(|group| {
            group
                .with_variant(ToggleVariant::Outline)
                .with_orientation(Orientation::Vertical)
        });
        let grid = TOGGLE_VARIANTS
            .into_iter()
            .map(|(variant, key)| {
                let groups = TOGGLE_SIZES
                    .into_iter()
                    .map(|size| {
                        ToggleGroup::single(t!(size_key(size)), alignment, cx.state(None).binding())
                            .with(move |group| group.with_variant(variant).with_size(size))
                    })
                    .collect::<Vec<_>>();
                // One group per line: three side by side overflow with long labels.
                section(
                    t!(key),
                    VStack::new(groups)
                        .alignment(HorizontalAlignment::Leading)
                        .spacing(Space::S2),
                )
            })
            .collect::<Vec<_>>();
        column((
            section(t!("toggle-group.single"), single),
            section(t!("toggle-group.multiple"), multiple),
            section(t!("toggle-group.small_vertical"), row((small, vertical))),
            VStack::new(grid)
                .alignment(HorizontalAlignment::Leading)
                .spacing(Space::S4),
        ))
    })
}

fn fruits(env: &Env) -> Vec<ListEntry> {
    vec![
        ListEntry::group(text(env, "select.fruits")),
        ListEntry::option(text(env, "select.apple")),
        ListEntry::option(text(env, "select.banana")),
        ListEntry::option(text(env, "select.blueberry")),
        ListEntry::disabled(text(env, "select.grapes")),
        ListEntry::option(text(env, "select.pineapple")),
        ListEntry::Separator,
        ListEntry::group(text(env, "select.vegetables")),
        ListEntry::option(text(env, "select.carrot")),
        ListEntry::option(text(env, "select.leek")),
    ]
}

/// The catalog strings of the time zones.
const ZONES: [&str; 12] = [
    "select.zone_eastern",
    "select.zone_central",
    "select.zone_mountain",
    "select.zone_pacific",
    "select.zone_alaska",
    "select.zone_hawaii",
    "select.zone_gmt",
    "select.zone_cet",
    "select.zone_eet",
    "select.zone_india",
    "select.zone_china",
    "select.zone_japan",
];

pub fn select(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let fruit = |cx: &mut Cx, key: &'static str, selected: Option<usize>| {
            let placeholder = text(cx.env(), "select.fruit_placeholder");
            Select::new(t!(key), fruits, cx.state(selected).binding())
                .with(move |select| select.with_placeholder(placeholder.clone()))
        };
        let zone_placeholder = text(cx.env(), "select.zone_placeholder");
        let zones = Select::new(
            t!("select.zone"),
            |env: &Env| {
                ZONES
                    .iter()
                    .map(|zone| ListEntry::option(text(env, zone)))
                    .collect()
            },
            cx.state(None).binding(),
        )
        .with(move |select| select.with_placeholder(zone_placeholder.clone()));
        column((
            section(
                t!("switch.default"),
                row((
                    fruit(cx, "select.fruit", None),
                    fruit(cx, "select.fruit", Some(2)),
                )),
            ),
            section(
                t!("switch.small"),
                fruit(cx, "select.small_fruit", None)
                    .with(|select| select.with_size(ControlSize::Sm)),
            ),
            section(
                t!("select.invalid"),
                fruit(cx, "select.required_fruit", None).with(|mut select| {
                    select.set_invalid(true);
                    select
                }),
            ),
            section(t!("select.scrolling"), zones),
        ))
    })
}

/// The status options of the native selects.
fn statuses(env: &Env) -> Vec<ListEntry> {
    vec![
        ListEntry::option(text(env, "select.todo")),
        ListEntry::option(text(env, "select.in_progress")),
        ListEntry::option(text(env, "select.done")),
    ]
}

/// A status select with the native look, named by the catalog string
/// `key`.
fn native_select(cx: &mut Cx, key: &'static str, size: ControlSize, invalid: bool) -> impl View {
    Select::native(t!(key), statuses, cx.state(None).binding()).with_env(move |select, env| {
        let mut select = select
            .with_placeholder(text(env, "select.status_placeholder"))
            .with_size(size);
        select.set_invalid(invalid);
        select
    })
}

/// The native look of `Select`: default and small, then invalid.
pub fn native_select_page(_: &PageCx) -> AnyView {
    stateful(|cx| {
        column((
            section(
                t!("switch.default"),
                native_select(cx, "select.status", ControlSize::Default, false),
            ),
            section(
                t!("switch.small"),
                native_select(cx, "select.small_status", ControlSize::Sm, false),
            ),
            section(
                t!("select.invalid"),
                native_select(cx, "select.required_status", ControlSize::Default, true),
            ),
        ))
    })
}

/// The invalid username sample of the input and field pages.
fn invalid_username(cx: &mut Cx) -> impl View {
    let error = text(cx.env(), "input.username_error");
    Input::new(t!("input.username"), cx.state("a".to_owned()).binding()).with(move |mut input| {
        input.set_validation(ValidationState::Invalid(error.clone()));
        input
    })
}

/// An empty input named by the catalog string `key`, with `placeholder`.
fn input(cx: &mut Cx, key: &'static str, placeholder: &'static str) -> impl View {
    Input::new(t!(key), cx.state(String::new()).binding())
        .with(move |input| input.with_placeholder(placeholder))
}

pub fn input_page(_: &PageCx) -> AnyView {
    stateful(|cx| {
        column((
            section(
                t!("input.text"),
                input(cx, "input.email", "name@example.com"),
            ),
            section(
                t!("input.password"),
                Input::password(
                    t!("input.password"),
                    cx.state("hunter2".to_owned()).binding(),
                ),
            ),
            section(t!("select.invalid"), invalid_username(cx)),
        ))
    })
}

/// An input group named by the catalog string `key` that starts with
/// `value`, with the placeholder `placeholder` and `addons` from the
/// environment.
fn input_group(
    cx: &mut Cx,
    key: &'static str,
    value: &str,
    placeholder: Option<&'static str>,
    addons: impl Fn(&Env) -> Vec<InputGroupAddon> + 'static,
) -> InputGroup {
    InputGroup::new(t!(key), addons, cx.state(value.to_owned()).binding()).with_env(
        move |group, env| match placeholder {
            Some(placeholder) => group.with_placeholder(text(env, placeholder)),
            None => group,
        },
    )
}

pub fn input_group_page(_: &PageCx) -> AnyView {
    stateful(input_groups)
}

/// Input groups with inline, text, button, and block addons, then an
/// invalid one.
fn input_groups(cx: &mut Cx) -> impl View + use<> {
    column((
        section(
            t!("input-group.inline"),
            input_group(
                cx,
                "input-group.search",
                "",
                Some("input-group.search_placeholder"),
                |env| {
                    let results = env.plural("input-group.results", 12, &[("count", "12")]);
                    vec![
                        InputGroupAddon::icon(AddonAlign::InlineStart, Icon::Search),
                        InputGroupAddon::text(AddonAlign::InlineEnd, results),
                    ]
                },
            ),
        ),
        section(
            t!("input-group.text_buttons"),
            row((
                input_group(cx, "input-group.website", "example", None, |_| {
                    vec![
                        InputGroupAddon::text(AddonAlign::InlineStart, "https://"),
                        InputGroupAddon::text(AddonAlign::InlineEnd, ".com"),
                    ]
                }),
                input_group(cx, "input-group.link", "https://choscor.com", None, |env| {
                    vec![
                        InputGroupAddon::button(
                            AddonAlign::InlineEnd,
                            text(env, "input-group.copy"),
                            Some(Icon::Copy),
                            ControlSize::IconXs,
                        ),
                        InputGroupAddon::button(
                            AddonAlign::InlineEnd,
                            text(env, "input-group.share"),
                            None,
                            ControlSize::Xs,
                        ),
                    ]
                }),
            )),
        ),
        section(
            t!("input-group.block"),
            input_group(
                cx,
                "input-group.message",
                "",
                Some("input-group.ask"),
                |env| {
                    vec![
                        InputGroupAddon::text(AddonAlign::BlockStart, text(env, "input-group.to")),
                        InputGroupAddon::button(
                            AddonAlign::BlockEnd,
                            text(env, "input-group.send"),
                            Some(Icon::Send),
                            ControlSize::Sm,
                        ),
                    ]
                },
            ),
        ),
        section(
            t!("select.invalid"),
            input_group(cx, "input.email", "ana@", None, |env| {
                vec![InputGroupAddon::button(
                    AddonAlign::InlineEnd,
                    text(env, "input-group.clear"),
                    Some(Icon::X),
                    ControlSize::IconSm,
                )]
            })
            .with_env(|mut group, env| {
                group.set_validation(ValidationState::Invalid(text(
                    env,
                    "input-group.email_error",
                )));
                group
            }),
        ),
    ))
}

pub fn textarea(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let message = text(cx.env(), "textarea.placeholder");
        let bio = text(cx.env(), "textarea.bio_placeholder");
        column((
            section(
                t!("textarea.grows"),
                Textarea::new(t!("input-group.message"), cx.state(String::new()).binding())
                    .with(move |textarea| textarea.with_placeholder(message.clone())),
            ),
            section(
                t!("select.invalid"),
                Textarea::new(t!("textarea.bio"), cx.state(String::new()).binding()).with(
                    move |mut textarea| {
                        textarea.set_invalid(true);
                        textarea.with_placeholder(bio.clone())
                    },
                ),
            ),
        ))
    })
}

pub fn input_otp(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let code = |cx: &mut Cx, key: &'static str| {
            InputOtp::new(t!(key), 6, cx.state(String::new()).binding())
        };
        column((
            section(t!("input-otp.six"), code(cx, "input-otp.code")),
            section(
                t!("input-otp.groups"),
                code(cx, "input-otp.grouped").with(|otp| otp.with_groups(vec![3, 3])),
            ),
            section(
                t!("input-otp.letters"),
                code(cx, "input-otp.recovery")
                    .with(|otp| otp.with_groups(vec![2, 2, 2]).alphanumeric()),
            ),
            section(
                t!("select.invalid"),
                code(cx, "input-otp.invalid").with(|mut otp| {
                    otp.set_invalid(true);
                    otp
                }),
            ),
        ))
    })
}

/// The widest a field gets; its parts wrap to it.
const FIELD_WIDTH: f64 = 480.0;

/// `field` no wider than [`FIELD_WIDTH`].
fn narrow(field: impl View) -> impl View {
    field.frame(Frame::new().max_width(FIELD_WIDTH))
}

pub fn field(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let username = cx.state("a".to_owned());
        column((
            section(
                t!("field.set"),
                narrow(
                    FieldSet::new(
                        t!("field.payment"),
                        FieldGroup::new(Field::new(
                            t!("field.name_on_card"),
                            input(cx, "field.name_on_card", "Evil Rabbit"),
                        )),
                    )
                    .description(t!("field.secure")),
                ),
            ),
            section(
                t!("field.vertical"),
                narrow(
                    Field::new(
                        t!("input.email"),
                        input(cx, "input.email", "name@example.com"),
                    )
                    .description(t!("field.never_share")),
                ),
            ),
            section(
                t!("field.horizontal"),
                narrow(
                    Field::new(
                        t!("field.marketing"),
                        switch(cx, "field.marketing", true, ControlSize::Default),
                    )
                    .description(t!("field.receive"))
                    .orientation(FieldOrientation::Horizontal),
                ),
            ),
            // The responsive field takes the page width, so a narrow
            // window stacks it.
            section(
                t!("field.responsive"),
                Field::new(
                    t!("field.display_name"),
                    Input::new(t!("field.display_name"), cx.state(String::new()).binding()),
                )
                .orientation(FieldOrientation::Responsive),
            ),
            section(
                t!("field.invalid"),
                narrow(
                    Field::new(
                        t!("input.username"),
                        Input::new(t!("input.username"), username.binding()),
                    )
                    .error(t!("field.username_error"), move || {
                        username.get().chars().count() < 3
                    }),
                ),
            ),
            section(
                t!("field.separator"),
                narrow(FieldSeparator::new().text(t!("field.continue"))),
            ),
        ))
    })
}

pub fn date_picker(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let today = CivilDate::new(2026, 10, 12).expect("the demo date is valid");
        let placeholder = text(cx.env(), "date-picker.placeholder");
        let pick = move |picker: controls::DatePicker| {
            picker
                .with_placeholder(placeholder.clone())
                .with_today(today)
        };
        let single = |cx: &mut Cx, key: &'static str, date: Option<CivilDate>| {
            DatePicker::new(t!(key), today, cx.state(date).binding()).with(pick.clone())
        };
        let dates = |cx: &mut Cx, key: &'static str, mode: CalendarMode| {
            DatePicker::dates(t!(key), today, mode, cx.state(Vec::new()).binding())
                .with(pick.clone())
        };
        column((
            section(
                t!("date-picker.single"),
                row((
                    single(cx, "date-picker.start", None),
                    single(cx, "date-picker.due", Some(today)),
                )),
            ),
            section(
                t!("date-picker.range"),
                dates(cx, "date-picker.stay", CalendarMode::Range),
            ),
            section(
                t!("date-picker.several"),
                dates(cx, "date-picker.meeting_days", CalendarMode::Multiple),
            ),
            section(
                t!("select.invalid"),
                single(cx, "date-picker.required", None).with(|picker| picker.with_invalid(true)),
            ),
        ))
    })
}

pub fn range_slider(_: &PageCx) -> AnyView {
    stateful(|cx| {
        column((
            section(
                t!("range-slider.horizontal"),
                RangeSlider::new(t!("range-slider.price"), cx.state((25.0, 75.0)).binding())
                    .range(0.0..=100.0)
                    .step(5.0),
            ),
            section(
                t!("toggle-group.vertical"),
                RangeSlider::new(
                    t!("range-slider.temperature"),
                    cx.state((18.0, 24.0)).binding(),
                )
                .range(0.0..=40.0)
                .step(1.0)
                .with(|slider| slider.with_orientation(Orientation::Vertical)),
            ),
        ))
    })
}

/// Key caps for a combination of keys.
fn keys(keys: &[&str]) -> Kbd {
    Kbd::keys(keys.iter().map(|key| (*key).to_owned()).collect::<Vec<_>>())
}

pub fn kbd(_: &PageCx) -> AnyView {
    let inverted = |kbd: controls::Kbd| kbd.with_variant(KbdVariant::Inverted);
    column((
        section(
            t!("kbd.keys"),
            row((
                Kbd::new("Esc"),
                Kbd::new("⇧"),
                keys(&["⌘", "K"]),
                keys(&["Ctrl", "Shift", "P"]),
            )),
        ),
        section(
            t!("kbd.inverted"),
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
        "native-select" => native_select_page,
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
    fn the_invalid_field_clears_its_error_once_the_name_is_long_enough() {
        const ERROR: &str = "Use at least three characters.";
        let mut page = Harness::new("field");
        assert!(page.host.find(ERROR).is_some());
        let (snapshot, _) = page.host.semantics();
        let input = snapshot
            .iter()
            .find(|entry| entry.node.name == "Username" && entry.node.state.invalid)
            .map(|entry| entry.id)
            .expect("the username input is invalid");
        assert_eq!(page.host.semantic(input).unwrap().description, ERROR);
        page.host.focus(Some(input));
        assert!(page.host.text("bcd"));
        page.host.layout();
        assert!(page.host.find(ERROR).is_none());
        assert!(!page.host.semantic(input).unwrap().state.invalid);
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
        let required = page.node("Required fruit");
        assert!(page.host.semantic(required).unwrap().state.invalid);

        let mut page = Harness::new("native-select");
        for name in ["Status", "Small status"] {
            assert_eq!(
                page.value(name).as_deref(),
                Some("Select a status"),
                "{name}"
            );
        }
        let required = page.node("Required status");
        assert!(page.host.semantic(required).unwrap().state.invalid);
    }
}
