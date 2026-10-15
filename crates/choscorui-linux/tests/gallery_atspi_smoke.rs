//! The gallery on a real accessibility bus. The test starts the gallery
//! binary, and an AT-SPI client in the test process walks its window.
//! The client selects pages through the sidebar buttons and finds the
//! expected role and name of each `SemanticRole` that the gallery shows.
//! It sees the effect of the declared actions that it invokes.
//!
//! The walk runs only with `CHOSCORUI_RUN_LINUX_SMOKE=1`, inside
//! `scripts/linux/headless-session.sh`, which starts `at-spi2-registryd`.
//! `tests/atspi_smoke.rs` checks every role on a synthetic window. This
//! test does not verify a screen reader.
//!
//! The test lives here, not in the gallery, because the gallery depends on
//! the facade alone. Build the gallery first (`cargo build -p
//! choscorui-gallery`): the test runs the binary next to its own
//! directory of test executables.
#![cfg(target_os = "linux")]

use choscorui_core::SemanticRole;
use choscorui_linux::dbus::{Connection, MatchRule, Subscription, Value};
use std::cell::RefCell;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const ACTION: &str = "org.a11y.atspi.Action";
const COMPONENT: &str = "org.a11y.atspi.Component";
const REGISTRY: &str = "org.a11y.atspi.Registry";
const ROOT: &str = "/org/a11y/atspi/accessible/root";
const EVENTS: &str = "org.a11y.atspi.Event.Object";
const BUS: &str = "org.freedesktop.DBus";

/// The application id of the gallery, which is its accessible name.
const APP_ID: &str = "com.choscor.gallery";
/// The English window title, before the page title.
const TITLE: &str = "choscorui gallery — ";
/// The AT-SPI state `checked`.
const CHECKED: u32 = 4;
/// How long the client waits for one condition.
const PATIENCE: Duration = Duration::from_secs(20);

fn enabled() -> bool {
    std::env::var("CHOSCORUI_RUN_LINUX_SMOKE").is_ok_and(|value| value == "1")
}

/// Every semantic role.
const ALL: &[SemanticRole] = &[
    SemanticRole::Application,
    SemanticRole::Window,
    SemanticRole::Group,
    SemanticRole::Button,
    SemanticRole::Checkbox,
    SemanticRole::RadioButton,
    SemanticRole::RadioGroup,
    SemanticRole::TextField,
    SemanticRole::PasswordField,
    SemanticRole::SearchField,
    SemanticRole::StaticText,
    SemanticRole::List,
    SemanticRole::ListItem,
    SemanticRole::ScrollArea,
    SemanticRole::Slider,
    SemanticRole::SpinButton,
    SemanticRole::ProgressIndicator,
    SemanticRole::Tab,
    SemanticRole::TabList,
    SemanticRole::TabPanel,
    SemanticRole::Table,
    SemanticRole::Row,
    SemanticRole::Cell,
    SemanticRole::ColumnHeader,
    SemanticRole::Tree,
    SemanticRole::TreeItem,
    SemanticRole::Menu,
    SemanticRole::MenuItem,
    SemanticRole::ComboBox,
    SemanticRole::PopUpButton,
    SemanticRole::Tooltip,
    SemanticRole::Dialog,
    SemanticRole::Toolbar,
    SemanticRole::StatusBar,
    SemanticRole::Image,
    SemanticRole::Link,
    SemanticRole::ScrollBar,
    SemanticRole::MenuBar,
    SemanticRole::MenuBarItem,
    SemanticRole::Switch,
];

/// The AT-SPI role name and the `xml-roles` attribute that
/// `docs/platform-linux.md` gives a role. A single-line text field is an
/// `entry`. The match has no wildcard, so a new role needs a row here.
fn atspi(role: SemanticRole) -> (&'static str, &'static str) {
    match role {
        SemanticRole::Application => ("application", ""),
        SemanticRole::Window => ("window", ""),
        SemanticRole::Group => ("panel", ""),
        SemanticRole::Button => ("push button", ""),
        SemanticRole::Checkbox => ("check box", ""),
        SemanticRole::RadioButton => ("radio button", ""),
        SemanticRole::RadioGroup => ("panel", "radiogroup"),
        SemanticRole::TextField => ("entry", ""),
        SemanticRole::PasswordField => ("password text", ""),
        SemanticRole::SearchField => ("entry", "searchbox"),
        SemanticRole::StaticText => ("label", ""),
        SemanticRole::List => ("list", ""),
        SemanticRole::ListItem => ("list item", ""),
        SemanticRole::ScrollArea => ("scroll pane", ""),
        SemanticRole::Slider => ("slider", ""),
        SemanticRole::SpinButton => ("spin button", ""),
        SemanticRole::ProgressIndicator => ("progress bar", ""),
        SemanticRole::Tab => ("page tab", ""),
        SemanticRole::TabList => ("page tab list", ""),
        SemanticRole::TabPanel => ("panel", "tabpanel"),
        SemanticRole::Table => ("table", ""),
        SemanticRole::Row => ("table row", ""),
        SemanticRole::Cell => ("table cell", ""),
        SemanticRole::ColumnHeader => ("column header", ""),
        SemanticRole::Tree => ("tree", ""),
        SemanticRole::TreeItem => ("tree item", ""),
        SemanticRole::Menu => ("menu", ""),
        SemanticRole::MenuItem => ("menu item", ""),
        SemanticRole::ComboBox => ("combo box", ""),
        SemanticRole::PopUpButton => ("combo box", "button"),
        SemanticRole::Tooltip => ("tool tip", ""),
        SemanticRole::Dialog => ("dialog", ""),
        SemanticRole::Toolbar => ("tool bar", ""),
        SemanticRole::StatusBar => ("status bar", ""),
        SemanticRole::Image => ("image", ""),
        SemanticRole::Link => ("link", ""),
        SemanticRole::ScrollBar => ("scroll bar", ""),
        SemanticRole::MenuBar => ("menu bar", ""),
        SemanticRole::MenuBarItem => ("menu", "menuitem"),
        SemanticRole::Switch => ("toggle button", "switch"),
    }
}

/// The AT-SPI roles of more than one semantic role.
const SHARED_ROLES: &[&str] = &["panel", "entry", "combo box", "menu", "toggle button"];

/// The roles that no gallery page shows, and why.
/// `crates/choscorui-linux/tests/atspi_smoke.rs` checks them.
const NOT_SHOWN: &[(SemanticRole, &str)] = &[
    (
        SemanticRole::Window,
        "a window is the `frame` object of the bridge, and no node has the Window role",
    ),
    (
        SemanticRole::TreeItem,
        "the column browser shows its levels as lists, and no page shows a tree item view",
    ),
];

/// What the client does on a page before it looks for the objects.
#[derive(Clone, Copy, Debug)]
enum Probe {
    /// Nothing.
    Look,
    /// It invokes `click` on the push button with this name.
    Click(&'static str),
    /// It calls `Component.GrabFocus` on the object with this name.
    Focus(&'static str),
}

/// One page, the probe, and the role and the name of the objects that
/// the walk must find.
struct Visit {
    /// The page title, which is also the name of its sidebar button.
    page: &'static str,
    probe: Probe,
    expect: &'static [(SemanticRole, &'static str)],
}

/// The pages in the order of the walk. The window opens on the first.
const VISITS: &[Visit] = &[
    Visit {
        page: "Label",
        probe: Probe::Look,
        expect: &[
            (SemanticRole::Application, APP_ID),
            (SemanticRole::Group, ""),
            (SemanticRole::Button, "Separator"),
            (SemanticRole::Checkbox, "Enabled"),
            (SemanticRole::TextField, "Filter pages"),
            (SemanticRole::StaticText, "Static"),
            (SemanticRole::ScrollArea, ""),
            (SemanticRole::MenuBar, "Menu bar"),
            (SemanticRole::MenuBarItem, "File"),
        ],
    },
    Visit {
        page: "Button group",
        probe: Probe::Look,
        expect: &[(SemanticRole::RadioButton, "Left")],
    },
    Visit {
        page: "Toggle group",
        probe: Probe::Look,
        expect: &[(SemanticRole::RadioGroup, "Alignment")],
    },
    Visit {
        page: "Switch",
        probe: Probe::Look,
        expect: &[(SemanticRole::Switch, "Wi-Fi")],
    },
    Visit {
        page: "Select",
        probe: Probe::Look,
        expect: &[(SemanticRole::ComboBox, "Fruit")],
    },
    Visit {
        page: "Native select",
        probe: Probe::Look,
        expect: &[(SemanticRole::PopUpButton, "Status")],
    },
    Visit {
        page: "Input",
        probe: Probe::Look,
        expect: &[(SemanticRole::PasswordField, "Password")],
    },
    Visit {
        page: "Tabs",
        probe: Probe::Look,
        expect: &[
            (SemanticRole::TabList, "Settings"),
            (SemanticRole::Tab, "General"),
            (SemanticRole::TabPanel, "Name, icon, and startup behavior."),
        ],
    },
    Visit {
        page: "Slider",
        probe: Probe::Look,
        expect: &[(SemanticRole::Slider, "Volume")],
    },
    Visit {
        page: "Spin box",
        probe: Probe::Look,
        expect: &[(SemanticRole::SpinButton, "Copies (1–99)")],
    },
    Visit {
        page: "Progress",
        probe: Probe::Look,
        expect: &[(SemanticRole::ProgressIndicator, "Download")],
    },
    Visit {
        page: "Scroll bar",
        probe: Probe::Look,
        expect: &[(SemanticRole::ScrollBar, "Vertical scroll")],
    },
    Visit {
        page: "Item view",
        probe: Probe::Look,
        expect: &[
            (SemanticRole::List, "Records"),
            (SemanticRole::ListItem, "Record 000000 · Hà Nội · 0.00 ₫"),
        ],
    },
    Visit {
        page: "Header view",
        probe: Probe::Look,
        expect: &[
            (SemanticRole::Table, "Products"),
            (SemanticRole::ColumnHeader, "Name"),
            (SemanticRole::Row, "Bánh mì"),
        ],
    },
    Visit {
        page: "Column browser",
        probe: Probe::Look,
        expect: &[(SemanticRole::Tree, "Places")],
    },
    Visit {
        page: "Data table",
        probe: Probe::Look,
        expect: &[
            (SemanticRole::SearchField, "Filter emails..."),
            (SemanticRole::Cell, "success"),
        ],
    },
    Visit {
        page: "Avatar",
        probe: Probe::Look,
        expect: &[(SemanticRole::Image, "Ana Lee")],
    },
    Visit {
        page: "Breadcrumb",
        probe: Probe::Look,
        expect: &[(SemanticRole::Link, "Home")],
    },
    Visit {
        page: "Toolbar",
        probe: Probe::Look,
        expect: &[(SemanticRole::Toolbar, "Editor")],
    },
    Visit {
        page: "Status bar",
        probe: Probe::Look,
        expect: &[(SemanticRole::StatusBar, "Status")],
    },
    Visit {
        page: "Alert dialog",
        probe: Probe::Look,
        expect: &[(SemanticRole::Dialog, "Unsaved changes")],
    },
    // The trigger opens the menu in the window.
    Visit {
        page: "Dropdown menu",
        probe: Probe::Click("Edit ▾"),
        expect: &[
            (SemanticRole::Menu, "Edit ▾"),
            (SemanticRole::MenuItem, "Cut"),
        ],
    },
    // Keyboard focus shows the tooltip of a control at once.
    Visit {
        page: "Tooltip",
        probe: Probe::Focus("Bold"),
        expect: &[(SemanticRole::Tooltip, "Make the selection bold (⌘B)")],
    },
];

/// One accessible object of the gallery.
#[derive(Clone, Debug)]
struct Object {
    path: String,
    role: String,
    xml_roles: String,
    name: String,
}

impl Object {
    fn is(&self, role: SemanticRole, name: &str) -> bool {
        let (atspi_role, xml_roles) = atspi(role);
        self.role == atspi_role && self.xml_roles == xml_roles && self.name == name
    }
}

/// The gallery executable of the same build profile: test executables
/// are in `deps/` under the profile directory, and binaries are in it.
fn gallery_binary() -> PathBuf {
    let test = std::env::current_exe().expect("the test knows its path");
    let binary = test
        .parent()
        .and_then(std::path::Path::parent)
        .expect("the test runs from the deps directory of a profile")
        .join("choscorui-gallery");
    assert!(
        binary.is_file(),
        "build the gallery first: cargo build -p choscorui-gallery ({})",
        binary.display()
    );
    binary
}

/// The gallery process. A drop stops it, also when an assertion fails.
struct Gallery {
    child: Child,
    settings: PathBuf,
}

impl Gallery {
    /// Starts the gallery on `page` in English, with private settings.
    fn start(page: &str) -> Self {
        let settings = std::env::temp_dir().join(format!(
            "choscorui-gallery-atspi-{}.txt",
            std::process::id()
        ));
        let child = Command::new(gallery_binary())
            .arg(page)
            .env("CHOSCORUI_GALLERY_SETTINGS", &settings)
            .env("CHOSCORUI_GALLERY_LANG", "en")
            // The gallery quits by itself if the test cannot stop it.
            .env("CHOSCORUI_GALLERY_AUTOCLOSE_MS", "300000")
            .spawn()
            .expect("the gallery starts");
        Self { child, settings }
    }
}

impl Drop for Gallery {
    fn drop(&mut self) {
        // The process can be gone already; then there is nothing to stop
        // or to remove.
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.settings);
    }
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

/// Calls `condition` until it gives a value, and fails with `what` after
/// [`PATIENCE`].
fn wait_for<T>(what: impl Fn() -> String, mut condition: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(value) = condition() {
            return value;
        }
        assert!(Instant::now() < deadline, "{}", what());
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The AT-SPI client of the test.
struct Client {
    bus: Connection,
    /// The unique name of the gallery on the accessibility bus.
    app: String,
}

impl Client {
    /// Waits until the registry lists the application of process `pid`.
    fn find(pid: u32) -> Self {
        let bus = accessibility_bus();
        let app = wait_for(
            || format!("the registry does not list the gallery (process {pid})"),
            || {
                let desktop = bus
                    .call_method(REGISTRY, ROOT, ACCESSIBLE, "GetChildren", &[])
                    .ok()?;
                desktop[0]
                    .as_array()?
                    .iter()
                    .map(reference)
                    .filter(|(_, path)| path == ROOT)
                    .map(|(name, _)| name)
                    .find(|name| {
                        bus.call_method(
                            BUS,
                            "/org/freedesktop/DBus",
                            BUS,
                            "GetConnectionUnixProcessID",
                            &[Value::from(name.as_str())],
                        )
                        .is_ok_and(|reply| reply[0].as_u32() == Some(pid))
                    })
            },
        );
        Self { bus, app }
    }

    fn call(
        &self,
        path: &str,
        interface: &str,
        member: &str,
        args: &[Value],
    ) -> Option<Vec<Value>> {
        self.bus
            .call_method(&self.app, path, interface, member, args)
            .ok()
    }

    /// Every object of the application in tree order, or `None` when an
    /// object went away during the walk.
    fn walk(&self) -> Option<Vec<Object>> {
        let mut found = Vec::new();
        let mut pending = vec![ROOT.to_owned()];
        while let Some(path) = pending.pop() {
            let role = self.call(&path, ACCESSIBLE, "GetRoleName", &[])?;
            let name = self
                .bus
                .get_property(&self.app, &path, ACCESSIBLE, "Name")
                .ok()?;
            let role = role[0].as_str()?.to_owned();
            // Only the roles that several semantic roles share have an
            // `xml-roles` attribute.
            let xml_roles = if SHARED_ROLES.contains(&role.as_str()) {
                let attributes = self.call(&path, ACCESSIBLE, "GetAttributes", &[])?;
                let xml_roles = attributes[0].get("xml-roles").and_then(Value::as_str);
                xml_roles.unwrap_or_default().to_owned()
            } else {
                String::new()
            };
            let children = self.call(&path, ACCESSIBLE, "GetChildren", &[])?;
            found.push(Object {
                role,
                xml_roles,
                name: name.as_str()?.to_owned(),
                path,
            });
            let children = children[0].as_array()?;
            pending.extend(children.iter().rev().map(|child| reference(child).1));
        }
        Some(found)
    }

    /// Walks until `condition` holds for the objects, and returns them.
    fn walk_until(&self, what: &str, condition: impl Fn(&[Object]) -> bool) -> Vec<Object> {
        let last = RefCell::new(Vec::new());
        wait_for(
            || format!("{what}; the last walk found {:#?}", last.borrow()),
            || {
                let found = self.walk()?;
                if condition(&found) {
                    return Some(found);
                }
                *last.borrow_mut() = found;
                None
            },
        )
    }

    /// Waits until a window of the application shows `page` in its
    /// name.
    fn wait_title(&self, page: &str) {
        let title = format!("{TITLE}{page}");
        let last = RefCell::new(Vec::new());
        wait_for(
            || {
                format!(
                    "no window is {title:?}; the windows are {:?}",
                    last.borrow()
                )
            },
            || {
                let windows = self.call(ROOT, ACCESSIBLE, "GetChildren", &[])?;
                let names: Vec<String> = windows[0]
                    .as_array()?
                    .iter()
                    .filter_map(|window| {
                        let path = reference(window).1;
                        let name = self.bus.get_property(&self.app, &path, ACCESSIBLE, "Name");
                        name.ok()?.as_str().map(str::to_owned)
                    })
                    .collect();
                let shown = names.contains(&title);
                *last.borrow_mut() = names;
                shown.then_some(())
            },
        );
    }

    /// Invokes the first action of `path`, which must be `click`.
    fn click(&self, path: &str) {
        let name = self
            .call(path, ACTION, "GetName", &[Value::from(0i32)])
            .unwrap();
        assert_eq!(name[0].as_str(), Some("click"), "{path}");
        let done = self
            .call(path, ACTION, "DoAction", &[Value::from(0i32)])
            .unwrap();
        assert_eq!(done[0].as_bool(), Some(true), "{path}");
    }

    /// True when the object `path` has the AT-SPI state `state`.
    fn has_state(&self, path: &str, state: u32) -> bool {
        let reply = self.call(path, ACCESSIBLE, "GetState", &[]).unwrap();
        let words = reply[0].as_array().unwrap();
        let word = words[(state / 32) as usize].as_u32().unwrap();
        word & (1 << (state % 32)) != 0
    }

    fn events(&self) -> Subscription {
        self.bus
            .subscribe(MatchRule::signal().sender(&self.app).interface(EVENTS))
            .unwrap()
    }

    /// Waits for the event `member` with `detail` from `path`, and
    /// returns its arguments.
    fn event(&self, events: &Subscription, member: &str, path: &str, detail: &str) -> Vec<Value> {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "no {member} {detail} event from {path}");
            let signal = self
                .bus
                .wait_signal(events, Some(left))
                .unwrap_or_else(|error| panic!("no {member} {detail} event from {path}: {error}"));
            let args = signal.args().unwrap();
            if signal.member().as_deref() == Some(member)
                && signal.path().as_deref() == Some(path)
                && args[0].as_str() == Some(detail)
            {
                return args;
            }
        }
    }
}

/// The path of the first object with `role` and `name`.
fn path_of(found: &[Object], role: SemanticRole, name: &str) -> String {
    found
        .iter()
        .find(|object| object.is(role, name))
        .map(|object| object.path.clone())
        .unwrap_or_else(|| panic!("no {role:?} named {name:?}: {found:#?}"))
}

#[test]
fn each_role_is_walked_or_has_a_reason() {
    for &role in ALL {
        let walked = VISITS
            .iter()
            .flat_map(|visit| visit.expect)
            .filter(|(expected, _)| *expected == role)
            .count();
        let skipped = NOT_SHOWN
            .iter()
            .filter(|(skipped, _)| *skipped == role)
            .count();
        assert_eq!(walked + skipped, 1, "{role:?} is {:?}", atspi(role));
        let (atspi_role, xml_roles) = atspi(role);
        assert!(xml_roles.is_empty() || SHARED_ROLES.contains(&atspi_role));
    }
}

#[test]
fn a_client_walks_the_gallery_and_invokes_actions() {
    if !enabled() {
        return;
    }
    let gallery = Gallery::start("label");
    let client = Client::find(gallery.child.id());
    let first = VISITS[0].page;
    client.wait_title(first);
    let mut found = client.walk_until("the sidebar has no buttons", |found| {
        found
            .iter()
            .any(|object| object.is(SemanticRole::Button, VISITS[1].page))
    });

    for visit in VISITS {
        if visit.page != first {
            // Selecting a page is a declared action. Its effect is the
            // new name of the window.
            client.click(&path_of(&found, SemanticRole::Button, visit.page));
            client.wait_title(visit.page);
        }
        match visit.probe {
            Probe::Look => {}
            Probe::Click(name) => {
                let found = client.walk_until(&format!("no button {name:?}"), |found| {
                    found
                        .iter()
                        .any(|object| object.is(SemanticRole::Button, name))
                });
                client.click(&path_of(&found, SemanticRole::Button, name));
            }
            Probe::Focus(name) => {
                let found = client.walk_until(&format!("no object {name:?}"), |found| {
                    found.iter().any(|object| object.name == name)
                });
                let target = found.iter().find(|object| object.name == name).unwrap();
                let done = client
                    .call(&target.path, COMPONENT, "GrabFocus", &[])
                    .unwrap();
                assert_eq!(done[0].as_bool(), Some(true));
            }
        }
        found = client.walk_until(
            &format!("the {} page lacks {:?}", visit.page, visit.expect),
            |found| {
                visit
                    .expect
                    .iter()
                    .all(|(role, name)| found.iter().any(|object| object.is(*role, name)))
            },
        );
    }

    // The Enabled check box is on. A click turns it off, and the client
    // receives the state change.
    let checkbox = path_of(&found, SemanticRole::Checkbox, "Enabled");
    assert!(client.has_state(&checkbox, CHECKED));
    let events = client.events();
    client.click(&checkbox);
    let changed = client.event(&events, "StateChanged", &checkbox, "checked");
    assert_eq!(changed[1].as_i64(), Some(0));
    assert!(!client.has_state(&checkbox, CHECKED));
}
