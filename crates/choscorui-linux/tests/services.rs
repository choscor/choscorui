//! The desktop services against a private `dbus-daemon` with fakes of the
//! portal, the notification server, and the status notifier watcher.

#![cfg(target_os = "linux")]

#[allow(dead_code, reason = "each test binary uses a part of the shared fakes")]
mod support;

use choscorui_core::{
    Condition, DesktopEvent, FileDialogKind, FileDialogRequest, MenuCommandId, MenuEntry,
    MenuItemState, MenuSpec, Service, ServiceBackend, ServiceError, StatusItemSpec, Support,
    SystemNotification, user_settings_path,
};
use choscorui_linux::dbus::{Connection, Error, Message, Value};
use choscorui_linux::services::{
    FileDialogResponse, NativeServices, NotificationRoute, ServiceOptions,
};
use std::path::PathBuf;
use std::time::Duration;
use support::bus::TestBus;
use support::notifications::FakeNotifications;
use support::portal::{FakePortal, PortalScript};
use support::watcher::FakeWatcher;
use support::{in_child, pump_until, run_child};

const APP_ID: &str = "org.choscorui.ServicesTest";

fn options(flatpak: bool) -> ServiceOptions {
    ServiceOptions {
        app_id: APP_ID.to_owned(),
        flatpak,
    }
}

fn services(bus: &TestBus, flatpak: bool) -> NativeServices {
    NativeServices::with_connection(Some(bus.connect()), options(flatpak))
}

/// Calls a method of the services from `desktop`, as a shell would,
/// and turns both loops until the reply comes.
fn desktop_call(
    services: &NativeServices,
    desktop: &Connection,
    destination: &str,
    path: &str,
    interface: &str,
    member: &str,
    args: &[Value],
) -> Result<Message, Error> {
    let message = Message::method_call(Some(destination), path, Some(interface), member)
        .unwrap()
        .with(args)
        .unwrap();
    let pending = desktop.call_async(message, Duration::from_secs(5)).unwrap();
    let connection = services.connection().unwrap();
    pump_until(&[connection, desktop], || desktop.take_reply(pending))
}

/// Takes the events of `services`, turning its loop until there are some.
fn next_events(services: &NativeServices) -> Vec<DesktopEvent> {
    let connection = services.connection().unwrap();
    pump_until(&[connection], || {
        let events = services.take_events();
        (!events.is_empty()).then_some(events)
    })
}

fn settings() -> Vec<(String, String, Value)> {
    let appearance = "org.freedesktop.appearance";
    let gnome = "org.gnome.desktop.interface";
    vec![
        (appearance.into(), "color-scheme".into(), Value::from(1u32)),
        (appearance.into(), "contrast".into(), Value::from(0u32)),
        (
            gnome.into(),
            "text-scaling-factor".into(),
            Value::Double(1.25),
        ),
        (gnome.into(), "enable-animations".into(), Value::from(false)),
        (
            gnome.into(),
            "font-name".into(),
            Value::from("Cantarell 11"),
        ),
    ]
}

#[test]
fn without_a_bus_every_bus_service_is_unsupported() {
    let services = NativeServices::with_connection(None, options(false));
    let capabilities = services.capabilities();
    assert_eq!(capabilities.notifications, Support::Unsupported);
    assert_eq!(capabilities.status_items, Support::Unsupported);
    assert_eq!(capabilities.system_appearance, Support::Unsupported);
    assert_eq!(capabilities.native_dialogs, Support::Unsupported);
    assert_eq!(capabilities.clipboard, Support::Unsupported);
    assert_eq!(capabilities.open_urls, Support::Supported);
    let notification = SystemNotification::new("a", "Title", "Body");
    assert_eq!(
        services.post_notification(&notification),
        Err(ServiceError::Unsupported(Service::Notifications))
    );
    assert_eq!(
        services.set_status_item(Some(&StatusItemSpec::new("Item"))),
        Err(ServiceError::Unsupported(Service::StatusItem))
    );
    assert_eq!(
        services.choose_files(&FileDialogRequest::new(FileDialogKind::Open)),
        Err(ServiceError::Unsupported(Service::FileDialogs))
    );
    assert_eq!(services.is_primary_instance(), None);
    assert!(!services.system_prefers_dark());
    assert!(services.take_events().is_empty());
}

#[test]
fn focus_changes_and_launch_arguments_become_desktop_events() {
    let services = NativeServices::with_connection(None, options(false));
    services
        .open_launch_arguments(["--flag", "/tmp/a.txt", "https://example.com/"].map(Into::into));
    services.set_active(true);
    services.set_active(true);
    services.set_active(false);
    assert_eq!(
        services.take_events(),
        vec![
            DesktopEvent::OpenFiles(vec![PathBuf::from("/tmp/a.txt")]),
            DesktopEvent::OpenUrls(vec!["https://example.com/".to_owned()]),
            DesktopEvent::Activated,
            DesktopEvent::Deactivated,
        ]
    );
}

#[test]
fn an_empty_bus_offers_only_the_conditional_status_item() {
    let bus = TestBus::start();
    let services = services(&bus, false);
    let capabilities = services.capabilities();
    assert_eq!(capabilities.notifications, Support::Unsupported);
    assert_eq!(
        capabilities.status_items,
        Support::Conditional(Condition::StatusNotifierHost)
    );
    assert_eq!(capabilities.system_appearance, Support::Unsupported);
    assert_eq!(capabilities.native_dialogs, Support::Unsupported);
    assert_eq!(services.notification_route(), None);
    assert_eq!(services.is_primary_instance(), Some(true));
}

#[test]
fn server_notifications_replace_by_identifier_and_report_clicks() {
    let bus = TestBus::start();
    let server = FakeNotifications::start(bus.address());
    let services = services(&bus, false);
    assert_eq!(services.capabilities().notifications, Support::Supported);
    assert_eq!(
        services.notification_route(),
        Some(NotificationRoute::Server)
    );
    let first = SystemNotification::new("build", "Build", "Running");
    services.post_notification(&first).unwrap();
    let again = SystemNotification::new("build", "Build", "Done");
    services.post_notification(&again).unwrap();
    services
        .post_notification(&SystemNotification::new("other", "Other", ""))
        .unwrap();
    let notifies = server.service.calls_of("Notify");
    assert_eq!(notifies.len(), 3);
    assert_eq!(notifies[0].arg(0).as_str(), Some(APP_ID));
    assert_eq!(notifies[0].arg(1).as_u32(), Some(0));
    assert_eq!(notifies[0].arg(3).as_str(), Some("Build"));
    assert_eq!(notifies[1].arg(1).as_u32(), Some(1), "the same id replaces");
    assert_eq!(notifies[1].arg(4).as_str(), Some("Done"));
    assert_eq!(notifies[2].arg(1).as_u32(), Some(0));
    assert_eq!(
        notifies[0]
            .arg(6)
            .get("desktop-entry")
            .and_then(Value::as_str),
        Some(APP_ID)
    );
    server.invoke(1, "default");
    assert_eq!(
        next_events(&services),
        [DesktopEvent::NotificationActivated {
            identifier: "build".to_owned(),
            action: "default".to_owned(),
        }]
    );
}

#[test]
fn portal_notifications_carry_their_identifier() {
    let bus = TestBus::start();
    let portal = FakePortal::start(bus.address(), PortalScript::default());
    let services = services(&bus, true);
    assert_eq!(
        services.notification_route(),
        Some(NotificationRoute::Portal)
    );
    services
        .post_notification(&SystemNotification::new("mail-7", "Mail", "Hello"))
        .unwrap();
    let call = portal.service.wait_for("AddNotification");
    assert_eq!(call.arg(0).as_str(), Some("mail-7"));
    assert_eq!(
        call.arg(1).get("title").and_then(Value::as_str),
        Some("Mail")
    );
    assert_eq!(
        call.arg(1).get("default-action").and_then(Value::as_str),
        Some("default")
    );
    portal.invoke_notification("mail-7", "default");
    assert_eq!(
        next_events(&services),
        [DesktopEvent::NotificationActivated {
            identifier: "mail-7".to_owned(),
            action: "default".to_owned(),
        }]
    );
}

fn status_spec() -> StatusItemSpec {
    StatusItemSpec::new("Choscor")
        .tooltip("Ready")
        .menu(MenuSpec::new(
            "Status",
            vec![MenuEntry::command(
                MenuCommandId(9),
                MenuItemState::new("Open"),
            )],
        ))
}

#[test]
fn a_status_item_without_a_host_stays_conditional() {
    let bus = TestBus::start();
    let _watcher = FakeWatcher::start(bus.address(), false);
    let services = services(&bus, false);
    assert_eq!(
        services.capabilities().status_items,
        Support::Conditional(Condition::StatusNotifierHost)
    );
}

#[test]
fn a_status_item_registers_and_its_menu_clicks() {
    let bus = TestBus::start();
    let watcher = FakeWatcher::start(bus.address(), true);
    let services = services(&bus, false);
    assert_eq!(services.capabilities().status_items, Support::Supported);
    services.set_status_item(Some(&status_spec())).unwrap();
    let name = services.status_item_name().expect("the item shows");
    let register = watcher.service.wait_for("RegisterStatusNotifierItem");
    assert_eq!(register.arg(0).as_str(), Some(name.as_str()));

    let desktop = bus.connect();
    let title = desktop_call(
        &services,
        &desktop,
        &name,
        "/StatusNotifierItem",
        "org.freedesktop.DBus.Properties",
        "Get",
        &[
            Value::from("org.kde.StatusNotifierItem"),
            Value::from("Title"),
        ],
    )
    .unwrap();
    assert_eq!(
        title.args().unwrap()[0].unvariant().as_str(),
        Some("Choscor")
    );
    let event = desktop_call(
        &services,
        &desktop,
        &name,
        "/MenuBar",
        "com.canonical.dbusmenu",
        "Event",
        &[
            Value::from(1i32),
            Value::from("clicked"),
            Value::variant(Value::from(0i32)),
            Value::from(0u32),
        ],
    );
    assert!(event.is_ok());
    assert_eq!(
        services.take_events(),
        [DesktopEvent::StatusMenu(MenuCommandId(9))]
    );
    services.set_status_item(None).unwrap();
    assert_eq!(services.status_item_name(), None);
}

#[test]
fn urls_open_through_the_portal() {
    let bus = TestBus::start();
    let portal = FakePortal::start(bus.address(), PortalScript::default());
    let services = services(&bus, false);
    services.set_parent_window("x11:2a");
    services.open_url("https://example.com/a?b=c").unwrap();
    let call = portal.service.wait_for("OpenURI");
    assert_eq!(call.arg(0).as_str(), Some("x11:2a"));
    assert_eq!(call.arg(1).as_str(), Some("https://example.com/a?b=c"));
    assert!(call.arg(2).get("handle_token").is_some());
    assert_eq!(
        services.open_url("not a url"),
        Err(choscorui_linux::services::DesktopError::InvalidUrl)
    );
}

#[test]
fn the_appearance_follows_the_portal_settings() {
    let bus = TestBus::start();
    let portal = FakePortal::start(
        bus.address(),
        PortalScript {
            settings: Some(settings()),
            ..PortalScript::default()
        },
    );
    let services = services(&bus, false);
    assert_eq!(
        services.capabilities().system_appearance,
        Support::Supported
    );
    let appearance = services.appearance();
    assert!(appearance.prefers_dark);
    assert!(!appearance.accessibility.high_contrast);
    assert!(appearance.accessibility.reduced_motion);
    assert_eq!(appearance.accessibility.text_scale, 1.25);
    assert_eq!(appearance.font_name.as_deref(), Some("Cantarell 11"));
    assert!(!services.take_appearance_changed());

    portal.change_setting(
        "org.freedesktop.appearance",
        "color-scheme",
        Value::from(2u32),
    );
    let connection = services.connection().unwrap();
    pump_until(&[connection], || {
        services.take_appearance_changed().then_some(())
    });
    assert!(!services.system_prefers_dark());
    portal.change_setting("org.freedesktop.appearance", "contrast", Value::from(1u32));
    portal.change_setting(
        "org.gnome.desktop.interface",
        "text-scaling-factor",
        Value::Double(1.5),
    );
    pump_until(&[connection], || {
        let preferences = services.accessibility_preferences();
        (preferences.high_contrast && preferences.text_scale == 1.5).then_some(())
    });
    assert!(services.take_appearance_changed());
    assert!(!services.take_appearance_changed());
}

#[test]
fn the_file_chooser_returns_the_chosen_paths() {
    let bus = TestBus::start();
    let portal = FakePortal::start(
        bus.address(),
        PortalScript {
            file_chooser: Some((
                0,
                vec![
                    "file:///tmp/a%20b.txt".to_owned(),
                    "file:///tmp/c.txt".to_owned(),
                ],
            )),
            ..PortalScript::default()
        },
    );
    let services = services(&bus, false);
    assert_eq!(services.capabilities().native_dialogs, Support::Supported);
    let mut request = FileDialogRequest::new(FileDialogKind::OpenMany);
    request.allowed_types = vec!["txt".to_owned()];
    assert_eq!(
        services.choose_files(&request).unwrap(),
        [PathBuf::from("/tmp/a b.txt"), PathBuf::from("/tmp/c.txt")]
    );
    let call = portal.service.wait_for("OpenFile");
    assert_eq!(
        call.arg(2).get("multiple").and_then(Value::as_bool),
        Some(true)
    );
}

#[test]
fn a_cancelled_file_chooser_returns_no_paths() {
    let bus = TestBus::start();
    let portal = FakePortal::start(bus.address(), PortalScript::default());
    let services = services(&bus, false);
    let mut request = FileDialogRequest::new(FileDialogKind::Save);
    request.suggested_name = Some("notes.txt".to_owned());
    assert_eq!(
        services.choose_files(&request).unwrap(),
        Vec::<PathBuf>::new()
    );
    let call = portal.service.wait_for("SaveFile");
    assert_eq!(
        call.arg(2).get("current_name").and_then(Value::as_str),
        Some("notes.txt")
    );
}

#[test]
fn without_a_portal_the_file_dialog_falls_back() {
    let bus = TestBus::start();
    let services = services(&bus, false);
    assert_eq!(services.capabilities().native_dialogs, Support::Unsupported);
    services.set_file_dialog_fallback(Some(Box::new(|request| {
        assert_eq!(request.kind, FileDialogKind::Directory);
        Ok(FileDialogResponse::Selected(vec![PathBuf::from("/srv")]))
    })));
    assert_eq!(services.capabilities().native_dialogs, Support::Supported);
    assert_eq!(
        services
            .choose_files(&FileDialogRequest::new(FileDialogKind::Directory))
            .unwrap(),
        [PathBuf::from("/srv")]
    );
}

#[test]
fn the_application_interface_opens_files_and_reopens() {
    let bus = TestBus::start();
    let services = services(&bus, false);
    assert_eq!(services.is_primary_instance(), Some(true));
    let second = NativeServices::with_connection(Some(bus.connect()), options(false));
    assert_eq!(second.is_primary_instance(), Some(false));

    let desktop = bus.connect();
    let path = "/org/choscorui/ServicesTest";
    let interface = "org.freedesktop.Application";
    let platform_data = || Value::vardict(Vec::<(String, Value)>::new());
    desktop_call(
        &services,
        &desktop,
        APP_ID,
        path,
        interface,
        "Open",
        &[
            Value::strings(["file:///tmp/x%20y.txt", "https://example.com"]),
            platform_data(),
        ],
    )
    .unwrap();
    assert_eq!(
        services.take_events(),
        [
            DesktopEvent::OpenFiles(vec![PathBuf::from("/tmp/x y.txt")]),
            DesktopEvent::OpenUrls(vec!["https://example.com".to_owned()]),
        ]
    );
    services.set_has_visible_windows(true);
    desktop_call(
        &services,
        &desktop,
        APP_ID,
        path,
        interface,
        "Activate",
        &[platform_data()],
    )
    .unwrap();
    assert_eq!(
        services.take_events(),
        [DesktopEvent::Reopen {
            has_visible_windows: true
        }]
    );
    let unknown =
        desktop_call(&services, &desktop, APP_ID, path, interface, "Quit", &[]).unwrap_err();
    assert_eq!(
        unknown.name(),
        Some("org.freedesktop.DBus.Error.UnknownMethod")
    );
}

#[test]
fn the_settings_path_follows_xdg_config_home() {
    if in_child() {
        assert_eq!(
            user_settings_path("org.example.App"),
            Some(PathBuf::from("/xdg/config/org.example.App/settings.txt"))
        );
        return;
    }
    run_child(
        "the_settings_path_follows_xdg_config_home",
        &[
            ("XDG_CONFIG_HOME", Some("/xdg/config")),
            ("HOME", Some("/home/u")),
        ],
    );
}

#[test]
fn the_settings_path_falls_back_to_the_home_config() {
    if in_child() {
        assert_eq!(
            user_settings_path("org.example.App"),
            Some(PathBuf::from(
                "/home/u/.config/org.example.App/settings.txt"
            ))
        );
        return;
    }
    run_child(
        "the_settings_path_falls_back_to_the_home_config",
        &[("XDG_CONFIG_HOME", None), ("HOME", Some("/home/u"))],
    );
}
