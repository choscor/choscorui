//! The status item: a `StatusNotifierItem` that registers with the
//! `org.kde.StatusNotifierWatcher`, with its menu on `com.canonical.dbusmenu`.

use super::dbusmenu::{self, MenuModel};
use super::introspect;
use crate::dbus::{
    BUS_NAME, Connection, Error, Export, MatchRule, Message, MethodHandler, NameFlags, Reply,
    Subscription, Value,
};
use choscorui_core::{DesktopEvent, StatusItemSpec};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

pub const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
pub const ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";
pub const ITEM_PATH: &str = "/StatusNotifierItem";
pub const MENU_PATH: &str = "/MenuBar";

/// True when a watcher runs and a host shows its items. A watcher that
/// does not answer the host property counts as one with a host.
pub fn host_available(connection: &Connection) -> bool {
    if !connection.name_has_owner(WATCHER).unwrap_or(false) {
        return false;
    }
    connection
        .get_property(
            WATCHER,
            WATCHER_PATH,
            WATCHER,
            "IsStatusNotifierHostRegistered",
        )
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(true)
}

/// What the item shows.
struct ItemState {
    app_id: String,
    title: String,
    tooltip: String,
    visible: bool,
    has_menu: bool,
    menu: MenuModel,
}

impl ItemState {
    fn status(&self) -> &'static str {
        if self.visible { "Active" } else { "Passive" }
    }

    fn properties(&self) -> Vec<(&'static str, Value)> {
        let no_pixmaps = || Value::array("(iiay)", Vec::new());
        vec![
            ("Category", Value::from("ApplicationStatus")),
            ("Id", Value::from(self.app_id.as_str())),
            ("Title", Value::from(self.title.as_str())),
            ("Status", Value::from(self.status())),
            ("WindowId", Value::from(0i32)),
            ("IconName", Value::from(self.app_id.as_str())),
            ("IconPixmap", no_pixmaps()),
            ("OverlayIconName", Value::from("")),
            ("OverlayIconPixmap", no_pixmaps()),
            ("AttentionIconName", Value::from("")),
            ("AttentionIconPixmap", no_pixmaps()),
            ("AttentionMovieName", Value::from("")),
            (
                "ToolTip",
                Value::Struct(vec![
                    Value::from(""),
                    no_pixmaps(),
                    Value::from(self.title.as_str()),
                    Value::from(self.tooltip.as_str()),
                ]),
            ),
            ("ItemIsMenu", Value::from(self.has_menu)),
            ("Menu", Value::object_path(MENU_PATH)),
        ]
    }
}

const INTROSPECTION: &str = r#"<interface name="org.kde.StatusNotifierItem">
<property name="Category" type="s" access="read"/>
<property name="Id" type="s" access="read"/>
<property name="Title" type="s" access="read"/>
<property name="Status" type="s" access="read"/>
<property name="WindowId" type="i" access="read"/>
<property name="IconName" type="s" access="read"/>
<property name="IconPixmap" type="a(iiay)" access="read"/>
<property name="OverlayIconName" type="s" access="read"/>
<property name="OverlayIconPixmap" type="a(iiay)" access="read"/>
<property name="AttentionIconName" type="s" access="read"/>
<property name="AttentionIconPixmap" type="a(iiay)" access="read"/>
<property name="AttentionMovieName" type="s" access="read"/>
<property name="ToolTip" type="(sa(iiay)ss)" access="read"/>
<property name="ItemIsMenu" type="b" access="read"/>
<property name="Menu" type="o" access="read"/>
<method name="ContextMenu"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
<method name="Activate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
<method name="SecondaryActivate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
<method name="Scroll"><arg type="i" direction="in"/><arg type="s" direction="in"/></method>
<signal name="NewTitle"/>
<signal name="NewIcon"/>
<signal name="NewAttentionIcon"/>
<signal name="NewOverlayIcon"/>
<signal name="NewToolTip"/>
<signal name="NewStatus"><arg type="s"/></signal>
</interface>"#;

fn handle_item(state: &ItemState, message: &Message) -> Reply {
    if let Some(reply) = introspect::answer(message, INTROSPECTION) {
        return reply;
    }
    if let Some(reply) = introspect::properties(message, |interface| {
        (interface == ITEM_INTERFACE).then(|| state.properties())
    }) {
        return reply;
    }
    match (message.interface().as_deref(), message.member().as_deref()) {
        // The host opens the menu itself; a click has no other action.
        (
            Some(ITEM_INTERFACE),
            Some("ContextMenu" | "Activate" | "SecondaryActivate" | "Scroll"),
        ) => Reply::Return(Vec::new()),
        _ => Reply::unknown_method(message),
    }
}

/// A status item on the bus. Its drop removes the objects and gives the
/// name back, so the watcher forgets the item.
pub struct StatusItem {
    connection: Connection,
    name: String,
    state: Rc<RefCell<ItemState>>,
    watcher: Subscription,
    _item: Export,
    _menu: Export,
}

impl StatusItem {
    /// Exports the item of `spec` and registers it with the watcher.
    /// Menu clicks go to `events`.
    pub fn new(
        connection: &Connection,
        app_id: &str,
        spec: &StatusItemSpec,
        events: Rc<RefCell<VecDeque<DesktopEvent>>>,
    ) -> Result<Self, Error> {
        static NEXT: AtomicU32 = AtomicU32::new(1);
        let name = format!(
            "org.kde.StatusNotifierItem-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let state = Rc::new(RefCell::new(ItemState {
            app_id: app_id.to_owned(),
            title: spec.title.clone(),
            tooltip: spec.tooltip.clone().unwrap_or_default(),
            visible: true,
            has_menu: spec.menu.is_some(),
            menu: MenuModel::new(spec.menu.as_ref()),
        }));
        let watcher = connection.subscribe(
            MatchRule::signal()
                .sender(BUS_NAME)
                .interface(BUS_NAME)
                .member("NameOwnerChanged")
                .arg0(WATCHER),
        )?;
        connection.request_name(
            &name,
            NameFlags {
                do_not_queue: true,
                ..NameFlags::default()
            },
        )?;
        let item_state = Rc::clone(&state);
        let item: MethodHandler = Rc::new(move |_connection: &Connection, message: &Message| {
            handle_item(&item_state.borrow(), message)
        });
        let menu_state = Rc::clone(&state);
        let menu: MethodHandler = Rc::new(move |_connection: &Connection, message: &Message| {
            let mut clicks = Vec::new();
            let reply = dbusmenu::handle(&menu_state.borrow().menu, message, &mut clicks);
            events.borrow_mut().extend(clicks);
            reply
        });
        let item = Self {
            connection: connection.clone(),
            _item: connection.export(ITEM_PATH, item)?,
            _menu: connection.export(MENU_PATH, menu)?,
            name,
            state,
            watcher,
        };
        item.register()?;
        Ok(item)
    }

    fn register(&self) -> Result<(), Error> {
        self.connection
            .call_method(
                WATCHER,
                WATCHER_PATH,
                WATCHER,
                "RegisterStatusNotifierItem",
                &[Value::from(self.name.as_str())],
            )
            .map(drop)
    }

    /// The bus name of the item.
    pub fn bus_name(&self) -> &str {
        &self.name
    }

    fn signal(
        &self,
        interface: &str,
        path: &str,
        member: &str,
        args: &[Value],
    ) -> Result<(), Error> {
        self.connection.emit(path, interface, member, args)
    }

    /// Shows `spec`: it sends a signal for each part that changed.
    pub fn update(&self, spec: &StatusItemSpec) -> Result<(), Error> {
        let tooltip = spec.tooltip.clone().unwrap_or_default();
        let (title_changed, tooltip_changed, menu_revision) = {
            let mut state = self.state.borrow_mut();
            let title_changed = state.title != spec.title;
            let tooltip_changed = title_changed || state.tooltip != tooltip;
            state.title.clone_from(&spec.title);
            state.tooltip = tooltip;
            let new_menu = MenuModel::new(spec.menu.as_ref());
            let menu_changed =
                state.has_menu != spec.menu.is_some() || !state.menu.same_items(&new_menu);
            state.has_menu = spec.menu.is_some();
            if menu_changed {
                state.menu.replace(spec.menu.as_ref());
            }
            (
                title_changed,
                tooltip_changed,
                menu_changed.then(|| state.menu.revision()),
            )
        };
        if title_changed {
            self.signal(ITEM_INTERFACE, ITEM_PATH, "NewTitle", &[])?;
        }
        if tooltip_changed {
            self.signal(ITEM_INTERFACE, ITEM_PATH, "NewToolTip", &[])?;
        }
        if let Some(revision) = menu_revision {
            self.signal(
                dbusmenu::INTERFACE,
                MENU_PATH,
                "LayoutUpdated",
                &[Value::from(revision), Value::from(0i32)],
            )?;
        }
        Ok(())
    }

    /// Shows or hides the item: the `Active` or `Passive` status.
    pub fn set_visible(&self, visible: bool) -> Result<(), Error> {
        let status = {
            let mut state = self.state.borrow_mut();
            if state.visible == visible {
                return Ok(());
            }
            state.visible = visible;
            state.status()
        };
        self.signal(
            ITEM_INTERFACE,
            ITEM_PATH,
            "NewStatus",
            &[Value::from(status)],
        )
    }

    /// Registers again after a new watcher took the name.
    pub fn poll(&self) {
        let restarted = self.watcher.take().iter().any(|signal| {
            signal
                .args()
                .ok()
                .and_then(|args| {
                    args.get(2)
                        .and_then(Value::as_str)
                        .map(|owner| !owner.is_empty())
                })
                .unwrap_or(false)
        });
        if restarted {
            let _ = self.register();
        }
    }
}

impl Drop for StatusItem {
    fn drop(&mut self) {
        let _ = self.connection.release_name(&self.name);
    }
}
