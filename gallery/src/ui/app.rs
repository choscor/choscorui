//! The macOS application around [`GalleryRoot`]: the menu bar, windows,
//! desktop integration, and settings.

use crate::app::MacDesktop;
use crate::catalog::PAGES;
use crate::page::PageState;
use crate::settings::{self, Saved};
use crate::strings::{self, LANGUAGE_NAMES, LANGUAGES};
use crate::ui::page::PageStore;
use crate::ui::shell::{GalleryRoot, Request, Shared, WindowState, matching_pages};
use crate::view::Theme;
use choscorui::compose::Runtime;
use choscorui::compose::view::Env;
use choscorui::core::{Settings, Size, WindowGeometry};
use choscorui::platform::{
    MenuBarModel, MenuCommandId, MenuEntry, MenuError, MenuItemState, MenuShortcut,
    MenuShortcutModifiers, MenuSpec, WindowOptions, screen_visible_frames,
};
use choscorui::{App, AppContext, AppError, Scene, WindowId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

pub const DEFAULT_SIZE: Size = Size::new(1100.0, 720.0);
const MINIMUM_SIZE: Size = Size::new(720.0, 480.0);

pub const NEW_WINDOW: MenuCommandId = MenuCommandId(1);
pub const CLOSE_WINDOW: MenuCommandId = MenuCommandId(2);
pub const QUIT: MenuCommandId = MenuCommandId(3);
pub const NEXT_PAGE: MenuCommandId = MenuCommandId(4);
pub const PREVIOUS_PAGE: MenuCommandId = MenuCommandId(5);
const THEME_HEADING: u64 = 9;
const THEME_BASE: u64 = 10;
const LANGUAGE_HEADING: u64 = 19;
const LANGUAGE_BASE: u64 = 20;

pub fn theme_command(theme: Theme) -> MenuCommandId {
    let index = Theme::ALL.iter().position(|t| *t == theme).unwrap_or(0);
    MenuCommandId(THEME_BASE + index as u64)
}

pub fn language_command(tag: &str) -> MenuCommandId {
    let index = LANGUAGES.iter().position(|t| *t == tag).unwrap_or(0);
    MenuCommandId(LANGUAGE_BASE + index as u64)
}

/// What the application was asked at launch.
pub struct Launch {
    /// A page id from the command line.
    pub page: Option<usize>,
    /// Quit (saving settings) after this long.
    pub autoclose: Option<Duration>,
}

/// The application state that outlives single windows.
pub struct Gallery {
    pub shared: Shared,
    pub runtime: Runtime,
    pub windows: RefCell<HashMap<WindowId, WindowState>>,
    /// The first window; its page, filter, page state, and geometry are
    /// saved.
    pub main: RefCell<Option<WindowId>>,
    pub settings: RefCell<Settings>,
    /// The main window's session, kept when that window closes before the
    /// application quits.
    pub session: RefCell<Saved>,
    /// Translation and settings problems found at launch.
    pub notes: Vec<String>,
}

impl Gallery {
    /// Opens a window on page `page` with `filter` and page `state`.
    /// `geometry` places the main window where it was last time.
    pub fn open(
        self: &Rc<Self>,
        cx: &mut AppContext,
        page: usize,
        filter: &str,
        state: &PageState,
        geometry: Option<WindowGeometry>,
    ) -> Option<WindowId> {
        let window = WindowState {
            page: self.runtime.property(page),
            filter: self.runtime.property(filter.to_owned()),
            enabled: self.runtime.property(true),
            readout: self.runtime.property(self.shared.text("readout.none", &[])),
            store: PageStore::new(&self.runtime, state),
        };
        let (shared, root_window) = (self.shared.clone(), window.clone());
        let (titles, titled) = (self.shared.clone(), window.page.clone());
        let options = WindowOptions::new(
            self.shared.window_title(page),
            DEFAULT_SIZE.width,
            DEFAULT_SIZE.height,
        );
        let scene = Scene::new(options, move || GalleryRoot {
            shared: shared.clone(),
            window: root_window.clone(),
        })
        .title(move || titles.window_title(titled.get()));
        let id = match cx.open(scene) {
            Ok(id) => id,
            Err(error) => {
                eprintln!("choscorui-gallery: could not open a window: {error}");
                return None;
            }
        };
        if let Some(geometry) = geometry
            && let Ok(screens) = screen_visible_frames()
            && let Some(native) = cx.window(id)
        {
            let frame = geometry.fit_to_screens(&screens, MINIMUM_SIZE);
            let _ = native.set_geometry(&WindowGeometry { frame, ..geometry });
        }
        self.windows.borrow_mut().insert(id, window);
        self.main.borrow_mut().get_or_insert(id);
        Some(id)
    }

    /// Remembers the main window's session for saving.
    pub fn snapshot_main(&self, cx: &mut AppContext) {
        let Some(main) = *self.main.borrow() else {
            return;
        };
        let Some(state) = self.windows.borrow().get(&main).cloned() else {
            return;
        };
        let mut session = self.session.borrow_mut();
        session.page = Some(PAGES[state.page.get()].id.to_owned());
        session.filter = state.filter.get();
        session.state = state.store.snapshot();
        if let Some(window) = cx.window(main) {
            session.window = Some(window.geometry());
        }
    }

    /// The session as it would be saved now.
    pub fn session(&self, cx: &mut AppContext) -> Saved {
        self.snapshot_main(cx);
        let mut session = self.session.borrow_mut();
        session.theme = self.shared.theme.get();
        session.base = self.shared.base.get();
        session.language = self.shared.language.get();
        session.clone()
    }

    /// Writes the session to the settings file, when it has one.
    pub fn save(&self, cx: &mut AppContext) -> Result<(), String> {
        let session = self.session(cx);
        let mut settings = self.settings.borrow_mut();
        session
            .write(&mut settings)
            .map_err(|error| format!("{error:?}"))?;
        if settings.path().is_none() {
            return Ok(());
        }
        settings.save().map_err(|error| format!("{error:?}"))
    }

    /// Forgets windows that closed, keeping the main window's session.
    fn forget_closed(&self, cx: &mut AppContext) {
        let open = cx.windows();
        let main = *self.main.borrow();
        if let Some(main) = main
            && !open.contains(&main)
        {
            // The window is gone; its session was saved while it was open.
            self.main.borrow_mut().take();
        }
        self.windows.borrow_mut().retain(|id, _| open.contains(id));
    }

    /// Moves the active window to the next or previous page in sidebar
    /// order.
    fn step_page(&self, cx: &AppContext, forward: bool) {
        let Some(state) = cx
            .active()
            .and_then(|id| self.windows.borrow().get(&id).cloned())
        else {
            return;
        };
        let visible = matching_pages(&state.filter.get(), &self.shared);
        if visible.is_empty() {
            return;
        }
        let next = match visible.iter().position(|index| *index == state.page.get()) {
            None => 0,
            Some(position) if forward => (position + 1) % visible.len(),
            Some(position) => (position + visible.len() - 1) % visible.len(),
        };
        state.page.set(visible[next]);
    }

    /// Acts on a menu command.
    pub fn command(self: &Rc<Self>, cx: &mut AppContext, command: MenuCommandId) {
        match command {
            NEW_WINDOW => {
                let page = cx
                    .active()
                    .and_then(|id| self.windows.borrow().get(&id).map(|state| state.page.get()))
                    .unwrap_or(0);
                self.new_window(cx, page);
            }
            CLOSE_WINDOW => {
                if let Some(id) = cx.active() {
                    self.close(cx, id);
                }
            }
            QUIT => cx.quit(),
            NEXT_PAGE => self.step_page(cx, true),
            PREVIOUS_PAGE => self.step_page(cx, false),
            MenuCommandId(id)
                if (THEME_BASE..THEME_BASE + Theme::ALL.len() as u64).contains(&id) =>
            {
                self.shared
                    .theme
                    .set(Theme::ALL[(id - THEME_BASE) as usize]);
            }
            MenuCommandId(id)
                if (LANGUAGE_BASE..LANGUAGE_BASE + LANGUAGES.len() as u64).contains(&id) =>
            {
                self.shared
                    .language
                    .set(LANGUAGES[(id - LANGUAGE_BASE) as usize]);
            }
            _ => {}
        }
    }

    /// Opens page `page` in a new window with the active window's page
    /// state.
    pub fn new_window(self: &Rc<Self>, cx: &mut AppContext, page: usize) {
        let state = cx
            .active()
            .and_then(|id| {
                self.windows
                    .borrow()
                    .get(&id)
                    .map(|state| state.store.snapshot())
            })
            .unwrap_or_default();
        self.open(cx, page, "", &state, None);
    }

    /// Closes window `id`, keeping the main window's session.
    pub fn close(&self, cx: &mut AppContext, id: WindowId) {
        if *self.main.borrow() == Some(id) {
            self.snapshot_main(cx);
        }
        cx.close(id);
        self.forget_closed(cx);
    }

    /// Runs once each turn of the event loop: acts on window requests,
    /// keeps the main window's session, and updates the focus readouts.
    pub fn tick(self: &Rc<Self>, cx: &mut AppContext) {
        let requests: Vec<Request> = self.shared.requests.borrow_mut().drain(..).collect();
        for request in requests {
            match request {
                Request::NewWindow(page) => self.new_window(cx, page),
            }
        }
        let main = *self.main.borrow();
        if main.is_some_and(|id| cx.windows().contains(&id)) {
            self.snapshot_main(cx);
        }
        self.forget_closed(cx);
        let windows: Vec<(WindowId, WindowState)> = self
            .windows
            .borrow()
            .iter()
            .map(|(id, state)| (*id, state.clone()))
            .collect();
        for (id, state) in windows {
            let Some(host) = cx.host(id) else {
                continue;
            };
            let focused = host.tree().focus().and_then(|node| host.semantic(node));
            let readout = match focused {
                Some(node) => match node.value.filter(|value| !value.is_empty()) {
                    Some(value) => self
                        .shared
                        .text("readout.value", &[("name", &node.name), ("value", &value)]),
                    None => self.shared.text("readout.focus", &[("name", &node.name)]),
                },
                None => self.shared.text("readout.none", &[]),
            };
            if state.readout.get() != readout {
                state.readout.set(readout);
            }
        }
    }

    /// The menu bar in the current language.
    pub fn menu(&self) -> Result<MenuBarModel, MenuError> {
        let shared = &self.shared;
        let shortcut = |key: &str| {
            Some(MenuShortcut {
                key: key.into(),
                modifiers: MenuShortcutModifiers {
                    command: true,
                    ..MenuShortcutModifiers::default()
                },
            })
        };
        let item = |command: MenuCommandId, title: String, key: Option<&str>| {
            MenuEntry::command(
                command,
                MenuItemState {
                    shortcut: key.and_then(shortcut),
                    ..MenuItemState::new(title)
                },
            )
        };
        // Disabled items label the theme and language groups; menu ids must
        // be unique, so each heading has its own.
        let heading = |id: u64, title: String| {
            MenuEntry::command(
                MenuCommandId(id),
                MenuItemState {
                    enabled: false,
                    ..MenuItemState::new(title)
                },
            )
        };
        let theme = shared.theme.get();
        let mut view = vec![heading(THEME_HEADING, shared.text("menu.theme", &[]))];
        for candidate in Theme::ALL {
            view.push(MenuEntry::command(
                theme_command(candidate),
                MenuItemState {
                    checked: candidate == theme,
                    ..MenuItemState::new(shared.text(&format!("theme.{}", candidate.id()), &[]))
                },
            ));
        }
        view.push(MenuEntry::separator());
        view.push(heading(LANGUAGE_HEADING, shared.text("menu.language", &[])));
        let current = shared.language.get();
        for (tag, name) in LANGUAGES.iter().zip(LANGUAGE_NAMES) {
            view.push(MenuEntry::command(
                language_command(tag),
                MenuItemState {
                    checked: current == *tag,
                    ..MenuItemState::new(name)
                },
            ));
        }
        MenuBarModel::new(vec![
            // The first menu is the application menu on macOS.
            MenuSpec::new(
                shared.text("app.name", &[]),
                vec![item(QUIT, shared.text("menu.quit", &[]), Some("q"))],
            ),
            MenuSpec::new(
                shared.text("menu.file", &[]),
                vec![
                    item(NEW_WINDOW, shared.text("menu.new_window", &[]), Some("n")),
                    item(
                        CLOSE_WINDOW,
                        shared.text("menu.close_window", &[]),
                        Some("w"),
                    ),
                ],
            ),
            MenuSpec::new(shared.text("menu.view", &[]), view),
            MenuSpec::new(
                shared.text("menu.go", &[]),
                vec![
                    item(NEXT_PAGE, shared.text("menu.next_page", &[]), Some("]")),
                    item(
                        PREVIOUS_PAGE,
                        shared.text("menu.previous_page", &[]),
                        Some("["),
                    ),
                ],
            ),
        ])
    }
}

/// Starts the gallery: loads translations and settings, opens the main
/// window, and installs the menu bar.
pub fn start(launch: &Launch) -> Result<(Rc<Gallery>, AppContext), AppError> {
    let (catalog, _, mut notes) = strings::load(&strings::resources_dir());
    let (settings, issues) = settings::load(settings::path());
    notes.extend(issues);
    let mut session = Saved::read(&settings);
    if let Ok(tag) = std::env::var("CHOSCORUI_GALLERY_LANG") {
        match LANGUAGES.into_iter().find(|known| *known == tag) {
            Some(known) => session.language = known,
            None => notes.push(format!(
                "CHOSCORUI_GALLERY_LANG={tag} is not one of {}; ignored",
                LANGUAGES.join(", ")
            )),
        }
    }
    let app = App::new();
    let runtime = app.runtime().clone();
    let catalog = Rc::new(catalog);
    let shared = Shared {
        theme: runtime.property(session.theme),
        base: runtime.property(session.base),
        language: runtime.property(session.language),
        catalog: catalog.clone(),
        desktop: Rc::new(MacDesktop::default()),
        requests: Rc::default(),
    };
    let start = settings::start_page(launch.page, session.page.as_deref());
    let gallery = Rc::new(Gallery {
        shared: shared.clone(),
        runtime,
        windows: RefCell::default(),
        main: RefCell::default(),
        settings: RefCell::new(settings),
        session: RefCell::new(session.clone()),
        notes,
    });
    let locale = shared.clone();
    let tokens = (shared.theme.clone(), shared.base.clone());
    let (menu, commands, ticks) = (gallery.clone(), gallery.clone(), gallery.clone());
    let mut cx = app
        .env(Env::new(catalog, move || locale.locale()))
        .tokens(move || tokens.0.get().tokens(tokens.1.get()))
        .commands(move || menu.menu(), move |cx, id| commands.command(cx, id))
        .on_tick(move |cx| ticks.tick(cx))
        .start()?;
    gallery.open(
        &mut cx,
        start,
        &session.filter,
        &session.state,
        session.window,
    );
    cx.application().activate();
    Ok((gallery, cx))
}

/// Runs the gallery until the user quits, the last window closes, or the
/// autoclose time passes; then saves the settings.
pub fn run(launch: Launch) -> Result<(), AppError> {
    let (gallery, mut cx) = start(&launch)?;
    for note in &gallery.notes {
        eprintln!("choscorui-gallery: {note}");
    }
    while cx.pump() {
        if launch.autoclose.is_some_and(|after| cx.elapsed() >= after) {
            break;
        }
    }
    if let Err(error) = gallery.save(&mut cx) {
        eprintln!("choscorui-gallery: settings not saved: {error}");
    }
    Ok(())
}
