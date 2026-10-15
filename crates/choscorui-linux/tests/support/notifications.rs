//! A fake `org.freedesktop.Notifications` server.

use super::fake::{Emit, FakeService, Object};
use choscorui_linux::dbus::{Reply, Value};
use std::sync::atomic::{AtomicU32, Ordering};

pub const NAME: &str = "org.freedesktop.Notifications";
pub const PATH: &str = "/org/freedesktop/Notifications";

/// The fake server. `Notify` returns the `replaces_id`, or a new id;
/// `CloseNotification` sends `NotificationClosed`.
pub struct FakeNotifications {
    pub service: FakeService,
}

impl FakeNotifications {
    pub fn start(address: &str) -> Self {
        let next = AtomicU32::new(1);
        let service = FakeService::start(
            address,
            &[NAME],
            vec![Object::new(
                PATH,
                Box::new(move |call, emits| match call.member.as_str() {
                    "Notify" => {
                        let replaces = call.arg(1).as_u32().unwrap_or(0);
                        let id = if replaces == 0 {
                            next.fetch_add(1, Ordering::SeqCst)
                        } else {
                            replaces
                        };
                        Reply::Return(vec![Value::from(id)])
                    }
                    "CloseNotification" => {
                        let id = call.arg(0).as_u32().unwrap_or(0);
                        emits.push(Emit::new(
                            PATH,
                            NAME,
                            "NotificationClosed",
                            vec![Value::from(id), Value::from(3u32)],
                        ));
                        Reply::Return(Vec::new())
                    }
                    "GetCapabilities" => Reply::Return(vec![Value::strings(["body", "actions"])]),
                    "GetServerInformation" => Reply::Return(vec![
                        Value::from("fake"),
                        Value::from("choscorui"),
                        Value::from("1.0"),
                        Value::from("1.2"),
                    ]),
                    _ => Reply::error("org.freedesktop.DBus.Error.UnknownMethod", "no such method"),
                }),
            )],
        );
        Self { service }
    }

    /// Sends `ActionInvoked`, as after a click on the notification `id`.
    pub fn invoke(&self, id: u32, action: &str) {
        self.service.emit(Emit::new(
            PATH,
            NAME,
            "ActionInvoked",
            vec![Value::from(id), Value::from(action)],
        ));
    }

    /// Sends `NotificationClosed` for `id`.
    pub fn close(&self, id: u32) {
        self.service.emit(Emit::new(
            PATH,
            NAME,
            "NotificationClosed",
            vec![Value::from(id), Value::from(2u32)],
        ));
    }
}
