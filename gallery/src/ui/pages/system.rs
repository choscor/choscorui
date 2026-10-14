//! Desktop integration: status items, URL handlers, and notifications,
//! offered only where the backend reports support.

use crate::ui::page::{Build, PageCx, column, section};
use choscorui::compose::t;
use choscorui::compose::view::{
    Alignment, AnyView, Badge, Button, Cx, Env, ForEach, Frame, HStack, Localized, Text, VStack,
    View, ViewExt,
};
use choscorui::controls::{BadgeVariant, ButtonVariant};
use choscorui::core::{
    AnnouncementPriority, BackendCapabilities, Services, Space, StatusItemSpec, Support,
    SystemNotification, TextPreset,
};

/// Every capability the backend reports, by the catalog key of its
/// display name.
pub fn capability_rows(capabilities: &BackendCapabilities) -> Vec<(&'static str, Support)> {
    vec![
        ("desktop.multiple_windows", capabilities.multiple_windows),
        ("desktop.native_menus", capabilities.native_menus),
        ("desktop.context_menus", capabilities.context_menus),
        ("desktop.native_dialogs", capabilities.native_dialogs),
        ("desktop.clipboard", capabilities.clipboard),
        ("desktop.drag_and_drop", capabilities.drag_and_drop),
        ("desktop.input_methods", capabilities.input_methods),
        ("desktop.accessibility", capabilities.accessibility),
        ("desktop.status_items", capabilities.status_items),
        ("desktop.notifications", capabilities.notifications),
        ("desktop.open_urls", capabilities.open_urls),
        ("desktop.system_appearance", capabilities.system_appearance),
        ("desktop.text_shaping", capabilities.text_shaping),
        ("desktop.image_codecs", capabilities.image_codecs),
        ("desktop.gpu_painting", capabilities.gpu_painting),
    ]
}

/// How well the backend supports a capability. A condition is the
/// backend's own technical term.
fn support_text(support: Support) -> Localized {
    match support {
        Support::Supported => t!("desktop.supported"),
        Support::Unsupported => t!("desktop.unsupported"),
        Support::Conditional(condition) => t!(condition.message_id()),
    }
}

/// The badge look of a support level: filled when supported, outlined
/// when it depends on a condition, and muted when not supported.
fn support_variant(support: Support) -> BadgeVariant {
    match support {
        Support::Supported => BadgeVariant::Default,
        Support::Conditional(_) => BadgeVariant::Outline,
        Support::Unsupported => BadgeVariant::Secondary,
    }
}

/// The backend's capability report plus buttons for the desktop features
/// it supports; unsupported ones are disabled with the reason.
struct DesktopPage;

/// What a desktop button does.
#[derive(Clone, Copy)]
enum Action {
    StatusItem,
    Url,
    Notify,
}

/// Runs `action` and describes the outcome in `env`'s language.
fn run(env: &Env, services: &Services, action: Action) -> String {
    let text = |key: &str| env.text(key, &[]);
    let failed =
        |key: &str, error: &dyn std::fmt::Display| env.text(key, &[("error", &error.to_string())]);
    let app = text("app.name");
    match action {
        Action::StatusItem => {
            let shown = services.status_item().is_some();
            let item = (!shown).then(|| StatusItemSpec::new(app.clone()));
            match services.set_status_item(item) {
                Ok(()) if shown => text("desktop.status_item_removed"),
                Ok(()) => text("desktop.status_item_shown"),
                Err(error) => failed("desktop.status_item_failed", &error),
            }
        }
        Action::Url => match services.url_handler("https://example.com") {
            Ok(Some(handler)) => env.text("desktop.url_handler", &[("app", &handler)]),
            Ok(None) => text("desktop.no_url_handler"),
            Err(error) => failed("desktop.url_handler_failed", &error),
        },
        Action::Notify => {
            let body = text("desktop.notification_body");
            let notification = SystemNotification::new("com.choscor.gallery.demo", &app, &body);
            match services.post_notification(&notification) {
                Ok(()) => text("desktop.notification_sent"),
                Err(error) => failed("desktop.notification_failed", &error),
            }
        }
    }
}

impl View for DesktopPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let env = cx.env().clone();
        let services = env.services().clone();
        let capabilities = services.capabilities();
        let backend = capabilities.backend;
        let last = cx.state(env.text("desktop.backend", &[("backend", backend)]));
        let button = |label: &'static str, action: Action, support: Support| {
            let services = services.clone();
            let (ran, hinted) = (env.clone(), env.clone());
            // The result line is also spoken.
            Button::new(t!(label), move || {
                let result = run(&ran, &services, action);
                ran.announcer()
                    .announce(result.clone(), AnnouncementPriority::Medium);
                last.set(result);
            })
            .variant(ButtonVariant::Outline)
            .disabled(!support.is_supported())
            .accessibility_hint(move || {
                if support.is_supported() {
                    String::new()
                } else {
                    support_text(support).resolve(&hinted)
                }
            })
        };
        column((
            HStack::new((
                button(
                    "desktop.toggle_status_item",
                    Action::StatusItem,
                    capabilities.status_items,
                ),
                button("desktop.who_opens", Action::Url, capabilities.open_urls),
                button(
                    "desktop.send_notification",
                    Action::Notify,
                    capabilities.notifications,
                ),
            ))
            .spacing(Space::S3),
            Text::new(move || last.get()).text_style(TextPreset::Muted),
            section(
                t!("desktop.capabilities", backend = backend),
                VStack::new(ForEach::new(
                    capability_rows(&capabilities),
                    |(name, _)| *name,
                    |(name, support)| {
                        HStack::new((
                            Text::new(t!(*name))
                                .text_style(TextPreset::Small)
                                .frame(Frame::new().width(180.0).alignment(Alignment::LEADING)),
                            Badge::new(support_text(*support)).with({
                                let variant = support_variant(*support);
                                move |badge| badge.with_variant(variant)
                            }),
                        ))
                    },
                ))
                .spacing(Space::S1),
            ),
        ))
    }
}

pub fn desktop(_: &PageCx) -> AnyView {
    DesktopPage.into_any()
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
