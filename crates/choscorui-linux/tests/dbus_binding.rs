//! The D-Bus binding against a private `dbus-daemon`: calls, replies,
//! errors, signals, exported objects, value marshaling, and the poll
//! loop integration.

#![cfg(target_os = "linux")]

#[allow(dead_code, reason = "each test binary uses a part of the shared fakes")]
mod support;

use choscorui_linux::dbus::{
    Connection, Error, MatchRule, Message, MessageKind, MethodHandler, NameFlags, NameReply,
    Readiness, Reply, Value,
};
use std::os::fd::AsFd;
use std::rc::Rc;
use std::time::Duration;
use support::bus::TestBus;
use support::fake::{Emit, FakeService, Object};

const NAME: &str = "org.choscorui.Test";
const PATH: &str = "/org/choscorui/Test";
const INTERFACE: &str = "org.choscorui.Test";

/// Every kind of value, nested.
fn sample() -> Vec<Value> {
    vec![
        Value::Byte(7),
        Value::Bool(true),
        Value::Int16(-2),
        Value::Uint16(3),
        Value::Int32(-4),
        Value::Uint32(5),
        Value::Int64(-6),
        Value::Uint64(7),
        Value::Double(1.5),
        Value::str("héllo"),
        Value::object_path("/a/b"),
        Value::Signature("a{sv}".to_owned()),
        Value::array("y", vec![Value::Byte(1), Value::Byte(2)]),
        Value::strings(Vec::<String>::new()),
        Value::vardict([
            ("count", Value::from(3u32)),
            ("nested", Value::strings(["x", "y"])),
        ]),
        Value::Struct(vec![Value::from(1i32), Value::variant(Value::from("v"))]),
        Value::array(
            "(iiay)",
            vec![Value::Struct(vec![
                Value::from(1i32),
                Value::from(1i32),
                Value::array("y", vec![Value::Byte(255); 4]),
            ])],
        ),
    ]
}

fn echo_service(bus: &TestBus) -> FakeService {
    FakeService::start(
        bus.address(),
        &[NAME],
        vec![Object::new(
            PATH,
            Box::new(|call, emits| match call.member.as_str() {
                "Echo" => Reply::Return(call.args.clone()),
                "Fail" => Reply::error("org.choscorui.Error.Scripted", "it failed"),
                "Ping" => {
                    emits.push(Emit::new(
                        PATH,
                        INTERFACE,
                        "Pinged",
                        vec![Value::from("pong")],
                    ));
                    Reply::Return(Vec::new())
                }
                // No reply: the caller times out.
                "Silent" => Reply::Sent,
                _ => Reply::error("org.freedesktop.DBus.Error.UnknownMethod", "no such method"),
            }),
        )],
    )
}

fn call(member: &str, args: &[Value]) -> Message {
    Message::method_call(Some(NAME), PATH, Some(INTERFACE), member)
        .unwrap()
        .with(args)
        .unwrap()
}

#[test]
fn every_value_survives_a_round_trip() {
    let bus = TestBus::start();
    let _service = echo_service(&bus);
    let connection = bus.connect();
    let reply = connection
        .call(call("Echo", &sample()), Duration::from_secs(5))
        .unwrap();
    assert_eq!(reply.kind(), Some(MessageKind::MethodReturn));
    assert_eq!(reply.args().unwrap(), sample());
    assert_eq!(
        reply.signature(),
        sample().iter().map(Value::signature).collect::<String>()
    );
}

#[test]
fn an_error_reply_names_the_error() {
    let bus = TestBus::start();
    let _service = echo_service(&bus);
    let connection = bus.connect();
    let error = connection
        .call(call("Fail", &[]), Duration::from_secs(5))
        .unwrap_err();
    assert_eq!(
        error,
        Error::Remote {
            name: "org.choscorui.Error.Scripted".to_owned(),
            message: "it failed".to_owned(),
        }
    );
}

#[test]
fn an_async_call_queues_its_reply_and_times_out() {
    let bus = TestBus::start();
    let _service = echo_service(&bus);
    let connection = bus.connect();
    let first = connection
        .call_async(call("Echo", &[Value::from(1u32)]), Duration::from_secs(5))
        .unwrap();
    let reply = connection
        .wait_reply(first, Some(Duration::from_secs(5)))
        .unwrap();
    assert_eq!(reply.args().unwrap(), vec![Value::from(1u32)]);
    // The libdbus timeout of a call fires through the poll loop.
    let silent = connection
        .call_async(call("Silent", &[]), Duration::from_millis(100))
        .unwrap();
    let error = connection
        .wait_reply(silent, Some(Duration::from_secs(5)))
        .unwrap_err();
    assert_eq!(error.name(), Some("org.freedesktop.DBus.Error.NoReply"));
}

#[test]
fn a_subscription_receives_its_signals() {
    let bus = TestBus::start();
    let service = echo_service(&bus);
    let connection = bus.connect();
    let pinged = connection
        .subscribe(MatchRule::signal().interface(INTERFACE).member("Pinged"))
        .unwrap();
    let other = connection
        .subscribe(MatchRule::signal().interface(INTERFACE).member("Other"))
        .unwrap();
    connection
        .call(call("Ping", &[]), Duration::from_secs(5))
        .unwrap();
    let signal = connection
        .wait_signal(&pinged, Some(Duration::from_secs(5)))
        .unwrap();
    assert_eq!(signal.args().unwrap(), vec![Value::from("pong")]);
    assert_eq!(signal.path().as_deref(), Some(PATH));
    service.emit(Emit::new(PATH, INTERFACE, "Other", vec![Value::from(9u32)]));
    let signal = connection
        .wait_signal(&other, Some(Duration::from_secs(5)))
        .unwrap();
    assert_eq!(signal.args().unwrap(), vec![Value::from(9u32)]);
    assert!(pinged.next().is_none());
}

#[test]
fn a_wait_for_a_signal_times_out() {
    let bus = TestBus::start();
    let connection = bus.connect();
    let never = connection
        .subscribe(MatchRule::signal().interface(INTERFACE).member("Never"))
        .unwrap();
    let error = connection
        .wait_signal(&never, Some(Duration::from_millis(50)))
        .unwrap_err();
    assert_eq!(error, Error::TimedOut);
}

#[test]
fn an_exported_object_answers_calls() {
    let bus = TestBus::start();
    let server = bus.connect();
    let client = bus.connect();
    let flags = NameFlags {
        do_not_queue: true,
        ..NameFlags::default()
    };
    assert_eq!(
        server.request_name(NAME, flags).unwrap(),
        NameReply::PrimaryOwner
    );
    assert_eq!(client.request_name(NAME, flags).unwrap(), NameReply::Exists);
    assert!(client.name_has_owner(NAME).unwrap());
    let handler: MethodHandler = Rc::new(
        |_connection: &Connection, message: &Message| match message.member().as_deref() {
            Some("Add") => {
                let args = message.args().unwrap();
                let sum = args[0].as_u32().unwrap() + args[1].as_u32().unwrap();
                Reply::Return(vec![Value::from(sum)])
            }
            _ => Reply::unknown_method(message),
        },
    );
    let export = server.export_fallback(PATH, handler).unwrap();
    // The server turns its loop while the client waits for the reply.
    let pending = client
        .call_async(
            Message::method_call(Some(NAME), &format!("{PATH}/child"), Some(INTERFACE), "Add")
                .unwrap()
                .with(&[Value::from(2u32), Value::from(3u32)])
                .unwrap(),
            Duration::from_secs(5),
        )
        .unwrap();
    let reply = client
        .run_until(Some(Duration::from_secs(5)), |client| {
            server.turn(Some(Duration::from_millis(5))).unwrap();
            client.turn(Some(Duration::from_millis(5))).unwrap();
            client.take_reply(pending)
        })
        .unwrap()
        .unwrap();
    assert_eq!(reply.args().unwrap(), vec![Value::from(5u32)]);
    drop(export);
    let pending = client
        .call_async(
            call("Add", &[Value::from(1u32), Value::from(1u32)]),
            Duration::from_secs(5),
        )
        .unwrap();
    let error = client
        .run_until(Some(Duration::from_secs(5)), |client| {
            server.turn(Some(Duration::from_millis(5))).unwrap();
            client.turn(Some(Duration::from_millis(5))).unwrap();
            client.take_reply(pending)
        })
        .unwrap()
        .unwrap_err();
    assert_eq!(
        error.name(),
        Some("org.freedesktop.DBus.Error.UnknownMethod")
    );
}

#[test]
fn the_watches_join_an_outside_poll_loop() {
    let bus = TestBus::start();
    let _service = echo_service(&bus);
    let connection = bus.connect();
    let watches = connection.watches();
    assert!(
        watches.iter().any(|watch| watch.read()),
        "libdbus watches its socket"
    );
    assert!(
        watches
            .iter()
            .all(|watch| watch.as_fd().try_clone_to_owned().is_ok())
    );
    let pending = connection
        .call_async(call("Echo", &[Value::from("x")]), Duration::from_secs(5))
        .unwrap();
    // A pending call arms a libdbus timeout.
    assert!(connection.next_timeout().is_some());
    let reply = connection
        .run_until(Some(Duration::from_secs(5)), |connection| {
            // What a backend loop does: wait on the descriptors, handle
            // what is ready, then dispatch.
            for watch in connection.watches() {
                connection.handle(
                    &watch,
                    Readiness {
                        read: watch.read(),
                        write: watch.write(),
                        hangup: false,
                    },
                );
            }
            connection.dispatch().unwrap();
            connection.take_reply(pending)
        })
        .unwrap()
        .unwrap();
    assert_eq!(reply.args().unwrap(), vec![Value::from("x")]);
}

#[test]
fn invalid_names_and_values_are_errors() {
    assert!(matches!(
        Message::method_call(Some("bad"), PATH, None, "M"),
        Err(Error::InvalidArgument(_))
    ));
    assert!(Message::method_call(None, "no-slash", None, "M").is_err());
    assert!(Message::signal(PATH, INTERFACE, "1bad").is_err());
    let message = Message::signal(PATH, INTERFACE, "Changed").unwrap();
    assert!(
        message
            .with(&[Value::array("s", vec![Value::from(1u8)])])
            .is_err()
    );
}

#[test]
fn a_closed_bus_disconnects_the_connection() {
    let bus = TestBus::start();
    let connection = bus.connect();
    assert!(connection.is_connected());
    drop(bus);
    let result = connection.run_until(Some(Duration::from_secs(5)), |_| None::<()>);
    assert_eq!(result, Err(Error::Disconnected));
    assert!(!connection.is_connected());
    assert!(
        connection
            .call(call("Echo", &[]), Duration::from_secs(1))
            .is_err()
    );
}
