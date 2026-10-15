//! The default application of a MIME type, after the XDG "Association
//! between MIME types and applications" specification: `mimeapps.list`
//! files, then `mimeinfo.cache`. Also the MIME type of a file name from
//! the `globs2` files of the shared MIME database.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The XDG base directories and the desktop names of the session.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct XdgDirs {
    pub config_home: Option<PathBuf>,
    pub config_dirs: Vec<PathBuf>,
    pub data_home: Option<PathBuf>,
    pub data_dirs: Vec<PathBuf>,
    /// `XDG_CURRENT_DESKTOP` in lower case, such as `gnome`.
    pub desktops: Vec<String>,
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn env_list(name: &str, default: &str) -> Vec<PathBuf> {
    let value = std::env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_owned());
    value
        .split(':')
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .collect()
}

impl XdgDirs {
    /// The directories from the environment, with the defaults of the
    /// specification.
    pub fn from_env() -> Self {
        let home = env_path("HOME");
        Self {
            config_home: env_path("XDG_CONFIG_HOME")
                .or_else(|| home.as_ref().map(|home| home.join(".config"))),
            config_dirs: env_list("XDG_CONFIG_DIRS", "/etc/xdg"),
            data_home: env_path("XDG_DATA_HOME")
                .or_else(|| home.as_ref().map(|home| home.join(".local/share"))),
            data_dirs: env_list("XDG_DATA_DIRS", "/usr/local/share:/usr/share"),
            desktops: std::env::var("XDG_CURRENT_DESKTOP")
                .unwrap_or_default()
                .split(':')
                .filter(|name| !name.is_empty())
                .map(str::to_ascii_lowercase)
                .collect(),
        }
    }

    /// The data directories, the user's first.
    fn data(&self) -> impl Iterator<Item = &PathBuf> {
        self.data_home.iter().chain(&self.data_dirs)
    }

    /// The `mimeapps.list` files in the order of precedence.
    fn list_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut add = |directory: &Path| {
            for desktop in &self.desktops {
                files.push(directory.join(format!("{desktop}-mimeapps.list")));
            }
            files.push(directory.join("mimeapps.list"));
        };
        for directory in self.config_home.iter().chain(&self.config_dirs) {
            add(directory);
        }
        for directory in self.data() {
            add(&directory.join("applications"));
        }
        files
    }

    /// The installed desktop file of the desktop file id `id`, such as
    /// `firefox.desktop`. An id with dashes can name a file in a
    /// subdirectory: `kde-okular.desktop` is `kde/okular.desktop`.
    pub fn desktop_file(&self, id: &str) -> Option<PathBuf> {
        for directory in self.data() {
            let applications = directory.join("applications");
            let direct = applications.join(id);
            if direct.is_file() {
                return Some(direct);
            }
            for (index, _) in id.match_indices('-') {
                let nested = applications.join(&id[..index]).join(&id[index + 1..]);
                if nested.is_file() {
                    return Some(nested);
                }
            }
        }
        None
    }
}

/// The entries of one group of a desktop-entry style file: each line
/// `key=value` under `[group]`.
fn group_entries(text: &str, group: &str) -> Vec<(String, String)> {
    let mut inside = false;
    let mut entries = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            inside = name == group;
            continue;
        }
        if inside && let Some((key, value)) = line.split_once('=') {
            entries.push((key.trim().to_owned(), value.trim().to_owned()));
        }
    }
    entries
}

/// The desktop file ids that `group` of `text` lists for `mime`.
fn listed(text: &str, group: &str, mime: &str) -> Vec<String> {
    group_entries(text, group)
        .into_iter()
        .filter(|(key, _)| key == mime)
        .flat_map(|(_, value)| {
            value
                .split(';')
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The desktop file of the default application for `mime`.
pub fn default_application(dirs: &XdgDirs, mime: &str) -> Option<PathBuf> {
    let files: Vec<String> = dirs
        .list_files()
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .collect();
    // 1. The first installed default, in the order of precedence.
    for text in &files {
        for id in listed(text, "Default Applications", mime) {
            if let Some(path) = dirs.desktop_file(&id) {
                return Some(path);
            }
        }
    }
    // 2. The added associations, without those that a file of higher
    // precedence removed.
    let mut removed = HashSet::new();
    for text in &files {
        for id in listed(text, "Added Associations", mime) {
            if !removed.contains(&id)
                && let Some(path) = dirs.desktop_file(&id)
            {
                return Some(path);
            }
        }
        removed.extend(listed(text, "Removed Associations", mime));
    }
    // 3. The applications that declare the type, from the caches.
    for directory in dirs.data() {
        let cache = directory.join("applications/mimeinfo.cache");
        let Ok(text) = std::fs::read_to_string(cache) else {
            continue;
        };
        for id in listed(&text, "MIME Cache", mime) {
            if !removed.contains(&id)
                && let Some(path) = dirs.desktop_file(&id)
            {
                return Some(path);
            }
        }
    }
    None
}

/// The `Name` of a desktop file, or `None` when it has none.
pub fn application_name(desktop_file: &Path) -> Option<String> {
    let text = std::fs::read_to_string(desktop_file).ok()?;
    group_entries(&text, "Desktop Entry")
        .into_iter()
        .find(|(key, _)| key == "Name")
        .map(|(_, value)| value)
}

/// The MIME type of `file_name` from the `mime/globs2` files: the match
/// with the highest weight, then the longest pattern.
pub fn mime_type_of(dirs: &XdgDirs, file_name: &str) -> Option<String> {
    let lower = file_name.to_lowercase();
    let mut best: Option<(u32, usize, String)> = None;
    for directory in dirs.data() {
        let Ok(text) = std::fs::read_to_string(directory.join("mime/globs2")) else {
            continue;
        };
        for line in text.lines().filter(|line| !line.starts_with('#')) {
            let mut fields = line.split(':');
            let (Some(weight), Some(mime), Some(pattern)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let Ok(weight) = weight.parse::<u32>() else {
                continue;
            };
            // Only the simple `*.ext` patterns, which match most files.
            let Some(suffix) = pattern.strip_prefix('*') else {
                continue;
            };
            if suffix.contains(['*', '?', '[']) || !lower.ends_with(&suffix.to_lowercase()) {
                continue;
            }
            let better = best.as_ref().is_none_or(|(best_weight, length, _)| {
                (weight, pattern.len()) > (*best_weight, *length)
            });
            if better {
                best = Some((weight, pattern.len(), mime.to_owned()));
            }
        }
    }
    best.map(|(_, _, mime)| mime)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory tree under the system temporary directory.
    struct Tree(PathBuf);

    impl Tree {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("choscorui-mimeapps-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            Self(root)
        }

        fn write(&self, path: &str, text: &str) -> PathBuf {
            let path = self.0.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
            path
        }

        fn dirs(&self) -> XdgDirs {
            XdgDirs {
                config_home: Some(self.0.join("config")),
                config_dirs: vec![self.0.join("etc")],
                data_home: Some(self.0.join("home-data")),
                data_dirs: vec![self.0.join("usr")],
                desktops: vec!["gnome".to_owned()],
            }
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const ENTRY: &str = "[Desktop Entry]\nName=Web Browser\nExec=browser %u\n";

    #[test]
    fn the_user_default_wins_over_the_system_default() {
        let tree = Tree::new("default");
        tree.write("usr/applications/browser.desktop", ENTRY);
        let other = tree.write("usr/applications/other.desktop", ENTRY);
        tree.write(
            "etc/mimeapps.list",
            "[Default Applications]\nx-scheme-handler/https=browser.desktop\n",
        );
        tree.write(
            "config/gnome-mimeapps.list",
            "[Default Applications]\nx-scheme-handler/https=missing.desktop;other.desktop;\n",
        );
        let dirs = tree.dirs();
        assert_eq!(
            default_application(&dirs, "x-scheme-handler/https"),
            Some(other)
        );
        assert_eq!(default_application(&dirs, "x-scheme-handler/ftp"), None);
    }

    #[test]
    fn associations_and_caches_fill_in_without_a_default() {
        let tree = Tree::new("associations");
        let browser = tree.write("home-data/applications/browser.desktop", ENTRY);
        let viewer = tree.write("usr/applications/kde/viewer.desktop", ENTRY);
        tree.write(
            "config/mimeapps.list",
            "[Removed Associations]\nimage/png=browser.desktop;\n",
        );
        tree.write(
            "usr/applications/mimeinfo.cache",
            "[MIME Cache]\nimage/png=browser.desktop;kde-viewer.desktop;\ntext/html=browser.desktop;\n",
        );
        let dirs = tree.dirs();
        assert_eq!(default_application(&dirs, "image/png"), Some(viewer));
        assert_eq!(
            default_application(&dirs, "text/html"),
            Some(browser.clone())
        );
        assert_eq!(application_name(&browser).as_deref(), Some("Web Browser"));
    }

    #[test]
    fn globs_give_the_type_of_a_file_name() {
        let tree = Tree::new("globs");
        tree.write(
            "usr/mime/globs2",
            "# comment\n50:text/plain:*.txt\n50:image/png:*.png\n60:application/x-compressed-tar:*.tar.gz\n50:application/gzip:*.gz\n",
        );
        let dirs = tree.dirs();
        assert_eq!(
            mime_type_of(&dirs, "Notes.TXT").as_deref(),
            Some("text/plain")
        );
        assert_eq!(
            mime_type_of(&dirs, "a.tar.gz").as_deref(),
            Some("application/x-compressed-tar")
        );
        assert_eq!(mime_type_of(&dirs, "a.unknown"), None);
    }
}
