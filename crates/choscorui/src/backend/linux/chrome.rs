//! The window chrome that the toolkit draws on Linux: the title bar of a
//! window without server-side decorations, and the in-window menu bar.
//! Both are controls that the facade puts above the content of a window.
//! The policy is in the controls; this module connects them to the
//! native window and to the menu queue.

use choscorui_compose::view::{
    AnyView, Binding, Env, Frame, Localized, Menubar, TitleBar, VStack, View, ViewExt, ViewHost,
};
use choscorui_compose::{Property, Runtime};
use choscorui_controls::{
    CommandId, CommandState, MenuCheck, MenuItem, MenuItemVariant, ResizeEdge as FrameEdge,
    Shortcut, ShortcutModifiers, TitleBarHandle, TitleBarRequest, WindowFrame, WindowRegion,
    shortcut_label,
};
use choscorui_core::{
    CursorShape, Key, KeyConvention, KeyEvent, MenuBarModel, MenuCommandId, MenuEntry,
    MenuShortcut, MenuShortcutModifiers, Point, PointerButton, PointerEvent, Rect,
};
use choscorui_linux::{MainMenu, ResizeEdge, Window};
use std::cell::RefCell;
use std::rc::Rc;

/// The catalog id of the menu bar's accessible name.
const MENU_BAR_NAME: &str = "choscorui.menubar.name";

/// The menus that the menu bars of all windows show.
#[derive(Default)]
pub(super) struct Menus {
    /// The installed menus, with the queue of chosen commands.
    pub(super) installed: Rc<RefCell<Option<MainMenu>>>,
    /// The model that the menu bar views read, made with the runtime of
    /// the first window.
    model: RefCell<Option<Property<Option<MenuBarModel>>>>,
}

impl Menus {
    /// Replaces the installed menus. Each menu bar shows the new model.
    pub(super) fn install(&self, menu: MainMenu) {
        let model = menu.model();
        *self.installed.borrow_mut() = Some(menu);
        if let Some(property) = &*self.model.borrow() {
            property.set(Some(model));
        }
    }

    /// The model property of the menu bars.
    fn model(&self, runtime: &Runtime) -> Property<Option<MenuBarModel>> {
        let installed = self.installed.borrow().as_ref().map(MainMenu::model);
        self.model
            .borrow_mut()
            .get_or_insert_with(|| runtime.property(installed))
            .clone()
    }
}

/// The chrome of one window and the state that connects it to the
/// native window.
pub(super) struct Chrome {
    /// The title bar of a window that draws its own decorations.
    title_bar: Option<TitleBarHandle>,
    /// The menu bar's focus, and the menus that it shows.
    menu_bar: Option<(Property<bool>, Property<Option<MenuBarModel>>)>,
    installed: Rc<RefCell<Option<MainMenu>>>,
    /// The resize edge under the pointer.
    hover: Option<ResizeEdge>,
    active: bool,
}

impl Chrome {
    /// The chrome of `window`, and `content` below it. A window gets a
    /// title bar when the display server does not draw decorations, and
    /// a menu bar when the application installed menus.
    pub(super) fn compose(
        window: &Window,
        menus: &Menus,
        runtime: &Runtime,
        content: AnyView,
    ) -> (Self, AnyView) {
        let title_bar = (!window.has_server_decorations()).then(|| {
            let handle = TitleBarHandle::new();
            handle.set_title(window.title());
            handle
        });
        let menu_bar = menus
            .installed
            .borrow()
            .is_some()
            .then(|| (runtime.property(false), menus.model(runtime)));
        let chrome = Self {
            title_bar: title_bar.clone(),
            menu_bar: menu_bar.clone(),
            installed: menus.installed.clone(),
            hover: None,
            active: true,
        };
        if title_bar.is_none() && menu_bar.is_none() {
            return (chrome, content);
        }
        let fill = Frame::new()
            .max_width(f64::INFINITY)
            .max_height(f64::INFINITY);
        let title_bar = title_bar.map(|handle| {
            TitleBar::new(handle.title(), handle).frame(Frame::new().max_width(f64::INFINITY))
        });
        let menu_bar =
            menu_bar.map(|(focus, model)| menu_bar_view(focus, model, menus.installed.clone()));
        let view = VStack::new((title_bar, menu_bar, content.frame(fill)))
            .spacing(0.0)
            .frame(fill);
        (chrome, AnyView::new(view))
    }

    /// Handles a pointer event before the views: a primary press on the
    /// resize band starts a resize. Returns true when it used the event.
    pub(super) fn pointer(
        &mut self,
        window: &Window,
        host: &ViewHost,
        event: PointerEvent,
    ) -> bool {
        if self.title_bar.is_none() {
            return false;
        }
        let geometry = window.geometry();
        let band = host.themes().application().spacing;
        let frame = WindowFrame::new(
            Rect::new(Point::new(0.0, 0.0), window.content_size()),
            Rect::default(),
            Vec::new(),
        )
        .with_resize_edges(0.0, band)
        .with_maximized(geometry.zoomed);
        self.hover = match frame.region(event.position()) {
            WindowRegion::Resize(edge) => Some(native_edge(edge)),
            _ => None,
        };
        match (self.hover, event) {
            (
                Some(edge),
                PointerEvent::Down {
                    button: PointerButton::Primary,
                    ..
                },
            ) => {
                window.begin_resize(edge);
                true
            }
            _ => false,
        }
    }

    /// Handles a key before the views: a menu shortcut runs its command,
    /// F10 focuses the menu bar, and Alt with a mnemonic opens a menu.
    /// Returns true when it used the key.
    pub(super) fn key(
        &self,
        host: &mut ViewHost,
        event: KeyEvent,
        modifiers: ShortcutModifiers,
    ) -> bool {
        let KeyEvent::Down { key, repeat } = event else {
            return false;
        };
        // On Linux, Ctrl is the menu shortcut key: `command` in the model.
        let logical = modifiers.logical(KeyConvention::Pc);
        if let Key::Character(character) = key
            && logical.is_chord()
            && !repeat
            && let Some(menu) = &*self.installed.borrow()
            && menu
                .perform_key_equivalent(
                    &character.to_string(),
                    MenuShortcutModifiers {
                        shift: logical.shift,
                        control: logical.control,
                        option: logical.option,
                        command: logical.command,
                    },
                )
                .unwrap_or(false)
        {
            return true;
        }
        let Some((focus, model)) = &self.menu_bar else {
            return false;
        };
        let alt = ShortcutModifiers {
            option: true,
            ..ShortcutModifiers::default()
        };
        let opens = match key {
            Key::F(10) => modifiers == ShortcutModifiers::default(),
            Key::Character(character) => {
                modifiers == alt
                    && model.with(|model| {
                        model
                            .as_ref()
                            .and_then(|model| model.menu_for_mnemonic(character))
                            .is_some()
                    })
            }
            _ => false,
        };
        if !opens {
            return false;
        }
        focus.set(true);
        host.flush();
        if key != Key::F(10) {
            // The focused bar opens the menu of the mnemonic.
            host.key(event, modifiers);
        }
        true
    }

    /// Records the window's focus for the title bar.
    pub(super) fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Runs the title bar's requests on `window` and shows the window's
    /// state in the bar. Returns true when the bar asked to close.
    pub(super) fn sync(&self, window: &Window) -> bool {
        let Some(handle) = &self.title_bar else {
            return false;
        };
        let mut closing = false;
        for request in handle.take_requests() {
            match request {
                TitleBarRequest::Move => window.begin_move(),
                TitleBarRequest::Resize(edge) => window.begin_resize(native_edge(edge)),
                TitleBarRequest::Minimize => window.minimize(),
                TitleBarRequest::ToggleMaximize => {
                    window.set_maximized(!window.geometry().zoomed);
                }
                TitleBarRequest::Close => closing = true,
                TitleBarRequest::ShowWindowMenu(position) => window.show_window_menu(position),
            }
        }
        handle.set_title(window.title());
        handle.set_maximized(window.geometry().zoomed);
        handle.set_active(self.active);
        closing
    }

    /// The cursor over the resize band, or `shape` from the views.
    pub(super) fn cursor(&self, shape: CursorShape) -> CursorShape {
        match self.hover {
            Some(ResizeEdge::Left | ResizeEdge::Right) => CursorShape::ResizeLeftRight,
            Some(ResizeEdge::Top | ResizeEdge::Bottom) => CursorShape::ResizeUpDown,
            Some(ResizeEdge::TopLeft | ResizeEdge::BottomRight) => {
                CursorShape::ResizeNorthWestSouthEast
            }
            Some(ResizeEdge::TopRight | ResizeEdge::BottomLeft) => {
                CursorShape::ResizeNorthEastSouthWest
            }
            None => shape,
        }
    }
}

/// The menu bar of `model`. A chosen command goes to the queue of the
/// installed menus.
fn menu_bar_view(
    focus: Property<bool>,
    model: Property<Option<MenuBarModel>>,
    installed: Rc<RefCell<Option<MainMenu>>>,
) -> impl View {
    let read = model.clone();
    let states = model.clone();
    let focused = focus.clone();
    Menubar::new(Localized::new(MENU_BAR_NAME), move |_: &Env| {
        read.with(|model| {
            model.as_ref().map_or_else(Vec::new, |model| {
                model
                    .menus()
                    .iter()
                    .map(|menu| (menu.title.clone(), menu_items(&menu.entries)))
                    .collect()
            })
        })
    })
    .with(move |mut bar| {
        states.with(|model| {
            let menus = model.as_ref().map_or(&[][..], MenuBarModel::menus);
            for (command, state) in menus.iter().flat_map(|menu| command_states(&menu.entries)) {
                let _ = bar.set_command(command, state);
            }
        });
        bar
    })
    .on_command(move |command| {
        if let Some(menu) = &*installed.borrow() {
            let _ = menu.perform_action(MenuCommandId(command.0));
        }
    })
    .focused(Binding::new(
        move || focused.get(),
        move |value| focus.set(value),
    ))
    .frame(Frame::new().max_width(f64::INFINITY))
}

/// The rows of `entries` for a menu control.
pub(super) fn menu_items(entries: &[MenuEntry]) -> Vec<MenuItem> {
    entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Command { command, state } => MenuItem::Action {
                label: state.title.clone(),
                command: CommandId(command.0),
                shortcut: state.shortcut.as_ref().and_then(shortcut),
                variant: MenuItemVariant::Default,
                check: if state.checked {
                    MenuCheck::Checkbox
                } else {
                    MenuCheck::None
                },
                icon: None,
            },
            MenuEntry::Submenu { title, entries } => MenuItem::Submenu {
                label: title.clone(),
                items: menu_items(entries),
            },
            MenuEntry::Separator => MenuItem::Separator,
        })
        .collect()
}

/// The enabled and checked state of each command in `entries`.
pub(super) fn command_states(entries: &[MenuEntry]) -> Vec<(CommandId, CommandState)> {
    fn collect(entries: &[MenuEntry], states: &mut Vec<(CommandId, CommandState)>) {
        for entry in entries {
            match entry {
                MenuEntry::Command { command, state } => states.push((
                    CommandId(command.0),
                    CommandState {
                        enabled: state.enabled,
                        checked: state.checked,
                    },
                )),
                MenuEntry::Submenu { entries, .. } => collect(entries, states),
                MenuEntry::Separator => {}
            }
        }
    }
    let mut states = Vec::new();
    collect(entries, &mut states);
    states
}

/// The PC label of a menu shortcut, such as `Ctrl+S`.
fn shortcut(shortcut: &MenuShortcut) -> Option<String> {
    let mut characters = shortcut.key.chars();
    let (Some(character), None) = (characters.next(), characters.next()) else {
        return None;
    };
    let modifiers = shortcut.modifiers;
    Some(shortcut_label(
        Shortcut {
            key: Key::Character(character),
            modifiers: ShortcutModifiers {
                shift: modifiers.shift,
                control: modifiers.control,
                option: modifiers.option,
                command: modifiers.command,
            },
        },
        KeyConvention::Pc,
    ))
}

fn native_edge(edge: FrameEdge) -> ResizeEdge {
    match edge {
        FrameEdge::Top => ResizeEdge::Top,
        FrameEdge::Bottom => ResizeEdge::Bottom,
        FrameEdge::Left => ResizeEdge::Left,
        FrameEdge::Right => ResizeEdge::Right,
        FrameEdge::TopLeft => ResizeEdge::TopLeft,
        FrameEdge::TopRight => ResizeEdge::TopRight,
        FrameEdge::BottomLeft => ResizeEdge::BottomLeft,
        FrameEdge::BottomRight => ResizeEdge::BottomRight,
    }
}
