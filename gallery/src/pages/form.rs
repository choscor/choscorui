//! Form components mounted through `ControlHost`: switches, toggles,
//! toggle groups, and keyboard keys.

use crate::page::{Env, Page};
use crate::pages::hosted::{HostedPage, Row};
use choscorui::controls::{
    ControlSize, CustomControl, Kbd, KbdVariant, Orientation, Switch, Toggle, ToggleGroup,
    ToggleItem, ToggleVariant,
};
use choscorui::core::{Icon, NodeId, Tree};

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
