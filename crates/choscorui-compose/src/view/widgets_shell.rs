//! View constructors for window parts: toolbars, status bars, docks, and
//! size grips. Like the views in `widgets`, each takes its content, a
//! `Binding` where the control edits a value, and an action where it
//! fires one.

use super::configured::Configured;
use super::env::Env;
use super::state::IntoLive;
use super::text::TextContent;
use super::widgets::{Items, label, text};
use choscorui_controls as controls;
use choscorui_controls::{
    ControlKind, ControlMessage, ControlValue, DockLayout, StatusSection, ToolbarArea, ToolbarItem,
};
use choscorui_core::Size;

/// A row of command buttons on the `area` edge of a window, with one tab
/// stop and arrow keys between its items. Items that do not fit move to
/// a chevron menu. A command runs `.on_command`.
pub type Toolbar = Configured<controls::HostedToolbar>;

impl Configured<controls::HostedToolbar> {
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
pub type StatusBar = Configured<controls::StatusBar>;

impl Configured<controls::StatusBar> {
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
/// a panel runs `.on_dismiss`.
pub type Dock = Configured<DockLayout>;

impl Configured<DockLayout> {
    pub fn new(layout: impl Fn(&Env) -> DockLayout + 'static) -> Self {
        Configured::custom(ControlKind::Dock, layout)
    }
}

/// A grip in the bottom trailing corner that resizes its window. A drag
/// runs `on_resize` with the new content size, inside `minimum` and
/// `maximum`. In right-to-left layouts the grip sits at the bottom left.
pub type SizeGrip = Configured<controls::SizeGrip>;

impl Configured<controls::SizeGrip> {
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
