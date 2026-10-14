//! Static content: labels and separators.

use crate::ui::page::{PageCx, column, section};
use choscorui::compose::t;
use choscorui::compose::view::{AnyView, Divider, Frame, HStack, Label, VStack, ViewExt};
use choscorui::core::Space;

/// Labels in the label recipe; the second one is disabled.
pub fn label(_: &PageCx) -> AnyView {
    column((
        Label::new(t!("label.email")),
        Label::new(t!("label.disabled")).disabled(true),
        Label::new(t!("label.vietnamese")),
        Label::new(t!("label.cjk")),
    ))
}

pub fn separator(_: &PageCx) -> AnyView {
    column((
        section(
            t!("separator.horizontal"),
            VStack::new((Divider, Divider, Divider))
                .spacing(Space::S4)
                .frame(Frame::new().width(320.0)),
        ),
        section(
            t!("separator.vertical"),
            HStack::new((Divider, Divider, Divider))
                .spacing(Space::S12)
                .frame(Frame::new().height(120.0)),
        ),
    ))
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<crate::ui::page::Build> {
    Some(match id {
        "label" => label,
        "separator" => separator,
        _ => return None,
    })
}
