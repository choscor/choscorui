//! The standard interfaces of an exported object: `Introspectable` and
//! the read-only part of `Properties`.

use crate::dbus::{Message, PROPERTIES, Reply, Value};

const INTROSPECTABLE: &str = "org.freedesktop.DBus.Introspectable";

/// The reply to `Introspect`, when `message` is that call. `interfaces`
/// is the XML of the object's own interfaces.
pub fn answer(message: &Message, interfaces: &str) -> Option<Reply> {
    if message.interface().as_deref() != Some(INTROSPECTABLE)
        || message.member().as_deref() != Some("Introspect")
    {
        return None;
    }
    let xml = format!(
        "<!DOCTYPE node PUBLIC \"-//freedesktop//DTD D-BUS Object Introspection 1.0//EN\" \
         \"http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd\">\n<node>\n{interfaces}\n\
         <interface name=\"{INTROSPECTABLE}\"><method name=\"Introspect\">\
         <arg type=\"s\" direction=\"out\"/></method></interface>\n\
         <interface name=\"{PROPERTIES}\">\
         <method name=\"Get\"><arg type=\"s\" direction=\"in\"/><arg type=\"s\" direction=\"in\"/>\
         <arg type=\"v\" direction=\"out\"/></method>\
         <method name=\"GetAll\"><arg type=\"s\" direction=\"in\"/>\
         <arg type=\"a{{sv}}\" direction=\"out\"/></method></interface>\n</node>\n"
    );
    Some(Reply::Return(vec![Value::from(xml)]))
}

/// The reply to `Properties.Get` and `Properties.GetAll`, when `message`
/// is one of them. `properties` gives the properties of an interface.
pub fn properties(
    message: &Message,
    properties: impl Fn(&str) -> Option<Vec<(&'static str, Value)>>,
) -> Option<Reply> {
    if message.interface().as_deref() != Some(PROPERTIES) {
        return None;
    }
    let args = message.args().unwrap_or_default();
    let interface = args.first().and_then(Value::as_str).unwrap_or_default();
    let unknown = || {
        Reply::error(
            "org.freedesktop.DBus.Error.UnknownInterface",
            format!("no interface {interface}"),
        )
    };
    Some(match message.member().as_deref() {
        Some("GetAll") => match properties(interface) {
            Some(values) => Reply::Return(vec![Value::vardict(values)]),
            None => unknown(),
        },
        Some("Get") => {
            let name = args.get(1).and_then(Value::as_str).unwrap_or_default();
            match properties(interface) {
                Some(values) => match values.into_iter().find(|(key, _)| *key == name) {
                    Some((_, value)) => Reply::Return(vec![Value::variant(value)]),
                    None => Reply::error(
                        "org.freedesktop.DBus.Error.UnknownProperty",
                        format!("no property {name}"),
                    ),
                },
                None => unknown(),
            }
        }
        Some("Set") => Reply::error(
            "org.freedesktop.DBus.Error.PropertyReadOnly",
            "the properties are read-only",
        ),
        _ => Reply::unknown_method(message),
    })
}
