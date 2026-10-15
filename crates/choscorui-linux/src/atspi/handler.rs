//! The method calls of the accessible objects: `Accessible`,
//! `Component`, `Application`, `Cache`, and `Properties`. The interfaces
//! of the node roles are in `interfaces`.

use super::interfaces;
use super::model::{Model, NULL_PATH, Target, WindowModel};
use super::roles::{self, StateSet, state};
use super::text;
use super::tree::Node;
use crate::dbus::{Connection, Message, PROPERTIES, Reply, Value};
use choscorui_core::{Point, Rect, SemanticAction, SemanticRelation, SemanticRole};
use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};

pub const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
pub const COMPONENT: &str = "org.a11y.atspi.Component";
pub const APPLICATION: &str = "org.a11y.atspi.Application";
pub const ACTION: &str = "org.a11y.atspi.Action";
pub const TEXT: &str = "org.a11y.atspi.Text";
pub const EDITABLE_TEXT: &str = "org.a11y.atspi.EditableText";
pub const VALUE: &str = "org.a11y.atspi.Value";
pub const SELECTION: &str = "org.a11y.atspi.Selection";
pub const TABLE: &str = "org.a11y.atspi.Table";
const CACHE: &str = "org.a11y.atspi.Cache";

/// The version of the AT-SPI protocol that the bridge speaks.
const ATSPI_VERSION: &str = "2.1";

/// Answers one method call. This is the callback boundary of the
/// bridge: a panic becomes an error reply, and the event loop goes on.
pub fn handle(model: &RefCell<Model>, connection: &Connection, message: &Message) -> Reply {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let Ok(mut model) = model.try_borrow_mut() else {
            return Reply::error(
                "org.freedesktop.DBus.Error.Failed",
                "the accessibility tree is being updated",
            );
        };
        let bus = connection.unique_name().unwrap_or_default();
        let call = Call {
            bus,
            args: message.args().unwrap_or_default(),
            message,
        };
        call.answer(&mut model)
    }));
    outcome.unwrap_or_else(|_| {
        eprintln!("choscorui: a panic in the AT-SPI method handler was caught");
        Reply::error(
            "org.freedesktop.DBus.Error.Failed",
            "the accessibility bridge failed",
        )
    })
}

/// A reference to an object: a bus name and a path.
pub fn reference(bus: &str, path: &str) -> Value {
    Value::Struct(vec![Value::from(bus), Value::object_path(path)])
}

/// A rectangle in whole pixels: `(iiii)`.
pub fn extents_value(rect: Rect) -> Vec<Value> {
    vec![
        Value::from(rect.origin.x.round() as i32),
        Value::from(rect.origin.y.round() as i32),
        Value::from(rect.size.width.round() as i32),
        Value::from(rect.size.height.round() as i32),
    ]
}

fn offset(rect: Rect, by: Point, sign: f64) -> Rect {
    Rect::new(
        Point::new(rect.origin.x + sign * by.x, rect.origin.y + sign * by.y),
        rect.size,
    )
}

/// `rect`, in window coordinates, in the coordinates of `coord_type`:
/// 0 the screen, 1 the window, 2 the parent.
pub fn convert(window: &WindowModel, rect: Rect, parent: Option<Rect>, coord_type: u32) -> Rect {
    match coord_type {
        0 => offset(rect, window.origin(), 1.0),
        2 => match parent {
            Some(parent) => offset(rect, parent.origin, -1.0),
            None => offset(rect, window.origin(), 1.0),
        },
        _ => rect,
    }
}

/// A point in `coord_type` coordinates, in window coordinates.
fn to_window(window: &WindowModel, point: Point, parent: Option<Rect>, coord_type: u32) -> Point {
    let base = match coord_type {
        0 => window.origin(),
        2 => parent.map_or(window.origin(), |parent| parent.origin),
        _ => Point::new(0.0, 0.0),
    };
    Point::new(point.x - base.x, point.y - base.y)
}

fn contains(rect: Rect, point: Point) -> bool {
    point.x >= rect.origin.x
        && point.y >= rect.origin.y
        && point.x < rect.origin.x + rect.size.width
        && point.y < rect.origin.y + rect.size.height
}

/// The interfaces of a node, as `GetInterfaces` lists them.
pub fn node_interfaces(node: &Node) -> Vec<&'static str> {
    let semantic = &node.entry.node;
    let mut list = vec![ACCESSIBLE, COMPONENT];
    if semantic
        .actions
        .iter()
        .any(|&action| roles::action_name(action).is_some())
    {
        list.push(ACTION);
    }
    if text::content(&node.entry).is_some() {
        list.push(TEXT);
        if semantic.text.is_some()
            && semantic.actions.contains(&SemanticAction::SetValue)
            && !semantic.state.read_only
        {
            list.push(EDITABLE_TEXT);
        }
    }
    if semantic.state.range.is_some() {
        list.push(VALUE);
    }
    if matches!(
        semantic.role,
        SemanticRole::List
            | SemanticRole::Tree
            | SemanticRole::Table
            | SemanticRole::TabList
            | SemanticRole::Menu
            | SemanticRole::MenuBar
    ) {
        list.push(SELECTION);
    }
    if semantic.role == SemanticRole::Table {
        list.push(TABLE);
    }
    list
}

/// One method call and its arguments.
pub struct Call<'a> {
    pub bus: String,
    pub args: Vec<Value>,
    pub message: &'a Message,
}

impl Call<'_> {
    pub fn int(&self, index: usize) -> i64 {
        self.args.get(index).and_then(Value::as_i64).unwrap_or(0)
    }

    pub fn uint(&self, index: usize) -> u32 {
        self.args.get(index).and_then(Value::as_u32).unwrap_or(0)
    }

    pub fn text(&self, index: usize) -> String {
        self.args
            .get(index)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    }

    pub fn reference(&self, target: Target) -> Value {
        reference(&self.bus, &target.path())
    }

    pub fn null(&self) -> Value {
        reference(&self.bus, NULL_PATH)
    }

    fn member(&self) -> String {
        self.message.member().unwrap_or_default()
    }

    fn answer(&self, model: &mut Model) -> Reply {
        let path = self.message.path().unwrap_or_default();
        let Some(target) = Target::parse(&path) else {
            return unknown_object(&path);
        };
        let interface = self.message.interface().unwrap_or_default();
        let member = self.member();
        if target == Target::Cache {
            return match (interface.as_str(), member.as_str()) {
                (CACHE, "GetItems") => {
                    Reply::Return(vec![Value::array("((so)(so)(so)iiassusau)", Vec::new())])
                }
                _ => Reply::unknown_method(self.message),
            };
        }
        if !exists(model, target) {
            return unknown_object(&path);
        }
        match interface.as_str() {
            PROPERTIES => self.properties(model, target),
            ACCESSIBLE => self.accessible(model, target),
            COMPONENT => self.component(model, target),
            APPLICATION if target == Target::Root => self.application(),
            _ => match target {
                Target::Node(window, node) => {
                    let Some(window) = model.windows.get_mut(&window) else {
                        return unknown_object(&path);
                    };
                    interfaces::answer(self, window, node, &interface, &member)
                }
                _ => Reply::unknown_method(self.message),
            },
        }
    }

    /// `Properties.Get`, `GetAll`, and `Set`.
    fn properties(&self, model: &mut Model, target: Target) -> Reply {
        let interface = self.text(0);
        let member = self.member();
        if member == "Set" {
            let name = self.text(1);
            let value = self.args.get(2).map(|value| value.unvariant().clone());
            return match (target, interface.as_str(), name.as_str(), value) {
                (Target::Root, APPLICATION, "Id", Some(value)) => {
                    model.app_id = value
                        .as_i64()
                        .and_then(|id| i32::try_from(id).ok())
                        .unwrap_or(0);
                    Reply::Return(Vec::new())
                }
                (Target::Node(window, node), VALUE, "CurrentValue", Some(value)) => {
                    let queued = value.as_f64().is_some_and(|number| {
                        model.windows.get_mut(&window).is_some_and(|window| {
                            window.request(node, SemanticAction::SetValue, Some(number.to_string()))
                        })
                    });
                    if queued {
                        Reply::Return(Vec::new())
                    } else {
                        Reply::error(
                            "org.freedesktop.DBus.Error.PropertyReadOnly",
                            "the node does not take a value",
                        )
                    }
                }
                _ => Reply::error(
                    "org.freedesktop.DBus.Error.PropertyReadOnly",
                    "the property is read-only",
                ),
            };
        }
        let Some(values) = self.property_values(model, target, &interface) else {
            return Reply::error(
                "org.freedesktop.DBus.Error.UnknownInterface",
                format!("no interface {interface}"),
            );
        };
        match member.as_str() {
            "GetAll" => Reply::Return(vec![Value::vardict(values)]),
            "Get" => {
                let name = self.text(1);
                match values.into_iter().find(|(key, _)| *key == name) {
                    Some((_, value)) => Reply::Return(vec![Value::variant(value)]),
                    None => Reply::error(
                        "org.freedesktop.DBus.Error.UnknownProperty",
                        format!("no property {name}"),
                    ),
                }
            }
            _ => Reply::unknown_method(self.message),
        }
    }

    fn property_values(
        &self,
        model: &Model,
        target: Target,
        interface: &str,
    ) -> Option<Vec<(&'static str, Value)>> {
        match interface {
            ACCESSIBLE => Some(vec![
                ("Name", Value::from(name(model, target))),
                ("Description", Value::from(description(model, target))),
                ("Parent", self.parent(model, target)),
                (
                    "ChildCount",
                    Value::from(count(children(model, target).len())),
                ),
                ("Locale", Value::from(locale())),
                ("AccessibleId", Value::from("")),
                ("HelpText", Value::from("")),
            ]),
            APPLICATION if target == Target::Root => Some(vec![
                ("ToolkitName", Value::from("ChoscorUI")),
                ("Version", Value::from(env!("CARGO_PKG_VERSION"))),
                ("AtspiVersion", Value::from(ATSPI_VERSION)),
                ("Id", Value::from(model.app_id)),
            ]),
            _ => match target {
                Target::Node(window, node) => {
                    let window = model.windows.get(&window)?;
                    interfaces::properties(self, window, node, interface)
                }
                _ => None,
            },
        }
    }

    fn parent(&self, model: &Model, target: Target) -> Value {
        match parent(model, target) {
            Some(parent) => self.reference(parent),
            None => match (&model.parent, target) {
                (Some((bus, path)), Target::Root) => reference(bus, path),
                _ => self.null(),
            },
        }
    }

    fn accessible(&self, model: &mut Model, target: Target) -> Reply {
        let values = match self.member().as_str() {
            "GetChildAtIndex" => {
                let index = usize::try_from(self.int(0)).ok();
                let child = index.and_then(|index| children(model, target).get(index).copied());
                vec![child.map_or_else(|| self.null(), |child| self.reference(child))]
            }
            "GetChildren" => vec![Value::array(
                "(so)",
                children(model, target)
                    .into_iter()
                    .map(|child| self.reference(child))
                    .collect(),
            )],
            "GetIndexInParent" => {
                let index = parent(model, target).and_then(|parent| {
                    children(model, parent)
                        .iter()
                        .position(|&child| child == target)
                });
                vec![Value::from(index.map_or(-1, count))]
            }
            "GetRelationSet" => vec![self.relations(model, target)],
            "GetRole" => vec![Value::from(role(model, target).number)],
            "GetRoleName" | "GetLocalizedRoleName" => {
                vec![Value::from(role(model, target).name)]
            }
            "GetState" => vec![Value::array(
                "u",
                states(model, target)
                    .words()
                    .into_iter()
                    .map(Value::from)
                    .collect(),
            )],
            "GetAttributes" => vec![attributes(model, target)],
            "GetApplication" => vec![self.reference(Target::Root)],
            "GetInterfaces" => vec![Value::strings(match target {
                Target::Root => vec![ACCESSIBLE, APPLICATION],
                Target::Window(_) => vec![ACCESSIBLE, COMPONENT],
                Target::Node(window, node) => model
                    .windows
                    .get(&window)
                    .and_then(|window| window.node(node))
                    .map(node_interfaces)
                    .unwrap_or_default(),
                Target::Cache => Vec::new(),
            })],
            _ => return Reply::unknown_method(self.message),
        };
        Reply::Return(values)
    }

    /// The relations of a node, and the reverse relations that other
    /// nodes declare to it.
    fn relations(&self, model: &Model, target: Target) -> Value {
        let mut relations: Vec<(u32, Vec<Target>)> = Vec::new();
        if let Target::Node(window_id, node_id) = target
            && let Some(window) = model.windows.get(&window_id)
            && let Some(node) = window.node(node_id)
        {
            let mut add = |kind: u32, other: u64| {
                let other = Target::Node(window_id, other);
                match relations.iter_mut().find(|(known, _)| *known == kind) {
                    Some((_, targets)) => targets.push(other),
                    None => relations.push((kind, vec![other])),
                }
            };
            // `AtspiRelationType`: 1 label-for, 2 labelled-by,
            // 3 controller-for, 4 controlled-by, 17 description-for,
            // 18 described-by.
            for relation in &node.entry.node.relations {
                let (kind, other) = match *relation {
                    SemanticRelation::LabelledBy(other) => (2, other),
                    SemanticRelation::DescribedBy(other) => (18, other),
                    SemanticRelation::Controls(other) => (3, other),
                };
                if let Some(other) = window.tree.identity(other) {
                    add(kind, other);
                }
            }
            if node.entry.part.is_none() {
                for (&other, candidate) in &window.tree.nodes {
                    for relation in &candidate.entry.node.relations {
                        let kind = match *relation {
                            SemanticRelation::LabelledBy(to) if to == node.entry.id => 1,
                            SemanticRelation::DescribedBy(to) if to == node.entry.id => 17,
                            SemanticRelation::Controls(to) if to == node.entry.id => 4,
                            _ => continue,
                        };
                        add(kind, other);
                    }
                }
            }
        }
        Value::array(
            "(ua(so))",
            relations
                .into_iter()
                .map(|(kind, targets)| {
                    Value::Struct(vec![
                        Value::from(kind),
                        Value::array(
                            "(so)",
                            targets
                                .into_iter()
                                .map(|target| self.reference(target))
                                .collect(),
                        ),
                    ])
                })
                .collect(),
        )
    }

    fn component(&self, model: &mut Model, target: Target) -> Reply {
        let (window_id, node) = match target {
            Target::Window(window) => (window, None),
            Target::Node(window, node) => (window, Some(node)),
            _ => return Reply::unknown_method(self.message),
        };
        let Some(window) = model.windows.get_mut(&window_id) else {
            return unknown_object(&target.path());
        };
        let (rect, parent) = match node.and_then(|node| window.node(node)) {
            Some(node) => (
                node.entry.bounds,
                Some(
                    node.parent
                        .and_then(|parent| window.node(parent))
                        .map_or(window.bounds(), |parent| parent.entry.bounds),
                ),
            ),
            None => (window.bounds(), None),
        };
        let values = match self.member().as_str() {
            "GetExtents" => vec![Value::Struct(extents_value(convert(
                window,
                rect,
                parent,
                self.uint(0),
            )))],
            "GetPosition" => {
                let rect = convert(window, rect, parent, self.uint(0));
                extents_value(rect).into_iter().take(2).collect()
            }
            "GetSize" => extents_value(rect).into_iter().skip(2).collect(),
            "Contains" => {
                let point = Point::new(self.int(0) as f64, self.int(1) as f64);
                vec![Value::from(contains(
                    rect,
                    to_window(window, point, parent, self.uint(2)),
                ))]
            }
            "GetAccessibleAtPoint" => {
                let point = Point::new(self.int(0) as f64, self.int(1) as f64);
                let point = to_window(window, point, parent, self.uint(2));
                let hit = deepest(window, node, point);
                vec![hit.map_or_else(
                    || self.null(),
                    |hit| self.reference(Target::Node(window_id, hit)),
                )]
            }
            // `AtspiComponentLayer`: 3 widget, 7 window.
            "GetLayer" => vec![Value::from(if node.is_some() { 3u32 } else { 7 })],
            "GetMDIZOrder" => vec![Value::from(0i16)],
            "GetAlpha" => vec![Value::from(1.0)],
            "GrabFocus" => {
                vec![Value::from(node.is_some_and(|node| {
                    window.request(node, SemanticAction::Focus, None)
                }))]
            }
            "ScrollTo" | "ScrollToPoint" | "SetExtents" | "SetPosition" | "SetSize" => {
                vec![Value::from(false)]
            }
            _ => return Reply::unknown_method(self.message),
        };
        Reply::Return(values)
    }

    fn application(&self) -> Reply {
        match self.member().as_str() {
            "GetLocale" => Reply::Return(vec![Value::from(locale())]),
            "RegisterEventListener" | "DeregisterEventListener" => Reply::Return(Vec::new()),
            "GetApplicationBusAddress" => Reply::Return(vec![Value::from("")]),
            _ => Reply::unknown_method(self.message),
        }
    }
}

fn unknown_object(path: &str) -> Reply {
    Reply::error(
        "org.freedesktop.DBus.Error.UnknownObject",
        format!("no accessible object at {path}"),
    )
}

/// A count or an index as an AT-SPI `i`.
pub fn count(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// The language of the application, such as `en_US`.
fn locale() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty())
        .map(|value| value.split(['.', '@']).next().unwrap_or("C").to_owned())
        .unwrap_or_else(|| "C".to_owned())
}

fn exists(model: &Model, target: Target) -> bool {
    match target {
        Target::Root | Target::Cache => true,
        Target::Window(window) => model.windows.contains_key(&window),
        Target::Node(window, node) => model
            .windows
            .get(&window)
            .is_some_and(|window| window.node(node).is_some()),
    }
}

fn parent(model: &Model, target: Target) -> Option<Target> {
    match target {
        Target::Root | Target::Cache => None,
        Target::Window(_) => Some(Target::Root),
        Target::Node(window_id, node) => {
            let parent = model.windows.get(&window_id)?.node(node)?.parent;
            Some(parent.map_or(Target::Window(window_id), |parent| {
                Target::Node(window_id, parent)
            }))
        }
    }
}

fn children(model: &Model, target: Target) -> Vec<Target> {
    match target {
        Target::Root => model.windows.keys().map(|&id| Target::Window(id)).collect(),
        Target::Window(window_id) => {
            model
                .windows
                .get(&window_id)
                .map_or_else(Vec::new, |window| {
                    window
                        .tree
                        .roots
                        .iter()
                        .map(|&node| Target::Node(window_id, node))
                        .collect()
                })
        }
        Target::Node(window_id, node) => model
            .windows
            .get(&window_id)
            .and_then(|window| window.node(node))
            .map_or_else(Vec::new, |node| {
                node.children
                    .iter()
                    .map(|&child| Target::Node(window_id, child))
                    .collect()
            }),
        Target::Cache => Vec::new(),
    }
}

fn node_of(model: &Model, target: Target) -> Option<&Node> {
    match target {
        Target::Node(window, node) => model.windows.get(&window)?.node(node),
        _ => None,
    }
}

fn name(model: &Model, target: Target) -> String {
    match target {
        Target::Root => model.app_name.clone(),
        Target::Window(window) => model
            .windows
            .get(&window)
            .map(WindowModel::current_title)
            .unwrap_or_default(),
        _ => node_of(model, target).map_or_else(String::new, |node| node.entry.node.name.clone()),
    }
}

fn description(model: &Model, target: Target) -> String {
    node_of(model, target).map_or_else(String::new, |node| node.entry.node.description.clone())
}

fn role(model: &Model, target: Target) -> roles::Role {
    match target {
        Target::Root | Target::Cache => roles::APPLICATION,
        Target::Window(_) => roles::FRAME,
        Target::Node(..) => node_of(model, target).map_or(roles::PANEL, |node| {
            let multiline = node.entry.node.text.as_ref().is_some_and(|t| t.multiline);
            roles::atspi_role(node.entry.node.role, multiline)
        }),
    }
}

/// The states of a window frame.
pub fn window_states(window: &WindowModel) -> StateSet {
    let mut set = StateSet::default();
    for state in [state::ENABLED, state::SENSITIVE, state::RESIZABLE] {
        set.insert(state);
    }
    if window.visible() {
        set.insert(state::VISIBLE);
        set.insert(state::SHOWING);
    }
    if window.active() {
        set.insert(state::ACTIVE);
    }
    set
}

fn states(model: &Model, target: Target) -> StateSet {
    match target {
        Target::Window(window) => model
            .windows
            .get(&window)
            .map(window_states)
            .unwrap_or_default(),
        Target::Node(..) => node_of(model, target)
            .map(|node| roles::states(&node.entry))
            .unwrap_or_default(),
        Target::Root | Target::Cache => StateSet::default(),
    }
}

fn attributes(model: &Model, target: Target) -> Value {
    let mut entries = vec![("toolkit".to_owned(), "ChoscorUI".to_owned())];
    if let Target::Node(window_id, _) = target
        && let Some(node) = node_of(model, target)
    {
        if let Some(role) = roles::xml_role(node.entry.node.role) {
            entries.push(("xml-roles".to_owned(), role.to_owned()));
        }
        if let Some(index) = node.entry.item_index {
            entries.push(("posinset".to_owned(), (index + 1).to_string()));
            let size = node
                .parent
                .and_then(|parent| model.windows.get(&window_id)?.node(parent))
                .and_then(|parent| parent.entry.collection)
                .map(|collection| collection.item_count);
            if let Some(size) = size {
                entries.push(("setsize".to_owned(), size.to_string()));
            }
        }
    }
    Value::Dict {
        key: "s".to_owned(),
        value: "s".to_owned(),
        entries: entries
            .into_iter()
            .map(|(key, value)| (Value::from(key), Value::from(value)))
            .collect(),
    }
}

/// The deepest node under `from` (the window for `None`) that contains
/// `point`. A later sibling paints over an earlier one, so it wins.
fn deepest(window: &WindowModel, from: Option<u64>, point: Point) -> Option<u64> {
    let mut found = None;
    let mut level = window.tree.children(from);
    loop {
        let hit = level.iter().rev().copied().find(|&child| {
            window.node(child).is_some_and(|node| {
                !node.entry.node.state.hidden && contains(node.entry.bounds, point)
            })
        });
        match hit {
            Some(hit) => {
                found = Some(hit);
                level = window.tree.children(Some(hit));
            }
            None => return found,
        }
    }
}
