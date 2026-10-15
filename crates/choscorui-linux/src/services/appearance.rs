//! The appearance and accessibility settings of the desktop, from the
//! `Settings` portal: the color scheme, the contrast, animations, the
//! text scale, and the interface font.

use super::portal;
use crate::dbus::{Connection, Error, MatchRule, Subscription, Value};
use choscorui_core::AccessibilityPreferences;
use std::cell::{Cell, RefCell};

const INTERFACE: &str = "org.freedesktop.portal.Settings";
const APPEARANCE: &str = "org.freedesktop.appearance";
const GNOME: &str = "org.gnome.desktop.interface";

/// What the desktop asks for.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemAppearance {
    pub prefers_dark: bool,
    pub accessibility: AccessibilityPreferences,
    /// The interface font, such as `Cantarell 11`, for `system-ui`.
    pub font_name: Option<String>,
}

impl Default for SystemAppearance {
    /// Light, with the default accessibility settings.
    fn default() -> Self {
        Self {
            prefers_dark: false,
            accessibility: AccessibilityPreferences::default(),
            font_name: None,
        }
    }
}

impl SystemAppearance {
    /// Applies the setting `key` of `namespace`.
    fn apply(&mut self, namespace: &str, key: &str, value: &Value) {
        match (namespace, key) {
            // 0: no preference, 1: prefer dark, 2: prefer light.
            (APPEARANCE, "color-scheme") => {
                self.prefers_dark = value.as_u32() == Some(1);
            }
            // 0: no preference, 1: higher contrast.
            (APPEARANCE, "contrast") => {
                self.accessibility.high_contrast = value.as_u32() == Some(1);
            }
            (GNOME, "enable-animations") => {
                self.accessibility.reduced_motion = value.as_bool() == Some(false);
            }
            (GNOME, "text-scaling-factor") => {
                self.accessibility.text_scale = value
                    .as_f64()
                    .filter(|scale| scale.is_finite() && *scale > 0.0)
                    .unwrap_or(1.0);
            }
            (GNOME, "font-name") => {
                self.font_name = value
                    .as_str()
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned);
            }
            _ => {}
        }
    }
}

/// Every setting of the namespaces, from `ReadAll`. `None` when the
/// portal does not answer.
fn read_all(connection: &Connection) -> Option<SystemAppearance> {
    let reply = connection
        .call_method(
            portal::DESKTOP,
            portal::PATH,
            INTERFACE,
            "ReadAll",
            &[Value::strings([APPEARANCE, GNOME])],
        )
        .ok()?;
    let mut current = SystemAppearance::default();
    let namespaces = reply.first().and_then(Value::as_dict).unwrap_or_default();
    for (namespace, settings) in namespaces {
        let Some(namespace) = namespace.as_str() else {
            continue;
        };
        for (key, value) in settings.as_dict().unwrap_or_default() {
            if let Some(key) = key.as_str() {
                current.apply(namespace, key, value.unvariant());
            }
        }
    }
    Some(current)
}

/// The settings of the portal, kept current by `SettingChanged`.
pub struct AppearanceWatch {
    current: RefCell<SystemAppearance>,
    changed: Cell<bool>,
    signals: Subscription,
}

impl AppearanceWatch {
    /// Reads the settings. `None` when no portal answers.
    pub fn new(connection: &Connection) -> Result<Option<Self>, Error> {
        if !connection.name_available(portal::DESKTOP)? {
            return Ok(None);
        }
        let signals = connection.subscribe(
            MatchRule::signal()
                .sender(portal::DESKTOP)
                .path(portal::PATH)
                .interface(INTERFACE)
                .member("SettingChanged"),
        )?;
        let Some(current) = read_all(connection) else {
            return Ok(None);
        };
        Ok(Some(Self {
            current: RefCell::new(current),
            changed: Cell::new(false),
            signals,
        }))
    }

    /// Reads every setting again, for example when a window gains focus.
    pub fn reload(&self, connection: &Connection) {
        self.update();
        if let Some(fresh) = read_all(connection) {
            let changed = *self.current.borrow() != fresh;
            if changed {
                *self.current.borrow_mut() = fresh;
                self.changed.set(true);
            }
        }
    }

    /// Applies the queued `SettingChanged` signals.
    fn update(&self) {
        for signal in self.signals.take() {
            let Ok(args) = signal.args() else {
                continue;
            };
            if let (Some(namespace), Some(key), Some(value)) = (
                args.first().and_then(Value::as_str),
                args.get(1).and_then(Value::as_str),
                args.get(2),
            ) {
                let before = self.current.borrow().clone();
                self.current
                    .borrow_mut()
                    .apply(namespace, key, value.unvariant());
                if *self.current.borrow() != before {
                    self.changed.set(true);
                }
            }
        }
    }

    /// The current settings.
    pub fn current(&self) -> SystemAppearance {
        self.update();
        self.current.borrow().clone()
    }

    /// True once after a setting changed.
    pub fn take_changed(&self) -> bool {
        self.update();
        self.changed.replace(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_map_to_the_appearance() {
        let mut appearance = SystemAppearance::default();
        appearance.apply(APPEARANCE, "color-scheme", &Value::from(1u32));
        appearance.apply(APPEARANCE, "contrast", &Value::from(1u32));
        appearance.apply(GNOME, "enable-animations", &Value::from(false));
        appearance.apply(GNOME, "text-scaling-factor", &Value::from(1.25));
        appearance.apply(GNOME, "font-name", &Value::from("Inter 10"));
        assert!(appearance.prefers_dark);
        assert_eq!(
            appearance.accessibility,
            AccessibilityPreferences {
                high_contrast: true,
                reduced_motion: true,
                text_scale: 1.25,
            }
        );
        assert_eq!(appearance.font_name.as_deref(), Some("Inter 10"));
        appearance.apply(APPEARANCE, "color-scheme", &Value::from(2u32));
        appearance.apply(GNOME, "text-scaling-factor", &Value::from(-1.0));
        assert!(!appearance.prefers_dark);
        assert_eq!(appearance.accessibility.text_scale, 1.0);
    }
}
