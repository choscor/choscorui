//! `com.canonical.dbusmenu`: the menu of the status item, which the
//! status notifier host draws.

use super::introspect;
use crate::dbus::{Message, Reply, Value};
use choscorui_core::{DesktopEvent, MenuCommandId, MenuEntry, MenuItemState, MenuSpec};

pub const INTERFACE: &str = "com.canonical.dbusmenu";

/// One item of the menu. The index in `MenuModel::items` is its id; the
/// root is 0.
#[derive(Clone, Debug, PartialEq)]
enum Item {
    Root,
    Command {
        command: MenuCommandId,
        state: MenuItemState,
    },
    Submenu(String),
    Separator,
}

/// The menu as a tree of numbered items.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MenuModel {
    items: Vec<(Item, Vec<i32>)>,
    revision: u32,
}

/// A title with each `_` doubled, because a single `_` marks a mnemonic.
fn label(title: &str) -> String {
    title.replace('_', "__")
}

impl MenuModel {
    /// The menu of `spec`, or an empty menu.
    pub fn new(spec: Option<&MenuSpec>) -> Self {
        let mut model = Self {
            items: vec![(Item::Root, Vec::new())],
            revision: 1,
        };
        if let Some(spec) = spec {
            let children = model.add_entries(&spec.entries);
            model.items[0].1 = children;
        }
        model
    }

    fn add_entries(&mut self, entries: &[MenuEntry]) -> Vec<i32> {
        let mut ids = Vec::new();
        for entry in entries {
            let index = self.items.len();
            let item = match entry {
                MenuEntry::Command { command, state } => Item::Command {
                    command: *command,
                    state: state.clone(),
                },
                MenuEntry::Submenu { title, .. } => Item::Submenu(title.clone()),
                MenuEntry::Separator => Item::Separator,
            };
            self.items.push((item, Vec::new()));
            if let MenuEntry::Submenu { entries, .. } = entry {
                let children = self.add_entries(entries);
                self.items[index].1 = children;
            }
            ids.push(i32::try_from(index).unwrap_or(i32::MAX));
        }
        ids
    }

    /// Replaces the menu and counts a new revision.
    pub fn replace(&mut self, spec: Option<&MenuSpec>) {
        let revision = self.revision + 1;
        *self = Self::new(spec);
        self.revision = revision;
    }

    /// True when `other` has the same items, whatever its revision.
    pub fn same_items(&self, other: &Self) -> bool {
        self.items == other.items
    }

    pub fn revision(&self) -> u32 {
        self.revision
    }

    fn item(&self, id: i32) -> Option<&(Item, Vec<i32>)> {
        usize::try_from(id)
            .ok()
            .and_then(|index| self.items.get(index))
    }

    /// The properties of the item `id`, limited to `names` unless empty.
    fn properties(&self, id: i32, names: &[String]) -> Value {
        let mut properties: Vec<(&'static str, Value)> = Vec::new();
        match self.item(id) {
            Some((Item::Root, _)) => {
                properties.push(("children-display", Value::from("submenu")));
            }
            Some((Item::Command { state, .. }, _)) => {
                properties.push(("label", Value::from(label(&state.title))));
                properties.push(("enabled", Value::from(state.enabled)));
                if state.checked {
                    properties.push(("toggle-type", Value::from("checkmark")));
                    properties.push(("toggle-state", Value::from(1i32)));
                }
                if let Some(shortcut) = &state.shortcut {
                    let mut keys = Vec::new();
                    let modifiers = shortcut.modifiers;
                    // The command modifier is Control on Linux.
                    if modifiers.control || modifiers.command {
                        keys.push("Control".to_owned());
                    }
                    if modifiers.option {
                        keys.push("Alt".to_owned());
                    }
                    if modifiers.shift {
                        keys.push("Shift".to_owned());
                    }
                    keys.push(shortcut.key.to_uppercase());
                    properties.push(("shortcut", Value::array("as", vec![Value::strings(keys)])));
                }
            }
            Some((Item::Submenu(title), _)) => {
                properties.push(("label", Value::from(label(title))));
                properties.push(("children-display", Value::from("submenu")));
            }
            Some((Item::Separator, _)) => {
                properties.push(("type", Value::from("separator")));
            }
            None => {}
        }
        if !names.is_empty() {
            properties.retain(|(name, _)| names.iter().any(|wanted| wanted == name));
        }
        Value::vardict(properties)
    }

    /// The layout `(ia{sv}av)` of the item `id` and its children to
    /// `depth` levels (all for -1).
    fn layout(&self, id: i32, depth: i32, names: &[String]) -> Value {
        let children = match self.item(id) {
            Some((_, children)) if depth != 0 => children
                .iter()
                .map(|child| Value::variant(self.layout(*child, depth - 1, names)))
                .collect(),
            _ => Vec::new(),
        };
        Value::Struct(vec![
            Value::from(id),
            self.properties(id, names),
            Value::array("v", children),
        ])
    }

    /// The command of the item `id`, when it is enabled.
    fn command(&self, id: i32) -> Option<MenuCommandId> {
        match self.item(id) {
            Some((Item::Command { command, state }, _)) if state.enabled => Some(*command),
            _ => None,
        }
    }

    /// The event of a `clicked` event on the item `id`.
    fn click(&self, id: i32, event: &str) -> Option<DesktopEvent> {
        (event == "clicked")
            .then(|| self.command(id))
            .flatten()
            .map(DesktopEvent::StatusMenu)
    }
}

const INTROSPECTION: &str = r#"<interface name="com.canonical.dbusmenu">
<property name="Version" type="u" access="read"/>
<property name="TextDirection" type="s" access="read"/>
<property name="Status" type="s" access="read"/>
<property name="IconThemePath" type="as" access="read"/>
<method name="GetLayout"><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="as" direction="in"/><arg type="u" direction="out"/><arg type="(ia{sv}av)" direction="out"/></method>
<method name="GetGroupProperties"><arg type="ai" direction="in"/><arg type="as" direction="in"/><arg type="a(ia{sv})" direction="out"/></method>
<method name="GetProperty"><arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
<method name="Event"><arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="in"/><arg type="u" direction="in"/></method>
<method name="EventGroup"><arg type="a(isvu)" direction="in"/><arg type="ai" direction="out"/></method>
<method name="AboutToShow"><arg type="i" direction="in"/><arg type="b" direction="out"/></method>
<method name="AboutToShowGroup"><arg type="ai" direction="in"/><arg type="ai" direction="out"/><arg type="ai" direction="out"/></method>
<signal name="ItemsPropertiesUpdated"><arg type="a(ia{sv})"/><arg type="a(ias)"/></signal>
<signal name="LayoutUpdated"><arg type="u"/><arg type="i"/></signal>
<signal name="ItemActivationRequested"><arg type="i"/><arg type="u"/></signal>
</interface>"#;

fn int(value: Option<&Value>) -> Option<i32> {
    value
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
}

fn ints(value: Option<&Value>) -> Vec<i32> {
    value
        .and_then(Value::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(|item| int(Some(item)))
        .collect()
}

/// The answer to a call of the menu object. Clicks go to `events`.
pub fn handle(model: &MenuModel, message: &Message, events: &mut Vec<DesktopEvent>) -> Reply {
    if let Some(reply) = introspect::answer(message, INTROSPECTION) {
        return reply;
    }
    if let Some(reply) = introspect::properties(message, |interface| {
        (interface == INTERFACE).then(|| {
            vec![
                ("Version", Value::from(3u32)),
                ("TextDirection", Value::from("ltr")),
                ("Status", Value::from("normal")),
                ("IconThemePath", Value::strings(Vec::<String>::new())),
            ]
        })
    }) {
        return reply;
    }
    if message.interface().as_deref() != Some(INTERFACE) {
        return Reply::unknown_method(message);
    }
    let args = message.args().unwrap_or_default();
    let names = args.get(2).and_then(Value::as_strings).unwrap_or_default();
    match message.member().as_deref() {
        Some("GetLayout") => {
            let parent = int(args.first()).unwrap_or(0);
            let depth = int(args.get(1)).unwrap_or(-1);
            Reply::Return(vec![
                Value::from(model.revision()),
                model.layout(parent, depth, &names),
            ])
        }
        Some("GetGroupProperties") => {
            let names = args.get(1).and_then(Value::as_strings).unwrap_or_default();
            let items = ints(args.first())
                .into_iter()
                .filter(|id| model.item(*id).is_some())
                .map(|id| Value::Struct(vec![Value::from(id), model.properties(id, &names)]))
                .collect();
            Reply::Return(vec![Value::array("(ia{sv})", items)])
        }
        Some("GetProperty") => {
            let id = int(args.first()).unwrap_or(-1);
            let name = args
                .get(1)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            match model.properties(id, std::slice::from_ref(&name)).get(&name) {
                Some(value) => Reply::Return(vec![Value::variant(value.clone())]),
                None => Reply::invalid_args(format!("item {id} has no property {name}")),
            }
        }
        Some("Event") => {
            let id = int(args.first()).unwrap_or(-1);
            let event = args.get(1).and_then(Value::as_str).unwrap_or_default();
            events.extend(model.click(id, event));
            Reply::Return(Vec::new())
        }
        Some("EventGroup") => {
            let mut missing = Vec::new();
            for group in args.first().and_then(Value::as_array).unwrap_or_default() {
                let fields = group.as_struct().unwrap_or_default();
                let id = int(fields.first()).unwrap_or(-1);
                if model.item(id).is_none() {
                    missing.push(Value::from(id));
                    continue;
                }
                let event = fields.get(1).and_then(Value::as_str).unwrap_or_default();
                events.extend(model.click(id, event));
            }
            Reply::Return(vec![Value::array("i", missing)])
        }
        Some("AboutToShow") => Reply::Return(vec![Value::from(false)]),
        Some("AboutToShowGroup") => Reply::Return(vec![
            Value::array("i", Vec::new()),
            Value::array("i", Vec::new()),
        ]),
        _ => Reply::unknown_method(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use choscorui_core::{MenuShortcut, MenuShortcutModifiers};

    fn spec() -> MenuSpec {
        let mut save = MenuItemState::new("Save_As");
        save.shortcut = Some(MenuShortcut {
            key: "s".to_owned(),
            modifiers: MenuShortcutModifiers {
                command: true,
                shift: true,
                ..MenuShortcutModifiers::default()
            },
        });
        let mut off = MenuItemState::new("Off");
        off.enabled = false;
        MenuSpec::new(
            "Status",
            vec![
                MenuEntry::command(MenuCommandId(1), save),
                MenuEntry::separator(),
                MenuEntry::submenu("More", vec![MenuEntry::command(MenuCommandId(2), off)]),
            ],
        )
    }

    #[test]
    fn the_layout_numbers_items_depth_first() {
        let model = MenuModel::new(Some(&spec()));
        let layout = model.layout(0, -1, &[]);
        assert_eq!(layout.signature(), "(ia{sv}av)");
        assert!(layout.check().is_ok());
        let fields = layout.as_struct().unwrap();
        let children = fields[2].as_array().unwrap();
        assert_eq!(children.len(), 3);
        let save = children[0].as_struct().unwrap();
        assert_eq!(save[0].as_i64(), Some(1));
        assert_eq!(
            save[1].get("label").and_then(Value::as_str),
            Some("Save__As")
        );
        let shortcut = save[1].get("shortcut").unwrap().as_array().unwrap()[0].as_strings();
        assert_eq!(shortcut.unwrap(), ["Control", "Shift", "S"]);
        let separator = children[1].as_struct().unwrap();
        assert_eq!(
            separator[1].get("type").and_then(Value::as_str),
            Some("separator")
        );
        let more = children[2].as_struct().unwrap();
        assert_eq!(more[2].as_array().unwrap().len(), 1);
        // A depth of 1 leaves out the grandchildren.
        let shallow = model.layout(0, 1, &[]);
        let more = shallow.as_struct().unwrap()[2].as_array().unwrap()[2]
            .as_struct()
            .unwrap()[2]
            .clone();
        assert_eq!(more.as_array().unwrap().len(), 0);
    }

    #[test]
    fn only_enabled_commands_click() {
        let model = MenuModel::new(Some(&spec()));
        assert_eq!(
            model.click(1, "clicked"),
            Some(DesktopEvent::StatusMenu(MenuCommandId(1)))
        );
        assert_eq!(model.click(1, "hovered"), None);
        assert_eq!(model.click(4, "clicked"), None);
        assert_eq!(model.click(99, "clicked"), None);
        let mut model = model;
        model.replace(None);
        assert_eq!(model.revision(), 2);
        assert!(
            model.layout(0, -1, &[]).as_struct().unwrap()[2]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}
