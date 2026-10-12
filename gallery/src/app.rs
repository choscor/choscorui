//! The macOS application: native windows, the menu bar, desktop
//! integration, and settings, around the platform-neutral
//! [`GalleryView`].

use crate::catalog::PAGES;
use crate::page::{Desktop, Env, Page, PageState};
use crate::settings::{self, Saved};
use crate::strings::{self, LANGUAGE_NAMES, LANGUAGES, Source};
use crate::view::{Ctx, GalleryView, Request, Theme};
use choscorui::controls::ShortcutModifiers;
use choscorui::core::{
    BackendCapabilities, BaseColor, Locale, Localizer, Settings, Size, Tree, WindowGeometry,
};
use choscorui::platform::{
    AccessibilityActionRequest, Application, CoreTextShaper, MainMenu, MenuBarModel, MenuCommandId,
    MenuEntry, MenuItemState, MenuShortcut, MenuShortcutModifiers, MenuSpec, Modifiers,
    NotificationPermission, StatusItem, Window, WindowEvent, WindowOptions,
    notification_permission, post_notification, request_notification_permission,
    screen_visible_frames, url_handler,
};
use std::cell::RefCell;
use std::time::{Duration, Instant};

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

/// Desktop integration through the safe macOS API.
#[derive(Default)]
pub struct MacDesktop {
    status_item: RefCell<Option<StatusItem>>,
}

impl Desktop for MacDesktop {
    fn capabilities(&self) -> BackendCapabilities {
        choscorui::capabilities()
    }

    fn toggle_status_item(&self, title: &str) -> Result<bool, String> {
        let mut slot = self.status_item.borrow_mut();
        if slot.take().is_some() {
            return Ok(false);
        }
        *slot = Some(StatusItem::new(title).map_err(|error| error.to_string())?);
        Ok(true)
    }

    fn url_handler(&self, url: &str) -> Result<Option<String>, String> {
        url_handler(url)
            .map(|app| {
                app.map(|path| {
                    path.file_stem().map_or_else(
                        || path.display().to_string(),
                        |stem| stem.to_string_lossy().into_owned(),
                    )
                })
            })
            .map_err(|error| error.to_string())
    }

    fn notify(&self, title: &str, body: &str) -> Result<(), String> {
        if notification_permission() == NotificationPermission::NotRequested {
            request_notification_permission().map_err(|error| error.to_string())?;
        }
        post_notification("com.choscor.gallery.demo", title, body)
            .map_err(|error| error.to_string())
    }
}

/// The text shaper every window measures with.
static SHAPER: CoreTextShaper = CoreTextShaper;

fn ctx<'a>(
    localizer: &'a Localizer,
    theme: Theme,
    base: BaseColor,
    desktop: &'a MacDesktop,
    started: Instant,
) -> Ctx<'a> {
    Ctx {
        localizer,
        theme,
        base,
        shaper: &SHAPER,
        desktop,
        now: started.elapsed(),
    }
}

fn shortcut_modifiers(modifiers: Modifiers) -> ShortcutModifiers {
    ShortcutModifiers {
        shift: modifiers.shift,
        control: modifiers.control,
        option: modifiers.option,
        command: modifiers.command,
    }
}

pub struct GalleryWindow {
    pub window: Window,
    pub view: GalleryView,
}

/// What the application was asked at launch.
pub struct Launch {
    /// A page id from the command line.
    pub page: Option<usize>,
}

pub struct Gallery {
    menu: Option<MainMenu>,
    pub localizer: Localizer,
    pub theme: Theme,
    pub base: BaseColor,
    pub windows: Vec<GalleryWindow>,
    pub desktop: MacDesktop,
    pub settings: Settings,
    /// Translation and settings problems found at launch.
    pub notes: Vec<String>,
    pub source: Source,
    /// The main window's session, kept when that window closes before the
    /// application quits.
    session: Saved,
    /// Whether the main window has closed. Later windows then never
    /// overwrite its saved session.
    main_closed: bool,
    /// Index of the window that last became key; File > Close Window acts
    /// on it.
    active: usize,
    started: Instant,
    quit: bool,
}

impl Gallery {
    pub fn new(app: &Application, launch: Launch) -> Self {
        let (catalog, source, mut notes) = strings::load(&strings::resources_dir());
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
        let locale = strings::language(session.language)
            .unwrap_or_else(|| Locale::parse(strings::REFERENCE).expect("valid tag"));
        let start = settings::start_page(launch.page, session.page.as_deref());
        let mut gallery = Self {
            menu: None,
            localizer: Localizer::new(locale, catalog),
            theme: session.theme,
            base: session.base,
            windows: Vec::new(),
            desktop: MacDesktop::default(),
            settings,
            notes,
            source,
            session,
            main_closed: false,
            active: 0,
            started: Instant::now(),
            quit: false,
        };
        gallery.install_menu(app);
        let filter = gallery.session.filter.clone();
        let state = gallery.session.state.clone();
        let geometry = gallery.session.window;
        gallery.open_window(app, start, &filter, state, geometry);
        app.activate();
        gallery
    }

    pub fn ctx(&self) -> Ctx<'_> {
        ctx(
            &self.localizer,
            self.theme,
            self.base,
            &self.desktop,
            self.started,
        )
    }

    pub fn menu(&self) -> Option<&MainMenu> {
        self.menu.as_ref()
    }

    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    pub fn quit_requested(&self) -> bool {
        self.quit
    }

    /// Builds the menu bar in the current language; installing replaces
    /// the previous menu bar.
    pub fn install_menu(&mut self, app: &Application) {
        let ctx = self.ctx();
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
        let mut view = vec![heading(THEME_HEADING, ctx.text("menu.theme", &[]))];
        for theme in Theme::ALL {
            view.push(MenuEntry::command(
                theme_command(theme),
                MenuItemState {
                    checked: theme == self.theme,
                    ..MenuItemState::new(ctx.text(&format!("theme.{}", theme.id()), &[]))
                },
            ));
        }
        view.push(MenuEntry::separator());
        view.push(heading(LANGUAGE_HEADING, ctx.text("menu.language", &[])));
        let current = self.localizer.locale().tag();
        for (tag, name) in LANGUAGES.iter().zip(LANGUAGE_NAMES) {
            view.push(MenuEntry::command(
                language_command(tag),
                MenuItemState {
                    checked: current == *tag,
                    ..MenuItemState::new(name)
                },
            ));
        }
        let model = MenuBarModel::new(vec![
            // The first menu is the application menu on macOS.
            MenuSpec::new(
                ctx.text("app.name", &[]),
                vec![item(QUIT, ctx.text("menu.quit", &[]), Some("q"))],
            ),
            MenuSpec::new(
                ctx.text("menu.file", &[]),
                vec![
                    item(NEW_WINDOW, ctx.text("menu.new_window", &[]), Some("n")),
                    item(CLOSE_WINDOW, ctx.text("menu.close_window", &[]), Some("w")),
                ],
            ),
            MenuSpec::new(ctx.text("menu.view", &[]), view),
            MenuSpec::new(
                ctx.text("menu.go", &[]),
                vec![
                    item(NEXT_PAGE, ctx.text("menu.next_page", &[]), Some("]")),
                    item(
                        PREVIOUS_PAGE,
                        ctx.text("menu.previous_page", &[]),
                        Some("["),
                    ),
                ],
            ),
        ]);
        match model.and_then(|model| app.install_main_menu(model)) {
            Ok(menu) => self.menu = Some(menu),
            Err(error) => self.notes.push(format!("menu unavailable: {error:?}")),
        }
    }

    /// Opens a window on page `selected`. `geometry` places the main
    /// window where it was last time.
    pub fn open_window(
        &mut self,
        app: &Application,
        selected: usize,
        filter: &str,
        state: PageState,
        geometry: Option<WindowGeometry>,
    ) {
        let ctx = ctx(
            &self.localizer,
            self.theme,
            self.base,
            &self.desktop,
            self.started,
        );
        let view = GalleryView::new(DEFAULT_SIZE, selected, filter, state, &ctx);
        let title = view.window_title(&ctx);
        let mut window = match app.create_window(WindowOptions::new(
            title,
            DEFAULT_SIZE.width,
            DEFAULT_SIZE.height,
        )) {
            Ok(window) => window,
            Err(error) => {
                eprintln!("choscorui-gallery: could not open a window: {error}");
                return;
            }
        };
        if let Some(geometry) = geometry
            && let Ok(screens) = screen_visible_frames()
        {
            let frame = geometry.fit_to_screens(&screens, MINIMUM_SIZE);
            let _ = window.set_geometry(&WindowGeometry { frame, ..geometry });
        }
        let mut entry = GalleryWindow { window, view };
        entry.view.resize(entry.window.content_size(), &ctx);
        entry.window.show();
        self.windows.push(entry);
        self.active = self.windows.len() - 1;
        self.refresh(self.active);
    }

    /// Remembers the main (first) window's session for saving.
    fn snapshot_main(&mut self) {
        if self.main_closed {
            return;
        }
        let Some(main) = self.windows.first_mut() else {
            return;
        };
        self.session.page = Some(main.view.selected_id().to_owned());
        self.session.filter = main.view.filter_text().to_owned();
        self.session.state = main.view.page_state();
        self.session.window = Some(main.window.geometry());
    }

    pub fn close_window(&mut self, index: usize) {
        if index >= self.windows.len() {
            return;
        }
        if index == 0 {
            self.snapshot_main();
            self.main_closed = true;
        }
        let mut closed = self.windows.remove(index);
        closed.window.close();
        if index < self.active {
            self.active -= 1;
        }
        self.active = self.active.min(self.windows.len().saturating_sub(1));
    }

    /// The session as it would be saved now.
    pub fn session(&mut self) -> Saved {
        self.snapshot_main();
        self.session.theme = self.theme;
        self.session.base = self.base;
        self.session.language = LANGUAGES
            .into_iter()
            .find(|tag| *tag == self.localizer.locale().tag())
            .unwrap_or(strings::REFERENCE);
        self.session.clone()
    }

    /// Writes the session to the settings file, when it has one.
    pub fn save(&mut self) -> Result<(), String> {
        let session = self.session();
        session
            .write(&mut self.settings)
            .map_err(|error| format!("{error:?}"))?;
        if self.settings.path().is_none() {
            return Ok(());
        }
        self.settings.save().map_err(|error| format!("{error:?}"))
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        if let Some(menu) = &mut self.menu {
            for candidate in Theme::ALL {
                let command = theme_command(candidate);
                if let Ok(state) = menu.model().state(command) {
                    let state = MenuItemState {
                        checked: candidate == theme,
                        ..state.clone()
                    };
                    let _ = menu.set_state(command, state);
                }
            }
        }
        self.refresh_all();
    }

    pub fn set_base(&mut self, base: BaseColor) {
        self.base = base;
        self.refresh_all();
    }

    pub fn set_language(&mut self, app: &Application, tag: &str) {
        let Some(locale) = strings::language(tag) else {
            return;
        };
        self.localizer.set_locale(locale);
        self.install_menu(app);
        self.refresh_all();
    }

    /// Moves the active window to the next or previous page in sidebar
    /// order.
    fn step_page(&mut self, forward: bool) {
        let Some(entry) = self.windows.get_mut(self.active) else {
            return;
        };
        let ctx = ctx(
            &self.localizer,
            self.theme,
            self.base,
            &self.desktop,
            self.started,
        );
        let visible = entry.view.visible_pages();
        let Some(position) = visible.iter().position(|i| *i == entry.view.selected()) else {
            if let Some(first) = visible.first() {
                entry.view.select(*first, &ctx);
            }
            self.refresh(self.active);
            return;
        };
        let next = if forward {
            (position + 1) % visible.len()
        } else {
            (position + visible.len() - 1) % visible.len()
        };
        entry.view.select(visible[next], &ctx);
        self.refresh(self.active);
    }

    pub fn run_command(&mut self, app: &Application, command: MenuCommandId) {
        match command {
            NEW_WINDOW => {
                let page = self
                    .windows
                    .get(self.active)
                    .map_or(0, |entry| entry.view.selected());
                self.handle_request(app, self.active, Request::NewWindow(page));
            }
            CLOSE_WINDOW => self.close_window(self.active),
            QUIT => self.quit = true,
            NEXT_PAGE => self.step_page(true),
            PREVIOUS_PAGE => self.step_page(false),
            MenuCommandId(id)
                if (THEME_BASE..THEME_BASE + Theme::ALL.len() as u64).contains(&id) =>
            {
                self.set_theme(Theme::ALL[(id - THEME_BASE) as usize]);
            }
            MenuCommandId(id)
                if (LANGUAGE_BASE..LANGUAGE_BASE + LANGUAGES.len() as u64).contains(&id) =>
            {
                self.set_language(app, LANGUAGES[(id - LANGUAGE_BASE) as usize]);
            }
            _ => {}
        }
    }

    /// Acts on what window `source` asked for.
    pub fn handle_request(&mut self, app: &Application, source: usize, request: Request) {
        match request {
            Request::Theme(theme) => self.set_theme(theme),
            Request::Base(base) => self.set_base(base),
            Request::Language(tag) => self.set_language(app, tag),
            Request::NewWindow(page) => {
                let state = self
                    .windows
                    .get_mut(source)
                    .map_or_else(PageState::default, |entry| entry.view.page_state());
                self.open_window(app, page, "", state, None);
            }
        }
    }

    pub fn refresh_all(&mut self) {
        for index in 0..self.windows.len() {
            self.refresh(index);
        }
    }

    /// Repaints one window, updates its title and accessibility tree.
    pub fn refresh(&mut self, index: usize) {
        let ctx = ctx(
            &self.localizer,
            self.theme,
            self.base,
            &self.desktop,
            self.started,
        );
        let Some(entry) = self.windows.get_mut(index) else {
            return;
        };
        entry.window.set_display_list(entry.view.paint(&ctx));
        let title = entry.view.window_title(&ctx);
        if entry.window.title() != title {
            let _ = entry.window.set_title(&title);
        }
        let (snapshot, changes) = entry.view.sync_semantics(&ctx);
        if let Err(error) = entry.window.set_accessibility(&snapshot, &changes) {
            eprintln!("choscorui-gallery: accessibility update failed: {error}");
        }
    }

    /// Waits up to `timeout` for one native event, then handles menu
    /// commands, window events, accessibility actions, and page timers.
    pub fn pump(&mut self, app: &Application, timeout: Duration) {
        app.dispatch_next_event(timeout);
        let commands = self
            .menu
            .as_ref()
            .map(MainMenu::take_activations)
            .unwrap_or_default();
        for command in commands {
            self.run_command(app, command);
        }
        let mut index = 0;
        while index < self.windows.len() {
            let events = self.windows[index].window.take_events();
            let requests = self.windows[index].window.take_accessibility_actions();
            let before = self.windows.len();
            self.deliver(app, index, events, requests);
            if self.windows.len() >= before {
                index += 1;
            }
        }
        let ctx = ctx(
            &self.localizer,
            self.theme,
            self.base,
            &self.desktop,
            self.started,
        );
        let mut stale = Vec::new();
        for (index, entry) in self.windows.iter_mut().enumerate() {
            if entry.view.tick(&ctx) {
                stale.push(index);
            }
        }
        for index in stale {
            self.refresh(index);
        }
    }

    /// Routes one window's events and accessibility requests, then acts on
    /// what they asked for.
    pub fn deliver(
        &mut self,
        app: &Application,
        index: usize,
        events: Vec<WindowEvent>,
        requests: Vec<AccessibilityActionRequest>,
    ) {
        if events.is_empty() && requests.is_empty() {
            return;
        }
        let ctx = ctx(
            &self.localizer,
            self.theme,
            self.base,
            &self.desktop,
            self.started,
        );
        let Some(entry) = self.windows.get_mut(index) else {
            return;
        };
        let view = &mut entry.view;
        let mut asks = Vec::new();
        let mut repaint = false;
        let mut closing = false;
        let mut focused = false;
        for event in events {
            match event {
                WindowEvent::Closing => closing = true,
                WindowEvent::Focused(true) => focused = true,
                WindowEvent::Focused(false) => {
                    view.cancel_interaction();
                    repaint = true;
                }
                WindowEvent::Resized { size, .. } => {
                    view.resize(size, &ctx);
                    repaint = true;
                }
                WindowEvent::Pointer { event, modifiers } => {
                    asks.extend(view.pointer(event, shortcut_modifiers(modifiers), &ctx));
                    repaint = true;
                }
                // Command-key combinations belong to the menu bar.
                WindowEvent::Key { event, modifiers } if !modifiers.command => {
                    asks.extend(view.key(event, shortcut_modifiers(modifiers), &ctx));
                    repaint = true;
                }
                WindowEvent::Text { text } => {
                    view.text(&text, &ctx);
                    repaint = true;
                }
                _ => {}
            }
        }
        for request in requests {
            asks.extend(view.action(request.node, request.action, request.value, &ctx));
            repaint = true;
        }
        if focused {
            self.active = index;
        }
        if closing {
            // The window is gone, so its own requests no longer apply.
            self.close_window(index);
            return;
        }
        if repaint {
            self.refresh(index);
        }
        for ask in asks {
            self.handle_request(app, index, ask);
        }
    }

    /// Shows catalog page `page` in window `index`.
    pub fn select(&mut self, index: usize, page: usize) {
        let ctx = ctx(
            &self.localizer,
            self.theme,
            self.base,
            &self.desktop,
            self.started,
        );
        if let Some(entry) = self.windows.get_mut(index) {
            entry.view.select(page, &ctx);
        }
        self.refresh(index);
    }

    /// Replaces the sidebar filter text of window `index`.
    pub fn set_filter(&mut self, index: usize, text: &str) {
        let ctx = ctx(
            &self.localizer,
            self.theme,
            self.base,
            &self.desktop,
            self.started,
        );
        if let Some(entry) = self.windows.get_mut(index) {
            entry.view.set_filter(text, &ctx);
        }
        self.refresh(index);
    }

    /// Runs `f` on window `index`'s current page, for scripted checks.
    pub fn with_page<R>(
        &mut self,
        index: usize,
        f: impl FnOnce(&mut Tree, &mut dyn Page, &Env<'_>) -> R,
    ) -> Option<R> {
        let ctx = ctx(
            &self.localizer,
            self.theme,
            self.base,
            &self.desktop,
            self.started,
        );
        let entry = self.windows.get_mut(index)?;
        Some(entry.view.with_page(&ctx, f))
    }

    /// Catalog index of the page shown in window `index`.
    pub fn page_of(&self, index: usize) -> Option<&'static str> {
        self.windows
            .get(index)
            .map(|entry| PAGES[entry.view.selected()].id)
    }
}
