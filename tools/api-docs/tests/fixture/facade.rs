//! The facade crate of the api-docs test fixture: a prelude, an
//! extension point outside it, and internal modules.

/// The types that most applications start with.
pub mod prelude {
    pub use crate::App;
    pub use choscorui_view::view::*;
}

/// The internal crate, in full.
pub mod core {
    pub use choscorui_view::internals::*;
}

/// Extension points for custom controls.
pub mod custom {
    /// A control with its own paint code.
    pub trait CustomControl {
        /// Paints the control.
        fn paint(&self);
    }
}

/// A view-based application.
///
/// # Examples
///
/// ```no_run
/// # use choscorui::prelude::*;
/// App::new().run();
/// ```
pub struct App {
    title: String,
}

impl App {
    /// An application without windows.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui::prelude::*;
    /// let app = App::new();
    /// ```
    pub fn new() -> Self {
        Self { title: String::new() }
    }

    /// Runs the application until it quits. Uses a [`Button`](crate::prelude::Button).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use choscorui::prelude::*;
    /// App::new().run();
    /// ```
    pub fn run(self) -> Result<(), String> {
        Err(self.title)
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// A root re-export kept for compatibility. It is not in scope.
pub use choscorui_view::internals::Tree;
