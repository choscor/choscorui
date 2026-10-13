//! Persistent per-user application settings.
//!
//! Settings are typed key/value pairs stored in a versioned, line-based
//! UTF-8 file:
//!
//! ```text
//! choscorui-settings 3
//! window.main.frame = rect:120,80,900,640
//! editor.font = text:Menlo
//! recent = list:a.txt,b\,c.txt
//! ```
//!
//! Loading reports, rather than hides, I/O and parse failures. A missing
//! file yields the schema defaults. Older files are migrated step by step.
//! A file written by a newer version is refused, so it is never
//! overwritten with data that newer version cannot read. Saving writes a
//! temporary sibling, syncs it, and renames it into place.

use crate::{Point, Rect, Size};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const MAGIC: &str = "choscorui-settings";

/// One stored setting value.
#[derive(Clone, Debug, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    Integer(i64),
    Float(f64),
    Text(String),
    List(Vec<String>),
    Rect(Rect),
}

#[derive(Debug)]
pub enum SettingsError {
    Io {
        path: PathBuf,
        kind: io::ErrorKind,
        message: String,
    },
    Parse {
        line: usize,
        message: String,
    },
    /// The file was written by a newer schema; it is left untouched.
    FutureVersion {
        found: u32,
        supported: u32,
    },
    InvalidKey(String),
    /// A migration step is missing between two versions.
    MissingMigration {
        from: u32,
    },
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                path,
                kind,
                message,
            } => write!(f, "{}: {kind:?}: {message}", path.display()),
            Self::Parse { line, message } => write!(f, "settings line {line}: {message}"),
            Self::FutureVersion { found, supported } => write!(
                f,
                "settings version {found} is newer than supported version {supported}"
            ),
            Self::InvalidKey(key) => write!(f, "invalid settings key {key:?}"),
            Self::MissingMigration { from } => {
                write!(f, "no settings migration from version {from}")
            }
        }
    }
}

impl std::error::Error for SettingsError {}

fn io_error(path: &Path, error: io::Error) -> SettingsError {
    SettingsError::Io {
        path: path.to_owned(),
        kind: error.kind(),
        message: error.to_string(),
    }
}

/// Keys are non-empty and use ASCII letters, digits, `.`, `_`, `-`, `/`.
pub fn is_valid_setting_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 256
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
}

/// A migration upgrades values in place from `from` to `from + 1`.
pub type SettingsMigration = fn(&mut BTreeMap<String, SettingValue>);

/// Current version, defaults, and the migration chain.
#[derive(Clone, Default)]
pub struct SettingsSchema {
    pub version: u32,
    defaults: BTreeMap<String, SettingValue>,
    migrations: BTreeMap<u32, SettingsMigration>,
}

impl SettingsSchema {
    /// A schema at `version`, with no defaults and no migrations.
    pub fn new(version: u32) -> Self {
        Self {
            version,
            ..Self::default()
        }
    }
    /// Adds the default value of `key`.
    pub fn with_default(mut self, key: &str, value: SettingValue) -> Self {
        debug_assert!(is_valid_setting_key(key));
        self.defaults.insert(key.to_owned(), value);
        self
    }
    /// Registers the step that upgrades `from` to `from + 1`.
    pub fn with_migration(mut self, from: u32, migration: SettingsMigration) -> Self {
        self.migrations.insert(from, migration);
        self
    }
    /// The default value of `key`, when the schema has one.
    pub fn default_value(&self, key: &str) -> Option<&SettingValue> {
        self.defaults.get(key)
    }
}

/// How a load completed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadReport {
    /// No file existed; defaults are in effect.
    pub created: bool,
    pub migrated_from: Option<u32>,
    /// A corrupt file was moved aside to this path (`load_or_recover`).
    pub recovered_to: Option<PathBuf>,
}

/// Settings for one application, backed by a file.
#[derive(Clone)]
pub struct Settings {
    schema: SettingsSchema,
    values: BTreeMap<String, SettingValue>,
    path: Option<PathBuf>,
    dirty: bool,
}

impl Settings {
    /// In-memory settings with the schema defaults.
    pub fn new(schema: SettingsSchema) -> Self {
        Self {
            schema,
            values: BTreeMap::new(),
            path: None,
            dirty: false,
        }
    }

    /// Reads the settings file at `path`, and migrates it to the schema version.
    /// A file that does not exist gives the defaults.
    pub fn load(
        schema: SettingsSchema,
        path: impl Into<PathBuf>,
    ) -> Result<(Self, LoadReport), SettingsError> {
        let path = path.into();
        let mut settings = Self::new(schema);
        settings.path = Some(path.clone());
        let text = match fs::read(&path) {
            Ok(bytes) => String::from_utf8(bytes).map_err(|_| SettingsError::Parse {
                line: 0,
                message: "file is not UTF-8".into(),
            })?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok((
                    settings,
                    LoadReport {
                        created: true,
                        ..LoadReport::default()
                    },
                ));
            }
            Err(error) => return Err(io_error(&path, error)),
        };
        let report = settings.read_str(&text)?;
        Ok((settings, report))
    }

    /// Like `load`, but a corrupt file is renamed to `<name>.corrupt` and
    /// defaults are used. I/O errors and newer versions still fail.
    pub fn load_or_recover(
        schema: SettingsSchema,
        path: impl Into<PathBuf>,
    ) -> Result<(Self, LoadReport), SettingsError> {
        let path = path.into();
        match Self::load(schema.clone(), path.clone()) {
            Err(SettingsError::Parse { .. }) => {
                let mut aside = path.clone().into_os_string();
                aside.push(".corrupt");
                let aside = PathBuf::from(aside);
                fs::rename(&path, &aside).map_err(|error| io_error(&path, error))?;
                let mut settings = Self::new(schema);
                settings.path = Some(path);
                Ok((
                    settings,
                    LoadReport {
                        recovered_to: Some(aside),
                        ..LoadReport::default()
                    },
                ))
            }
            other => other,
        }
    }

    /// Parses settings text, migrating older versions.
    pub fn read_str(&mut self, text: &str) -> Result<LoadReport, SettingsError> {
        let mut lines = text.lines().enumerate();
        let version = match lines.next() {
            Some((_, header)) => header
                .strip_prefix(MAGIC)
                .and_then(|rest| rest.trim().parse::<u32>().ok())
                .ok_or_else(|| SettingsError::Parse {
                    line: 1,
                    message: "missing settings header".into(),
                })?,
            None => {
                return Err(SettingsError::Parse {
                    line: 1,
                    message: "empty file".into(),
                });
            }
        };
        if version > self.schema.version {
            return Err(SettingsError::FutureVersion {
                found: version,
                supported: self.schema.version,
            });
        }
        let mut values = BTreeMap::new();
        for (index, line) in lines {
            let line_no = index + 1;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let parse_error = |message: &str| SettingsError::Parse {
                line: line_no,
                message: message.into(),
            };
            let (key, value) = line
                .split_once(" = ")
                .ok_or_else(|| parse_error("expected `key = type:value`"))?;
            if !is_valid_setting_key(key) {
                return Err(parse_error("invalid key"));
            }
            let value = decode_value(value).map_err(|message| parse_error(&message))?;
            values.insert(key.to_owned(), value);
        }
        let mut current = version;
        while current < self.schema.version {
            let migration = self
                .schema
                .migrations
                .get(&current)
                .ok_or(SettingsError::MissingMigration { from: current })?;
            migration(&mut values);
            current += 1;
        }
        self.values = values;
        self.dirty = version != self.schema.version;
        Ok(LoadReport {
            migrated_from: (version != self.schema.version).then_some(version),
            ..LoadReport::default()
        })
    }

    /// Serialized form, keys sorted; values equal to defaults are omitted.
    pub fn to_text(&self) -> String {
        let mut out = format!("{MAGIC} {}\n", self.schema.version);
        for (key, value) in &self.values {
            if self.schema.defaults.get(key) == Some(value) {
                continue;
            }
            out.push_str(key);
            out.push_str(" = ");
            out.push_str(&encode_value(value));
            out.push('\n');
        }
        out
    }

    /// Atomically writes to the load path (creating parent directories).
    pub fn save(&mut self) -> Result<(), SettingsError> {
        let path = self.path.clone().ok_or_else(|| SettingsError::Io {
            path: PathBuf::new(),
            kind: io::ErrorKind::NotFound,
            message: "settings have no file path".into(),
        })?;
        self.save_to(&path)?;
        self.dirty = false;
        Ok(())
    }

    /// Writes the settings to `path` atomically: a temporary file, a sync, and a rename.
    pub fn save_to(&self, path: &Path) -> Result<(), SettingsError> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
        }
        let mut temp = path.as_os_str().to_owned();
        temp.push(".tmp");
        let temp = PathBuf::from(temp);
        let write = || -> io::Result<()> {
            let mut file = fs::File::create(&temp)?;
            file.write_all(self.to_text().as_bytes())?;
            file.sync_all()?;
            fs::rename(&temp, path)
        };
        write().map_err(|error| {
            let _ = fs::remove_file(&temp);
            io_error(path, error)
        })
    }

    /// The file that `save` writes.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    /// Sets the file that `save` writes.
    pub fn set_path(&mut self, path: impl Into<PathBuf>) {
        self.path = Some(path.into());
    }
    /// True when a value changed after the last load or save.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
    /// The schema version of the settings.
    pub fn version(&self) -> u32 {
        self.schema.version
    }

    /// The stored value, else the schema default.
    pub fn get(&self, key: &str) -> Option<&SettingValue> {
        self.values
            .get(key)
            .or_else(|| self.schema.defaults.get(key))
    }
    /// Stores `value` under `key`. A key that is not valid gives an error.
    pub fn set(&mut self, key: &str, value: SettingValue) -> Result<(), SettingsError> {
        if !is_valid_setting_key(key) {
            return Err(SettingsError::InvalidKey(key.to_owned()));
        }
        if self.values.get(key) != Some(&value) {
            self.values.insert(key.to_owned(), value);
            self.dirty = true;
        }
        Ok(())
    }
    /// Removes a stored value so the default applies again.
    pub fn reset(&mut self, key: &str) -> bool {
        let removed = self.values.remove(key).is_some();
        self.dirty |= removed;
        removed
    }
    /// Keys with stored values under `prefix` (for example `"dock."`).
    pub fn keys_with_prefix<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = &'a str> {
        self.values
            .keys()
            .filter(move |key| key.starts_with(prefix))
            .map(String::as_str)
    }

    /// The value of `key` as a boolean, when it is one.
    pub fn bool(&self, key: &str) -> Option<bool> {
        match self.get(key)? {
            SettingValue::Bool(value) => Some(*value),
            _ => None,
        }
    }
    /// The value of `key` as an integer, when it is one.
    pub fn integer(&self, key: &str) -> Option<i64> {
        match self.get(key)? {
            SettingValue::Integer(value) => Some(*value),
            _ => None,
        }
    }
    /// The value of `key` as a float. An integer value converts.
    pub fn float(&self, key: &str) -> Option<f64> {
        match self.get(key)? {
            SettingValue::Float(value) => Some(*value),
            SettingValue::Integer(value) => Some(*value as f64),
            _ => None,
        }
    }
    /// The value of `key` as text, when it is text.
    pub fn text(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            SettingValue::Text(value) => Some(value),
            _ => None,
        }
    }
    /// The value of `key` as a list of strings, when it is one.
    pub fn list(&self, key: &str) -> Option<&[String]> {
        match self.get(key)? {
            SettingValue::List(value) => Some(value),
            _ => None,
        }
    }
    /// The value of `key` as a rectangle, when it is one.
    pub fn rect(&self, key: &str) -> Option<Rect> {
        match self.get(key)? {
            SettingValue::Rect(value) => Some(*value),
            _ => None,
        }
    }
}

fn escape(text: &str, out: &mut String, comma: bool) {
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ',' if comma => out.push_str("\\,"),
            ch => out.push(ch),
        }
    }
}

fn encode_value(value: &SettingValue) -> String {
    let mut out = String::new();
    match value {
        SettingValue::Bool(value) => out.push_str(if *value { "bool:true" } else { "bool:false" }),
        SettingValue::Integer(value) => out.push_str(&format!("int:{value}")),
        SettingValue::Float(value) => out.push_str(&format!("float:{value:?}")),
        SettingValue::Text(value) => {
            out.push_str("text:");
            escape(value, &mut out, false);
        }
        SettingValue::List(items) => {
            out.push_str("list:");
            if items.len() == 1 && items[0].is_empty() {
                // Distinguish [""] from [].
                out.push_str("\\e");
            }
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                escape(item, &mut out, true);
            }
        }
        SettingValue::Rect(rect) => out.push_str(&format!(
            "rect:{:?},{:?},{:?},{:?}",
            rect.origin.x, rect.origin.y, rect.size.width, rect.size.height
        )),
    }
    out
}

/// Splits on unescaped commas (when `split`) and unescapes.
fn unescape(text: &str, split: bool) -> Result<Vec<String>, String> {
    let mut parts = vec![String::new()];
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => match chars.next() {
                Some('\\') => parts.last_mut().unwrap().push('\\'),
                Some('n') => parts.last_mut().unwrap().push('\n'),
                Some('r') => parts.last_mut().unwrap().push('\r'),
                Some('t') => parts.last_mut().unwrap().push('\t'),
                Some(',') => parts.last_mut().unwrap().push(','),
                Some('e') if split => {}
                other => return Err(format!("invalid escape {other:?}")),
            },
            ',' if split => parts.push(String::new()),
            ch => parts.last_mut().unwrap().push(ch),
        }
    }
    Ok(parts)
}

fn decode_value(text: &str) -> Result<SettingValue, String> {
    let (kind, body) = text
        .split_once(':')
        .ok_or_else(|| "missing value type".to_string())?;
    let finite = |value: &str| {
        value
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("invalid number {value:?}"))
    };
    match kind {
        "bool" => match body {
            "true" => Ok(SettingValue::Bool(true)),
            "false" => Ok(SettingValue::Bool(false)),
            _ => Err(format!("invalid bool {body:?}")),
        },
        "int" => body
            .parse()
            .map(SettingValue::Integer)
            .map_err(|_| format!("invalid integer {body:?}")),
        "float" => finite(body).map(SettingValue::Float),
        "text" => Ok(SettingValue::Text(unescape(body, false)?.remove(0))),
        "list" if body.is_empty() => Ok(SettingValue::List(Vec::new())),
        "list" => Ok(SettingValue::List(unescape(body, true)?)),
        "rect" => {
            let numbers = body.split(',').map(finite).collect::<Result<Vec<_>, _>>()?;
            match numbers[..] {
                [x, y, width, height] if width >= 0.0 && height >= 0.0 => Ok(SettingValue::Rect(
                    Rect::new(Point::new(x, y), Size::new(width, height)),
                )),
                _ => Err("rect needs x,y,width,height".into()),
            }
        }
        _ => Err(format!("unknown value type {kind:?}")),
    }
}

/// The per-user settings file for `app_id` (a reverse-DNS identifier):
/// `~/Library/Application Support/<app_id>/settings.txt` on macOS,
/// `$XDG_CONFIG_HOME/<app_id>/settings.txt` (or `~/.config/...`)
/// elsewhere. `None` when the home directory is unknown.
pub fn user_settings_path(app_id: &str) -> Option<PathBuf> {
    if !is_valid_setting_key(app_id) || app_id.contains('/') || app_id.starts_with('.') {
        return None;
    }
    let home = std::env::var_os("HOME").filter(|home| !home.is_empty());
    let base = if cfg!(target_os = "macos") {
        PathBuf::from(home?).join("Library/Application Support")
    } else if let Some(config) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        PathBuf::from(config)
    } else {
        PathBuf::from(home?).join(".config")
    };
    Some(base.join(app_id).join("settings.txt"))
}

/// Saved top-level window placement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowGeometry {
    /// Frame in screen coordinates (top-left origin).
    pub frame: Rect,
    pub zoomed: bool,
    pub full_screen: bool,
}

impl WindowGeometry {
    pub fn store(&self, settings: &mut Settings, prefix: &str) -> Result<(), SettingsError> {
        settings.set(&format!("{prefix}.frame"), SettingValue::Rect(self.frame))?;
        settings.set(&format!("{prefix}.zoomed"), SettingValue::Bool(self.zoomed))?;
        settings.set(
            &format!("{prefix}.full_screen"),
            SettingValue::Bool(self.full_screen),
        )
    }

    pub fn restore(settings: &Settings, prefix: &str) -> Option<Self> {
        Some(Self {
            frame: settings.rect(&format!("{prefix}.frame"))?,
            zoomed: settings.bool(&format!("{prefix}.zoomed")).unwrap_or(false),
            full_screen: settings
                .bool(&format!("{prefix}.full_screen"))
                .unwrap_or(false),
        })
    }

    /// Fits a saved frame onto the current screens (visible frames, the
    /// first is the main screen). A frame whose title strip is no longer
    /// reachable (a display was removed) is centered on the main screen;
    /// an oversized frame shrinks to the screen it is on, never below
    /// `minimum`.
    pub fn fit_to_screens(&self, screens: &[Rect], minimum: Size) -> Rect {
        let Some(main) = screens.first().copied() else {
            return self.frame;
        };
        let title_strip = Rect::new(
            self.frame.origin,
            Size::new(self.frame.size.width, self.frame.size.height.min(24.0)),
        );
        let reachable = screens.iter().copied().find(|screen| {
            title_strip
                .intersection(*screen)
                .is_some_and(|area| area.size.width >= 48.0 && area.size.height >= 8.0)
        });
        let screen = reachable.unwrap_or(main);
        let width = self
            .frame
            .size
            .width
            .min(screen.size.width)
            .max(minimum.width);
        let height = self
            .frame
            .size
            .height
            .min(screen.size.height)
            .max(minimum.height);
        let (x, y) = if reachable.is_some() {
            (
                self.frame
                    .origin
                    .x
                    .clamp(screen.min_x(), (screen.max_x() - width).max(screen.min_x())),
                self.frame.origin.y.clamp(
                    screen.min_y(),
                    (screen.max_y() - height).max(screen.min_y()),
                ),
            )
        } else {
            (
                screen.center().x - width / 2.0,
                screen.center().y - height / 2.0,
            )
        };
        Rect::new(Point::new(x, y), Size::new(width, height))
    }
}
