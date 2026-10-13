//! Choosing one of several: radio groups, tabs, combo boxes, and the font
//! chooser.

use crate::ui::page::{Build, PageCx, column};
use choscorui::compose::t;
use choscorui::compose::view::{
    Alignment, AnyView, Canvas, Combobox, Cx, Dynamic, Env, FontChooser, Frame, HStack,
    HorizontalAlignment, IntoLive, RadioGroup, Tabs, Text, TextContent, VStack, View, ViewExt,
};
use choscorui::controls::{CompletionMode, ControlMessage, ControlValue, Orientation, TabsVariant};
use choscorui::core::{ColorRole, Point, RadiusScale, Space, TextPreset};

/// A small muted caption. It repeats the name of the control below it, so
/// assistive technology skips it and reads the control.
fn hidden_caption<M>(text: impl IntoLive<TextContent, M>) -> impl View {
    Text::new(text)
        .text_style(TextPreset::Small)
        .foreground(ColorRole::MutedForeground)
        .accessibility_hidden(true)
}

/// A captioned control.
fn captioned(caption: impl View, control: impl View) -> impl View {
    VStack::new((caption, control))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S2)
}

/// A radio group whose first option is selected, with the option at
/// `disabled` unavailable, and invalid when `invalid` is true. The name
/// and options are catalog keys.
fn radios(
    cx: &mut Cx,
    name: &'static str,
    options: [&'static str; 3],
    disabled: Option<usize>,
    invalid: bool,
) -> impl View {
    let selected = cx.state(0usize);
    let labels = move |env: &Env| options.map(|option| env.text(option, &[])).to_vec();
    let group = RadioGroup::new(t!(name), labels, selected.binding())
        .with(move |group| group.with_invalid(invalid));
    let group = match disabled {
        Some(index) => group.disabled_option(index),
        None => group,
    };
    captioned(hidden_caption(t!(name)), group)
}

/// Three radio groups: one with a disabled option, and one invalid.
struct RadioPage;

impl View for RadioPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        column((
            radios(
                cx,
                "radio.paper_size",
                ["radio.a4", "radio.letter", "radio.legal"],
                None,
                false,
            ),
            radios(
                cx,
                "radio.quality",
                ["radio.draft", "radio.normal", "radio.best"],
                Some(2),
                false,
            ),
            radios(
                cx,
                "radio.required",
                ["radio.a4", "radio.letter", "radio.legal"],
                None,
                true,
            ),
        ))
    }
}

pub fn radio(_: &PageCx) -> AnyView {
    RadioPage.into_any()
}

/// The catalog keys of each tab's label and of the text of its panel.
const TAB_CONTENT: [(&str, &str); 3] = [
    ("tabs.general", "tabs.general_panel"),
    ("tabs.appearance", "tabs.appearance_panel"),
    ("tabs.advanced", "tabs.advanced_panel"),
];

/// A tab list in `variant` over the panel of the selected tab, or beside
/// it for vertical tabs; the last tab is disabled.
fn tab_set(cx: &mut Cx, variant: TabsVariant, orientation: Orientation) -> AnyView {
    let selected = cx.state(0usize);
    let labels = |env: &Env| TAB_CONTENT.map(|(label, _)| env.text(label, &[])).to_vec();
    let tabs = Tabs::new(t!("tabs.settings"), labels, selected.binding())
        .with(move |tabs| tabs.with_variant(variant).with_orientation(orientation))
        .disabled_tab(2);
    let panel = Text::new(move || t!(TAB_CONTENT[selected.get()].1))
        .padding(Space::S3)
        .frame(
            Frame::new()
                .max_width(f64::INFINITY)
                .height(96.0)
                .alignment(Alignment::TOP_LEADING),
        )
        .background(ColorRole::Card)
        .border(ColorRole::Border, 1.0)
        .corner_radius(RadiusScale::Lg);
    match orientation {
        Orientation::Horizontal => VStack::new((tabs, panel))
            .alignment(HorizontalAlignment::Leading)
            .spacing(Space::S2)
            .into_any(),
        Orientation::Vertical => HStack::new((tabs, panel.layout_priority(1.0)))
            .spacing(Space::S2)
            .into_any(),
    }
}

/// Tabs in the default and line looks, each horizontal and vertical, with
/// three panels each; the last tab is disabled.
struct TabsPage;

impl View for TabsPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let mut sets = Vec::new();
        for orientation in [Orientation::Horizontal, Orientation::Vertical] {
            for variant in TabsVariant::ALL {
                sets.push(tab_set(cx, *variant, orientation));
            }
        }
        column(sets)
    }
}

pub fn tabs(_: &PageCx) -> AnyView {
    TabsPage.into_any()
}

const CITIES: [&str; 8] = [
    "Hà Nội",
    "Huế",
    "Đà Lạt",
    "Hội An",
    "Sa Pa",
    "Hải Phòng",
    "Cần Thơ",
    "Nha Trang",
];

fn city_items() -> Vec<String> {
    CITIES.iter().map(|city| (*city).to_owned()).collect()
}

/// A city combobox labeled by catalog key `label`, invalid when `invalid`
/// is true; the read-only one starts on Huế. The caption names the
/// selected city.
fn city_combo(
    cx: &mut Cx,
    label: &'static str,
    editable: bool,
    mode: CompletionMode,
    invalid: bool,
) -> impl View {
    let start = (!editable).then(|| CITIES.iter().position(|city| *city == "Huế"));
    let selected = cx.state(start.flatten());
    let combo = if editable {
        // The caption follows the chosen city, not the typed text.
        Combobox::editable(t!(label), city_items(), cx.state(String::new()).binding()).on_message(
            move |message| {
                if let ControlMessage::Changed(ControlValue::Index(index)) = message {
                    selected.set(*index);
                }
            },
        )
    } else {
        Combobox::new(t!(label), city_items(), selected.binding())
    }
    .with(move |mut combo| {
        combo.set_completion_mode(mode);
        combo.with_invalid(invalid)
    })
    .frame(Frame::new().width(280.0).alignment(Alignment::FILL));
    let env = cx.env().clone();
    let caption = hidden_caption(move || {
        let city = match selected.get() {
            Some(index) => CITIES[index].to_owned(),
            None => env.text("combobox.none", &[]),
        };
        let label = env.text(label, &[]);
        env.text("combobox.selected", &[("label", &label), ("city", &city)])
    });
    captioned(caption, combo)
}

/// A read-only combo, an editable prefix-completing combo, an editable
/// contains-matching combo, and an invalid combo.
struct ComboboxPage;

impl View for ComboboxPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        column((
            city_combo(
                cx,
                "combobox.read_only",
                false,
                CompletionMode::Prefix,
                false,
            ),
            city_combo(cx, "combobox.prefix", true, CompletionMode::Prefix, false),
            city_combo(
                cx,
                "combobox.contains",
                true,
                CompletionMode::Contains,
                false,
            ),
            city_combo(cx, "combobox.required", true, CompletionMode::Prefix, true),
        ))
    }
}

pub fn combobox(_: &PageCx) -> AnyView {
    ComboboxPage.into_any()
}

/// Font families every page can offer without asking the platform.
const FAMILIES: [&str; 8] = [
    "Helvetica",
    "Times New Roman",
    "Courier New",
    "Georgia",
    "Menlo",
    "Avenir Next",
    "Gill Sans",
    "Futura",
];

const FALLBACK: &str = "Helvetica";

/// The font chooser with a preview in the resolved family.
struct FontChooserPage;

impl View for FontChooserPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        // The chosen family's index; `None` while the typed family is
        // missing and the fallback renders.
        let chosen = cx.state(FAMILIES.iter().position(|family| *family == "Georgia"));
        let families: Vec<String> = FAMILIES.iter().map(|family| (*family).to_owned()).collect();
        let chooser = FontChooser::new(families, FALLBACK, chosen.binding())
            .frame(Frame::new().width(280.0).alignment(Alignment::FILL));
        let family = move || chosen.get().map_or(FALLBACK, |index| FAMILIES[index]);
        let env = cx.env().clone();
        let preview = Dynamic::new(move || {
            let family = family();
            let sample = env.text("font-chooser.sample", &[]);
            Canvas::new(move |canvas, _| {
                let _ = canvas.list.draw_text(
                    Point::new(canvas.bounds.origin.x + 12.0, canvas.bounds.origin.y + 40.0),
                    &sample,
                    family,
                    24.0,
                    canvas.tokens.color(ColorRole::Foreground),
                );
            })
            .label(env.text("font-chooser.preview", &[("family", family)]))
            .frame(Frame::new().max_width(f64::INFINITY).height(64.0))
            .background(ColorRole::Card)
            .border(ColorRole::Border, 1.0)
            .corner_radius(RadiusScale::Lg)
        });
        let note = Text::new(move || match chosen.get() {
            Some(_) => t!("font-chooser.rendering", family = family()),
            None => t!("font-chooser.missing", family = FALLBACK),
        })
        .text_style(TextPreset::Small)
        .foreground(ColorRole::MutedForeground);
        column((chooser, preview, note))
    }
}

pub fn font_chooser(_: &PageCx) -> AnyView {
    FontChooserPage.into_any()
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "radio" => radio,
        "tabs" => tabs,
        "combobox" => combobox,
        "font-chooser" => font_chooser,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::core::{Key, KeyEvent, SemanticAction};

    fn press(page: &mut Harness, key: Key) {
        page.host
            .key(KeyEvent::Down { key, repeat: false }, Default::default());
        page.host.layout();
    }

    #[test]
    fn radio_groups_select_with_arrows_and_skip_the_disabled_option() {
        let mut page = Harness::new("radio");
        assert_eq!(page.value("Paper size").as_deref(), Some("A4"));
        page.act("Paper size", SemanticAction::Increment);
        assert_eq!(page.value("Paper size").as_deref(), Some("Letter"));
        let quality = page.node("Quality");
        assert!(!page.host.perform(
            quality,
            SemanticAction::SetValue,
            Some("Best (unavailable)")
        ));
        page.act("Quality", SemanticAction::Focus);
        press(&mut page, Key::ArrowDown);
        assert_eq!(page.value("Quality").as_deref(), Some("Normal"));
        // The disabled option is skipped; the selection wraps around.
        press(&mut page, Key::ArrowDown);
        assert_eq!(page.value("Quality").as_deref(), Some("Draft"));
    }

    #[test]
    fn a_tab_shows_its_panel_and_the_disabled_tab_stays_closed() {
        let mut page = Harness::new("tabs");
        assert_eq!(page.value("Settings").as_deref(), Some("General"));
        assert!(
            page.host
                .find("Name, icon, and startup behavior.")
                .is_some()
        );
        page.act("Settings", SemanticAction::Increment);
        assert_eq!(page.value("Settings").as_deref(), Some("Appearance"));
        assert!(
            page.host
                .find("Theme, accent color, and font size.")
                .is_some()
        );
        // The disabled tab is skipped; the selection wraps around.
        page.act("Settings", SemanticAction::Increment);
        assert_eq!(page.value("Settings").as_deref(), Some("General"));
        assert!(page.host.find("Hidden settings.").is_none());
    }

    #[test]
    fn combos_complete_typed_text_and_choose_cities() {
        let mut page = Harness::new("combobox");
        assert_eq!(page.value("City (read-only)").as_deref(), Some("Huế"));
        let prefix = page.node("City (editable, prefix completion)");
        assert!(
            page.host
                .perform(prefix, SemanticAction::SetValue, Some("Đà Lạt"))
        );
        page.host.layout();
        assert_eq!(
            page.value("City (editable, prefix completion)").as_deref(),
            Some("Đà Lạt")
        );
        page.act("City (editable, contains match)", SemanticAction::Focus);
        assert!(page.host.text("An"));
        page.host.layout();
        assert_eq!(
            page.value("City (editable, contains match)").as_deref(),
            Some("An")
        );
    }

    #[test]
    fn a_missing_family_previews_in_the_fallback() {
        let mut page = Harness::new("font-chooser");
        assert!(page.host.find("Preview in Georgia").is_some());
        assert!(page.host.find("Rendering with Georgia").is_some());
        let font = page.node("Font");
        assert!(
            page.host
                .perform(font, SemanticAction::SetValue, Some("Menlo"))
        );
        page.host.layout();
        assert!(page.host.find("Preview in Menlo").is_some());
        assert!(
            page.host
                .perform(font, SemanticAction::SetValue, Some("Comic Sans"))
        );
        page.host.layout();
        let node = page.host.semantic(font).unwrap();
        assert!(node.state.invalid, "{node:?}");
        assert!(page.host.find("Preview in Helvetica").is_some());
        assert!(
            page.host
                .find("Missing family; rendering with the fallback Helvetica")
                .is_some()
        );
    }
}
