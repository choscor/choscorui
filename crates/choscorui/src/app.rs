//! The view-based application entry point: windows that show views, and
//! the menu bar commands. The runtime pumps native events, routes input to
//! the views, and draws a frame when state, layout, or an animation asks
//! for one.

use choscorui_compose::Runtime;
use choscorui_compose::view::{AnyView, Env, IntoLive, Live, View, ViewError, ViewHost};
use choscorui_controls::{ShapedText, ShortcutModifiers, TextMeasure};
use choscorui_core::{DesignTokens, Locale};
use choscorui_macos::{
    Application, CoreTextShaper, Error as PlatformError, MainMenu, MenuBarModel, MenuCommandId,
    MenuError, Modifiers, Window, WindowEvent, WindowOptions,
};
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

static SHAPER: CoreTextShaper = CoreTextShaper;

/// The longest wait for a native event when nothing is due.
const IDLE_WAIT: Duration = Duration::from_millis(250);
/// The wait between frames while an animation runs.
const FRAME_WAIT: Duration = Duration::from_millis(16);

/// What can stop an application from starting.
#[derive(Debug)]
pub enum AppError {
    /// The platform could not start or open a window.
    Platform(PlatformError),
    /// The menu bar model is not valid or could not be installed.
    Menu(MenuError),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Platform(error) => write!(f, "platform error: {error}"),
            Self::Menu(error) => write!(f, "menu error: {error:?}"),
        }
    }
}

impl std::error::Error for AppError {}

/// Identifies an open window for the life of the application.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WindowId(u64);

type Root = Rc<dyn Fn() -> AnyView>;
type Menu = Rc<dyn Fn() -> Result<MenuBarModel, MenuError>>;
type CommandHandler = Rc<dyn Fn(&mut AppContext, MenuCommandId)>;
type Hook = Rc<dyn Fn(&mut AppContext)>;
type ErrorHandler = Rc<dyn Fn(WindowId, &ViewError)>;

/// A window to open: its options, its root view, and an optional title
/// that follows state.
pub struct Scene {
    options: WindowOptions,
    title: Option<Live<String>>,
    root: Root,
}

impl Scene {
    /// A window with `options` that shows the view that `root` makes.
    pub fn new<V: View>(options: WindowOptions, root: impl Fn() -> V + 'static) -> Self {
        Self {
            options,
            title: None,
            root: Rc::new(move || root().into_any()),
        }
    }

    /// Keeps the window title equal to `title`, which can read state.
    #[must_use]
    pub fn title<M>(mut self, title: impl IntoLive<String, M>) -> Self {
        self.title = Some(title.into_live());
        self
    }
}

/// A view-based application. All windows share one reactive runtime, so
/// state made with [`App::runtime`] reaches every window.
///
/// ```no_run
/// use choscorui::prelude::*;
///
/// struct Counter;
///
/// impl View for Counter {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let count = cx.state(0);
///         VStack::new((
///             Text::new(move || format!("Count: {}", count.get())),
///             Button::new("Increment", move || count.update(|n| *n += 1)),
///         ))
///         .spacing(Space::S4)
///         .padding(Space::S6)
///     }
/// }
///
/// fn main() -> Result<(), choscorui::AppError> {
///     App::new()
///         .window(WindowOptions::new("Counter", 400.0, 300.0), || Counter)
///         .run()
/// }
/// ```
pub struct App {
    runtime: Runtime,
    env: Env,
    tokens: Live<DesignTokens>,
    scenes: Vec<Scene>,
    menu: Option<(Menu, CommandHandler)>,
    on_tick: Option<Hook>,
    on_error: ErrorHandler,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// An application with light tokens and an environment in the root
    /// locale with no catalog.
    pub fn new() -> Self {
        Self {
            runtime: Runtime::new(),
            env: Env::new(Rc::default(), Locale::root()),
            tokens: Live::Value(DesignTokens::light()),
            scenes: Vec::new(),
            menu: None,
            on_tick: None,
            on_error: Rc::new(|_, error| eprintln!("choscorui: {error}")),
        }
    }

    /// The reactive runtime that every window's state uses.
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// The root environment of every window: the catalog, the locale,
    /// and environment values.
    #[must_use]
    pub fn env(mut self, env: Env) -> Self {
        self.env = env;
        self
    }

    /// The application tokens that themes start from. When state that
    /// `tokens` reads changes, every window takes the new tokens.
    #[must_use]
    pub fn tokens<M>(mut self, tokens: impl IntoLive<DesignTokens, M>) -> Self {
        self.tokens = tokens.into_live();
        self
    }

    /// Opens a window with `options` that shows the view that `root`
    /// makes.
    #[must_use]
    pub fn window<V: View>(self, options: WindowOptions, root: impl Fn() -> V + 'static) -> Self {
        self.scene(Scene::new(options, root))
    }

    /// Opens the window that `scene` describes.
    #[must_use]
    pub fn scene(mut self, scene: Scene) -> Self {
        self.scenes.push(scene);
        self
    }

    /// Installs the menu bar that `menu` makes, and calls `handler` with
    /// each command that the user chooses. When state that `menu` read
    /// changes, the menu bar is made again, for example for check marks
    /// or another language.
    #[must_use]
    pub fn commands(
        mut self,
        menu: impl Fn() -> Result<MenuBarModel, MenuError> + 'static,
        handler: impl Fn(&mut AppContext, MenuCommandId) + 'static,
    ) -> Self {
        self.menu = Some((Rc::new(menu), Rc::new(handler)));
        self
    }

    /// Calls `tick` once each turn of the event loop, after input and
    /// before drawing.
    #[must_use]
    pub fn on_tick(mut self, tick: impl Fn(&mut AppContext) + 'static) -> Self {
        self.on_tick = Some(Rc::new(tick));
        self
    }

    /// Calls `handler` with each error that a window's views report. By
    /// default the errors go to standard error.
    #[must_use]
    pub fn on_error(mut self, handler: impl Fn(WindowId, &ViewError) + 'static) -> Self {
        self.on_error = Rc::new(handler);
        self
    }

    /// Starts the platform, opens the windows, and installs the menu bar.
    /// Drive the result with [`AppContext::pump`].
    pub fn start(self) -> Result<AppContext, AppError> {
        let application = Application::new().map_err(AppError::Platform)?;
        let rebuild = Rc::new(Cell::new(false));
        let retheme = Rc::new(Cell::new(false));
        let tokens = self.tokens;
        let watched = tokens.clone();
        let flag = retheme.clone();
        let theme_subscription = self.runtime.effect(move || {
            let _ = watched.get();
            flag.set(true);
        });
        let mut context = AppContext {
            application,
            runtime: self.runtime,
            env: self.env,
            current: Runtime::untracked(|| tokens.get()),
            tokens,
            retheme,
            _theme_subscription: theme_subscription,
            text: Rc::new(ShapedText::new(&SHAPER)),
            windows: Vec::new(),
            next_window: 0,
            active: None,
            menu: None,
            installed: None,
            rebuild: rebuild.clone(),
            on_tick: self.on_tick,
            on_error: self.on_error,
            quit: false,
            started: Instant::now(),
        };
        if let Some((menu, handler)) = self.menu {
            // Make the menu bar again after a change to what it read.
            let watched = menu.clone();
            let subscription = context.runtime.effect(move || {
                let _ = watched();
                rebuild.set(true);
            });
            context.menu = Some((menu, handler, subscription));
            context.install_menu()?;
        }
        for scene in self.scenes {
            context.open(scene).map_err(AppError::Platform)?;
        }
        Ok(context)
    }

    /// Runs the application until the user quits or closes the last
    /// window.
    pub fn run(self) -> Result<(), AppError> {
        let mut context = self.start()?;
        while context.pump() {}
        Ok(())
    }
}

/// An open window and the views that it shows.
struct AppWindow {
    id: WindowId,
    window: Window,
    host: ViewHost,
    title: Option<Live<String>>,
}

/// A running application. Command handlers and the tick hook get it to
/// open and close windows or to quit.
pub struct AppContext {
    application: Application,
    runtime: Runtime,
    env: Env,
    tokens: Live<DesignTokens>,
    /// The tokens that the windows have now.
    current: DesignTokens,
    retheme: Rc<Cell<bool>>,
    _theme_subscription: choscorui_compose::Subscription,
    text: Rc<dyn TextMeasure>,
    windows: Vec<AppWindow>,
    next_window: u64,
    active: Option<WindowId>,
    menu: Option<(Menu, CommandHandler, choscorui_compose::Subscription)>,
    installed: Option<MainMenu>,
    rebuild: Rc<Cell<bool>>,
    on_tick: Option<Hook>,
    on_error: ErrorHandler,
    quit: bool,
    started: Instant,
}

fn shortcut_modifiers(modifiers: Modifiers) -> ShortcutModifiers {
    ShortcutModifiers {
        shift: modifiers.shift,
        control: modifiers.control,
        option: modifiers.option,
        command: modifiers.command,
    }
}

impl AppContext {
    /// The platform application, for dialogs, the clipboard, and timers.
    pub fn application(&self) -> &Application {
        &self.application
    }

    /// The reactive runtime that every window's state uses.
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// Opens the window that `scene` describes and makes it active.
    pub fn open(&mut self, scene: Scene) -> Result<WindowId, PlatformError> {
        let Scene {
            options,
            title,
            root,
        } = scene;
        let mut window = self.application.create_window(options)?;
        let mut host = ViewHost::new(
            window.content_size(),
            self.current,
            self.env.clone(),
            self.runtime.clone(),
            self.text.clone(),
        );
        host.mount(root());
        window.show();
        let id = WindowId(self.next_window);
        self.next_window += 1;
        self.windows.push(AppWindow {
            id,
            window,
            host,
            title,
        });
        self.active = Some(id);
        self.draw(id);
        Ok(id)
    }

    /// Closes window `id`.
    pub fn close(&mut self, id: WindowId) {
        let Some(index) = self.index(id) else {
            return;
        };
        let mut closed = self.windows.remove(index);
        closed.window.close();
        if self.active == Some(id) {
            self.active = self.windows.last().map(|entry| entry.id);
        }
    }

    /// Ends the event loop after this turn.
    pub fn quit(&mut self) {
        self.quit = true;
    }

    /// The window that last had focus.
    pub fn active(&self) -> Option<WindowId> {
        self.active
    }

    /// The open windows, oldest first.
    pub fn windows(&self) -> Vec<WindowId> {
        self.windows.iter().map(|entry| entry.id).collect()
    }

    /// The native window `id`, for its geometry or paint timing.
    pub fn window(&mut self, id: WindowId) -> Option<&mut Window> {
        let index = self.index(id)?;
        Some(&mut self.windows[index].window)
    }

    /// The view runtime of window `id`, for scripted checks.
    pub fn host(&mut self, id: WindowId) -> Option<&mut ViewHost> {
        let index = self.index(id)?;
        Some(&mut self.windows[index].host)
    }

    /// Runs the `commands` handler for `command`, as if the user chose
    /// it in the menu bar.
    pub fn command(&mut self, command: MenuCommandId) {
        if let Some((_, handler, _)) = &self.menu {
            let handler = handler.clone();
            handler(self, command);
        }
    }

    /// The tokens that the windows have now.
    pub fn tokens(&self) -> DesignTokens {
        self.current
    }

    /// The installed menu bar, for its item states.
    pub fn menu(&self) -> Option<&MainMenu> {
        self.installed.as_ref()
    }

    /// The time since the application started.
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    fn index(&self, id: WindowId) -> Option<usize> {
        self.windows.iter().position(|entry| entry.id == id)
    }

    fn install_menu(&mut self) -> Result<(), AppError> {
        self.rebuild.set(false);
        let Some((menu, _, _)) = &self.menu else {
            return Ok(());
        };
        let model = Runtime::untracked(|| menu()).map_err(AppError::Menu)?;
        let installed = self
            .application
            .install_main_menu(model)
            .map_err(AppError::Menu)?;
        self.installed = Some(installed);
        Ok(())
    }

    /// Waits for native events up to the next due frame or timer, handles
    /// them, and draws what changed. Returns false when the application
    /// should stop.
    pub fn pump(&mut self) -> bool {
        let wait = self.wait();
        self.application.dispatch_next_event(wait);
        let commands = self
            .installed
            .as_ref()
            .map(MainMenu::take_activations)
            .unwrap_or_default();
        for command in commands {
            self.command(command);
        }
        for id in self.windows() {
            self.deliver(id);
        }
        if let Some(tick) = self.on_tick.clone() {
            tick(self);
        }
        if self.rebuild.get()
            && let Err(error) = self.install_menu()
        {
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
        !self.quit && !self.windows.is_empty()
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
        let events = entry.window.take_events();
        let requests = entry.window.take_accessibility_actions();
        let mut closing = false;
        for event in events {
            match event {
                WindowEvent::Closing => closing = true,
                WindowEvent::Focused(true) => self.active = Some(id),
                WindowEvent::Focused(false) => entry.host.cancel_interaction(),
                WindowEvent::Resized { size, .. } => entry.host.resize(size),
                WindowEvent::Pointer { event, modifiers } => {
                    entry.host.pointer(event, shortcut_modifiers(modifiers));
                }
                // Command-key combinations belong to the menu bar.
                WindowEvent::Key { event, modifiers } if !modifiers.command => {
                    entry.host.key(event, shortcut_modifiers(modifiers));
                }
                WindowEvent::Text { text } => {
                    entry.host.text(&text);
                }
                _ => {}
            }
        }
        for request in requests {
            entry
                .host
                .perform(request.node, request.action, request.value.as_deref());
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
            if entry.window.title() != title {
                let _ = entry.window.set_title(&title);
            }
        }
        if entry.host.needs_frame() {
            entry.host.layout();
            entry.window.set_display_list(entry.host.paint());
            let (snapshot, changes) = entry.host.semantics();
            if let Err(error) = entry.window.set_accessibility(&snapshot, &changes) {
                eprintln!("choscorui: accessibility update failed: {error}");
            }
        }
        for error in entry.host.take_errors() {
            (self.on_error)(id, &error);
        }
    }
}
