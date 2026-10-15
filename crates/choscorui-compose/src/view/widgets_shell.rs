//! View constructors for window parts: toolbars, status bars, docks,
//! size grips, and title bars. Like the views in `widgets`, each takes its content, a
//! `Binding` where the control edits a value, and an action where it
//! fires one.

use super::configured::Configured;
use super::control_ref::ControlRef;
use super::env::Env;
use super::state::{Binding, IntoLive};
use super::text::TextContent;
use super::widgets::{Items, label, text};
use choscorui_controls as controls;
use choscorui_controls::{
    ControlKind, ControlMessage, ControlValue, DockLayout, StatusSection, TitleBarHandle,
    ToolbarArea, ToolbarItem,
};
use choscorui_core::Size;

/// A row of command buttons on the `area` edge of a window, with one tab
/// stop and arrow keys between its items. Items that do not fit move to
/// a chevron menu. A command runs `.on_command`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::{CommandId, ToolbarArea, ToolbarItem};
/// let items = vec![ToolbarItem::command("new", "New", CommandId(1))];
/// let toolbar = Toolbar::new("Main", ToolbarArea::Top, items)
///     .on_command(|command| println!("run {command:?}"));
/// ```
pub type Toolbar = Configured<controls::HostedToolbar>;

impl Configured<controls::HostedToolbar> {
    /// A toolbar named `name` on the `area` edge with `items`.
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        area: ToolbarArea,
        items: impl Items<ToolbarItem>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Toolbar, move |env| {
            let mut toolbar = controls::Toolbar::new(text(&name, env), items.items(env));
            toolbar.move_to(area);
            toolbar.hosted()
        })
    }
}

/// A status bar that shows `message` at the start and `sections` at the
/// end. It takes no input.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::StatusSection;
/// let line = StatusSection { key: "line".into(), text: "Ln 12".into(), width: 60.0 };
/// let bar = StatusBar::new("Ready", vec![line]);
/// ```
pub type StatusBar = Configured<controls::StatusBar>;

impl Configured<controls::StatusBar> {
    /// A status bar with `message` and `sections`.
    pub fn new<M>(
        message: impl IntoLive<TextContent, M>,
        sections: impl Items<StatusSection>,
    ) -> Self {
        let message = label(message);
        Configured::custom(ControlKind::StatusBar, move |env| {
            let mut bar = controls::StatusBar::new();
            bar.set_default_text(text(&message, env));
            for section in sections.items(env) {
                bar.set_section(&section.key, section.text, section.width);
            }
            bar
        })
    }
}

/// Dockable panels around a center area, as `layout` arranges them.
/// Activating a tab reports the panel id through `.on_item`, and closing
/// a panel runs `.on_dismiss`. `.arrangement` keeps the saved text of the
/// arrangement in a binding. A hosted dock paints the text of each panel
/// and of the center: give them with `DockPanel::with_text` and
/// `DockLayout::with_center_text`.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::{DockArea, DockLayout, DockPanel, DockTarget, PanelId};
/// struct Editor;
///
/// impl View for Editor {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let saved = cx.state(String::new());
///         Dock::new(|_: &Env| {
///             let mut dock = DockLayout::new(28.0).with_center_text("Editor");
///             let files = DockPanel::new("files", "Files").with_text("12 files");
///             let target = DockTarget::Tab { area: DockArea::Left, group: 0 };
///             let _ = dock.add(PanelId(1), files, target);
///             dock
///         })
///         .arrangement(saved.binding())
///     }
/// }
/// ```
pub type Dock = Configured<DockLayout>;

impl Configured<DockLayout> {
    /// A dock with the layout that `layout` makes.
    pub fn new(layout: impl Fn(&Env) -> DockLayout + 'static) -> Self {
        Configured::custom(ControlKind::Dock, layout)
    }

    /// Keeps the arrangement in `saved`, as the text of
    /// [`DockLayout::save`]. Each change that the user makes in the dock
    /// writes the text. New text restores its arrangement in the dock;
    /// text that does not restore keeps the arrangement that the dock has.
    #[must_use]
    pub fn arrangement(self, saved: Binding<String>) -> Self {
        let dock = ControlRef::new();
        let read = dock.clone();
        self.control_ref(&dock)
            .value(move || ControlValue::Text(saved.get()))
            .on_message(move |_| {
                if let Some(text) = read.with(DockLayout::save) {
                    saved.set(text);
                }
            })
    }
}

/// A grip in the bottom trailing corner that resizes its window. A drag
/// runs `on_resize` with the new content size, inside `minimum` and
/// `maximum`. In right-to-left layouts the grip sits at the bottom left.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_core::Size;
/// let grip = SizeGrip::new(
///     Size::new(320.0, 240.0),
///     Size::new(1600.0, 1200.0),
///     |size| println!("{size:?}"),
/// );
/// ```
pub type SizeGrip = Configured<controls::SizeGrip>;

impl Configured<controls::SizeGrip> {
    /// A size grip that keeps the size between `minimum` and `maximum`.
    pub fn new(minimum: Size, maximum: Size, on_resize: impl Fn(Size) + 'static) -> Self {
        Configured::custom(ControlKind::SizeGrip, move |_| {
            controls::SizeGrip::new(minimum, maximum, false)
        })
        .on_message(move |message| {
            if let ControlMessage::Changed(ControlValue::Range(width, height)) = message {
                on_resize(Size::new(*width, *height));
            }
        })
    }
}

/// The title bar of a window that draws its own frame: the title and the
/// minimize, maximize, and close buttons. The bar puts each window
/// request in `handle` and then reports `Activated`, so `.on_activate`
/// takes the requests with [`TitleBarHandle::take_requests`]. The window
/// sets the maximized and active states through the handle.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_controls::TitleBarHandle;
/// let handle = TitleBarHandle::new();
/// let requests = handle.clone();
/// let bar = TitleBar::new("Notes", handle).on_activate(move || {
///     for request in requests.take_requests() {
///         println!("{request:?}");
///     }
/// });
/// ```
pub type TitleBar = Configured<controls::TitleBar>;

impl Configured<controls::TitleBar> {
    /// A title bar that shows `title` and shares the window state through `handle`.
    pub fn new<M>(title: impl IntoLive<TextContent, M>, handle: TitleBarHandle) -> Self {
        let title = label(title);
        Configured::custom(ControlKind::TitleBar, move |env| {
            controls::TitleBar::new(text(&title, env), handle.clone())
        })
    }
}
