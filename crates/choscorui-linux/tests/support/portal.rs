//! A fake `org.freedesktop.portal.Desktop` with `FileChooser`, `OpenURI`,
//! `Settings`, and `Notification`. A request answers with a `Response`
//! signal from its `Request` object.

use super::fake::{Behavior, Call, Emit, FakeService, Object};
use choscorui_linux::dbus::{Reply, Value};

pub const NAME: &str = "org.freedesktop.portal.Desktop";
pub const PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";
const NOTIFICATION: &str = "org.freedesktop.portal.Notification";

/// What the fake answers.
#[derive(Clone, Debug, PartialEq)]
pub struct PortalScript {
    /// The response code and the URIs of each file dialog.
    pub file_chooser: Option<(u32, Vec<String>)>,
    /// The settings, as (namespace, key, value). `None` leaves out the
    /// `Settings` interface.
    pub settings: Option<Vec<(String, String, Value)>>,
}

impl Default for PortalScript {
    /// A portal that answers every file dialog with a cancel and has no
    /// settings.
    fn default() -> Self {
        Self {
            file_chooser: Some((1, Vec::new())),
            settings: None,
        }
    }
}

/// The path of the request object of `call`, from its sender and the
/// `handle_token` of its options.
pub fn request_path(call: &Call) -> String {
    let sender = call.sender.trim_start_matches(':').replace('.', "_");
    let token = call
        .args
        .last()
        .and_then(|options| options.get("handle_token"))
        .and_then(Value::as_str)
        .unwrap_or("t")
        .to_owned();
    format!("{PATH}/request/{sender}/{token}")
}

fn response(path: &str, code: u32, results: Value) -> Emit {
    Emit::new(
        path,
        "org.freedesktop.portal.Request",
        "Response",
        vec![Value::from(code), results],
    )
}

fn behavior(script: PortalScript) -> Behavior {
    Box::new(move |call, emits| {
        let unknown = || Reply::error("org.freedesktop.DBus.Error.UnknownMethod", "no such method");
        match (call.interface.as_str(), call.member.as_str()) {
            ("org.freedesktop.portal.FileChooser", "OpenFile" | "SaveFile") => {
                let Some((code, uris)) = &script.file_chooser else {
                    return unknown();
                };
                let path = request_path(call);
                let results = Value::vardict([("uris", Value::strings(uris.clone()))]);
                emits.push(response(&path, *code, results));
                Reply::Return(vec![Value::object_path(path)])
            }
            ("org.freedesktop.portal.OpenURI", "OpenURI") => {
                let path = request_path(call);
                emits.push(response(
                    &path,
                    0,
                    Value::vardict(Vec::<(String, Value)>::new()),
                ));
                Reply::Return(vec![Value::object_path(path)])
            }
            (SETTINGS, "ReadAll") => {
                let Some(settings) = &script.settings else {
                    return unknown();
                };
                let mut namespaces: Vec<(Value, Value)> = Vec::new();
                for (namespace, key, value) in settings {
                    let entry = (Value::from(key.as_str()), Value::variant(value.clone()));
                    match namespaces
                        .iter_mut()
                        .find(|(name, _)| name.as_str() == Some(namespace))
                    {
                        Some((_, Value::Dict { entries, .. })) => entries.push(entry),
                        _ => namespaces.push((
                            Value::from(namespace.as_str()),
                            Value::Dict {
                                key: "s".to_owned(),
                                value: "v".to_owned(),
                                entries: vec![entry],
                            },
                        )),
                    }
                }
                Reply::Return(vec![Value::Dict {
                    key: "s".to_owned(),
                    value: "a{sv}".to_owned(),
                    entries: namespaces,
                }])
            }
            (SETTINGS, "Read") => {
                let Some(settings) = &script.settings else {
                    return unknown();
                };
                let wanted = (call.arg(0).as_str(), call.arg(1).as_str());
                match settings.iter().find(|(namespace, key, _)| {
                    (Some(namespace.as_str()), Some(key.as_str())) == wanted
                }) {
                    Some((_, _, value)) => {
                        Reply::Return(vec![Value::variant(Value::variant(value.clone()))])
                    }
                    None => {
                        Reply::error("org.freedesktop.portal.Error.NotFound", "no such setting")
                    }
                }
            }
            (NOTIFICATION, "AddNotification" | "RemoveNotification") => Reply::Return(Vec::new()),
            _ => unknown(),
        }
    })
}

/// The fake portal. It records every call.
pub struct FakePortal {
    pub service: FakeService,
}

impl FakePortal {
    pub fn start(address: &str, script: PortalScript) -> Self {
        Self {
            service: FakeService::start(
                address,
                &[NAME],
                vec![Object::new(PATH, behavior(script))],
            ),
        }
    }

    /// Sends `SettingChanged` for one setting.
    pub fn change_setting(&self, namespace: &str, key: &str, value: Value) {
        self.service.emit(Emit::new(
            PATH,
            SETTINGS,
            "SettingChanged",
            vec![
                Value::from(namespace),
                Value::from(key),
                Value::variant(value),
            ],
        ));
    }

    /// Sends `ActionInvoked` for the notification `id`.
    pub fn invoke_notification(&self, id: &str, action: &str) {
        self.service.emit(Emit::new(
            PATH,
            NOTIFICATION,
            "ActionInvoked",
            vec![
                Value::from(id),
                Value::from(action),
                Value::array("v", Vec::new()),
            ],
        ));
    }
}
