//! The AT-SPI2 bridge: the semantic tree of each window on the
//! accessibility bus, over the D-Bus binding of the crate.
//!
//! 1. `Bridge::connect` asks the session bus for the address of the
//!    accessibility bus (`org.a11y.Bus.GetAddress`), opens a private
//!    connection to it, and exports every accessible object with one
//!    fallback handler.
//! 2. It embeds the application root in the registry with
//!    `org.a11y.atspi.Socket.Embed`.
//! 3. The event loop serves the connection: give it to
//!    `Application::attach_bus`.
//! 4. `Bridge::add_window` gives an `AccessibleWindow`. Its `publish`
//!    keeps one object path for each live node and sends the change
//!    events. Its `take_actions` returns the actions of assistive
//!    technology. An action queues only when the node declares it.

mod handler;
mod interfaces;
mod model;
mod roles;
mod text;
mod tree;

pub use model::AccessibilityActionRequest;

use crate::dbus::{self, Connection, Export, Message, Value};
use crate::native::Window;
use choscorui_core::{AnnouncementPriority, SemanticEntry, Support};
use model::{Model, ROOT_PATH, Target, WindowModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use tree::Event;

const BUS_NAME: &str = "org.a11y.Bus";
const BUS_PATH: &str = "/org/a11y/bus";
const REGISTRY: &str = "org.a11y.atspi.Registry";
const REGISTRY_PATH: &str = "/org/a11y/atspi/registry";
const SOCKET: &str = "org.a11y.atspi.Socket";
const EVENT_OBJECT: &str = "org.a11y.atspi.Event.Object";
const EVENT_WINDOW: &str = "org.a11y.atspi.Event.Window";
const EVENT_FOCUS: &str = "org.a11y.atspi.Event.Focus";
/// The parent of every exported object path.
const EXPORT_PATH: &str = "/org/a11y/atspi";
/// How long the bridge waits for the bus launcher and the registry.
const TIMEOUT: Duration = Duration::from_secs(2);

/// Why the bridge did not start.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccessibilityError {
    /// The session bus or the accessibility bus did not answer.
    Bus(dbus::Error),
    /// The bus launcher gave no address.
    NoAddress,
    /// The registry refused to embed the application.
    Embed(dbus::Error),
}

impl std::fmt::Display for AccessibilityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bus(error) => write!(f, "the accessibility bus is not reachable: {error}"),
            Self::NoAddress => f.write_str("the accessibility bus launcher gave no address"),
            Self::Embed(error) => write!(f, "the AT-SPI registry did not embed the app: {error}"),
        }
    }
}

impl std::error::Error for AccessibilityError {}

/// The address of the accessibility bus: `AT_SPI_BUS_ADDRESS`, else the
/// answer of the bus launcher on the session bus.
fn bus_address() -> Result<String, AccessibilityError> {
    if let Ok(address) = std::env::var("AT_SPI_BUS_ADDRESS")
        && !address.is_empty()
    {
        return Ok(address);
    }
    let session = Connection::session().map_err(AccessibilityError::Bus)?;
    let call = Message::method_call(Some(BUS_NAME), BUS_PATH, Some(BUS_NAME), "GetAddress")
        .map_err(AccessibilityError::Bus)?;
    let reply = session
        .call(call, TIMEOUT)
        .and_then(|reply| reply.args())
        .map_err(AccessibilityError::Bus)?;
    reply
        .first()
        .and_then(Value::as_str)
        .filter(|address| !address.is_empty())
        .map(str::to_owned)
        .ok_or(AccessibilityError::NoAddress)
}

/// A private connection to the accessibility bus.
fn open_bus() -> Result<Connection, AccessibilityError> {
    Connection::open_private(&bus_address()?).map_err(AccessibilityError::Bus)
}

/// `Supported` when the accessibility bus is reachable now. Each call
/// asks again, because the bus can start or stop at any time.
pub fn accessibility_support() -> Support {
    if open_bus().is_ok() {
        Support::Supported
    } else {
        Support::Unsupported
    }
}

/// The application on the accessibility bus.
pub struct Bridge {
    connection: Connection,
    model: Rc<RefCell<Model>>,
    _export: Export,
}

impl Bridge {
    /// Connects to the accessibility bus and embeds the application,
    /// which assistive technology then shows as `app_name`.
    pub fn connect(app_name: &str) -> Result<Rc<Self>, AccessibilityError> {
        let connection = open_bus()?;
        let model = Rc::new(RefCell::new(Model::new(app_name.to_owned())));
        let handled = Rc::clone(&model);
        let export = connection
            .export_fallback(
                EXPORT_PATH,
                Rc::new(move |connection: &Connection, message: &Message| {
                    handler::handle(&handled, connection, message)
                }),
            )
            .map_err(AccessibilityError::Bus)?;
        let me = connection.unique_name().unwrap_or_default();
        let embed = Message::method_call(Some(REGISTRY), ROOT_PATH, Some(SOCKET), "Embed")
            .and_then(|message| message.with(&[handler::reference(&me, ROOT_PATH)]))
            .and_then(|message| connection.call(message, TIMEOUT))
            .and_then(|reply| reply.args())
            .map_err(AccessibilityError::Embed)?;
        let parent = embed.first().and_then(Value::as_struct).and_then(|fields| {
            Some((
                fields.first()?.as_str()?.to_owned(),
                fields.get(1)?.as_str()?.to_owned(),
            ))
        });
        model.borrow_mut().parent = parent;
        Ok(Rc::new(Self {
            connection,
            model,
            _export: export,
        }))
    }

    /// The connection that the event loop must serve.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Makes `window` a child of the application.
    pub fn add_window(self: &Rc<Self>, window: &Window) -> AccessibleWindow {
        let id = self.model.borrow_mut().add_window(WindowModel::new(
            Rc::clone(&window.shared),
            Rc::clone(&window.surface),
        ));
        let index = self.model.borrow().window_index(id).unwrap_or(0);
        self.children_changed(Target::Root, true, index, Target::Window(id));
        AccessibleWindow {
            bridge: Rc::clone(self),
            id,
        }
    }

    /// Sends `member` of `interface` from `target`, with the AT-SPI event
    /// arguments. A lost bus only loses the event.
    fn emit(
        &self,
        target: Target,
        interface: &str,
        member: &str,
        detail: &str,
        (detail1, detail2): (i32, i32),
        data: Value,
    ) {
        let args = [
            Value::from(detail),
            Value::from(detail1),
            Value::from(detail2),
            Value::variant(data),
            Value::Dict {
                key: "s".to_owned(),
                value: "v".to_owned(),
                entries: Vec::new(),
            },
        ];
        let _ = self
            .connection
            .emit(&target.path(), interface, member, &args);
    }

    fn reference(&self, target: Target) -> Value {
        handler::reference(
            &self.connection.unique_name().unwrap_or_default(),
            &target.path(),
        )
    }

    fn children_changed(&self, parent: Target, added: bool, index: usize, child: Target) {
        self.emit(
            parent,
            EVENT_OBJECT,
            "ChildrenChanged",
            if added { "add" } else { "remove" },
            (handler::count(index), 0),
            self.reference(child),
        );
    }

    fn state_changed(&self, target: Target, state: u32, on: bool) {
        self.emit(
            target,
            EVENT_OBJECT,
            "StateChanged",
            roles::state::name(state),
            (i32::from(on), 0),
            Value::from(0i32),
        );
    }

    /// True when a listener of the registry asked for
    /// `object:announcement` events, as AT-SPI 2.46 and later do.
    fn announcements_registered(&self) -> bool {
        let Ok(call) = Message::method_call(
            Some(REGISTRY),
            REGISTRY_PATH,
            Some(REGISTRY),
            "GetRegisteredEvents",
        ) else {
            return false;
        };
        let Ok(args) = self
            .connection
            .call(call, TIMEOUT)
            .and_then(|reply| reply.args())
        else {
            return false;
        };
        args.first()
            .and_then(Value::as_array)
            .is_some_and(|events| {
                events.iter().any(|event| {
                    event
                        .as_struct()
                        .and_then(|fields| fields.get(1)?.as_str())
                        .is_some_and(|name| {
                            let name = name.to_ascii_lowercase().replace([':', '-'], "");
                            name.starts_with("objectannouncement")
                        })
                })
            })
    }
}

/// The accessible side of one window. Its drop removes the window and
/// its nodes from the bus.
pub struct AccessibleWindow {
    bridge: Rc<Bridge>,
    id: u32,
}

impl AccessibleWindow {
    /// The object path of the window frame.
    pub fn path(&self) -> String {
        Target::Window(self.id).path()
    }

    /// The object path of node `id` itself, while it is published.
    pub fn node_path(&self, id: choscorui_core::NodeId) -> Option<String> {
        let model = self.bridge.model.borrow();
        let node = model.windows.get(&self.id)?.tree.identity(id)?;
        Some(Target::Node(self.id, node).path())
    }

    /// Publishes `snapshot`: new nodes get a path, removed nodes lose
    /// theirs, and the differences become AT-SPI events.
    pub fn publish(&self, snapshot: &[SemanticEntry]) {
        let (events, title) = {
            let mut model = self.bridge.model.borrow_mut();
            let Some(window) = model.windows.get_mut(&self.id) else {
                return;
            };
            let title = window.current_title();
            let renamed = (title != window.title).then(|| title.clone());
            window.title = title;
            (window.tree.reconcile(snapshot), renamed)
        };
        let window = Target::Window(self.id);
        if let Some(title) = title {
            self.bridge.emit(
                window,
                EVENT_OBJECT,
                "PropertyChange",
                "accessible-name",
                (0, 0),
                Value::from(title),
            );
        }
        let node = |identity: u64| Target::Node(self.id, identity);
        for event in events {
            match event {
                Event::Children {
                    parent,
                    added,
                    index,
                    child,
                } => self.bridge.children_changed(
                    parent.map_or(window, node),
                    added,
                    index,
                    node(child),
                ),
                Event::State {
                    node: identity,
                    state,
                    on,
                } => self.bridge.state_changed(node(identity), state, on),
                Event::Property {
                    node: identity,
                    name,
                    value,
                } => self.bridge.emit(
                    node(identity),
                    EVENT_OBJECT,
                    "PropertyChange",
                    name,
                    (0, 0),
                    value,
                ),
                Event::Text {
                    node: identity,
                    inserted,
                    start,
                    text,
                } => {
                    let length = handler::count(text::char_count(&text));
                    self.bridge.emit(
                        node(identity),
                        EVENT_OBJECT,
                        "TextChanged",
                        if inserted { "insert" } else { "delete" },
                        (handler::count(start), length),
                        Value::from(text),
                    );
                }
                Event::Caret {
                    node: identity,
                    offset,
                } => self.bridge.emit(
                    node(identity),
                    EVENT_OBJECT,
                    "TextCaretMoved",
                    "",
                    (handler::count(offset), 0),
                    Value::from(0i32),
                ),
                Event::TextSelection { node: identity } => self.bridge.emit(
                    node(identity),
                    EVENT_OBJECT,
                    "TextSelectionChanged",
                    "",
                    (0, 0),
                    Value::from(0i32),
                ),
                Event::Focus { node: identity } => self.bridge.emit(
                    node(identity),
                    EVENT_FOCUS,
                    "Focus",
                    "",
                    (0, 0),
                    Value::from(0i32),
                ),
            }
        }
    }

    /// Tells assistive technology that the window became active or
    /// inactive.
    pub fn set_active(&self, active: bool) {
        let title = {
            let model = self.bridge.model.borrow();
            match model.windows.get(&self.id) {
                Some(window) => window.current_title(),
                None => return,
            }
        };
        let window = Target::Window(self.id);
        self.bridge.emit(
            window,
            EVENT_WINDOW,
            if active { "Activate" } else { "Deactivate" },
            "",
            (0, 0),
            Value::from(title),
        );
        self.bridge
            .state_changed(window, roles::state::ACTIVE, active);
    }

    /// Asks assistive technology to speak `text`. A listener of
    /// `object:announcement` (AT-SPI 2.46 and later) gets the event from
    /// the window. Otherwise the event comes from the focused node, which
    /// older screen readers follow.
    pub fn announce(&self, text: &str, priority: AnnouncementPriority) {
        let focused = {
            let model = self.bridge.model.borrow();
            match model.windows.get(&self.id) {
                Some(window) => window.tree.focused,
                None => return,
            }
        };
        let source = match focused {
            Some(node) if !self.bridge.announcements_registered() => Target::Node(self.id, node),
            _ => Target::Window(self.id),
        };
        // `AtspiLive`: 1 polite, 2 assertive.
        let politeness = match priority {
            AnnouncementPriority::High => 2,
            AnnouncementPriority::Low | AnnouncementPriority::Medium => 1,
        };
        self.bridge.emit(
            source,
            EVENT_OBJECT,
            "Announcement",
            "",
            (politeness, 0),
            Value::from(text),
        );
    }

    /// The actions that assistive technology asked for since the last
    /// call, oldest first.
    pub fn take_actions(&self) -> Vec<AccessibilityActionRequest> {
        self.bridge
            .model
            .borrow_mut()
            .windows
            .get_mut(&self.id)
            .map(|window| std::mem::take(&mut window.actions))
            .unwrap_or_default()
    }
}

impl Drop for AccessibleWindow {
    fn drop(&mut self) {
        let index = {
            let mut model = self.bridge.model.borrow_mut();
            let index = model.window_index(self.id);
            model.windows.remove(&self.id);
            index
        };
        if let Some(index) = index {
            self.bridge
                .children_changed(Target::Root, false, index, Target::Window(self.id));
        }
    }
}
