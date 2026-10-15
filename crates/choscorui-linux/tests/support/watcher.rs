//! A fake `org.kde.StatusNotifierWatcher`.

use super::fake::{Emit, FakeService, Object};
use choscorui_linux::dbus::{Reply, Value};
use std::sync::{Arc, Mutex};

pub const NAME: &str = "org.kde.StatusNotifierWatcher";
pub const PATH: &str = "/StatusNotifierWatcher";

/// The fake watcher. `host` is the `IsStatusNotifierHostRegistered`
/// property.
pub struct FakeWatcher {
    pub service: FakeService,
}

impl FakeWatcher {
    pub fn start(address: &str, host: bool) -> Self {
        let items = Arc::new(Mutex::new(Vec::<String>::new()));
        let service = FakeService::start(
            address,
            &[NAME],
            vec![Object::new(
                PATH,
                Box::new(move |call, emits| {
                    let properties = || {
                        vec![
                            ("IsStatusNotifierHostRegistered", Value::from(host)),
                            ("ProtocolVersion", Value::from(0i32)),
                            (
                                "RegisteredStatusNotifierItems",
                                Value::strings(items.lock().unwrap().clone()),
                            ),
                        ]
                    };
                    match (call.interface.as_str(), call.member.as_str()) {
                        (NAME, "RegisterStatusNotifierItem") => {
                            let service = call.arg(0).as_str().unwrap_or_default().to_owned();
                            items.lock().unwrap().push(service.clone());
                            emits.push(Emit::new(
                                PATH,
                                NAME,
                                "StatusNotifierItemRegistered",
                                vec![Value::from(service)],
                            ));
                            Reply::Return(Vec::new())
                        }
                        (NAME, "RegisterStatusNotifierHost") => Reply::Return(Vec::new()),
                        ("org.freedesktop.DBus.Properties", "Get") => {
                            let name = call.arg(1).as_str().unwrap_or_default();
                            match properties().into_iter().find(|(key, _)| *key == name) {
                                Some((_, value)) => Reply::Return(vec![Value::variant(value)]),
                                None => Reply::error(
                                    "org.freedesktop.DBus.Error.UnknownProperty",
                                    "no such property",
                                ),
                            }
                        }
                        ("org.freedesktop.DBus.Properties", "GetAll") => {
                            Reply::Return(vec![Value::vardict(properties())])
                        }
                        _ => Reply::error(
                            "org.freedesktop.DBus.Error.UnknownMethod",
                            "no such method",
                        ),
                    }
                }),
            )],
        );
        Self { service }
    }
}
