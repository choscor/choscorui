//! The view-based application entry point: windows that show views, the
//! menu bar commands, and the settings that stored state and window
//! restore use. The same source compiles on every target; on a target
//! without a backend, [`App::start`] returns [`AppError::Unsupported`].

use crate::backend::{self, Platform};
use crate::context::{AppContext, Commands, ErrorHandler, Hook, Root};
use choscorui_compose::Runtime;
use choscorui_compose::view::{CommandMenu, Env, IntoLive, Live, Storage, View, ViewError};
use choscorui_core::{DesignTokens, Locale, MenuError, SettingsSchema, Support, WindowOptions};
use std::path::PathBuf;
use std::rc::Rc;

/// What can stop an application from starting.
#[derive(Debug)]
#[non_exhaustive]
pub enum AppError {
    /// No backend exists for the target operating system.
    Unsupported,
    /// The application must start on the main thread.
    WrongThread,
    /// The window options have an invalid size or title.
    InvalidWindowOptions,
    /// The native platform could not start or open a window.
    Unavailable,
    /// The menu bar model is not valid or could not be installed.
    Menu(MenuError),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => f.write_str("no backend exists for this operating system"),
            Self::WrongThread => f.write_str("the application must start on the main thread"),
            Self::InvalidWindowOptions => {
                f.write_str("the window size must be positive and finite, and the title text")
            }
            Self::Unavailable => f.write_str("the native platform is unavailable"),
            Self::Menu(error) => write!(f, "menu error: {error}"),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Menu(error) => Some(error),
            _ => None,
        }
    }
}

/// Identifies an open window for the life of the application.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WindowId(pub(crate) u64);

/// A window to open: its options, its root view, an optional title that
/// follows state, and an optional key that keeps its place between
/// launches.
pub struct Scene {
    pub(crate) options: WindowOptions,
    pub(crate) title: Option<Live<String>>,
    pub(crate) root: Root,
    pub(crate) restore: Option<String>,
}

impl Scene {
    /// A window with `options` that shows the view that `root` makes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let scene = Scene::new(WindowOptions::new("Notes", 640.0, 480.0), || Text::new("Notes"));
    /// let app = App::new().scene(scene);
    /// ```
    pub fn new<V: View>(options: WindowOptions, root: impl Fn() -> V + 'static) -> Self {
        Self {
            options,
            title: None,
            root: Rc::new(move || root().into_any()),
            restore: None,
        }
    }

    /// Keeps the window title equal to `title`, which can read state.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new();
    /// let count = app.runtime().property(0);
    /// let scene = Scene::new(WindowOptions::new("Inbox", 640.0, 480.0), || Text::new("Inbox"))
    ///     .title(move || format!("Inbox ({})", count.get()));
    /// ```
    #[must_use]
    pub fn title<M>(mut self, title: impl IntoLive<String, M>) -> Self {
        self.title = Some(title.into_live());
        self
    }

    /// Saves the window's frame under `key` in the settings when the
    /// window closes or the application quits, and puts the window there
    /// when it opens. A frame that no longer fits the screens moves onto
    /// them. When two windows use the same key, the last one to close
    /// wins.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use choscorui::prelude::*;
    ///
    /// let scene = Scene::new(WindowOptions::new("Notes", 640.0, 480.0), || {
    ///     Text::new("Hello")
    /// })
    /// .restore_geometry("window.main");
    /// App::new().id("com.example.notes").scene(scene).run()?;
    /// # Ok::<(), AppError>(())
    /// ```
    #[must_use]
    pub fn restore_geometry(mut self, key: &str) -> Self {
        self.restore = Some(key.to_owned());
        self
    }
}

/// A view-based application. All windows share one reactive runtime, so
/// state made with [`App::runtime`] reaches every window.
///
/// # Examples
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
/// fn main() -> Result<(), AppError> {
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
    commands: Vec<CommandMenu>,
    on_tick: Option<Hook>,
    on_error: ErrorHandler,
    id: Option<String>,
    settings_path: Option<PathBuf>,
    schema: SettingsSchema,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// An application with light tokens and an environment in the root
    /// locale with no catalog.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use choscorui::prelude::*;
    /// fn main() -> Result<(), choscorui::AppError> {
    ///     App::new()
    ///         .window(WindowOptions::new("Hello", 320.0, 200.0), || Text::new("Hello"))
    ///         .run()
    /// }
    /// ```
    pub fn new() -> Self {
        Self {
            runtime: Runtime::new(),
            env: Env::new(Rc::default(), Locale::root()),
            tokens: Live::Value(DesignTokens::light()),
            scenes: Vec::new(),
            commands: Vec::new(),
            on_tick: None,
            on_error: Rc::new(|_, error| eprintln!("choscorui: {error}")),
            id: None,
            settings_path: None,
            schema: SettingsSchema::new(1),
        }
    }

    /// The reactive runtime that every window's state uses.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new();
    /// // One state for all windows.
    /// let signed_in = app.runtime().property(false);
    /// let app = app.window(WindowOptions::new("Home", 400.0, 300.0), move || {
    ///     let signed_in = signed_in.clone();
    ///     Text::new(move || if signed_in.get() { "Welcome" } else { "Sign in" })
    /// });
    /// ```
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// The root environment of every window: the catalog, the locale,
    /// and environment values.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// # use std::rc::Rc;
    /// let env = Env::new(Rc::new(Catalog::new()), Locale::root());
    /// let app = App::new().env(env);
    /// ```
    #[must_use]
    pub fn env(mut self, env: Env) -> Self {
        self.env = env;
        self
    }

    /// The application tokens that themes start from. When state that
    /// `tokens` reads changes, every window takes the new tokens.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new();
    /// let dark = app.runtime().property(false);
    /// let app = app.tokens(move || if dark.get() { DesignTokens::dark() } else { DesignTokens::light() });
    /// ```
    #[must_use]
    pub fn tokens<M>(mut self, tokens: impl IntoLive<DesignTokens, M>) -> Self {
        self.tokens = tokens.into_live();
        self
    }

    /// Opens a window with `options` that shows the view that `root`
    /// makes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new()
    ///     .window(WindowOptions::new("Main", 800.0, 600.0), || Text::new("Main"))
    ///     .window(WindowOptions::new("Tools", 240.0, 400.0), || Text::new("Tools"));
    /// ```
    #[must_use]
    pub fn window<V: View>(self, options: WindowOptions, root: impl Fn() -> V + 'static) -> Self {
        self.scene(Scene::new(options, root))
    }

    /// Opens the window that `scene` describes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let scene = Scene::new(WindowOptions::new("Main", 800.0, 600.0), || Text::new("Main"))
    ///     .title("Untitled");
    /// let app = App::new().scene(scene);
    /// ```
    #[must_use]
    pub fn scene(mut self, scene: Scene) -> Self {
        self.scenes.push(scene);
        self
    }

    /// Adds `menus` to the menu bar. Each row runs its own action when
    /// the user chooses it, and its keyboard shortcut chooses it from any
    /// window. When state that a label, a check mark, or a disabled state
    /// reads changes, the menu bar is made again.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use choscorui::prelude::*;
    ///
    /// let app = App::new();
    /// let dark = app.runtime().property(false);
    /// let (read, write) = (dark.clone(), dark.clone());
    /// let command = ShortcutModifiers {
    ///     command: true,
    ///     ..ShortcutModifiers::default()
    /// };
    /// let view = CommandMenu::new(
    ///     "View",
    ///     vec![
    ///         MenuChoice::checkbox(
    ///             "Dark Mode",
    ///             Binding::new(move || read.get(), move |on| write.set(on)),
    ///         )
    ///         .keyboard_shortcut('d', command),
    ///         MenuChoice::separator(),
    ///         MenuChoice::action("Reload", || println!("reload")),
    ///     ],
    /// );
    /// app.commands([view])
    ///     .tokens(move || if dark.get() { DesignTokens::dark() } else { DesignTokens::light() })
    ///     .run()?;
    /// # Ok::<(), AppError>(())
    /// ```
    #[must_use]
    pub fn commands(mut self, menus: impl IntoIterator<Item = CommandMenu>) -> Self {
        self.commands.extend(menus);
        self
    }

    /// Calls `tick` once each turn of the event loop, after input and
    /// before drawing.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_tick(|cx| {
    ///     if cx.windows().is_empty() {
    ///         cx.quit();
    ///     }
    /// });
    /// ```
    #[must_use]
    pub fn on_tick(mut self, tick: impl Fn(&mut AppContext) + 'static) -> Self {
        self.on_tick = Some(Rc::new(tick));
        self
    }

    /// Calls `handler` with each error that a window's views report, with
    /// the window, and with each settings error, without a window. By
    /// default the errors go to standard error.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new().on_error(|window, error| {
    ///     eprintln!("window {window:?}: {error}");
    /// });
    /// ```
    #[must_use]
    pub fn on_error(mut self, handler: impl Fn(Option<WindowId>, &ViewError) + 'static) -> Self {
        self.on_error = Rc::new(handler);
        self
    }

    /// The reverse-DNS identifier of the application, such as
    /// `"com.example.notes"`. Stored state and window restore keep their
    /// values in the per-user settings file of this identifier. Without
    /// an identifier or a settings path, they last until the application
    /// quits.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use choscorui::prelude::*;
    ///
    /// App::new().id("com.example.notes").run()?;
    /// # Ok::<(), AppError>(())
    /// ```
    #[must_use]
    pub fn id(mut self, id: &str) -> Self {
        self.id = Some(id.to_owned());
        self
    }

    /// Keeps the settings in the file at `path` instead of the per-user
    /// location of [`App::id`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use choscorui::prelude::*;
    ///
    /// App::new().settings_path("/tmp/notes-settings.txt").run()?;
    /// # Ok::<(), AppError>(())
    /// ```
    #[must_use]
    pub fn settings_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.settings_path = Some(path.into());
        self
    }

    /// The version, defaults, and migrations of the settings file.
    /// Without a schema, the file has version 1 and no defaults.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use choscorui::prelude::*;
    /// use choscorui::core::{SettingValue, SettingsSchema};
    ///
    /// let schema = SettingsSchema::new(2)
    ///     .with_default("editor.wrap", SettingValue::Bool(true))
    ///     .with_migration(1, |values| {
    ///         if let Some(wrap) = values.remove("wrap") {
    ///             values.insert("editor.wrap".into(), wrap);
    ///         }
    ///     });
    /// App::new().id("com.example.notes").settings_schema(schema).run()?;
    /// # Ok::<(), AppError>(())
    /// ```
    #[must_use]
    pub fn settings_schema(mut self, schema: SettingsSchema) -> Self {
        self.schema = schema;
        self
    }

    /// Starts the platform, opens the windows, and installs the menu bar.
    /// Drive the result with [`AppContext::pump`]. Returns
    /// [`AppError::Unsupported`] on a target without a backend.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use choscorui::prelude::*;
    ///
    /// let mut cx = App::new()
    ///     .window(WindowOptions::new("Hello", 320.0, 200.0), || Text::new("Hello"))
    ///     .start()?;
    /// while cx.pump() {}
    /// # Ok::<(), AppError>(())
    /// ```
    pub fn start(self) -> Result<AppContext, AppError> {
        self.start_on(backend::window_support())
    }

    /// Starts the platform of a backend whose `multiple_windows`
    /// capability is `windows`. A backend that cannot show windows, such
    /// as the `none` backend of a target without a backend, gives
    /// [`AppError::Unsupported`].
    fn start_on(self, windows: Support) -> Result<AppContext, AppError> {
        if windows != Support::Supported {
            return Err(AppError::Unsupported);
        }
        let platform = Platform::new(self.id.as_deref())?;
        let path = self.settings_path.or_else(|| {
            self.id
                .as_deref()
                .and_then(choscorui_core::user_settings_path)
        });
        let storage = match path {
            Some(path) => Storage::open(self.schema, path),
            None => Storage::in_memory(self.schema),
        };
        let mut env = self.env;
        env.set_services(platform.services());
        env.set_storage(storage.clone());
        let mut context = AppContext::new(
            platform,
            self.runtime,
            env,
            storage,
            self.tokens,
            self.on_tick,
            self.on_error,
        );
        if !self.commands.is_empty() {
            context.commands = Some(Commands::new(&context, self.commands));
            context.install_menu()?;
        }
        for scene in self.scenes {
            context.open(scene)?;
        }
        Ok(context)
    }

    /// Runs the application until the user quits or closes the last
    /// window, then saves the settings.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use choscorui::prelude::*;
    /// fn main() -> Result<(), choscorui::AppError> {
    ///     App::new()
    ///         .window(WindowOptions::new("Hello", 320.0, 200.0), || Text::new("Hello"))
    ///         .run()
    /// }
    /// ```
    pub fn run(self) -> Result<(), AppError> {
        let mut context = self.start()?;
        while context.pump() {}
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{App, AppError};
    use crate::prelude::{Text, WindowOptions};
    use choscorui_core::BackendCapabilities;

    /// The `none` backend has these capabilities on each target without
    /// a backend. The app refuses to start before it touches a platform.
    #[test]
    fn start_without_a_backend_is_unsupported() {
        let result = App::new()
            .window(WindowOptions::new("Hello", 320.0, 200.0), || {
                Text::new("Hello")
            })
            .start_on(BackendCapabilities::none("headless").multiple_windows);
        assert!(matches!(result, Err(AppError::Unsupported)));
    }
}
