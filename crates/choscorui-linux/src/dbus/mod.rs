//! The D-Bus binding of the crate: libdbus-1, loaded at run time.
//!
//! One thread owns each `Connection`. The event loop of that thread
//! waits on `Connection::watches` and `Connection::next_timeout`, passes
//! what it saw to `Connection::handle`, and then calls
//! `Connection::dispatch`. A nested loop, such as the wait for the
//! `Response` of a portal request, uses `Connection::wait_signal`.
//!
//! - `Message` is one D-Bus message, and `Value` is one value of its body.
//! - `Connection::call` blocks for a reply; `Connection::call_async`
//!   queues the reply for a later `dispatch`.
//! - `Connection::subscribe` queues the signals of a `MatchRule`.
//! - `Connection::export` sends the method calls of an object path to a
//!   handler, which returns a `Reply`.

pub mod names;
mod rule;
mod value;

pub use crate::ffi::dbus::{
    CallId, Connection, Error, Export, Message, MessageKind, MethodHandler, NameFlags, NameReply,
    Readiness, Reply, Subscription, WatchFd,
};
pub use rule::MatchRule;
pub use value::{InvalidValue, Value};

use std::time::Duration;

/// The interface of the standard properties.
pub const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

/// The bus itself: its name, path, and interface.
pub const BUS_NAME: &str = "org.freedesktop.DBus";
const BUS_PATH: &str = "/org/freedesktop/DBus";

/// How long the crate waits for a method reply by default.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

impl Connection {
    /// Calls `member` and blocks until the reply. It returns the values
    /// of the reply.
    pub fn call_method(
        &self,
        destination: &str,
        path: &str,
        interface: &str,
        member: &str,
        args: &[Value],
    ) -> Result<Vec<Value>, Error> {
        let message =
            Message::method_call(Some(destination), path, Some(interface), member)?.with(args)?;
        self.call(message, DEFAULT_TIMEOUT)?.args()
    }

    /// Sends the signal `member` of `interface` from `path`.
    pub fn emit(
        &self,
        path: &str,
        interface: &str,
        member: &str,
        args: &[Value],
    ) -> Result<(), Error> {
        let message = Message::signal(path, interface, member)?.with(args)?;
        self.send(message).map(drop)
    }

    /// The property `name` of `interface` on the object `path` of
    /// `destination`, without its variant.
    pub fn get_property(
        &self,
        destination: &str,
        path: &str,
        interface: &str,
        name: &str,
    ) -> Result<Value, Error> {
        let reply = self.call_method(
            destination,
            path,
            PROPERTIES,
            "Get",
            &[Value::from(interface), Value::from(name)],
        )?;
        reply
            .into_iter()
            .next()
            .map(|value| value.unvariant().clone())
            .ok_or_else(|| Error::InvalidArgument("Get returned no value".to_owned()))
    }

    /// True when a connection owns `name` or the bus can start one that
    /// does.
    pub fn name_available(&self, name: &str) -> Result<bool, Error> {
        if self.name_has_owner(name)? {
            return Ok(true);
        }
        let reply = self.call_method(BUS_NAME, BUS_PATH, BUS_NAME, "ListActivatableNames", &[])?;
        Ok(reply
            .first()
            .and_then(Value::as_strings)
            .is_some_and(|names| names.iter().any(|known| known == name)))
    }
}
