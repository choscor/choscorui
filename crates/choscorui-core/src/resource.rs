//! Packaged assets: icons, images, fonts, translations, and other data.
//!
//! Names are slash-separated relative paths such as `icons/save.svg`.
//! Raster images may have scale variants named `icons/save@2x.png`; lookup
//! picks the closest variant at or above the requested scale.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceKind {
    Svg,
    Png,
    Jpeg,
    Font,
    Translation,
    Data,
}

impl ResourceKind {
    pub fn from_name(name: &str) -> Self {
        let extension = name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase());
        match extension.as_deref() {
            Some("svg") => Self::Svg,
            Some("png") => Self::Png,
            Some("jpg" | "jpeg") => Self::Jpeg,
            Some("ttf" | "otf" | "ttc") => Self::Font,
            Some("strings" | "ftl" | "lang") => Self::Translation,
            _ => Self::Data,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResourceError {
    /// No resource with this name (or any scale variant) is bundled.
    Missing {
        name: String,
    },
    /// Names must be relative, non-empty, and contain no `..` segments.
    InvalidName {
        name: String,
    },
    WrongKind {
        name: String,
        expected: ResourceKind,
        found: ResourceKind,
    },
    Io {
        path: PathBuf,
        message: String,
    },
    InvalidUtf8 {
        name: String,
    },
}

impl fmt::Display for ResourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { name } => write!(formatter, "resource `{name}` is not bundled"),
            Self::InvalidName { name } => write!(formatter, "invalid resource name `{name}`"),
            Self::WrongKind {
                name,
                expected,
                found,
            } => write!(
                formatter,
                "resource `{name}` is {found:?}, expected {expected:?}"
            ),
            Self::Io { path, message } => {
                write!(formatter, "cannot read `{}`: {message}", path.display())
            }
            Self::InvalidUtf8 { name } => write!(formatter, "resource `{name}` is not UTF-8"),
        }
    }
}

impl std::error::Error for ResourceError {}

/// A resolved resource and the scale it was authored for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResourceRef<'a> {
    pub name: &'a str,
    pub kind: ResourceKind,
    pub scale: u8,
    pub bytes: &'a [u8],
}

/// Immutable name → bytes map built at startup.
#[derive(Clone, Debug, Default)]
pub struct ResourceBundle {
    entries: BTreeMap<String, Vec<u8>>,
}

fn validate_name(name: &str) -> Result<(), ResourceError> {
    let invalid = name.is_empty()
        || name.starts_with('/')
        || name.contains('\\')
        || name
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..");
    if invalid {
        Err(ResourceError::InvalidName {
            name: name.to_owned(),
        })
    } else {
        Ok(())
    }
}

/// Splits `icons/save@2x.png` into (`icons/save.png`, 2).
fn split_scale(name: &str) -> (String, u8) {
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, ext)) if !ext.contains('/') => (stem, Some(ext)),
        _ => (name, None),
    };
    if let Some((base, scale)) = stem.rsplit_once('@') {
        if let Some(scale) = scale.strip_suffix('x').and_then(|s| s.parse::<u8>().ok()) {
            if scale > 0 {
                let base = match extension {
                    Some(ext) => format!("{base}.{ext}"),
                    None => base.to_owned(),
                };
                return (base, scale);
            }
        }
    }
    (name.to_owned(), 1)
}

impl ResourceBundle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces an entry, typically from `include_bytes!`.
    pub fn insert(&mut self, name: &str, bytes: impl Into<Vec<u8>>) -> Result<(), ResourceError> {
        validate_name(name)?;
        self.entries.insert(name.to_owned(), bytes.into());
        Ok(())
    }

    /// Loads every regular file below `root`. Hidden files are skipped.
    pub fn load_dir(root: &Path) -> Result<Self, ResourceError> {
        let mut bundle = Self::new();
        let mut pending = vec![root.to_path_buf()];
        let io = |path: &Path, error: std::io::Error| ResourceError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        };
        while let Some(directory) = pending.pop() {
            let entries = std::fs::read_dir(&directory).map_err(|e| io(&directory, e))?;
            for entry in entries {
                let entry = entry.map_err(|e| io(&directory, e))?;
                let path = entry.path();
                if entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let kind = entry.file_type().map_err(|e| io(&path, e))?;
                if kind.is_dir() {
                    pending.push(path);
                } else if kind.is_file() {
                    let relative = path
                        .strip_prefix(root)
                        .expect("walk stays below root")
                        .components()
                        .map(|part| part.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("/");
                    let bytes = std::fs::read(&path).map_err(|e| io(&path, e))?;
                    bundle.insert(&relative, bytes)?;
                }
            }
        }
        Ok(bundle)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.lookup(name, 1).is_ok()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Exact lookup without scale variants.
    pub fn bytes(&self, name: &str) -> Result<&[u8], ResourceError> {
        validate_name(name)?;
        self.entries
            .get(name)
            .map(Vec::as_slice)
            .ok_or_else(|| ResourceError::Missing {
                name: name.to_owned(),
            })
    }

    pub fn text(&self, name: &str) -> Result<&str, ResourceError> {
        std::str::from_utf8(self.bytes(name)?).map_err(|_| ResourceError::InvalidUtf8 {
            name: name.to_owned(),
        })
    }

    /// Picks the smallest variant whose scale is at least `scale`, else the
    /// largest available. Vector resources ignore scale.
    pub fn lookup(&self, name: &str, scale: u8) -> Result<ResourceRef<'_>, ResourceError> {
        validate_name(name)?;
        let (base, _) = split_scale(name);
        let mut candidates: Vec<(u8, &str, &[u8])> = self
            .entries
            .iter()
            .filter_map(|(entry, bytes)| {
                let (entry_base, entry_scale) = split_scale(entry);
                (entry_base == base).then_some((entry_scale, entry.as_str(), bytes.as_slice()))
            })
            .collect();
        if candidates.is_empty() {
            return Err(ResourceError::Missing {
                name: name.to_owned(),
            });
        }
        candidates.sort_by_key(|(entry_scale, _, _)| *entry_scale);
        let chosen = candidates
            .iter()
            .find(|(entry_scale, _, _)| *entry_scale >= scale.max(1))
            .or(candidates.last())
            .expect("not empty");
        Ok(ResourceRef {
            name: chosen.1,
            kind: ResourceKind::from_name(chosen.1),
            scale: chosen.0,
            bytes: chosen.2,
        })
    }

    pub fn expect_kind(
        &self,
        name: &str,
        scale: u8,
        expected: ResourceKind,
    ) -> Result<ResourceRef<'_>, ResourceError> {
        let found = self.lookup(name, scale)?;
        if found.kind != expected {
            return Err(ResourceError::WrongKind {
                name: name.to_owned(),
                expected,
                found: found.kind,
            });
        }
        Ok(found)
    }
}
