//! Persisted state. [`Storage`] holds the settings of an application in
//! memory and records which values changed; the application saves it at
//! most once each turn of its event loop, and when it quits.
//! [`Cx::stored`] makes state that loads from the storage and writes
//! each change back.

use super::host::{Cx, ViewError};
use super::state::State;
use choscorui_core::{Rect, SettingValue, Settings, SettingsError, SettingsSchema};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// A problem with the settings that back stored state. The state then
/// uses its default, and the application keeps running.
#[derive(Clone, Debug, PartialEq)]
pub enum StorageError {
    /// The settings file could not load, for example because a newer
    /// application version wrote it. The defaults stay in effect, and the
    /// file is not overwritten.
    Load { path: PathBuf, message: String },
    /// The settings file was corrupt. It moved to `moved_to`, and the
    /// defaults are in effect.
    Recovered { moved_to: PathBuf },
    /// The settings could not be written.
    Save { message: String },
    /// The stored value of `key` has another type than the state.
    TypeMismatch { key: String },
    /// `key` is not a valid settings key.
    InvalidKey(String),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Load { path, message } => write!(
                f,
                "settings at {} did not load ({message}); using defaults and not saving",
                path.display()
            ),
            Self::Recovered { moved_to } => write!(
                f,
                "settings file was corrupt; moved to {} and using defaults",
                moved_to.display()
            ),
            Self::Save { message } => write!(f, "settings were not saved: {message}"),
            Self::TypeMismatch { key } => {
                write!(f, "stored value {key} has another type; using the default")
            }
            Self::InvalidKey(key) => write!(f, "invalid settings key {key:?}"),
        }
    }
}

impl std::error::Error for StorageError {}

/// A value that stored state can hold: one of the `SettingValue` kinds.
pub trait StoredValue: Clone + PartialEq + 'static {
    /// The value as a setting.
    fn to_setting(&self) -> SettingValue;
    /// The value of `setting`, or `None` when it has another kind.
    fn from_setting(setting: &SettingValue) -> Option<Self>;
}

impl StoredValue for bool {
    fn to_setting(&self) -> SettingValue {
        SettingValue::Bool(*self)
    }
    fn from_setting(setting: &SettingValue) -> Option<Self> {
        match setting {
            SettingValue::Bool(value) => Some(*value),
            _ => None,
        }
    }
}

impl StoredValue for i64 {
    fn to_setting(&self) -> SettingValue {
        SettingValue::Integer(*self)
    }
    fn from_setting(setting: &SettingValue) -> Option<Self> {
        match setting {
            SettingValue::Integer(value) => Some(*value),
            _ => None,
        }
    }
}

macro_rules! stored_integer {
    ($($ty:ty),*) => {$(
        impl StoredValue for $ty {
            fn to_setting(&self) -> SettingValue {
                SettingValue::Integer(i64::try_from(*self).unwrap_or(i64::MAX))
            }
            fn from_setting(setting: &SettingValue) -> Option<Self> {
                match setting {
                    SettingValue::Integer(value) => <$ty>::try_from(*value).ok(),
                    _ => None,
                }
            }
        }
    )*};
}

stored_integer!(i32, u32, u64, usize);

impl StoredValue for f64 {
    fn to_setting(&self) -> SettingValue {
        SettingValue::Float(*self)
    }
    fn from_setting(setting: &SettingValue) -> Option<Self> {
        match setting {
            SettingValue::Float(value) => Some(*value),
            _ => None,
        }
    }
}

impl StoredValue for String {
    fn to_setting(&self) -> SettingValue {
        SettingValue::Text(self.clone())
    }
    fn from_setting(setting: &SettingValue) -> Option<Self> {
        match setting {
            SettingValue::Text(value) => Some(value.clone()),
            _ => None,
        }
    }
}

impl StoredValue for Vec<String> {
    fn to_setting(&self) -> SettingValue {
        SettingValue::List(self.clone())
    }
    fn from_setting(setting: &SettingValue) -> Option<Self> {
        match setting {
            SettingValue::List(value) => Some(value.clone()),
            _ => None,
        }
    }
}

impl StoredValue for Rect {
    fn to_setting(&self) -> SettingValue {
        SettingValue::Rect(*self)
    }
    fn from_setting(setting: &SettingValue) -> Option<Self> {
        match setting {
            SettingValue::Rect(value) => Some(*value),
            _ => None,
        }
    }
}

struct Shared {
    settings: RefCell<Settings>,
    errors: RefCell<Vec<StorageError>>,
}

/// The settings of an application, shared by its windows. Writes change
/// the settings in memory; [`Storage::save`] writes the file when a value
/// changed. The file write is atomic: a temporary file, a sync, and a
/// rename.
#[derive(Clone)]
pub struct Storage {
    shared: Rc<Shared>,
}

impl std::fmt::Debug for Storage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Storage")
            .field("path", &self.shared.settings.borrow().path())
            .finish_non_exhaustive()
    }
}

impl Storage {
    fn from_settings(settings: Settings) -> Self {
        Self {
            shared: Rc::new(Shared {
                settings: RefCell::new(settings),
                errors: RefCell::default(),
            }),
        }
    }

    /// Settings in memory only, with the defaults of `schema`.
    pub fn in_memory(schema: SettingsSchema) -> Self {
        Self::from_settings(Settings::new(schema))
    }

    /// Loads the settings file at `path` with the rules of
    /// `Settings::load_or_recover`. A corrupt file moves aside. A file
    /// that does not load leaves the defaults in effect without a save
    /// path, so that it is not overwritten. Each problem waits in
    /// [`Storage::take_errors`].
    pub fn open(schema: SettingsSchema, path: PathBuf) -> Self {
        match Settings::load_or_recover(schema.clone(), path.clone()) {
            Ok((settings, report)) => {
                let storage = Self::from_settings(settings);
                if let Some(moved_to) = report.recovered_to {
                    storage.report(StorageError::Recovered { moved_to });
                }
                storage
            }
            Err(error) => {
                let storage = Self::in_memory(schema);
                storage.report(StorageError::Load {
                    path,
                    message: error.to_string(),
                });
                storage
            }
        }
    }

    /// The stored value of `key`, else the schema default.
    pub fn get(&self, key: &str) -> Option<SettingValue> {
        self.shared.settings.borrow().get(key).cloned()
    }

    /// Stores `value` under `key` in memory.
    pub fn set(&self, key: &str, value: SettingValue) -> Result<(), StorageError> {
        self.shared
            .settings
            .borrow_mut()
            .set(key, value)
            .map_err(|error| match error {
                SettingsError::InvalidKey(key) => StorageError::InvalidKey(key),
                other => StorageError::Save {
                    message: other.to_string(),
                },
            })
    }

    /// Runs `read` with the settings, for types that store several
    /// values, such as `WindowGeometry`.
    pub fn with_settings<R>(&self, read: impl FnOnce(&Settings) -> R) -> R {
        read(&self.shared.settings.borrow())
    }

    /// Runs `change` with the settings, which record what it changes.
    pub fn update<R>(&self, change: impl FnOnce(&mut Settings) -> R) -> R {
        change(&mut self.shared.settings.borrow_mut())
    }

    /// The file that [`Storage::save`] writes, if any.
    pub fn path(&self) -> Option<PathBuf> {
        self.shared
            .settings
            .borrow()
            .path()
            .map(std::path::Path::to_path_buf)
    }

    /// Writes the file when a value changed since the last save and the
    /// settings have a path. Returns whether it wrote.
    pub fn save(&self) -> Result<bool, StorageError> {
        let mut settings = self.shared.settings.borrow_mut();
        if !settings.is_dirty() || settings.path().is_none() {
            return Ok(false);
        }
        settings.save().map_err(|error| StorageError::Save {
            message: error.to_string(),
        })?;
        Ok(true)
    }

    /// Records a problem for [`Storage::take_errors`].
    pub fn report(&self, error: StorageError) {
        self.shared.errors.borrow_mut().push(error);
    }

    /// The problems found since the last call.
    pub fn take_errors(&self) -> Vec<StorageError> {
        std::mem::take(&mut *self.shared.errors.borrow_mut())
    }
}

impl Cx<'_> {
    /// State that the environment's [`Storage`] keeps under `key`, like
    /// state that the user's settings hold. It starts at the stored value,
    /// or at `default` when no value is stored, and each change writes it
    /// back. A stored value of another type gives `default` and reports
    /// [`StorageError::TypeMismatch`]. Without a storage, it is plain
    /// state.
    pub fn stored<T: StoredValue>(&mut self, key: &str, default: T) -> State<T> {
        let storage = self.env.storage().cloned();
        let stored = storage.as_ref().and_then(|storage| storage.get(key));
        let initial = match &stored {
            Some(setting) => T::from_setting(setting).unwrap_or_else(|| {
                self.inner
                    .report(ViewError::Storage(StorageError::TypeMismatch {
                        key: key.to_owned(),
                    }));
                default.clone()
            }),
            None => default,
        };
        let state = self.state(initial);
        if let Some(storage) = storage {
            let read: Rc<dyn Fn() -> Option<SettingValue>> =
                Rc::new(move || state.try_get().ok().map(|value| value.to_setting()));
            write_back(self, storage, key, stored, read);
        }
        state
    }
}

/// Writes the value that `read` gives to `key` each time it changes.
/// `stored` is the value in the storage now, which is not written again.
fn write_back(
    cx: &mut Cx<'_>,
    storage: Storage,
    key: &str,
    stored: Option<SettingValue>,
    read: Rc<dyn Fn() -> Option<SettingValue>>,
) {
    let key = key.to_owned();
    let last = RefCell::new(stored);
    cx.react(move |inner| {
        let Some(value) = read() else {
            return;
        };
        let mut last = last.borrow_mut();
        if last.is_none() {
            // The first run records the default without storing it.
            *last = Some(value);
            return;
        }
        if last.as_ref() == Some(&value) {
            return;
        }
        if let Err(error) = storage.set(&key, value.clone()) {
            inner.report(ViewError::Storage(error));
        }
        *last = Some(value);
    });
}
