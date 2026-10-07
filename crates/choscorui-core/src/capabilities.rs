//! Backend capability queries. Applications check these instead of the
//! operating system, so a feature can degrade gracefully where a backend
//! lacks it or needs a precondition (for example an app bundle).

/// Whether a backend provides a feature.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Support {
    Supported,
    Unsupported,
    /// Supported once the stated condition holds.
    Conditional(&'static str),
}

impl Support {
    pub fn is_supported(self) -> bool {
        self == Self::Supported
    }
}

/// What the running backend can do. New fields may be added in minor
/// releases, so construct it with [`BackendCapabilities::none`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct BackendCapabilities {
    /// Backend name, such as `"macos"` or `"headless"`.
    pub backend: &'static str,
    pub multiple_windows: Support,
    pub native_menus: Support,
    pub context_menus: Support,
    pub native_dialogs: Support,
    pub clipboard: Support,
    pub drag_and_drop: Support,
    pub input_methods: Support,
    pub accessibility: Support,
    pub status_items: Support,
    pub notifications: Support,
    pub open_urls: Support,
    /// Light/dark appearance and accessibility display preferences.
    pub system_appearance: Support,
    /// Platform text shaping with font fallback.
    pub text_shaping: Support,
    pub image_codecs: Support,
    /// GPU-accelerated painting (as opposed to CPU rasterization).
    pub gpu_painting: Support,
}

impl BackendCapabilities {
    /// A backend that supports nothing; backends enable what they provide.
    pub fn none(backend: &'static str) -> Self {
        Self {
            backend,
            multiple_windows: Support::Unsupported,
            native_menus: Support::Unsupported,
            context_menus: Support::Unsupported,
            native_dialogs: Support::Unsupported,
            clipboard: Support::Unsupported,
            drag_and_drop: Support::Unsupported,
            input_methods: Support::Unsupported,
            accessibility: Support::Unsupported,
            status_items: Support::Unsupported,
            notifications: Support::Unsupported,
            open_urls: Support::Unsupported,
            system_appearance: Support::Unsupported,
            text_shaping: Support::Unsupported,
            image_codecs: Support::Unsupported,
            gpu_painting: Support::Unsupported,
        }
    }
}
