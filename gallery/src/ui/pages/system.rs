//! Desktop integration: status items, URL handlers, and notifications,
//! offered only where the backend reports support.

use crate::ui::page::{Build, PageCx, column, section};
use choscorui::compose::t;
use choscorui::compose::view::{
    Alignment, AnyView, Badge, Binding, Button, Cx, Env, ForEach, Frame, HStack, Localized, State,
    Switch, Text, VStack, VerticalAlignment, View, ViewExt,
};
use choscorui::controls::{BadgeVariant, ButtonVariant};
use choscorui::core::{
    AnnouncementPriority, BackendCapabilities, DesktopEvent, DesktopListener, MenuCommandId,
    MenuEntry, MenuItemState, MenuSpec, NotificationDelivery, ServiceError, Services, Space,
    StatusItemSpec, Support, SystemNotification, TextPreset,
};
use std::rc::Rc;

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

/// The backend's capability report plus controls for the desktop
/// features it supports; unsupported ones are disabled, with the reason
/// in a tooltip.
struct DesktopPage;

/// The status item menu command that sends a notification.
const NOTIFY: MenuCommandId = MenuCommandId(1);
/// The status item menu command that removes the item.
const REMOVE: MenuCommandId = MenuCommandId(2);

/// The page's status item: the app name, a tooltip, and a menu.
fn status_item(env: &Env) -> StatusItemSpec {
    let text = |key: &str| env.text(key, &[]);
    let command = |id, key: &str| MenuEntry::command(id, MenuItemState::new(text(key)));
    StatusItemSpec::new(text("app.name"))
        .tooltip(text("desktop.status_item_tooltip"))
        .menu(MenuSpec::new(
            text("app.name"),
            vec![
                command(NOTIFY, "desktop.send_notification"),
                command(REMOVE, "desktop.remove_status_item"),
            ],
        ))
}

/// The text of `key` with the error of `result`, or of `done`.
fn outcome(env: &Env, result: Result<&str, &ServiceError>, failed: &str) -> String {
    match result {
        Ok(done) => env.text(done, &[]),
        Err(error) => env.text(failed, &[("error", &error.to_string())]),
    }
}

/// Shows or removes the status item and describes the outcome.
fn show_status_item(env: &Env, services: &Services, show: bool) -> String {
    let done = if show {
        "desktop.status_item_shown"
    } else {
        "desktop.status_item_removed"
    };
    let result = services.set_status_item(show.then(|| status_item(env)));
    outcome(
        env,
        result.as_ref().map(|()| done),
        "desktop.status_item_failed",
    )
}

/// Posts the demo notification and describes the outcome.
fn notify(env: &Env, services: &Services) -> String {
    let app = env.text("app.name", &[]);
    let body = env.text("desktop.notification_body", &[]);
    let notification = SystemNotification::new("com.choscor.gallery.demo", &app, &body);
    let result = services.post_notification(&notification);
    let done = result.as_ref().map(|delivery| match delivery {
        NotificationDelivery::Posted => "desktop.notification_sent",
        NotificationDelivery::AwaitingPermission => "desktop.notification_waiting",
    });
    outcome(env, done, "desktop.notification_failed")
}

/// Names the application that opens https links.
fn url_handler(env: &Env, services: &Services) -> String {
    match services.url_handler("https://example.com") {
        Ok(Some(handler)) => env.text("desktop.url_handler", &[("app", &handler)]),
        Ok(None) => env.text("desktop.no_url_handler", &[]),
        Err(error) => env.text(
            "desktop.url_handler_failed",
            &[("error", &error.to_string())],
        ),
    }
}

/// Shows `text` in the result line and also speaks it.
fn report(env: &Env, last: State<String>, text: String) {
    env.announcer()
        .announce(text.clone(), AnnouncementPriority::Medium);
    last.set(text);
}

/// Disables `view` when `support` is not `Supported`, and shows why in a
/// tooltip.
fn offered(view: impl View, support: Support, env: &Env, backend: &str) -> AnyView {
    if support.is_supported() {
        return view.into_any();
    }
    let reason = match support {
        Support::Conditional(condition) => condition.message(&env.localizer()),
        _ => env.text("desktop.unsupported_reason", &[("backend", backend)]),
    };
    view.disabled(true).tooltip(reason).into_any()
}

impl View for DesktopPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let env = cx.env().clone();
        let services = env.services().clone();
        let capabilities = services.capabilities();
        let backend = capabilities.backend;
        let last = cx.state(env.text("desktop.backend", &[("backend", backend)]));
        let shown = cx.state(services.status_item().is_some());
        // Answers that arrive later, and the status item menu.
        let listener: DesktopListener = Rc::new({
            let (env, services) = (env.clone(), services.clone());
            move |event: &DesktopEvent| {
                let text = match event {
                    DesktopEvent::NotificationPosted { result, .. } => {
                        let done = result.as_ref().map(|()| "desktop.notification_sent");
                        outcome(&env, done, "desktop.notification_failed")
                    }
                    DesktopEvent::NotificationActivated { .. } => {
                        env.text("desktop.notification_clicked", &[])
                    }
                    DesktopEvent::StatusMenu(NOTIFY) => notify(&env, &services),
                    DesktopEvent::StatusMenu(REMOVE) => {
                        let text = show_status_item(&env, &services, false);
                        shown.set(services.status_item().is_some());
                        text
                    }
                    _ => return,
                };
                report(&env, last, text);
            }
        });
        services.listen(&listener);
        let status = Switch::new(
            t!("desktop.status_item"),
            Binding::new(move || shown.get(), {
                let (env, services) = (env.clone(), services.clone());
                move |show| {
                    let text = show_status_item(&env, &services, show);
                    // The switch shows whether the item really shows.
                    shown.set(services.status_item().is_some());
                    report(&env, last, text);
                }
            }),
        );
        let button = |label: &'static str, action: fn(&Env, &Services) -> String| {
            let (env, services) = (env.clone(), services.clone());
            Button::new(t!(label), move || {
                report(&env, last, action(&env, &services));
            })
            .variant(ButtonVariant::Outline)
        };
        column((
            HStack::new((
                offered(status, capabilities.status_items, &env, backend),
                offered(
                    button("desktop.who_opens", url_handler),
                    capabilities.open_urls,
                    &env,
                    backend,
                ),
                offered(
                    button("desktop.send_notification", notify),
                    capabilities.notifications,
                    &env,
                    backend,
                ),
            ))
            .spacing(Space::S3)
            .alignment(VerticalAlignment::Center),
            // The line keeps the listener while the page shows.
            Text::new(move || {
                let _ = &listener;
                last.get()
            })
            .text_style(TextPreset::Muted),
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
    use super::{NOTIFY, REMOVE};
    use crate::ui::pages::tests::Harness;
    use choscorui::controls::ShortcutModifiers;
    use choscorui::core::{
        BackendCapabilities, DesignTokens, DesktopEvent, NotificationPermission, PointerEvent,
        SemanticAction, SemanticRole, ServiceBackend, ServiceError, Services, StatusItemSpec,
        Support, SystemNotification,
    };
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Whether a tooltip with `text` shows after the pointer rests on the
    /// node named `name`.
    fn tooltip(page: &mut Harness, name: &str, text: &str) -> bool {
        let node = page.node(name);
        let center = page.host.tree().bounds(node).unwrap().center();
        page.host.pointer(
            PointerEvent::Move { position: center },
            ShortcutModifiers::default(),
        );
        page.host.advance(3.0);
        page.host.layout();
        page.host
            .find(text)
            .and_then(|node| page.host.semantic(node))
            .is_some_and(|node| node.role == SemanticRole::Tooltip)
    }

    #[test]
    fn unsupported_desktop_features_are_disabled_with_the_reason() {
        let mut page = Harness::new("desktop");
        assert!(page.host.find("Backend: headless").is_some());
        let status = page.node("Status item in the menu bar");
        let node = page.host.semantic(status).unwrap();
        assert!(node.state.disabled);
        assert!(!page.act("Status item in the menu bar", SemanticAction::Activate));
        assert!(tooltip(
            &mut page,
            "Send a notification",
            "The headless backend does not support this"
        ));
    }

    /// A desktop whose permission answer and events the test sets.
    #[derive(Default)]
    struct Desk {
        permission: RefCell<Option<NotificationPermission>>,
        posted: RefCell<usize>,
        item: RefCell<Option<StatusItemSpec>>,
        events: RefCell<Vec<DesktopEvent>>,
    }

    impl ServiceBackend for Desk {
        fn capabilities(&self) -> BackendCapabilities {
            let mut capabilities = BackendCapabilities::none("test");
            capabilities.notifications = Support::Supported;
            capabilities.status_items = Support::Supported;
            capabilities
        }

        fn notification_permission(&self) -> Result<NotificationPermission, ServiceError> {
            Ok(self
                .permission
                .borrow()
                .unwrap_or(NotificationPermission::NotRequested))
        }

        fn request_notification_permission(&self) -> Result<(), ServiceError> {
            *self.permission.borrow_mut() = Some(NotificationPermission::Pending);
            Ok(())
        }

        fn post_notification(&self, _: &SystemNotification) -> Result<(), ServiceError> {
            *self.posted.borrow_mut() += 1;
            Ok(())
        }

        fn set_status_item(&self, item: Option<&StatusItemSpec>) -> Result<(), ServiceError> {
            *self.item.borrow_mut() = item.cloned();
            Ok(())
        }

        fn take_events(&self) -> Vec<DesktopEvent> {
            self.events.borrow_mut().drain(..).collect()
        }
    }

    #[test]
    fn desktop_page_posts_after_permission_and_answers_the_status_menu() {
        let desk = Rc::new(Desk::default());
        let services = Services::new(desk.clone());
        let mut env = Harness::env("en");
        env.set_services(services.clone());
        let mut page = Harness::with_env("desktop", DesignTokens::light(), env, true);
        let switch = "Status item in the menu bar";
        let checked = |page: &mut Harness| {
            let node = page.node(switch);
            page.host.semantic(node).unwrap().state.checked
        };
        // The application loop polls the services, then the host runs
        // the reactions.
        let poll = |page: &mut Harness| {
            services.poll();
            page.host.flush();
            page.host.layout();
        };
        assert_eq!(checked(&mut page), Some(false));
        assert!(page.act(switch, SemanticAction::Activate));
        assert_eq!(checked(&mut page), Some(true));
        let item = desk.item.borrow().clone().expect("the item shows");
        assert_eq!(
            item.tooltip.as_deref(),
            Some("The status item of the Desktop page")
        );
        assert_eq!(item.menu.expect("a menu").entries.len(), 2);

        assert!(page.act("Send a notification", SemanticAction::Activate));
        assert!(
            page.host
                .find("The notification waits for permission")
                .is_some()
        );
        poll(&mut page);
        assert_eq!(*desk.posted.borrow(), 0, "nothing posts before the answer");
        *desk.permission.borrow_mut() = Some(NotificationPermission::Granted);
        poll(&mut page);
        assert_eq!(*desk.posted.borrow(), 1);
        assert!(page.host.find("Notification sent").is_some());

        desk.events
            .borrow_mut()
            .push(DesktopEvent::StatusMenu(NOTIFY));
        poll(&mut page);
        assert_eq!(*desk.posted.borrow(), 2, "the menu sends one too");
        desk.events
            .borrow_mut()
            .push(DesktopEvent::StatusMenu(REMOVE));
        poll(&mut page);
        assert!(desk.item.borrow().is_none());
        assert_eq!(
            checked(&mut page),
            Some(false),
            "the switch follows the item"
        );
        assert!(page.host.find("Status item removed").is_some());
    }
}
