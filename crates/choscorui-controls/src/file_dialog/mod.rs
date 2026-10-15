//! The file dialog that the toolkit draws when the desktop has no file
//! chooser service, such as a Linux desktop without xdg-desktop-portal.
//! It browses core's [`FileSystemModel`] and gives a
//! [`FileDialogOutcome`] to its host.

mod browse;
mod confirm;
mod control;
mod geometry;
mod paint;

use crate::hosted::{Parts, TypeAhead};
use crate::text_field::TextField;
use browse::Browse;
use choscorui_core::{FileDialogKind, FileDialogRequest, Localizer};
use confirm::Confirm;
use geometry::Geometry;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

/// What a file dialog gives back to its host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileDialogOutcome {
    /// The chosen paths: one or more files, one directory, or the path to
    /// save to.
    Selected(Vec<PathBuf>),
    Cancelled,
}

/// The outcome of a [`FileDialog`], shared with its host. Clones share
/// one value.
#[derive(Clone, Debug, Default)]
pub struct FileDialogResult(Rc<RefCell<Option<FileDialogOutcome>>>);

impl FileDialogResult {
    /// The outcome, or `None` while the dialog is open.
    pub fn get(&self) -> Option<FileDialogOutcome> {
        self.0.borrow().clone()
    }

    /// Takes the outcome, so that the next call returns `None`.
    pub fn take(&self) -> Option<FileDialogOutcome> {
        self.0.borrow_mut().take()
    }

    fn set(&self, outcome: FileDialogOutcome) {
        *self.0.borrow_mut() = Some(outcome);
    }
}

/// A focus stop of the dialog, in Tab order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stop {
    Location,
    List,
    Name,
    Filter,
    Hidden,
    Cancel,
    Accept,
}

/// A part that takes presses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Part {
    Parent,
    Filter,
    Hidden,
    Cancel,
    Accept,
    /// A button of the overwrite confirmation, by index into its buttons.
    Confirm(usize),
}

/// One choice of the file type filter: the extensions that it shows, or
/// every file.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Filter {
    /// The text of the choice.
    label: String,
    /// Lowercase extensions without the dot; empty shows every file.
    extensions: Vec<String>,
}

impl Filter {
    /// The filters of `types`: all of them together, then each one when
    /// there are more than one. Without types, one filter shows every
    /// file.
    fn list(types: &[String], all_files: &str) -> Vec<Filter> {
        let types: Vec<String> = types
            .iter()
            .map(|extension| extension.trim_start_matches('.').to_lowercase())
            .filter(|extension| !extension.is_empty())
            .collect();
        if types.is_empty() {
            return vec![Filter {
                label: all_files.to_owned(),
                extensions: Vec::new(),
            }];
        }
        let pattern = |extensions: &[String]| {
            extensions
                .iter()
                .map(|extension| format!("*.{extension}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut filters = vec![Filter {
            label: pattern(&types),
            extensions: types.clone(),
        }];
        if types.len() > 1 {
            filters.extend(types.iter().map(|extension| Filter {
                label: pattern(std::slice::from_ref(extension)),
                extensions: vec![extension.clone()],
            }));
        }
        filters
    }

    /// True when the filter shows a file named `name`.
    fn matches(&self, name: &str) -> bool {
        self.extensions.is_empty()
            || Path::new(name)
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    self.extensions
                        .iter()
                        .any(|allowed| allowed.eq_ignore_ascii_case(extension))
                })
    }
}

/// The built-in texts of the dialog, in the language of the localizer
/// that made it.
#[derive(Clone, Debug)]
struct Labels {
    title: String,
    accept: String,
    cancel: String,
    location: String,
    parent: String,
    files: String,
    name: String,
    filter: String,
    hidden: String,
    folder: String,
    empty: String,
    loading: String,
    failed: String,
    replace: String,
    /// The confirmation title with a `{name}` place for the file name.
    replace_title: String,
    replace_message: String,
}

impl Labels {
    fn new(kind: FileDialogKind, strings: &Localizer) -> Self {
        let text = |id: &str| strings.message(&format!("choscorui.file_dialog.{id}"), &[]);
        let (title, accept) = match kind {
            FileDialogKind::Open => ("title.open", "open"),
            FileDialogKind::OpenMany => ("title.open_many", "open"),
            FileDialogKind::Directory => ("title.directory", "select"),
            FileDialogKind::Save => ("title.save", "save"),
        };
        Self {
            title: text(title),
            accept: text(accept),
            cancel: strings.message("choscorui.cancel", &[]),
            location: text("location"),
            parent: text("parent"),
            files: text("files"),
            name: text("name"),
            filter: text("filter"),
            hidden: text("hidden"),
            folder: text("folder"),
            empty: text("empty"),
            loading: text("loading"),
            failed: text("failed"),
            replace: text("replace"),
            replace_title: strings
                .message("choscorui.file_dialog.replace.title", &[("name", "{name}")]),
            replace_message: text("replace.message"),
        }
    }
}

/// A file dialog drawn by the toolkit, for a desktop without a file
/// chooser service. It lists one directory of a [`FileSystemModel`]
/// and fills its bounds, for example the content of a dialog window.
///
/// - The modes are those of [`FileDialogKind`]: one file, several files,
///   one directory, or a path to save to. A save to a name that exists
///   asks for confirmation first.
/// - The filter shows the files of the allowed types; directories always
///   show. A checkbox shows or hides hidden files.
/// - Tab moves through the location field, the list, the name field of
///   a save dialog, the filter, the hidden-files checkbox, Cancel, and
///   the accept button.
/// - Enter opens the current directory row or accepts. Backspace in the
///   list and Alt+Up go to the parent directory. Escape cancels.
/// - The outcome goes to the [`FileDialogResult`] of [`FileDialog::result`].
///   An accept also reports `Activated`, and a cancel `Dismissed`.
///
/// [`FileSystemModel`]: choscorui_core::FileSystemModel
pub struct FileDialog {
    kind: FileDialogKind,
    labels: Labels,
    browse: Browse,
    filters: Vec<Filter>,
    filter: usize,
    location: TextField,
    name: TextField,
    focus: Stop,
    parts: Parts<Part>,
    /// The row under the pointer.
    hovered_row: Option<usize>,
    type_ahead: TypeAhead,
    /// The button-like stop that a held Space presses.
    space: Option<Stop>,
    confirm: Option<Confirm>,
    geometry: Geometry,
    result: FileDialogResult,
    /// The host time of the next poll of a listing that loads.
    next_poll: f64,
    enabled: bool,
}

/// How often the dialog merges a listing that loads, in seconds.
const POLL_INTERVAL: f64 = 0.05;

impl FileDialog {
    /// A dialog of `kind` that starts in `directory`, with the built-in
    /// texts of `strings`.
    pub fn new(kind: FileDialogKind, directory: impl Into<PathBuf>, strings: &Localizer) -> Self {
        let labels = Labels::new(kind, strings);
        let browse = Browse::new(directory.into());
        let location = TextField::new(labels.location.clone(), browse.path_text()).quiet();
        let name = TextField::new(labels.name.clone(), "").quiet();
        let filters = Filter::list(
            &[],
            &strings.message("choscorui.file_dialog.all_files", &[]),
        );
        let mut dialog = Self {
            kind,
            labels,
            browse,
            filters,
            filter: 0,
            location,
            name,
            focus: Stop::List,
            parts: Parts::default(),
            hovered_row: None,
            type_ahead: TypeAhead::default(),
            space: None,
            confirm: None,
            geometry: Geometry::default(),
            result: FileDialogResult::default(),
            next_poll: f64::NEG_INFINITY,
            enabled: true,
        };
        if kind == FileDialogKind::Save {
            dialog.focus = Stop::Name;
        }
        dialog.browse.load();
        dialog.sync();
        dialog
    }

    /// A dialog for `request` that starts in `directory`: its kind, its
    /// title, its allowed types, and its suggested name.
    pub fn from_request(
        request: &FileDialogRequest,
        directory: impl Into<PathBuf>,
        strings: &Localizer,
    ) -> Self {
        let mut dialog = Self::new(request.kind, directory, strings)
            .with_allowed_types(&request.allowed_types, strings);
        if let Some(title) = &request.title {
            dialog = dialog.with_title(title.clone());
        }
        if let Some(name) = &request.suggested_name {
            dialog = dialog.with_suggested_name(name.clone());
        }
        dialog
    }

    /// The dialog with `title` in place of the built-in title of its kind.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.labels.title = title.into();
        self
    }

    /// The dialog with `prompt` as the label of the accept button.
    #[must_use]
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.labels.accept = prompt.into();
        self
    }

    /// The dialog that shows only files with the extensions `types`,
    /// without the dot. Empty types show every file. `strings` gives the
    /// text of the every-file filter.
    #[must_use]
    pub fn with_allowed_types(mut self, types: &[String], strings: &Localizer) -> Self {
        let all = strings.message("choscorui.file_dialog.all_files", &[]);
        self.filters = Filter::list(types, &all);
        self.filter = 0;
        self.sync_rows();
        self
    }

    /// The save dialog with `name` in its name field.
    #[must_use]
    pub fn with_suggested_name(mut self, name: impl Into<String>) -> Self {
        self.name.replace_value(&name.into());
        self
    }

    /// The dialog that shows hidden files, or hides them.
    #[must_use]
    pub fn with_hidden_files(mut self, show: bool) -> Self {
        if self.browse.show_hidden() != show {
            self.browse.set_show_hidden(show);
        }
        self
    }

    /// The dialog that calls `waker` from a worker thread when a listing
    /// is ready, for example to wake the event loop.
    #[must_use]
    pub fn with_waker(mut self, waker: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.browse.set_waker(Some(waker));
        self
    }

    pub fn kind(&self) -> FileDialogKind {
        self.kind
    }

    /// The directory that the dialog shows.
    pub fn directory(&self) -> &Path {
        self.browse.path()
    }

    /// True when hidden files show.
    pub fn shows_hidden_files(&self) -> bool {
        self.browse.show_hidden()
    }

    /// True while the listing of the directory loads.
    pub fn is_loading(&self) -> bool {
        self.browse.is_loading()
    }

    /// The handle that receives the outcome.
    pub fn result(&self) -> FileDialogResult {
        self.result.clone()
    }

    /// Merges a finished listing. A control host calls it through the
    /// timer of the control; a host that drives the dialog directly calls
    /// it after the waker runs. Returns true when the rows changed.
    pub fn poll(&mut self) -> bool {
        self.browse.poll();
        self.sync()
    }

    /// Waits up to `timeout` for the listing, so that the first frame of a
    /// new dialog shows the files. Returns true when the rows changed.
    pub fn wait(&mut self, timeout: Duration) -> bool {
        self.browse.wait(timeout);
        self.sync()
    }

    /// Rebuilds the rows when the listing changed. Returns true when it
    /// did.
    fn sync(&mut self) -> bool {
        if self.browse.stale() {
            self.sync_rows();
            true
        } else {
            false
        }
    }

    fn sync_rows(&mut self) {
        let filter = &self.filters[self.filter.min(self.filters.len() - 1)];
        let directories_only = self.kind == FileDialogKind::Directory;
        self.browse
            .rebuild(|row| row.is_dir || (!directories_only && filter.matches(&row.name)));
        self.hovered_row = None;
    }

    /// Reports `outcome` to the host.
    fn finish(&mut self, cx: &mut crate::ControlContext<'_>, outcome: FileDialogOutcome) {
        let message = match outcome {
            FileDialogOutcome::Selected(_) => crate::ControlMessage::Activated,
            FileDialogOutcome::Cancelled => crate::ControlMessage::Dismissed,
        };
        self.result.set(outcome);
        cx.emit(message);
    }
}
