//! The application around [`GalleryRoot`]: the command menus, windows,
//! and settings.

use crate::catalog::PAGES;
use crate::page::PageState;
use crate::settings::{self, Saved};
use crate::strings::{self, LANGUAGE_NAMES, LANGUAGES, Source};
use crate::ui::page::PageStore;
use crate::ui::shell::{GalleryRoot, Request, Shared, Theme, WindowState, binding, matching_pages};
use choscorui::compose::Runtime;
use choscorui::compose::view::{CommandMenu, Env, Localized, MenuChoice};
use choscorui::controls::{EditCommand, ShortcutModifiers};
use choscorui::core::{Settings, Size};
use choscorui::{App, AppContext, AppError, Scene, WindowId, WindowOptions};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

pub const DEFAULT_SIZE: Size = Size::new(1100.0, 720.0);
const MINIMUM_SIZE: Size = Size::new(720.0, 480.0);

/// The command menus in the current language: the application menu, File,
/// Edit, View, and Go. The actions ask the application through the request
/// queue, which it reads each turn.
fn commands(shared: &Shared) -> Vec<CommandMenu> {
    let command = ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    };
    let ask = |request: Request| {
        let requests = shared.requests.clone();
        move || requests.borrow_mut().push(request)
    };
    let item = |key: &str, request: Request, shortcut: char| {
        MenuChoice::action(Localized::new(key), ask(request)).keyboard_shortcut(shortcut, command)
    };
    let themes = Theme::ALL
        .into_iter()
        .map(|theme| {
            let label = Localized::new(format!("theme.{}", theme.id()));
            MenuChoice::radio(label, binding(&shared.theme), theme)
        })
        .collect();
    let languages = LANGUAGES
        .into_iter()
        .zip(LANGUAGE_NAMES)
        .map(|(tag, name)| MenuChoice::radio(name, binding(&shared.language), tag))
        .collect();
    vec![
        // The first menu is the application menu on macOS.
        CommandMenu::new(
            Localized::new("app.name"),
            vec![item("menu.quit", Request::Quit, 'q')],
        ),
        CommandMenu::new(
            Localized::new("menu.file"),
            vec![
                item("menu.new_window", Request::NewActiveWindow, 'n'),
                item("menu.close_window", Request::CloseWindow, 'w'),
            ],
        ),
        CommandMenu::new(
            Localized::new("menu.edit"),
            vec![
                item("menu.undo", Request::Edit(EditCommand::Undo), 'z'),
                MenuChoice::action(
                    Localized::new("menu.redo"),
                    ask(Request::Edit(EditCommand::Redo)),
                )
                .keyboard_shortcut(
                    'z',
                    ShortcutModifiers {
                        shift: true,
                        ..command
                    },
                ),
                item("menu.cut", Request::Edit(EditCommand::Cut), 'x'),
                item("menu.copy", Request::Edit(EditCommand::Copy), 'c'),
                item("menu.paste", Request::Edit(EditCommand::Paste), 'v'),
                item(
                    "menu.select_all",
                    Request::Edit(EditCommand::SelectAll),
                    'a',
                ),
            ],
        ),
        CommandMenu::new(
            Localized::new("menu.view"),
            vec![
                MenuChoice::submenu(Localized::new("menu.theme"), themes),
                MenuChoice::submenu(Localized::new("menu.language"), languages),
            ],
        ),
        CommandMenu::new(
            Localized::new("menu.go"),
            vec![
                item("menu.next_page", Request::StepPage(true), ']'),
                item("menu.previous_page", Request::StepPage(false), '['),
            ],
        ),
    ]
}

/// What the application was asked at launch.
pub struct Launch {
    /// A page id from the command line.
    pub page: Option<usize>,
}

/// The application state that outlives single windows.
pub struct Gallery {
    pub shared: Shared,
    pub runtime: Runtime,
    pub windows: RefCell<HashMap<WindowId, WindowState>>,
    /// The first window; its page, filter, page state, and geometry are
    /// saved.
    pub main: RefCell<Option<WindowId>>,
    /// The main window's session, kept when that window closes before the
    /// application quits.
    pub session: RefCell<Saved>,
    /// Translation and language problems found at launch.
    pub notes: RefCell<Vec<String>>,
    /// Where the translations came from.
    pub source: Source,
    /// The errors that windows' views reported.
    pub errors: Rc<RefCell<Vec<String>>>,
}

impl Gallery {
    /// Opens a window on page `page` with `filter` and page `state`. A
    /// window that `restores` opens where the main window closed last
    /// time.
    pub fn open(
        self: &Rc<Self>,
        cx: &mut AppContext,
        page: usize,
        filter: &str,
        state: &PageState,
        restores: bool,
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
        )
        .minimum_size(MINIMUM_SIZE);
        let mut scene = Scene::new(options, move || GalleryRoot {
            shared: shared.clone(),
            window: root_window.clone(),
        })
        .title(move || titles.window_title(titled.get()));
        if restores {
            scene = scene.restore_geometry(settings::MAIN_WINDOW);
        }
        let id = match cx.open(scene) {
            Ok(id) => id,
            Err(error) => {
                eprintln!("choscorui-gallery: could not open a window: {error}");
                return None;
            }
        };
        self.windows.borrow_mut().insert(id, window);
        self.main.borrow_mut().get_or_insert(id);
        Some(id)
    }

    /// Remembers the main window's session for saving.
    pub fn snapshot_main(&self) {
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
    }

    /// The session as it would be saved now.
    pub fn session(&self) -> Saved {
        self.snapshot_main();
        let mut session = self.session.borrow_mut();
        session.theme = self.shared.theme.get();
        session.base = self.shared.base.get();
        session.language = self.shared.language.get();
        session.clone()
    }

    /// Writes the session and the main window's frame to the settings
    /// file, when it has one. A failed write goes to the error handler.
    pub fn save(&self, cx: &mut AppContext) -> Result<(), String> {
        let session = self.session();
        cx.storage()
            .update(|settings| session.write(settings))
            .map_err(|error| format!("{error:?}"))?;
        cx.save();
        Ok(())
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
        self.open(cx, page, "", &state, false);
    }

    /// Closes window `id`, keeping the main window's session.
    pub fn close(&self, cx: &mut AppContext, id: WindowId) {
        if *self.main.borrow() == Some(id) {
            self.snapshot_main();
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
                Request::NewActiveWindow => {
                    let page = cx
                        .active()
                        .and_then(|id| self.windows.borrow().get(&id).map(|state| state.page.get()))
                        .unwrap_or(0);
                    self.new_window(cx, page);
                }
                Request::CloseWindow => {
                    if let Some(id) = cx.active() {
                        self.close(cx, id);
                    }
                }
                Request::StepPage(forward) => self.step_page(cx, forward),
                Request::Edit(command) => {
                    if let Some(host) = cx.active().and_then(|id| cx.host(id)) {
                        host.edit(command);
                    }
                }
                Request::Quit => cx.quit(),
            }
        }
        let main = *self.main.borrow();
        if main.is_some_and(|id| cx.windows().contains(&id)) {
            self.snapshot_main();
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
}

/// Starts the gallery: loads translations and settings, installs the
/// command menus, and opens the main window.
pub fn start(launch: &Launch) -> Result<(Rc<Gallery>, AppContext), AppError> {
    let (catalog, source, notes) = strings::load(&strings::resources_dir());
    let mut app = App::new()
        .id(settings::APP_ID)
        .settings_schema(settings::schema());
    if let Some(path) = settings::override_path() {
        app = app.settings_path(path);
    }
    let runtime = app.runtime().clone();
    let defaults = Saved::read(&Settings::new(settings::schema()));
    let catalog = Rc::new(catalog);
    let shared = Shared {
        theme: runtime.property(defaults.theme),
        base: runtime.property(defaults.base),
        language: runtime.property(defaults.language),
        catalog: catalog.clone(),
        requests: Rc::default(),
    };
    let gallery = Rc::new(Gallery {
        shared: shared.clone(),
        runtime,
        windows: RefCell::default(),
        main: RefCell::default(),
        session: RefCell::new(defaults),
        notes: RefCell::new(notes),
        source,
        errors: Rc::default(),
    });
    let locale = shared.clone();
    let tokens = (shared.theme.clone(), shared.base.clone());
    let ticks = gallery.clone();
    let errors = gallery.errors.clone();
    let mut cx = app
        .env(Env::new(catalog, move || locale.locale()))
        .tokens(move || tokens.0.get().tokens(tokens.1.get()))
        .commands(commands(&shared))
        .on_tick(move |cx| ticks.tick(cx))
        .on_error(move |_, error| {
            eprintln!("choscorui-gallery: {error}");
            errors.borrow_mut().push(error.to_string());
        })
        .start()?;
    let mut session = cx.storage().with_settings(Saved::read);
    if let Ok(tag) = std::env::var("CHOSCORUI_GALLERY_LANG") {
        match LANGUAGES.into_iter().find(|known| *known == tag) {
            Some(known) => session.language = known,
            None => gallery.notes.borrow_mut().push(format!(
                "CHOSCORUI_GALLERY_LANG={tag} is not one of {}; ignored",
                LANGUAGES.join(", ")
            )),
        }
    }
    shared.theme.set(session.theme);
    shared.base.set(session.base);
    shared.language.set(session.language);
    let page = settings::start_page(launch.page, session.page.as_deref());
    gallery.open(&mut cx, page, &session.filter, &session.state, true);
    *gallery.session.borrow_mut() = session;
    cx.activate();
    Ok((gallery, cx))
}

/// Runs the gallery until the user quits, the last window closes, or the
/// autoclose time passes; then saves the settings.
pub fn run(gallery: &Gallery, cx: &mut AppContext, autoclose: Option<Duration>) {
    while cx.pump() {
        if autoclose.is_some_and(|after| cx.elapsed() >= after) {
            break;
        }
    }
    if let Err(error) = gallery.save(cx) {
        eprintln!("choscorui-gallery: settings not saved: {error}");
    }
}
