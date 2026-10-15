//! A fake D-Bus service: a thread with its own connection that owns bus
//! names, records each method call, answers with a scripted behavior,
//! and sends the signals that the test asks for.

use choscorui_linux::dbus::{
    Connection, Message, MethodHandler, NameFlags, NameReply, Reply, Value,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// One method call that a fake received.
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub sender: String,
    pub path: String,
    pub interface: String,
    pub member: String,
    pub args: Vec<Value>,
}

impl Call {
    fn of(message: &Message) -> Self {
        Self {
            sender: message.sender().unwrap_or_default(),
            path: message.path().unwrap_or_default(),
            interface: message.interface().unwrap_or_default(),
            member: message.member().unwrap_or_default(),
            args: message.args().unwrap_or_default(),
        }
    }

    /// Argument `index`.
    pub fn arg(&self, index: usize) -> &Value {
        &self.args[index]
    }
}

/// A signal that a fake sends.
#[derive(Clone, Debug, PartialEq)]
pub struct Emit {
    pub path: String,
    pub interface: String,
    pub member: String,
    pub args: Vec<Value>,
}

impl Emit {
    pub fn new(path: &str, interface: &str, member: &str, args: Vec<Value>) -> Self {
        Self {
            path: path.to_owned(),
            interface: interface.to_owned(),
            member: member.to_owned(),
            args,
        }
    }
}

/// The answer to a call. It can add signals, which the fake sends after
/// the reply.
pub type Behavior = Box<dyn Fn(&Call, &mut Vec<Emit>) -> Reply + Send>;

/// An object of a fake service.
pub struct Object {
    pub path: String,
    pub fallback: bool,
    pub behavior: Behavior,
}

impl Object {
    pub fn new(path: &str, behavior: Behavior) -> Self {
        Self {
            path: path.to_owned(),
            fallback: false,
            behavior,
        }
    }

    /// An object for `path` and every path below it.
    pub fn fallback(path: &str, behavior: Behavior) -> Self {
        Self {
            path: path.to_owned(),
            fallback: true,
            behavior,
        }
    }
}

/// A running fake. Its drop stops the thread and closes the connection,
/// so the bus releases the names.
pub struct FakeService {
    calls: Arc<Mutex<Vec<Call>>>,
    emits: Sender<Emit>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FakeService {
    /// Connects to the bus at `address`, owns `names`, and exports
    /// `objects`. It returns after the names are owned.
    pub fn start(address: &str, names: &[&str], objects: Vec<Object>) -> Self {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (emits, receiver) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        let address = address.to_owned();
        let names: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
        let thread = {
            let calls = Arc::clone(&calls);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                run(&address, &names, objects, &calls, &stop, &receiver, &ready)
            })
        };
        started
            .recv_timeout(Duration::from_secs(10))
            .expect("the fake service started");
        Self {
            calls,
            emits,
            stop,
            thread: Some(thread),
        }
    }

    /// The calls so far, oldest first.
    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    /// The calls of `member` so far.
    pub fn calls_of(&self, member: &str) -> Vec<Call> {
        self.calls()
            .into_iter()
            .filter(|call| call.member == member)
            .collect()
    }

    /// Waits until the fake received a call of `member`.
    pub fn wait_for(&self, member: &str) -> Call {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(call) = self.calls_of(member).pop() {
                return call;
            }
            assert!(Instant::now() < deadline, "no call of {member} came");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Sends `emit` from the fake's thread.
    pub fn emit(&self, emit: Emit) {
        self.emits.send(emit).expect("the fake service runs");
    }
}

impl Drop for FakeService {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(
    address: &str,
    names: &[String],
    objects: Vec<Object>,
    calls: &Arc<Mutex<Vec<Call>>>,
    stop: &AtomicBool,
    receiver: &Receiver<Emit>,
    ready: &Sender<()>,
) {
    let connection = Connection::open_private(address).expect("the fake connects");
    let flags = NameFlags {
        do_not_queue: true,
        ..NameFlags::default()
    };
    for name in names {
        let reply = connection
            .request_name(name, flags)
            .expect("request the name");
        assert_eq!(reply, NameReply::PrimaryOwner, "the fake owns {name}");
    }
    let outbox: Rc<RefCell<Vec<Emit>>> = Rc::default();
    let mut exports = Vec::new();
    for object in objects {
        let behavior = object.behavior;
        let calls = Arc::clone(calls);
        let outbox = Rc::clone(&outbox);
        let handler: MethodHandler = Rc::new(move |_connection: &Connection, message: &Message| {
            let call = Call::of(message);
            calls.lock().unwrap().push(call.clone());
            behavior(&call, &mut outbox.borrow_mut())
        });
        let export = if object.fallback {
            connection.export_fallback(&object.path, handler)
        } else {
            connection.export(&object.path, handler)
        };
        exports.push(export.expect("export the object"));
    }
    ready.send(()).expect("the test waits");
    while !stop.load(Ordering::SeqCst) {
        let mut pending: Vec<Emit> = receiver.try_iter().collect();
        pending.append(&mut outbox.borrow_mut());
        for emit in pending {
            connection
                .emit(&emit.path, &emit.interface, &emit.member, &emit.args)
                .expect("send the signal");
        }
        if connection.turn(Some(Duration::from_millis(5))).is_err() {
            break;
        }
    }
    connection.flush();
    drop(exports);
}
