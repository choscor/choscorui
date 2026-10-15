//! A running application: its windows, the menu bar commands, the
//! settings, and the event loop turn that [`AppContext::pump`] runs.

use crate::backend::{NativeMenu, NativeWindow, Platform};
use crate::{AppError, Scene, WindowId};
use choscorui_compose::view::{AnyView, CommandMenu, Env, Live, Storage, ViewError, ViewHost};
use choscorui_compose::{LoweredMenus, Runtime, Subscription};
use choscorui_controls::TextMeasure;
use choscorui_core::{DesignTokens, MenuBarModel, MenuCommandId, MenuError, Services};
use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The longest wait for a native event when nothing is due.
const IDLE_WAIT: Duration = Duration::from_millis(250);
/// The wait between frames while an animation runs.
const FRAME_WAIT: Duration = Duration::from_millis(16);

pub(crate) type Root = Rc<dyn Fn() -> AnyView>;
pub(crate) type Hook = Rc<dyn Fn(&mut AppContext)>;
pub(crate) type ErrorHandler = Rc<dyn Fn(Option<WindowId>, &ViewError)>;

/// The command menus, lowered again each time what they read changes.
pub(crate) struct Commands {
    /// The menus lowered last, waiting for installation.
    pending: Rc<RefCell<Option<Result<LoweredMenus, MenuError>>>>,
    /// The menus that the menu bar shows.
    current: Option<LoweredMenus>,
    pub(crate) native: Option<NativeMenu>,
    panics: usize,
    _subscription: Subscription,
}

impl Commands {
    pub(crate) fn new(context: &AppContext, menus: Vec<CommandMenu>) -> Self {
        let pending: Rc<RefCell<Option<Result<LoweredMenus, MenuError>>>> = Rc::default();
        let slot = pending.clone();
        let env = context.env.clone();
        // The effect records what the lowering read, so a change lowers
        // the menus again. The work is proportional to the menus.
        let subscription = context.runtime.effect(move || {
            *slot.borrow_mut() = Some(LoweredMenus::lower(&menus, &env));
        });
        Self {
            pending,
            current: None,
            native: None,
            panics: 0,
            _subscription: subscription,
        }
    }
}

/// An open window and the views that it shows.
pub(crate) struct AppWindow {
    pub(crate) id: WindowId,
    pub(crate) native: NativeWindow,
    host: ViewHost,
    title: Option<Live<String>>,
    restore: Option<String>,
}

/// A running application. Hooks and actions get it to open and close
/// windows, to quit, and to reach the services and the settings.
pub struct AppContext {
    pub(crate) platform: Platform,
    pub(crate) runtime: Runtime,
    pub(crate) env: Env,
    storage: Storage,
    tokens: Live<DesignTokens>,
    /// The tokens that the windows have now.
    pub(crate) current: DesignTokens,
    retheme: Rc<Cell<bool>>,
    _theme_subscription: Subscription,
    pub(crate) text: Rc<dyn TextMeasure>,
    pub(crate) windows: Vec<AppWindow>,
    next_window: u64,
    active: Option<WindowId>,
    pub(crate) commands: Option<Commands>,
    on_tick: Option<Hook>,
    on_error: ErrorHandler,
    quit: bool,
    started: Instant,
}

impl AppContext {
    pub(crate) fn new(
        platform: Platform,
        runtime: Runtime,
        env: Env,
        storage: Storage,
        tokens: Live<DesignTokens>,
        on_tick: Option<Hook>,
        on_error: ErrorHandler,
    ) -> Self {
        let retheme = Rc::new(Cell::new(false));
        let watched = tokens.clone();
        let flag = retheme.clone();
        let theme_subscription = runtime.effect(move || {
            let _ = watched.get();
            flag.set(true);
        });
        Self {
            text: platform.text(),
            platform,
            current: Runtime::untracked(|| tokens.get()),
            runtime,
            env,
            storage,
            tokens,
            retheme,
            _theme_subscription: theme_subscription,
            windows: Vec::new(),
            next_window: 0,
            active: None,
            commands: None,
            on_tick,
            on_error,
            quit: false,
            started: Instant::now(),
        }
    }

    /// The reactive runtime that every window's state uses.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_tick(|cx| {
    ///     let _count = cx.runtime().property(0);
    /// });
    /// ```
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// The desktop services of the backend: the clipboard,
    /// notifications, the status item, URL handlers, and file dialogs.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_tick(|cx| {
    ///     let _copied = cx.services().set_clipboard_text("Hello");
    /// });
    /// ```
    pub fn services(&self) -> &Services {
        self.env.services()
    }

    /// The settings that stored state and window restore use.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_tick(|cx| {
    ///     let _path = cx.storage().path();
    /// });
    /// ```
    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    /// Brings the application to the front.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_tick(|cx| {
    ///     cx.activate();
    /// });
    /// ```
    pub fn activate(&self) {
        self.platform.activate();
    }

    /// Opens the window that `scene` describes and makes it active. A
    /// scene with `restore_geometry` opens where it closed last time.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_tick(|cx| {
    ///     if cx.windows().is_empty() {
    ///         let scene = Scene::new(WindowOptions::new("New", 400.0, 300.0), || Text::new("New"));
    ///         let _ = cx.open(scene);
    ///     }
    /// });
    /// ```
    pub fn open(&mut self, scene: Scene) -> Result<WindowId, AppError> {
        let Scene {
            options,
            title,
            root,
            restore,
        } = scene;
        let mut native = self.platform.open_window(&options)?;
        if let Some(key) = &restore {
            self.place(&mut native, key, options.minimum());
        }
        let mut host = ViewHost::new(
            native.content_size(),
            self.current,
            self.env.clone(),
            self.runtime.clone(),
            self.text.clone(),
        );
        host.set_feedback(self.platform.feedback());
        host.set_key_convention(crate::capabilities().keyboard);
        host.mount(self.platform.frame(&mut native, root(), &self.runtime));
        native.show();
        let id = WindowId(self.next_window);
        self.next_window += 1;
        self.windows.push(AppWindow {
            id,
            native,
            host,
            title,
            restore,
        });
        self.active = Some(id);
        self.draw(id);
        Ok(id)
    }

    /// Puts `native` where the settings say the window of `key` was,
    /// fitted to the screens.
    fn place(&self, native: &mut NativeWindow, key: &str, minimum: choscorui_core::Size) {
        let saved = self
            .storage()
            .with_settings(|settings| choscorui_core::WindowGeometry::restore(settings, key));
        if let Some(geometry) = saved {
            let frame = geometry.fit_to_screens(&self.platform.screens(), minimum);
            native.set_geometry(&choscorui_core::WindowGeometry { frame, ..geometry });
        }
    }

    /// Closes window `id`. A window with `restore_geometry` saves its
    /// frame first.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_tick(|cx| {
    ///     if let Some(id) = cx.active() {
    ///         cx.close(id);
    ///     }
    /// });
    /// ```
    pub fn close(&mut self, id: WindowId) {
        let Some(index) = self.index(id) else {
            return;
        };
        self.remember(index);
        let mut closed = self.windows.remove(index);
        closed.native.close();
        if self.active == Some(id) {
            self.active = self.windows.last().map(|entry| entry.id);
        }
    }

    /// Stores the frame of the window at `index` when it has a restore
    /// key.
    fn remember(&self, index: usize) {
        let entry = &self.windows[index];
        let Some(key) = &entry.restore else {
            return;
        };
        let geometry = entry.native.geometry();
        if let Err(error) = self
            .storage()
            .update(|settings| geometry.store(settings, key))
        {
            let error = choscorui_compose::view::StorageError::Save {
                message: error.to_string(),
            };
            (self.on_error)(Some(entry.id), &ViewError::Storage(error));
        }
    }

    /// Ends the event loop after this turn. The settings are saved then.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_tick(|cx| {
    ///     if cx.elapsed().as_secs() > 60 {
    ///         cx.quit();
    ///     }
    /// });
    /// ```
    pub fn quit(&mut self) {
        self.quit = true;
    }

    /// The window that last had focus.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_tick(|cx| {
    ///     let _front = cx.active();
    /// });
    /// ```
    pub fn active(&self) -> Option<WindowId> {
        self.active
    }

    /// The open windows, oldest first.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_tick(|cx| {
    ///     for id in cx.windows() {
    ///         let _ = id;
    ///     }
    /// });
    /// ```
    pub fn windows(&self) -> Vec<WindowId> {
        self.windows.iter().map(|entry| entry.id).collect()
    }

    /// The title of window `id`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_tick(|cx| {
    ///     if let Some(id) = cx.active() {
    ///         let _title = cx.title(id);
    ///     }
    /// });
    /// ```
    pub fn title(&self, id: WindowId) -> Option<String> {
        let index = self.index(id)?;
        Some(self.windows[index].native.title())
    }

    /// The view runtime of window `id`, for scripted checks.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_tick(|cx| {
    ///     if let Some(id) = cx.active()
    ///         && let Some(host) = cx.host(id)
    ///     {
    ///         let _errors = host.take_errors();
    ///     }
    /// });
    /// ```
    pub fn host(&mut self, id: WindowId) -> Option<&mut ViewHost> {
        let index = self.index(id)?;
        Some(&mut self.windows[index].host)
    }

    /// The tokens that the windows have now.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_tick(|cx| {
    ///     let _tokens = cx.tokens();
    /// });
    /// ```
    pub fn tokens(&self) -> DesignTokens {
        self.current
    }

    /// The installed menu bar, for its item states. Each command menu
    /// row has a generated id.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_tick(|cx| {
    ///     let quit = cx.menu_bar().and_then(|menu| menu.find("Quit"));
    ///     let _ = quit;
    /// });
    /// ```
    pub fn menu_bar(&self) -> Option<&MenuBarModel> {
        let commands = self.commands.as_ref()?;
        commands.native.as_ref()?;
        commands.current.as_ref().map(LoweredMenus::model)
    }

    /// Chooses the menu bar item `command` through the native path, as
    /// the user does, and runs its action. Returns false when the item
    /// is disabled or not in the menu bar.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_tick(|cx| {
    ///     if let Some(quit) = cx.menu_bar().and_then(|menu| menu.find("Quit")) {
    ///         cx.choose(quit);
    ///     }
    /// });
    /// ```
    pub fn choose(&mut self, command: MenuCommandId) -> bool {
        let chosen = self
            .commands
            .as_ref()
            .and_then(|commands| commands.native.as_ref())
            .is_some_and(|native| native.perform(command));
        self.run_commands();
        chosen
    }

    /// The menu actions that panicked. A panic stops at the menu
    /// boundary, and the item stays usable.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_tick(|cx| {
    ///     let _panics = cx.menu_panic_count();
    /// });
    /// ```
    pub fn menu_panic_count(&self) -> usize {
        self.commands.as_ref().map_or(0, |commands| commands.panics)
    }

    /// The time since the application started.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_tick(|cx| {
    ///     let _seconds = cx.elapsed().as_secs_f64();
    /// });
    /// ```
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Stores the frame of each open window that has a restore key, and
    /// writes the settings when a value changed. [`AppContext::pump`]
    /// saves after each turn and when the application stops.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_tick(|cx| {
    ///     cx.save();
    /// });
    /// ```
    pub fn save(&mut self) {
        for index in 0..self.windows.len() {
            self.remember(index);
        }
        self.write_settings();
    }

    fn write_settings(&self) {
        if let Err(error) = self.storage.save() {
            self.storage.report(error);
        }
        for error in self.storage.take_errors() {
            (self.on_error)(None, &ViewError::Storage(error));
        }
    }

    pub(crate) fn index(&self, id: WindowId) -> Option<usize> {
        self.windows.iter().position(|entry| entry.id == id)
    }

    /// Installs the menus that the commands lowered last, if they
    /// changed.
    pub(crate) fn install_menu(&mut self) -> Result<(), AppError> {
        let Some(commands) = &mut self.commands else {
            return Ok(());
        };
        let Some(lowered) = commands.pending.borrow_mut().take() else {
            return Ok(());
        };
        let lowered = lowered.map_err(AppError::Menu)?;
        let native = self
            .platform
            .install_menu(lowered.model().clone())
            .map_err(AppError::Menu)?;
        commands.native = Some(native);
        commands.current = Some(lowered);
        Ok(())
    }

    /// Runs the action of each menu item that the user chose. A panic in
    /// an action is counted, and the item stays usable.
    fn run_commands(&mut self) {
        let Some(commands) = &mut self.commands else {
            return;
        };
        let (Some(native), Some(current)) = (&commands.native, &commands.current) else {
            return;
        };
        let actions: Vec<_> = native
            .take_activations()
            .into_iter()
            .filter_map(|id| current.action(id))
            .collect();
        for action in actions {
            if catch_unwind(AssertUnwindSafe(|| action())).is_err() {
                commands.panics += 1;
            }
        }
    }

    /// Waits for native events up to the next due frame or timer, handles
    /// them and the desktop events of the services, and draws what
    /// changed. Returns false when the application
    /// should stop; the settings are then saved.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use choscorui::prelude::*;
    /// let mut cx = App::new()
    ///     .window(WindowOptions::new("Hello", 320.0, 200.0), || Text::new("Hello"))
    ///     .start()?;
    /// while cx.pump() {}
    /// # Ok::<(), AppError>(())
    /// ```
    pub fn pump(&mut self) -> bool {
        let wait = self.wait();
        self.platform.wait(wait);
        self.run_commands();
        for id in self.windows() {
            self.deliver(id);
        }
        // An in-window menu bar queues its commands while it handles input.
        self.run_commands();
        self.env.services().poll();
        if let Some(tick) = self.on_tick.clone() {
            tick(self);
        }
        if let Err(error) = self.install_menu() {
            eprintln!("choscorui: {error}");
        }
        if self.retheme.replace(false) {
            let tokens = Runtime::untracked(|| self.tokens.get());
            if tokens != self.current {
                self.current = tokens;
                for entry in &mut self.windows {
                    entry.host.set_tokens(tokens);
                }
            }
        }
        for id in self.windows() {
            self.draw(id);
        }
        let running = !self.quit && !self.windows.is_empty();
        if running {
            self.write_settings();
        } else {
            self.save();
        }
        running
    }

    /// How long to wait for the next native event.
    fn wait(&self) -> Duration {
        let now = self.started.elapsed().as_secs_f64();
        let mut wait = IDLE_WAIT;
        for entry in &self.windows {
            if entry.host.needs_frame() {
                wait = wait.min(FRAME_WAIT);
            }
            if let Some(deadline) = entry.host.next_deadline() {
                wait = wait.min(Duration::from_secs_f64((deadline - now).max(0.0)));
            }
        }
        wait
    }

    /// Routes window `id`'s native events and accessibility requests to
    /// its views.
    fn deliver(&mut self, id: WindowId) {
        let Some(index) = self.index(id) else {
            return;
        };
        let entry = &mut self.windows[index];
        let (closing, focus) = entry.native.deliver(&mut entry.host);
        if focus == Some(true) {
            self.active = Some(id);
        }
        if closing {
            self.close(id);
        }
    }

    /// Advances window `id`'s timers and draws it when something changed.
    fn draw(&mut self, id: WindowId) {
        let Some(index) = self.index(id) else {
            return;
        };
        let now = self.started.elapsed().as_secs_f64();
        let entry = &mut self.windows[index];
        entry.host.advance(now);
        if let Some(title) = &entry.title {
            let title = title.get();
            if entry.native.title() != title {
                entry.native.set_title(&title);
            }
        }
        if entry.host.needs_frame() {
            entry.host.layout();
            entry.native.present(entry.host.paint());
            entry.native.set_cursor(entry.host.cursor());
            let (snapshot, changes) = entry.host.semantics();
            if let Err(error) = entry.native.publish(&snapshot, &changes) {
                eprintln!("choscorui: accessibility update failed: {error}");
            }
        }
        for (text, priority) in entry.host.take_announcements() {
            entry.native.announce(&text, priority);
        }
        for error in entry.host.take_errors() {
            (self.on_error)(Some(id), &error);
        }
    }
}
