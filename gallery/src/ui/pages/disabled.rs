//! Guards that each page shows the public control it documents, and a
//! disabled instance of it.

use super::tests::Harness;
use choscorui::controls::ControlKind;
use choscorui::core::NodeId;

/// Each page and the kind of control that it shows disabled.
const PAGES: &[(&str, ControlKind)] = &[
    ("switch", ControlKind::Switch),
    ("toggle", ControlKind::Toggle),
    ("accordion", ControlKind::Accordion),
    ("alert", ControlKind::Alert),
    ("attachment", ControlKind::Attachment),
    ("breadcrumb", ControlKind::Breadcrumb),
    ("button-recipes", ControlKind::Button),
    ("calendar", ControlKind::Calendar),
    ("carousel", ControlKind::Carousel),
    ("color-picker", ControlKind::ColorPicker),
    ("command", ControlKind::Command),
    ("context-menu", ControlKind::Menu),
    ("custom-control", ControlKind::Slider),
    ("date-picker", ControlKind::DatePicker),
    ("decimal-spin-box", ControlKind::Input),
    ("dock", ControlKind::Dock),
    ("empty", ControlKind::Empty),
    ("font-chooser", ControlKind::Combobox),
    ("font-dialog", ControlKind::Card),
    ("input-group", ControlKind::InputGroup),
    ("input-otp", ControlKind::InputOtp),
    ("item", ControlKind::Item),
    ("kbd", ControlKind::Kbd),
    ("key-sequence", ControlKind::Input),
    ("menubar", ControlKind::Menubar),
    ("navigation-menu", ControlKind::NavigationMenu),
    ("pagination", ControlKind::Pagination),
    ("questionnaire", ControlKind::Questionnaire),
    ("range-slider", ControlKind::Slider),
    ("sidebar", ControlKind::Sidebar),
    ("spinner", ControlKind::Spinner),
    ("text-editor", ControlKind::TextEditor),
    ("textarea", ControlKind::Textarea),
    ("tool-box", ControlKind::Accordion),
    ("tool-button", ControlKind::Toggle),
    ("tooltip", ControlKind::Button),
];

/// Every node under `node`, depth first.
fn walk(harness: &Harness, node: NodeId, nodes: &mut Vec<NodeId>) {
    nodes.push(node);
    let children = harness
        .host
        .tree()
        .children(node)
        .unwrap_or_default()
        .to_vec();
    for child in children {
        walk(harness, child, nodes);
    }
}

#[test]
fn every_listed_page_shows_its_control_and_a_disabled_one() {
    let mut missing = Vec::new();
    for (page, kind) in PAGES {
        let harness = Harness::new(page);
        let mut nodes = Vec::new();
        walk(&harness, harness.host.tree().root(), &mut nodes);
        let controls: Vec<_> = nodes
            .into_iter()
            .filter(|node| harness.host.control_kind(*node) == Some(*kind))
            .collect();
        if controls.is_empty() {
            missing.push(format!("{page}: no {}", kind.name()));
        } else if controls
            .iter()
            .all(|node| harness.host.controls().is_enabled(*node))
        {
            missing.push(format!("{page}: no disabled {}", kind.name()));
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}
