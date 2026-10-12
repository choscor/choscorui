//! Choosing one of several: radio groups, tabs, combo boxes, and the font
//! chooser.

use crate::ui::page::{Build, PageCx, column};
use choscorui::compose::t;
use choscorui::compose::view::{
    Alignment, AnyView, Canvas, Configured, Cx, Dynamic, Frame, HorizontalAlignment, IntoLive,
    Text, TextContent, VStack, View, ViewExt,
};
use choscorui::controls::{
    self, ComboItem, CompletionMode, ControlKind, ControlMessage, ControlValue, FontChooser,
    TabsVariant, text_key,
};
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
/// `disabled` unavailable. The name and options are catalog keys.
fn radios(
    cx: &mut Cx<'_>,
    name: &'static str,
    options: [&'static str; 3],
    disabled: Option<usize>,
) -> impl View {
    let selected = cx.state(0usize);
    let group = Configured::try_custom(ControlKind::RadioGroup, move |env| {
        let options = options.map(|option| env.text(option, &[]));
        let group = controls::RadioGroup::hosted(env.text(name, &[]), options, 0)?;
        match disabled {
            Some(index) => group.with_disabled(index),
            None => Ok(group),
        }
    })
    .value(move || ControlValue::Index(Some(selected.get())))
    .on_message(move |message| {
        if let ControlMessage::Changed(ControlValue::Index(Some(index))) = message {
            selected.set(*index);
        }
    });
    captioned(hidden_caption(t!(name)), group)
}

/// Two radio groups, one with a disabled option.
struct RadioPage;

impl View for RadioPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        column((
            radios(
                cx,
                "radio.paper_size",
                ["radio.a4", "radio.letter", "radio.legal"],
                None,
            ),
            radios(
                cx,
                "radio.quality",
                ["radio.draft", "radio.normal", "radio.best"],
                Some(2),
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

/// A tab list in `variant` over the panel of the selected tab; the last
/// tab is disabled.
fn tab_set(cx: &mut Cx<'_>, variant: TabsVariant) -> impl View {
    let selected = cx.state(0usize);
    let tabs = Configured::try_custom(ControlKind::Tabs, move |env| {
        let labels = TAB_CONTENT.map(|(label, _)| env.text(label, &[]));
        controls::Tabs::hosted(env.text("tabs.settings", &[]), labels, 0)?
            .with_variant(variant)
            .with_disabled(2)
    })
    .value(move || ControlValue::Index(Some(selected.get())))
    .on_message(move |message| {
        if let ControlMessage::Changed(ControlValue::Index(Some(index))) = message {
            selected.set(*index);
        }
    });
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
    VStack::new((tabs, panel))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S2)
}

/// Tabs in the default and line looks, with three panels each; the last
/// tab is disabled.
struct TabsPage;

impl View for TabsPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        column((
            tab_set(cx, TabsVariant::Default),
            tab_set(cx, TabsVariant::Line),
        ))
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

fn city_items() -> Vec<ComboItem> {
    CITIES
        .iter()
        .map(|text| ComboItem {
            key: text_key(text),
            text: (*text).to_owned(),
        })
        .collect()
}

/// A city combobox labeled by catalog key `label`; the read-only one
/// starts on Huế. The caption names the selected city.
fn city_combo(
    cx: &mut Cx<'_>,
    label: &'static str,
    editable: bool,
    mode: CompletionMode,
) -> impl View {
    let start = (!editable).then(|| CITIES.iter().position(|city| *city == "Huế"));
    let selected = cx.state(start.flatten());
    let combo = Configured::custom(ControlKind::Combobox, move |env| {
        let mut combo = controls::Combobox::new(env.text(label, &[]), editable);
        combo.set_completion_mode(mode);
        combo.set_items(city_items());
        if !editable {
            combo.select(Some(text_key("Huế")));
        }
        combo
    })
    .on_message(move |message| {
        if let ControlMessage::Changed(ControlValue::Index(index)) = message {
            selected.set(*index);
        }
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

/// A read-only combo, an editable prefix-completing combo, and an editable
/// contains-matching combo.
struct ComboboxPage;

impl View for ComboboxPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        column((
            city_combo(cx, "combobox.read_only", false, CompletionMode::Prefix),
            city_combo(cx, "combobox.prefix", true, CompletionMode::Prefix),
            city_combo(cx, "combobox.contains", true, CompletionMode::Contains),
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
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        // The chosen family's index; `None` while the typed family is
        // missing and the fallback renders.
        let chosen = cx.state(FAMILIES.iter().position(|family| *family == "Georgia"));
        let chooser = Configured::custom(ControlKind::Combobox, |_| {
            FontChooser::new(
                FAMILIES.iter().map(|family| (*family).to_owned()).collect(),
                "Georgia",
                FALLBACK,
            )
        })
        .on_message(move |message| {
            if let ControlMessage::Changed(ControlValue::Index(index)) = message {
                chosen.set(*index);
            }
        })
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
