//! The AT-SPI bridge on a real accessibility bus. A test client walks a
//! window with every `SemanticRole`, invokes a declared action, and
//! receives the events and the announcements. The test runs only with
//! `CHOSCORUI_RUN_LINUX_SMOKE=1`, inside `scripts/linux/headless-session.sh`,
//! which starts `at-spi2-registryd`. It does not verify a screen reader.
#![cfg(target_os = "linux")]

use choscorui_core::{
    AnnouncementPriority, NodeId, Point, Rect, SemanticAction, SemanticNode, SemanticRange,
    SemanticRole, SemanticState, SemanticText, SemanticTree, Size, Support, Tree, WindowOptions,
};
use choscorui_linux::atspi::{AccessibleWindow, Bridge, accessibility_support};
use choscorui_linux::dbus::{Connection, MatchRule, Message, Value};
use choscorui_linux::{Application, Window};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

fn enabled() -> bool {
    std::env::var("CHOSCORUI_RUN_LINUX_SMOKE").is_ok_and(|value| value == "1")
}

const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const REGISTRY: &str = "org.a11y.atspi.Registry";
const ROOT: &str = "/org/a11y/atspi/accessible/root";
const EVENTS: &str = "org.a11y.atspi.Event.Object";
const TITLE: &str = "every role";

/// Every semantic role and the AT-SPI role name that
/// `docs/platform-linux.md` gives it.
const ROLES: &[(SemanticRole, &str)] = &[
    (SemanticRole::Application, "application"),
    (SemanticRole::Window, "window"),
    (SemanticRole::Group, "panel"),
    (SemanticRole::Button, "push button"),
    (SemanticRole::Checkbox, "check box"),
    (SemanticRole::RadioButton, "radio button"),
    (SemanticRole::RadioGroup, "panel"),
    (SemanticRole::TextField, "entry"),
    (SemanticRole::PasswordField, "password text"),
    (SemanticRole::SearchField, "entry"),
    (SemanticRole::StaticText, "label"),
    (SemanticRole::List, "list"),
    (SemanticRole::ListItem, "list item"),
    (SemanticRole::ScrollArea, "scroll pane"),
    (SemanticRole::Slider, "slider"),
    (SemanticRole::SpinButton, "spin button"),
    (SemanticRole::ProgressIndicator, "progress bar"),
    (SemanticRole::Tab, "page tab"),
    (SemanticRole::TabList, "page tab list"),
    (SemanticRole::TabPanel, "panel"),
    (SemanticRole::Table, "table"),
    (SemanticRole::Row, "table row"),
    (SemanticRole::Cell, "table cell"),
    (SemanticRole::ColumnHeader, "column header"),
    (SemanticRole::Tree, "tree"),
    (SemanticRole::TreeItem, "tree item"),
    (SemanticRole::Menu, "menu"),
    (SemanticRole::MenuItem, "menu item"),
    (SemanticRole::ComboBox, "combo box"),
    (SemanticRole::PopUpButton, "combo box"),
    (SemanticRole::Tooltip, "tool tip"),
    (SemanticRole::Dialog, "dialog"),
    (SemanticRole::Toolbar, "tool bar"),
    (SemanticRole::StatusBar, "status bar"),
    (SemanticRole::Image, "image"),
    (SemanticRole::Link, "link"),
    (SemanticRole::ScrollBar, "scroll bar"),
    (SemanticRole::MenuBar, "menu bar"),
    (SemanticRole::MenuBarItem, "menu"),
    (SemanticRole::Switch, "toggle button"),
];

fn rect(x: f64, y: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(40.0, 20.0))
}

fn node(role: SemanticRole) -> SemanticNode {
    SemanticNode {
        role,
        name: format!("{role:?}"),
        description: String::new(),
        value: None,
        state: SemanticState::default(),
        relations: Vec::new(),
        actions: Vec::new(),
        text: None,
    }
}

/// A view model with one node of every role. A table holds the row, the
/// cell, and the column header.
struct Model {
    tree: Tree,
    semantics: SemanticTree,
    ids: HashMap<String, NodeId>,
}

impl Model {
    fn new() -> Self {
        let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(400.0, 300.0)));
        let mut semantics = SemanticTree::new();
        let mut ids = HashMap::new();
        let group = tree.attach(tree.root(), rect(0.0, 0.0)).unwrap();
        let mut root = node(SemanticRole::Group);
        root.name = "all roles".to_owned();
        semantics.set(group, root, &tree).unwrap();
        ids.insert("all roles".to_owned(), group);
        let mut table = None;
        for (index, &(role, _)) in ROLES.iter().enumerate() {
            let parent = match role {
                SemanticRole::Row | SemanticRole::ColumnHeader => table.unwrap(),
                SemanticRole::Cell => ids["Row"],
                _ => group,
            };
            let id = tree
                .attach(parent, rect(10.0 * index as f64, 5.0 * index as f64))
                .unwrap();
            let mut semantic = node(role);
            match role {
                SemanticRole::Button => {
                    semantic.actions = vec![SemanticAction::Activate, SemanticAction::Focus];
                    tree.set_focusable(id, true).unwrap();
                }
                SemanticRole::Checkbox => {
                    semantic.state.checked = Some(false);
                    semantic.actions = vec![SemanticAction::Activate];
                }
                SemanticRole::TextField => {
                    semantic.value = Some("hello".to_owned());
                    semantic.actions = vec![SemanticAction::SetValue];
                    semantic.text = Some(SemanticText {
                        character_count_utf16: 5,
                        selection_utf16: 5..5,
                        multiline: false,
                    });
                }
                SemanticRole::Slider => {
                    semantic.state.range = Some(SemanticRange {
                        value: 3.0,
                        minimum: 0.0,
                        maximum: 10.0,
                        step: 1.0,
                    });
                    semantic.actions = vec![SemanticAction::Increment, SemanticAction::Decrement];
                }
                SemanticRole::Table => table = Some(id),
                _ => {}
            }
            semantics.set(id, semantic, &tree).unwrap();
            ids.insert(format!("{role:?}"), id);
        }
        tree.set_focus(Some(ids["Button"])).unwrap();
        semantics.sync_focus(&tree);
        Self {
            tree,
            semantics,
            ids,
        }
    }

    fn publish(&self, window: &AccessibleWindow) {
        window.publish(&self.semantics.snapshot(&self.tree));
    }
}

/// What the client tells the application thread.
#[derive(Debug)]
enum Step {
    /// The client invoked the action of the button and of a label.
    Invoked,
    /// The client listens for events.
    Listening,
    /// The client registered for `object:announcement`.
    Registered,
    Finished,
}

/// The paths that the client needs.
struct Paths {
    app: String,
    window: String,
    button: String,
    checkbox: String,
    image: String,
    group: String,
}

fn accessibility_bus() -> Connection {
    let session = Connection::session().unwrap();
    let address = session
        .call_method(
            "org.a11y.Bus",
            "/org/a11y/bus",
            "org.a11y.Bus",
            "GetAddress",
            &[],
        )
        .unwrap();
    Connection::open_private(address[0].as_str().unwrap()).unwrap()
}

fn reference(value: &Value) -> (String, String) {
    let fields = value.as_struct().unwrap();
    (
        fields[0].as_str().unwrap().to_owned(),
        fields[1].as_str().unwrap().to_owned(),
    )
}

/// Each object under `path`: its path, role name, and name.
fn walk(bus: &Connection, app: &str, path: &str, found: &mut Vec<(String, String, String)>) {
    let role = bus
        .call_method(app, path, ACCESSIBLE, "GetRoleName", &[])
        .unwrap();
    let name = bus.get_property(app, path, ACCESSIBLE, "Name").unwrap();
    found.push((
        path.to_owned(),
        role[0].as_str().unwrap().to_owned(),
        name.as_str().unwrap().to_owned(),
    ));
    let children = bus
        .call_method(app, path, ACCESSIBLE, "GetChildren", &[])
        .unwrap();
    for child in children[0].as_array().unwrap() {
        let (_, child) = reference(child);
        walk(bus, app, &child, found);
    }
}

/// Waits for the next event `member` from `path` and returns its
/// arguments.
fn next_event(
    bus: &Connection,
    events: &choscorui_linux::dbus::Subscription,
    member: &str,
    path: &str,
    detail: &str,
) -> Vec<Value> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(!left.is_zero(), "no {member} {detail} event from {path}");
        let signal = bus.wait_signal(events, Some(left)).unwrap();
        let args = signal.args().unwrap();
        if signal.member().as_deref() == Some(member)
            && signal.path().as_deref() == Some(path)
            && args[0].as_str() == Some(detail)
        {
            return args;
        }
    }
}

/// The assistive technology side: it runs on its own thread with its
/// own connection.
fn client(paths: Paths, steps: Sender<Step>) {
    let bus = accessibility_bus();
    let desktop = bus
        .call_method(REGISTRY, ROOT, ACCESSIBLE, "GetChildren", &[])
        .unwrap();
    let embedded = desktop[0]
        .as_array()
        .unwrap()
        .iter()
        .map(reference)
        .any(|(name, path)| name == paths.app && path == ROOT);
    assert!(embedded, "the registry lists the application");
    let app = paths.app.as_str();

    let mut found = Vec::new();
    walk(&bus, app, ROOT, &mut found);
    assert_eq!(found[0].1, "application");
    assert!(found.contains(&(paths.window.clone(), "frame".to_owned(), TITLE.to_owned())));
    for &(role, atspi) in ROLES {
        let name = format!("{role:?}");
        assert!(
            found
                .iter()
                .any(|(_, found_role, found_name)| *found_name == name && found_role == atspi),
            "{name} is a {atspi}: {found:?}"
        );
    }

    let action = "org.a11y.atspi.Action";
    let name = bus
        .call_method(app, &paths.button, action, "GetName", &[Value::from(0i32)])
        .unwrap();
    assert_eq!(name[0].as_str(), Some("click"));
    let done = bus
        .call_method(app, &paths.button, action, "DoAction", &[Value::from(0i32)])
        .unwrap();
    assert_eq!(done[0].as_bool(), Some(true));
    // A node without declared actions has no `Action` interface.
    let label = found
        .iter()
        .find(|(_, _, name)| name == "StaticText")
        .map(|(path, _, _)| path.clone())
        .unwrap();
    assert!(
        bus.call_method(app, &label, action, "DoAction", &[Value::from(0i32)])
            .is_err()
    );
    let field = found
        .iter()
        .find(|(_, _, name)| name == "TextField")
        .map(|(path, _, _)| path.clone())
        .unwrap();
    let text = bus
        .call_method(
            app,
            &field,
            "org.a11y.atspi.Text",
            "GetText",
            &[Value::from(0i32), Value::from(-1i32)],
        )
        .unwrap();
    assert_eq!(text[0].as_str(), Some("hello"));
    let slider = found
        .iter()
        .find(|(_, _, name)| name == "Slider")
        .map(|(path, _, _)| path.clone())
        .unwrap();
    let value = bus
        .get_property(app, &slider, "org.a11y.atspi.Value", "CurrentValue")
        .unwrap();
    assert_eq!(value.as_f64(), Some(3.0));
    steps.send(Step::Invoked).unwrap();

    let events = bus
        .subscribe(MatchRule::signal().sender(app).interface(EVENTS))
        .unwrap();
    steps.send(Step::Listening).unwrap();
    // Without a listener of `object:announcement`, the focused node
    // speaks.
    let announcement = next_event(&bus, &events, "Announcement", &paths.button, "");
    assert_eq!(announcement[3].as_str(), Some("Saved"));
    let checked = next_event(&bus, &events, "StateChanged", &paths.checkbox, "checked");
    assert_eq!(checked[1].as_i64(), Some(1));
    let removed = next_event(&bus, &events, "ChildrenChanged", &paths.group, "remove");
    assert_eq!(reference(&removed[3]).1, paths.image);
    assert!(
        bus.call_method(app, &paths.image, ACCESSIBLE, "GetRole", &[])
            .is_err(),
        "a removed node loses its path"
    );

    bus.call_method(
        REGISTRY,
        "/org/a11y/atspi/registry",
        REGISTRY,
        "RegisterEvent",
        &[Value::from("object:announcement")],
    )
    .unwrap();
    steps.send(Step::Registered).unwrap();
    let announcement = next_event(&bus, &events, "Announcement", &paths.window, "");
    assert_eq!(announcement[3].as_str(), Some("Done"));
    assert_eq!(announcement[1].as_i64(), Some(2));
    bus.call_method(
        REGISTRY,
        "/org/a11y/atspi/registry",
        REGISTRY,
        "DeregisterEvent",
        &[Value::from("object:announcement")],
    )
    .unwrap();
    steps.send(Step::Finished).unwrap();
}

fn open(application: &Application) -> Window {
    let window = application
        .create_window(WindowOptions::new(TITLE, 400.0, 300.0))
        .unwrap();
    window.show();
    window
}

#[test]
fn a_client_walks_invokes_and_listens() {
    if !enabled() {
        return;
    }
    assert_eq!(accessibility_support(), Support::Supported);
    let application = Application::new().unwrap();
    let bridge = Bridge::connect("atspi-smoke").unwrap();
    application.attach_bus(bridge.connection().clone());
    let window = open(&application);
    let accessible = bridge.add_window(&window);
    let mut model = Model::new();
    model.publish(&accessible);

    let paths = Paths {
        app: bridge.connection().unique_name().unwrap(),
        window: accessible.path(),
        button: accessible.node_path(model.ids["Button"]).unwrap(),
        checkbox: accessible.node_path(model.ids["Checkbox"]).unwrap(),
        image: accessible.node_path(model.ids["Image"]).unwrap(),
        group: accessible.node_path(model.ids["all roles"]).unwrap(),
    };
    let (sender, steps): (Sender<Step>, Receiver<Step>) = mpsc::channel();
    let client = std::thread::spawn(move || client(paths, sender));

    let deadline = Instant::now() + Duration::from_secs(60);
    let mut finished = false;
    while !finished && Instant::now() < deadline {
        application
            .dispatch_next_event(Duration::from_millis(10))
            .unwrap();
        match steps.try_recv() {
            Ok(Step::Invoked) => {
                let actions = accessible.take_actions();
                assert_eq!(actions.len(), 1, "only the declared action queues");
                assert_eq!(actions[0].node, model.ids["Button"]);
                assert_eq!(actions[0].action, SemanticAction::Activate);
            }
            Ok(Step::Listening) => {
                accessible.announce("Saved", AnnouncementPriority::Medium);
                let checkbox = model.ids["Checkbox"];
                let mut checked = node(SemanticRole::Checkbox);
                checked.state.checked = Some(true);
                checked.actions = vec![SemanticAction::Activate];
                model.semantics.set(checkbox, checked, &model.tree).unwrap();
                model.tree.detach(model.ids["Image"]).unwrap();
                model.semantics.prune(&model.tree);
                model.publish(&accessible);
            }
            Ok(Step::Registered) => accessible.announce("Done", AnnouncementPriority::High),
            Ok(Step::Finished) => finished = true,
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => break,
        }
    }
    let outcome = client.join();
    assert!(outcome.is_ok(), "the AT-SPI client failed");
    assert!(finished, "the client did not finish in time");
    drop(accessible);
    window.close();
}

#[test]
fn a_message_without_a_node_is_refused() {
    if !enabled() {
        return;
    }
    let bus = accessibility_bus();
    let application = Application::new().unwrap();
    let bridge = Bridge::connect("atspi-smoke-unknown").unwrap();
    application.attach_bus(bridge.connection().clone());
    let app = bridge.connection().unique_name().unwrap();
    let call = Message::method_call(
        Some(&app),
        "/org/a11y/atspi/accessible/w99n1",
        Some(ACCESSIBLE),
        "GetRole",
    )
    .unwrap();
    let pending = bus.call_async(call, Duration::from_secs(5)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let reply = loop {
        application
            .dispatch_next_event(Duration::from_millis(10))
            .unwrap();
        bus.turn(Some(Duration::from_millis(10))).unwrap();
        if let Some(reply) = bus.take_reply(pending) {
            break reply;
        }
        assert!(Instant::now() < deadline, "no reply");
    };
    assert!(reply.is_err());
}
