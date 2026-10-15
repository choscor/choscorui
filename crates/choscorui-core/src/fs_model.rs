//! Lazy filesystem model. Directory listings run on worker threads and are
//! merged on the UI thread by [`FileSystemModel::poll`], so a slow volume
//! never blocks event handling. Refreshing a directory diffs the new
//! listing against the old one by name, keeping item keys (and therefore
//! selection and expansion) for entries that still exist.

use crate::order::stable_order;
use crate::{
    BasicCollator, Collator, ItemData, ItemKey, ItemModel, ModelChange, ModelError, Role,
    TreeStore, Value,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

pub const FS_COLUMN_NAME: usize = 0;
pub const FS_COLUMN_SIZE: usize = 1;
pub const FS_COLUMN_KIND: usize = 2;
pub const FS_COLUMN_MODIFIED: usize = 3;

#[derive(Clone, Debug, PartialEq)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    /// Seconds since the Unix epoch.
    pub modified: Option<u64>,
}

impl ItemData for FileEntry {
    fn data(&self, column: usize, role: Role) -> Value {
        match (column, role) {
            (FS_COLUMN_NAME, Role::Display | Role::Edit | Role::Sort | Role::Accessibility) => {
                Value::Text(self.name.clone())
            }
            (FS_COLUMN_NAME, Role::Icon) => {
                Value::Icon(if self.is_dir { "folder" } else { "file" }.into())
            }
            (FS_COLUMN_NAME, Role::ToolTip) => Value::Text(self.path.display().to_string()),
            (FS_COLUMN_SIZE, Role::Display) if !self.is_dir => Value::Text(format_size(self.size)),
            (FS_COLUMN_SIZE, Role::Sort) => Value::Int(self.size as i64),
            (FS_COLUMN_KIND, Role::Display | Role::Sort) => {
                Value::Text(if self.is_dir { "Folder" } else { "File" }.into())
            }
            (FS_COLUMN_MODIFIED, Role::Sort | Role::Display) => self
                .modified
                .map_or(Value::None, |secs| Value::Int(secs as i64)),
            _ => Value::None,
        }
    }
}

/// Byte count with binary units ("1.5 KiB").
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DirectoryState {
    Unloaded,
    Loading,
    Loaded,
    Failed(String),
}

type Listing = Result<Vec<FileEntry>, String>;
type Waker = Arc<dyn Fn() + Send + Sync>;
/// Listings that the worker threads finished, in arrival order.
type Arrivals = Mutex<Vec<(Option<ItemKey>, u64, Listing)>>;

pub struct FileSystemModel {
    root: PathBuf,
    store: TreeStore<FileEntry>,
    /// The state of each requested directory, and the generation of its
    /// last request. A listing of an older generation is dropped, and a
    /// directory with no entry is unloaded.
    states: HashMap<Option<ItemKey>, (DirectoryState, u64)>,
    /// The workers hold a weak reference, so a listing that ends after
    /// the model is gone neither stays nor wakes the event loop.
    arrivals: Arc<Arrivals>,
    waker: Option<Waker>,
    show_hidden: bool,
}

impl FileSystemModel {
    /// Browses `root`; nothing is read until [`fetch_more`] is called.
    ///
    /// [`fetch_more`]: crate::EditableModel::fetch_more
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            store: TreeStore::new(4),
            states: HashMap::new(),
            arrivals: Arc::default(),
            waker: None,
            show_hidden: false,
        }
    }

    /// Called from the worker thread when a listing is ready, typically to
    /// wake the event loop (for example a `WorkSender` or `Poster`).
    pub fn set_waker(&mut self, waker: Option<Waker>) {
        self.waker = waker;
    }
    pub fn set_show_hidden(&mut self, show: bool) {
        self.show_hidden = show;
    }
    pub fn root_path(&self) -> &Path {
        &self.root
    }
    pub fn entry(&self, key: ItemKey) -> Option<&FileEntry> {
        self.store.get(key)
    }
    pub fn state(&self, directory: Option<ItemKey>) -> DirectoryState {
        self.states
            .get(&directory)
            .map_or(DirectoryState::Unloaded, |(state, _)| state.clone())
    }

    fn directory_path(&self, directory: Option<ItemKey>) -> Option<PathBuf> {
        match directory {
            None => Some(self.root.clone()),
            Some(key) => self
                .store
                .get(key)
                .filter(|entry| entry.is_dir)
                .map(|entry| entry.path.clone()),
        }
    }

    /// Starts (or restarts) a background listing of `directory`.
    pub fn refresh(&mut self, directory: Option<ItemKey>) -> Result<(), ModelError> {
        let path = self
            .directory_path(directory)
            .ok_or(ModelError::MissingItem)?;
        let (state, generation) = self
            .states
            .entry(directory)
            .or_insert((DirectoryState::Unloaded, 0));
        *state = DirectoryState::Loading;
        *generation += 1;
        let generation = *generation;
        let arrivals = Arc::downgrade(&self.arrivals);
        let waker = self.waker.clone();
        let show_hidden = self.show_hidden;
        std::thread::Builder::new()
            .name("choscorui-fs-list".into())
            .spawn(move || {
                let listing = list_directory(&path, show_hidden);
                if deliver(&arrivals, (directory, generation, listing))
                    && let Some(waker) = waker
                {
                    waker();
                }
            })
            .map_err(|_| ModelError::InvalidValue)?;
        Ok(())
    }

    /// Merges finished listings. Returns how many directories changed.
    pub fn poll(&mut self) -> usize {
        let arrived =
            std::mem::take(&mut *self.arrivals.lock().unwrap_or_else(PoisonError::into_inner));
        let mut merged = 0;
        for (directory, generation, listing) in arrived {
            let current = self.states.get(&directory).map(|(_, current)| *current);
            if current != Some(generation)
                || directory.is_some_and(|key| self.store.get(key).is_none())
            {
                continue;
            }
            let state = match listing {
                Ok(entries) => {
                    self.merge(directory, entries);
                    DirectoryState::Loaded
                }
                Err(error) => DirectoryState::Failed(error),
            };
            self.states.insert(directory, (state, generation));
            merged += 1;
        }
        merged
    }

    /// Blocks until pending listings arrive or `timeout` passes (tests and
    /// command-line tools; UI code uses the waker and `poll`).
    pub fn wait(&mut self, timeout: std::time::Duration) -> usize {
        let deadline = std::time::Instant::now() + timeout;
        let mut merged = 0;
        loop {
            merged += self.poll();
            let loading = self
                .states
                .values()
                .any(|(state, _)| *state == DirectoryState::Loading);
            if !loading || std::time::Instant::now() >= deadline {
                return merged;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    fn merge(&mut self, directory: Option<ItemKey>, entries: Vec<FileEntry>) {
        let collator = BasicCollator;
        let order = stable_order(entries.len(), &|a, b| {
            let (a, b) = (&entries[a], &entries[b]);
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| collator.compare(&a.name, &b.name))
        });
        let mut slots: Vec<Option<FileEntry>> = entries.into_iter().map(Some).collect();
        let entries: Vec<FileEntry> = (order.into_iter())
            .filter_map(|index| slots[index].take())
            .collect();
        let mut existing: HashMap<String, ItemKey> = (0..self.store.row_count(directory))
            .filter_map(|row| self.store.key(directory, row))
            .filter_map(|key| self.store.get(key).map(|entry| (entry.name.clone(), key)))
            .collect();
        // Each listed entry takes its old key; the keys left are gone.
        let kept: Vec<Option<ItemKey>> = entries
            .iter()
            .map(|entry| existing.remove(&entry.name))
            .collect();
        let gone: Vec<ItemKey> = existing.into_values().collect();
        if !gone.is_empty() {
            for key in &gone {
                self.forget(*key);
            }
            let _ = self.store.remove(&gone);
        }
        for (row, (entry, kept)) in entries.into_iter().zip(kept).enumerate() {
            match kept {
                Some(key) => {
                    if self.store.get(key) != Some(&entry) {
                        let _ = self
                            .store
                            .update(key, vec![Role::Display, Role::Sort], |old| *old = entry);
                    }
                    if self.store.row(key) != Some(row) {
                        let _ = self.store.move_item(key, directory, row);
                    }
                }
                None => {
                    // A new directory has no state yet: it is unloaded.
                    let _ = self.store.insert(directory, row, [entry]);
                }
            }
        }
    }

    fn forget(&mut self, key: ItemKey) {
        let mut pending = vec![key];
        while let Some(current) = pending.pop() {
            self.states.remove(&Some(current));
            pending.extend(
                (0..self.store.row_count(Some(current)))
                    .filter_map(|row| self.store.key(Some(current), row)),
            );
        }
    }
}

/// Queues a finished listing. False when the model is gone.
fn deliver(arrivals: &Weak<Arrivals>, arrival: (Option<ItemKey>, u64, Listing)) -> bool {
    let Some(arrivals) = arrivals.upgrade() else {
        return false;
    };
    arrivals
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(arrival);
    true
}

fn list_directory(path: &Path, show_hidden: bool) -> Listing {
    let reader = std::fs::read_dir(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut entries = Vec::new();
    for item in reader {
        let Ok(item) = item else { continue };
        let name = item.file_name().to_string_lossy().into_owned();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        let Ok(metadata) = item.metadata() else {
            continue;
        };
        entries.push(FileEntry {
            name,
            path: item.path(),
            is_dir: metadata.is_dir(),
            size: if metadata.is_dir() { 0 } else { metadata.len() },
            modified: metadata
                .modified()
                .ok()
                .and_then(|time: SystemTime| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs()),
        });
    }
    Ok(entries)
}

impl ItemModel for FileSystemModel {
    fn row_count(&self, parent: Option<ItemKey>) -> usize {
        self.store.row_count(parent)
    }
    fn column_count(&self) -> usize {
        4
    }
    fn key(&self, parent: Option<ItemKey>, row: usize) -> Option<ItemKey> {
        self.store.key(parent, row)
    }
    fn parent(&self, key: ItemKey) -> Option<ItemKey> {
        self.store.parent(key)
    }
    fn row(&self, key: ItemKey) -> Option<usize> {
        self.store.row(key)
    }
    fn data(&self, key: ItemKey, column: usize, role: Role) -> Value {
        self.store.data(key, column, role)
    }
    fn header(&self, column: usize, role: Role) -> Value {
        match (column, role) {
            (FS_COLUMN_NAME, Role::Display) => Value::Text("Name".into()),
            (FS_COLUMN_SIZE, Role::Display) => Value::Text("Size".into()),
            (FS_COLUMN_KIND, Role::Display) => Value::Text("Kind".into()),
            (FS_COLUMN_MODIFIED, Role::Display) => Value::Text("Modified".into()),
            _ => Value::None,
        }
    }
    fn has_children(&self, key: ItemKey) -> bool {
        self.store.get(key).is_some_and(|entry| entry.is_dir)
    }
    fn revision(&self) -> u64 {
        self.store.revision()
    }
    fn changes_since(&self, revision: u64) -> Vec<ModelChange> {
        self.store.changes_since(revision)
    }
    fn can_fetch_more(&self, parent: Option<ItemKey>) -> bool {
        let is_directory = parent.is_none_or(|key| self.has_children(key));
        is_directory && self.state(parent) == DirectoryState::Unloaded
    }
}

impl crate::EditableModel for FileSystemModel {
    fn set_data(&mut self, _: ItemKey, _: usize, _: Role, _: Value) -> Result<(), ModelError> {
        Err(ModelError::ReadOnly)
    }
    /// Starts a background listing when the directory has not been loaded.
    fn fetch_more(&mut self, parent: Option<ItemKey>) {
        if self.can_fetch_more(parent) {
            let _ = self.refresh(parent);
        }
    }
}
