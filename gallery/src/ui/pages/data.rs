//! Pages of the `data` group, as views.

use crate::ui::page::Build;

/// The view of page `id`, when this module has it.
pub fn page(_id: &str) -> Option<Build> {
    None
}
