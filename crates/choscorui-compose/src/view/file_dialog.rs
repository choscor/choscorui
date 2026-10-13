//! File dialogs that a view presents through the environment's services:
//! the `file_importer` and `file_exporter` modifiers.

use super::env::Env;
use super::host::Cx;
use super::modifier::Modifier;
use super::state::Binding;
use crate::reactive::Runtime;
use choscorui_core::{FileDialogRequest, ServiceError};
use std::path::PathBuf;
use std::rc::Rc;

/// What a file dialog gives back.
pub(crate) type DialogResult = Rc<dyn Fn(Result<Vec<PathBuf>, ServiceError>)>;

/// A file dialog modifier: show `request` while `presented` is true.
pub(crate) fn modifier(
    presented: Binding<bool>,
    request: FileDialogRequest,
    done: DialogResult,
) -> Modifier {
    Modifier::FileDialog(presented, request, done)
}

/// Each time `presented` becomes true, shows the dialog of `request`
/// with the environment's services, sets `presented` back to false, and
/// calls `done` with the result.
pub(super) fn mount(
    cx: &mut Cx<'_>,
    env: &Env,
    presented: Binding<bool>,
    request: FileDialogRequest,
    done: DialogResult,
) {
    let services = env.services().clone();
    cx.react(move |_| {
        if !presented.try_get().unwrap_or(false) {
            return;
        }
        Runtime::untracked(|| {
            let result = services.choose_files(&request);
            presented.set(false);
            done(result);
        });
    });
}
