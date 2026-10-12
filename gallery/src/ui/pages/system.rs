//! Desktop integration: status items, URL handlers, and notifications,
//! offered only where the backend reports support.

use crate::page::Desktop;
use crate::ui::page::{Build, PageCx, column, section};
use choscorui::compose::view::{
    Alignment, AnyView, Button, Cx, ForEach, Frame, HStack, Text, VStack, View, ViewExt,
};
use choscorui::controls::ButtonVariant;
use choscorui::core::{BackendCapabilities, Space, Support, TextPreset};
use std::rc::Rc;

/// Every capability the backend reports, by display name.
pub fn capability_rows(capabilities: &BackendCapabilities) -> Vec<(&'static str, Support)> {
    vec![
        ("Multiple windows", capabilities.multiple_windows),
        ("Native menus", capabilities.native_menus),
        ("Context menus", capabilities.context_menus),
        ("Native dialogs", capabilities.native_dialogs),
        ("Clipboard", capabilities.clipboard),
        ("Drag and drop", capabilities.drag_and_drop),
        ("Input methods", capabilities.input_methods),
        ("Accessibility", capabilities.accessibility),
        ("Status items", capabilities.status_items),
        ("Notifications", capabilities.notifications),
        ("Open URLs", capabilities.open_urls),
        ("System appearance", capabilities.system_appearance),
        ("Text shaping", capabilities.text_shaping),
        ("Image codecs", capabilities.image_codecs),
        ("GPU painting", capabilities.gpu_painting),
    ]
}

fn support_text(support: Support) -> String {
    match support {
        Support::Supported => "supported".into(),
        Support::Unsupported => "not supported".into(),
        Support::Conditional(condition) => format!("needs {condition}"),
    }
}

/// The backend's capability report plus buttons for the desktop features
/// it supports; unsupported ones are disabled with the reason.
struct DesktopPage {
    desktop: Rc<dyn Desktop>,
}

/// What a desktop button does.
#[derive(Clone, Copy)]
enum Action {
    StatusItem,
    Url,
    Notify,
}

fn run(desktop: &dyn Desktop, action: Action) -> String {
    match action {
        Action::StatusItem => match desktop.toggle_status_item("choscorui gallery") {
            Ok(true) => "Status item shown in the menu bar".into(),
            Ok(false) => "Status item removed".into(),
            Err(error) => format!("Status item failed: {error}"),
        },
        Action::Url => match desktop.url_handler("https://example.com") {
            Ok(Some(app)) => format!("https:// links open in {app}"),
            Ok(None) => "No application handles https:// links".into(),
            Err(error) => format!("URL handler lookup failed: {error}"),
        },
        Action::Notify => {
            match desktop.notify("choscorui gallery", "Hello from the Desktop page") {
                Ok(()) => "Notification sent".into(),
                Err(error) => format!("Notification failed: {error}"),
            }
        }
    }
}

impl View for DesktopPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let capabilities = self.desktop.capabilities();
        let last = cx.state(format!("Backend: {}", capabilities.backend));
        let button = |label: &str, action: Action, support: Support| {
            let desktop = self.desktop.clone();
            Button::new(label.to_owned(), move || last.set(run(&*desktop, action)))
                .variant(ButtonVariant::Outline)
                .disabled(!support.is_supported())
                .accessibility_hint(if support.is_supported() {
                    String::new()
                } else {
                    support_text(support)
                })
        };
        column((
            HStack::new((
                button(
                    "Toggle status item",
                    Action::StatusItem,
                    capabilities.status_items,
                ),
                button(
                    "Who opens https:// links?",
                    Action::Url,
                    capabilities.open_urls,
                ),
                button(
                    "Send a notification",
                    Action::Notify,
                    capabilities.notifications,
                ),
            ))
            .spacing(Space::S3),
            Text::new(move || last.get()).text_style(TextPreset::Muted),
            section(
                format!("Capabilities of the {} backend", capabilities.backend),
                VStack::new(ForEach::new(
                    capability_rows(&capabilities),
                    |(name, _)| *name,
                    |(name, support)| {
                        HStack::new((
                            Text::new(*name)
                                .text_style(TextPreset::Small)
                                .frame(Frame::new().width(180.0).alignment(Alignment::LEADING)),
                            Text::new(support_text(*support))
                                .text_style(TextPreset::Small)
                                .opacity(if support.is_supported() { 1.0 } else { 0.5 }),
                        ))
                    },
                ))
                .spacing(Space::S1),
            ),
        ))
    }
}

pub fn desktop(cx: &PageCx) -> AnyView {
    DesktopPage {
        desktop: cx.desktop.clone(),
    }
    .into_any()
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "desktop" => desktop,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::core::SemanticAction;

    #[test]
    fn unsupported_desktop_features_are_disabled_with_the_reason() {
        let mut page = Harness::new("desktop");
        assert!(page.host.find("Backend: headless").is_some());
        let status = page.node("Toggle status item");
        let node = page.host.semantic(status).unwrap();
        assert!(node.state.disabled);
        assert!(!page.act("Toggle status item", SemanticAction::Activate));
        assert_eq!(page.value("Toggle status item"), None);
    }
}
