//! Notifications: the `org.freedesktop.Notifications` server, or the
//! `Notification` portal inside a Flatpak sandbox.

use super::portal;
use crate::dbus::{Connection, Error, MatchRule, Subscription, Value};
use choscorui_core::{DesktopEvent, SystemNotification};
use std::cell::RefCell;
use std::collections::HashMap;

pub const SERVER: &str = "org.freedesktop.Notifications";
const SERVER_PATH: &str = "/org/freedesktop/Notifications";
const PORTAL_INTERFACE: &str = "org.freedesktop.portal.Notification";

/// The action of a click on the notification itself.
pub const DEFAULT_ACTION: &str = "default";

/// Which service posts the notifications.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationRoute {
    Server,
    Portal,
}

/// The notifications of the application.
pub struct Notifier {
    route: NotificationRoute,
    app_id: String,
    /// The server id of each application identifier, to replace it.
    ids: RefCell<HashMap<String, u32>>,
    actions: Subscription,
    closed: Option<Subscription>,
}

impl Notifier {
    /// The notifier of `route`, when its service is on the bus.
    pub fn new(
        connection: &Connection,
        route: NotificationRoute,
        app_id: &str,
    ) -> Result<Option<Self>, Error> {
        let name = match route {
            NotificationRoute::Server => SERVER,
            NotificationRoute::Portal => portal::DESKTOP,
        };
        if !connection.name_available(name)? {
            return Ok(None);
        }
        let (actions, closed) = match route {
            NotificationRoute::Server => {
                let rule = MatchRule::signal()
                    .sender(SERVER)
                    .path(SERVER_PATH)
                    .interface(SERVER);
                (
                    connection.subscribe(rule.clone().member("ActionInvoked"))?,
                    Some(connection.subscribe(rule.member("NotificationClosed"))?),
                )
            }
            NotificationRoute::Portal => (
                connection.subscribe(
                    MatchRule::signal()
                        .sender(portal::DESKTOP)
                        .path(portal::PATH)
                        .interface(PORTAL_INTERFACE)
                        .member("ActionInvoked"),
                )?,
                None,
            ),
        };
        Ok(Some(Self {
            route,
            app_id: app_id.to_owned(),
            ids: RefCell::default(),
            actions,
            closed,
        }))
    }

    pub fn route(&self) -> NotificationRoute {
        self.route
    }

    /// Posts `notification`. One with the same identifier replaces it.
    pub fn post(
        &self,
        connection: &Connection,
        notification: &SystemNotification,
    ) -> Result<(), Error> {
        match self.route {
            NotificationRoute::Server => {
                let replaces = self
                    .ids
                    .borrow()
                    .get(&notification.identifier)
                    .copied()
                    .unwrap_or(0);
                let hints = Value::vardict([("desktop-entry", Value::from(self.app_id.as_str()))]);
                let reply = connection.call_method(
                    SERVER,
                    SERVER_PATH,
                    SERVER,
                    "Notify",
                    &[
                        Value::from(self.app_id.as_str()),
                        Value::from(replaces),
                        Value::from(""),
                        Value::from(notification.title.as_str()),
                        Value::from(notification.body.as_str()),
                        Value::strings([DEFAULT_ACTION, ""]),
                        hints,
                        Value::from(-1i32),
                    ],
                )?;
                let id = reply
                    .first()
                    .and_then(Value::as_u32)
                    .ok_or_else(|| Error::InvalidArgument("Notify returned no id".to_owned()))?;
                self.ids
                    .borrow_mut()
                    .insert(notification.identifier.clone(), id);
                Ok(())
            }
            NotificationRoute::Portal => {
                let content = Value::vardict([
                    ("title", Value::from(notification.title.as_str())),
                    ("body", Value::from(notification.body.as_str())),
                    ("default-action", Value::from(DEFAULT_ACTION)),
                ]);
                connection
                    .call_method(
                        portal::DESKTOP,
                        portal::PATH,
                        PORTAL_INTERFACE,
                        "AddNotification",
                        &[Value::from(notification.identifier.as_str()), content],
                    )
                    .map(drop)
            }
        }
    }

    /// The application identifier of the server id `id`.
    fn identifier(&self, id: u32) -> Option<String> {
        self.ids
            .borrow()
            .iter()
            .find(|(_, known)| **known == id)
            .map(|(identifier, _)| identifier.clone())
    }

    /// The clicks since the last call, as desktop events.
    pub fn take_events(&self) -> Vec<DesktopEvent> {
        let mut events = Vec::new();
        for signal in self.actions.take() {
            let Ok(args) = signal.args() else {
                continue;
            };
            let identifier = match self.route {
                NotificationRoute::Server => args
                    .first()
                    .and_then(Value::as_u32)
                    .and_then(|id| self.identifier(id)),
                NotificationRoute::Portal => {
                    args.first().and_then(Value::as_str).map(str::to_owned)
                }
            };
            let action = args.get(1).and_then(Value::as_str);
            if let (Some(identifier), Some(action)) = (identifier, action) {
                events.push(DesktopEvent::NotificationActivated {
                    identifier,
                    action: action.to_owned(),
                });
            }
        }
        // A click often closes the notification, so the ids go after the
        // clicks.
        if let Some(closed) = &self.closed {
            for signal in closed.take() {
                if let Some(id) = signal.args().ok().and_then(|args| args.first()?.as_u32()) {
                    self.ids.borrow_mut().retain(|_, known| *known != id);
                }
            }
        }
        events
    }
}
