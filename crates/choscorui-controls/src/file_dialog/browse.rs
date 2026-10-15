//! The directory that the dialog shows: its model, its filtered rows,
//! the current row, and the selection.

use choscorui_core::{
    DirectoryState, EditableModel, FileSystemModel, ItemKey, ItemModel, format_size,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

type Waker = Arc<dyn Fn() + Send + Sync>;

/// One entry of the shown directory.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Row {
    pub key: ItemKey,
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// The size text of a file, empty for a directory.
    pub size: String,
}

pub(super) struct Browse {
    model: FileSystemModel,
    path: PathBuf,
    show_hidden: bool,
    waker: Option<Waker>,
    /// The model revision and state that the rows show, or `None` before
    /// the first rebuild.
    shown: Option<(u64, DirectoryState)>,
    rows: Vec<Row>,
    /// The position of each row by key, so that input does not search.
    index: HashMap<ItemKey, usize>,
    current: Option<ItemKey>,
    /// The row where a Shift range starts.
    anchor: Option<ItemKey>,
    selected: HashSet<ItemKey>,
    /// The scroll offset of the list, in points.
    pub scroll: f64,
}

impl Browse {
    pub fn new(path: PathBuf) -> Self {
        Self {
            model: FileSystemModel::new(path.clone()),
            path,
            show_hidden: false,
            waker: None,
            shown: None,
            rows: Vec::new(),
            index: HashMap::new(),
            current: None,
            anchor: None,
            selected: HashSet::new(),
            scroll: 0.0,
        }
    }

    /// Starts the listing of the directory.
    pub fn load(&mut self) {
        self.model.set_show_hidden(self.show_hidden);
        self.model.set_waker(self.waker.clone());
        self.model.fetch_more(None);
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The path as the location field shows it.
    pub fn path_text(&self) -> String {
        self.path.display().to_string()
    }

    /// The parent directory, or `None` at the root of the path.
    pub fn parent(&self) -> Option<PathBuf> {
        self.path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
    }

    /// Shows `path`: a new listing, with no current row and no selection.
    pub fn navigate(&mut self, path: PathBuf) {
        self.model = FileSystemModel::new(path.clone());
        self.path = path;
        self.shown = None;
        self.rows.clear();
        self.index.clear();
        self.current = None;
        self.anchor = None;
        self.selected.clear();
        self.scroll = 0.0;
        self.load();
    }

    pub fn show_hidden(&self) -> bool {
        self.show_hidden
    }

    /// Lists the directory again with or without the hidden files. The
    /// rows keep their keys, so the selection stays.
    pub fn set_show_hidden(&mut self, show: bool) {
        self.show_hidden = show;
        self.model.set_show_hidden(show);
        // A refresh fails only when the thread cannot start; the state
        // then stays as it was.
        let _ = self.model.refresh(None);
    }

    pub fn set_waker(&mut self, waker: Option<Waker>) {
        self.waker = waker.clone();
        self.model.set_waker(waker);
    }

    pub fn state(&self) -> DirectoryState {
        self.model.state(None)
    }

    pub fn is_loading(&self) -> bool {
        matches!(
            self.state(),
            DirectoryState::Loading | DirectoryState::Unloaded
        )
    }

    pub fn poll(&mut self) {
        self.model.poll();
    }

    pub fn wait(&mut self, timeout: Duration) {
        self.model.wait(timeout);
    }

    /// True when the model changed since the last rebuild.
    pub fn stale(&self) -> bool {
        self.shown.as_ref() != Some(&(self.model.revision(), self.state()))
    }

    /// Makes the rows from the entries that `keep` accepts, in the model
    /// order: directories first, then by name. The current row and the
    /// selection keep the entries that still show.
    pub fn rebuild(&mut self, keep: impl Fn(&Row) -> bool) {
        let count = self.model.row_count(None);
        self.rows = (0..count)
            .filter_map(|row| {
                let key = self.model.key(None, row)?;
                let entry = self.model.entry(key)?;
                Some(Row {
                    key,
                    name: entry.name.clone(),
                    path: entry.path.clone(),
                    is_dir: entry.is_dir,
                    size: if entry.is_dir {
                        String::new()
                    } else {
                        format_size(entry.size)
                    },
                })
            })
            .filter(|row| keep(row))
            .collect();
        self.index = self
            .rows
            .iter()
            .enumerate()
            .map(|(position, row)| (row.key, position))
            .collect();
        let index = &self.index;
        self.selected.retain(|key| index.contains_key(key));
        self.current = self.current.filter(|key| index.contains_key(key));
        self.anchor = self.anchor.filter(|key| index.contains_key(key));
        self.shown = Some((self.model.revision(), self.state()));
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn row(&self, index: usize) -> Option<&Row> {
        self.rows.get(index)
    }

    /// The position of the current row.
    pub fn current(&self) -> Option<usize> {
        self.current.and_then(|key| self.index.get(&key).copied())
    }

    pub fn current_row(&self) -> Option<&Row> {
        self.current().and_then(|index| self.rows.get(index))
    }

    pub fn is_selected(&self, index: usize) -> bool {
        self.rows
            .get(index)
            .is_some_and(|row| self.selected.contains(&row.key))
    }

    /// The selected rows, in row order.
    pub fn selected_rows(&self) -> Vec<&Row> {
        self.rows
            .iter()
            .filter(|row| self.selected.contains(&row.key))
            .collect()
    }

    /// Makes `index` the current row and the start of the next range.
    pub fn set_current(&mut self, index: usize) {
        if let Some(row) = self.rows.get(index) {
            self.current = Some(row.key);
            self.anchor = Some(row.key);
        }
    }

    /// Selects only `index` when `selectable` accepts it, else nothing.
    pub fn select_only(&mut self, index: usize, selectable: impl Fn(&Row) -> bool) {
        self.selected.clear();
        if let Some(row) = self.rows.get(index).filter(|row| selectable(row)) {
            self.selected.insert(row.key);
        }
    }

    /// Adds `index` to the selection, or removes it.
    pub fn toggle(&mut self, index: usize, selectable: impl Fn(&Row) -> bool) {
        if let Some(row) = self.rows.get(index).filter(|row| selectable(row))
            && !self.selected.remove(&row.key)
        {
            self.selected.insert(row.key);
        }
    }

    /// Selects the rows from the anchor to `index` that `selectable`
    /// accepts, and makes `index` current. The anchor stays.
    pub fn extend(&mut self, index: usize, selectable: impl Fn(&Row) -> bool) {
        let anchor = self
            .anchor
            .and_then(|key| self.index.get(&key).copied())
            .unwrap_or(index);
        let (low, high) = (anchor.min(index), anchor.max(index));
        self.selected = self.rows[low..=high.min(self.rows.len().saturating_sub(1))]
            .iter()
            .filter(|row| selectable(row))
            .map(|row| row.key)
            .collect();
        if let Some(row) = self.rows.get(index) {
            self.current = Some(row.key);
        }
    }

    /// Selects every row that `selectable` accepts.
    pub fn select_all(&mut self, selectable: impl Fn(&Row) -> bool) {
        self.selected = self
            .rows
            .iter()
            .filter(|row| selectable(row))
            .map(|row| row.key)
            .collect();
    }
}
