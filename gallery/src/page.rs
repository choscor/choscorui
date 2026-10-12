//! The catalog entry of a page, the page state that survives restarts,
//! and the desktop integration that pages call.

use choscorui::core::BackendCapabilities;

/// Sidebar groups, in display order.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum Category {
    Static,
    Inputs,
    Selection,
    Range,
    Text,
    DateTime,
    Data,
    Containers,
    Navigation,
    Popups,
    Dialogs,
    Shell,
    Recipes,
    System,
}

impl Category {
    #[cfg(test)]
    pub const ALL: [Category; 14] = [
        Self::Static,
        Self::Inputs,
        Self::Selection,
        Self::Range,
        Self::Text,
        Self::DateTime,
        Self::Data,
        Self::Containers,
        Self::Navigation,
        Self::Popups,
        Self::Dialogs,
        Self::Shell,
        Self::Recipes,
        Self::System,
    ];

    /// Stable id, also the translation key suffix (`category.<id>`).
    pub const fn id(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Inputs => "inputs",
            Self::Selection => "selection",
            Self::Range => "range",
            Self::Text => "text",
            Self::DateTime => "datetime",
            Self::Data => "data",
            Self::Containers => "containers",
            Self::Navigation => "navigation",
            Self::Popups => "popups",
            Self::Dialogs => "dialogs",
            Self::Shell => "shell",
            Self::Recipes => "recipes",
            Self::System => "system",
        }
    }
}

/// One entry of the catalog.
pub struct PageInfo {
    /// Stable lowercase id used by the CLI, settings, and translations
    /// (`page.<id>`).
    pub id: &'static str,
    /// English title, also the fallback when a translation is missing.
    pub title: &'static str,
    pub category: Category,
}

/// Page state that survives restarts.
#[derive(Clone, Debug, PartialEq)]
pub struct PageState {
    pub canvas_zoom: f64,
    /// `DockLayout::save` text; empty means the default arrangement.
    pub dock: String,
}

impl Default for PageState {
    fn default() -> Self {
        Self {
            canvas_zoom: 1.0,
            dock: String::new(),
        }
    }
}

/// Desktop integration supplied by the platform. Pages call it from
/// buttons; tests and other OSes use [`NoDesktop`].
pub trait Desktop {
    fn capabilities(&self) -> BackendCapabilities;
    /// Shows or removes a status item; returns whether it is now shown.
    fn toggle_status_item(&self, title: &str) -> Result<bool, String>;
    /// The application that opens `url`, if any.
    fn url_handler(&self, url: &str) -> Result<Option<String>, String>;
    fn notify(&self, title: &str, body: &str) -> Result<(), String>;
}

/// No desktop integration: every capability is unsupported.
#[cfg(test)]
pub struct NoDesktop;

#[cfg(test)]
impl Desktop for NoDesktop {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::none("headless")
    }
    fn toggle_status_item(&self, _title: &str) -> Result<bool, String> {
        Err("status items are unsupported".into())
    }
    fn url_handler(&self, _url: &str) -> Result<Option<String>, String> {
        Err("URL handlers are unsupported".into())
    }
    fn notify(&self, _title: &str, _body: &str) -> Result<(), String> {
        Err("notifications are unsupported".into())
    }
}
