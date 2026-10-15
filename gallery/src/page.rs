//! The catalog entry of a page and the page state that survives
//! restarts.

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
    /// Every category, in display order.
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
