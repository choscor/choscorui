//! Environment values that flow from a view to its descendants, and the
//! token-aware value types that modifiers take.

use super::state::{IntoLive, Live};
use super::storage::Storage;
use choscorui_controls::{ButtonVariant, ControlSize};
use choscorui_core::{
    AnnouncementPriority, Catalog, Color, ColorRole, DesignTokens, FontWeight, LayoutDirection,
    Locale, Localizer, RadiusScale, Rect, Services, ShadowLayers, ShadowScale, Space, TextPreset,
    Tracking,
};
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// The queue of spoken announcements of a view host. Views take it from
/// [`Env::announcer`], and the window speaks each one, for example a
/// status line that changes after an action.
#[derive(Clone, Debug, Default)]
pub struct Announcer(Rc<RefCell<Vec<(String, AnnouncementPriority)>>>);

impl Announcer {
    /// Asks the screen reader to speak `text`. `High` interrupts the
    /// current speech. Empty text does nothing.
    pub fn announce(&self, text: impl Into<String>, priority: AnnouncementPriority) {
        let text = text.into();
        if !text.trim().is_empty() {
            self.0.borrow_mut().push((text, priority));
        }
    }

    pub(crate) fn take(&self) -> Vec<(String, AnnouncementPriority)> {
        std::mem::take(&mut self.0.borrow_mut())
    }
}

/// A color: a theme role, which follows the scheme and base color, or a
/// fixed color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fill {
    Role(ColorRole),
    Color(Color),
}

impl Fill {
    /// The color in `tokens`.
    pub fn resolve(self, tokens: &DesignTokens) -> Color {
        match self {
            Self::Role(role) => tokens.color(role),
            Self::Color(color) => color,
        }
    }
}

impl From<ColorRole> for Fill {
    fn from(role: ColorRole) -> Self {
        Self::Role(role)
    }
}

impl From<Color> for Fill {
    fn from(color: Color) -> Self {
        Self::Color(color)
    }
}

/// A length: a step of the spacing scale, or points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Metric {
    Space(Space),
    Points(f64),
}

impl Metric {
    /// The length in points in `tokens`.
    pub fn resolve(self, tokens: &DesignTokens) -> f64 {
        match self {
            Self::Space(step) => tokens.gap(step),
            Self::Points(points) => points,
        }
    }
}

impl From<Space> for Metric {
    fn from(step: Space) -> Self {
        Self::Space(step)
    }
}

impl From<f64> for Metric {
    fn from(points: f64) -> Self {
        Self::Points(points)
    }
}

/// A corner radius: a step of the radius scale, or points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Corner {
    Scale(RadiusScale),
    Points(f64),
}

impl Corner {
    /// The radius for a box of `bounds`, clamped to a pill.
    pub fn resolve(self, tokens: &DesignTokens, bounds: Rect) -> f64 {
        match self {
            Self::Scale(step) => tokens.radius_for(step, bounds),
            Self::Points(points) => {
                let half = bounds.size.width.min(bounds.size.height).max(0.0) / 2.0;
                points.min(half)
            }
        }
    }
}

impl From<RadiusScale> for Corner {
    fn from(step: RadiusScale) -> Self {
        Self::Scale(step)
    }
}

impl From<f64> for Corner {
    fn from(points: f64) -> Self {
        Self::Points(points)
    }
}

/// A shadow: a step of the elevation scale, or explicit layers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Elevation {
    Scale(ShadowScale),
    Layers(ShadowLayers),
}

impl Elevation {
    /// The shadow layers in `tokens`.
    pub fn resolve(self, tokens: &DesignTokens) -> ShadowLayers {
        match self {
            Self::Scale(step) => tokens.shadow(step),
            Self::Layers(layers) => layers,
        }
    }
}

impl From<ShadowScale> for Elevation {
    fn from(step: ShadowScale) -> Self {
        Self::Scale(step)
    }
}

impl From<ShadowLayers> for Elevation {
    fn from(layers: ShadowLayers) -> Self {
        Self::Layers(layers)
    }
}

/// The text appearance that descendant text inherits.
#[derive(Clone, Default)]
pub struct TextEnv {
    pub preset: Option<Live<TextPreset>>,
    pub weight: Option<Live<FontWeight>>,
    pub tracking: Option<Live<Tracking>>,
    pub foreground: Option<Live<Fill>>,
}

/// Values that a view passes to every descendant. Environment modifiers
/// change them for a subtree. Live values are read inside the
/// descendants' own live properties, so a change updates only the
/// descendants that read it.
#[derive(Clone)]
pub struct Env {
    pub(crate) catalog: Rc<Catalog>,
    pub(crate) locale: Live<Locale>,
    pub(crate) direction: Option<Live<LayoutDirection>>,
    pub(crate) disabled: Live<bool>,
    pub(crate) button_variant: Option<Live<ButtonVariant>>,
    pub(crate) control_size: Option<Live<ControlSize>>,
    pub(crate) text: TextEnv,
    services: Services,
    storage: Option<Storage>,
    announcer: Announcer,
    values: HashMap<TypeId, Rc<dyn Any>>,
}

impl Default for Env {
    fn default() -> Self {
        Self {
            catalog: Rc::new(Catalog::new()),
            locale: Live::Value(Locale::root()),
            direction: None,
            disabled: Live::Value(false),
            button_variant: None,
            control_size: None,
            text: TextEnv::default(),
            services: Services::none(),
            storage: None,
            announcer: Announcer::default(),
            values: HashMap::new(),
        }
    }
}

impl Env {
    /// An environment that translates through `catalog` in `locale`.
    pub fn new<M>(catalog: Rc<Catalog>, locale: impl IntoLive<Locale, M>) -> Self {
        Self {
            catalog,
            locale: locale.into_live(),
            ..Self::default()
        }
    }

    /// The locale. Inside a live property, the read is recorded.
    pub fn locale(&self) -> Locale {
        self.locale.get()
    }

    /// The layout direction: an explicit one, or the locale's.
    pub fn direction(&self) -> LayoutDirection {
        match &self.direction {
            Some(direction) => direction.get(),
            None => self.locale.get().direction(),
        }
    }

    /// Whether this subtree is disabled.
    pub fn disabled(&self) -> bool {
        self.disabled.get()
    }

    /// The button variant that buttons without their own use.
    pub fn button_variant(&self) -> Option<ButtonVariant> {
        self.button_variant.as_ref().map(Live::get)
    }

    /// The size that controls without their own use.
    pub fn control_size(&self) -> Option<ControlSize> {
        self.control_size.as_ref().map(Live::get)
    }

    /// The inherited text appearance.
    pub fn text_env(&self) -> &TextEnv {
        &self.text
    }

    /// The catalog and the locale as a [`Localizer`]. Controls take it
    /// for their built-in messages. Inside a live property, the locale
    /// read is recorded.
    pub fn localizer(&self) -> Localizer {
        Localizer::new(self.locale.get(), Catalog::clone(&self.catalog))
    }

    /// The translation of `id` with `args`, or `id` itself when no catalog
    /// has it. An id that the catalog does not define comes from the
    /// built-in catalog. Inside a live property, the locale read is
    /// recorded.
    pub fn text(&self, id: &str, args: &[(&str, &str)]) -> String {
        self.localizer().message(id, args)
    }

    /// The plural translation of `id` for `count`, or `id` itself.
    pub fn plural(&self, id: &str, count: u64, args: &[(&str, &str)]) -> String {
        self.localizer()
            .plural(id, count, args)
            .unwrap_or_else(|_| id.to_owned())
    }

    /// The desktop services of the backend: the clipboard,
    /// notifications, the status item, URL handlers, and file dialogs.
    /// Without a backend, each service reports that it is unsupported.
    pub fn services(&self) -> &Services {
        &self.services
    }

    /// Sets the desktop services. The application sets the backend's
    /// services in the root environment.
    pub fn set_services(&mut self, services: Services) {
        self.services = services;
    }

    /// The announcements of the view host, which the window speaks.
    pub fn announcer(&self) -> &Announcer {
        &self.announcer
    }

    /// The storage of stored state, if the application has one.
    pub fn storage(&self) -> Option<&Storage> {
        self.storage.as_ref()
    }

    /// Sets the storage that `Cx::stored` loads from and writes to.
    pub fn set_storage(&mut self, storage: Storage) {
        self.storage = Some(storage);
    }

    /// A custom environment value set with `.environment(value)`.
    pub fn value<T: Any>(&self) -> Option<&T> {
        self.values
            .get(&TypeId::of::<T>())
            .and_then(|value| value.downcast_ref::<T>())
    }

    /// Sets a custom value for descendants, inside `.environment(...)`.
    pub fn set_value<T: Any>(&mut self, value: T) {
        self.values.insert(TypeId::of::<T>(), Rc::new(value));
    }
}
