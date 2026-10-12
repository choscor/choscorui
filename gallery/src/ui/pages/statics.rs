//! Static content: labels and separators.

use crate::ui::page::{PageCx, column, section};
use choscorui::compose::t;
use choscorui::compose::view::{AnyView, Divider, Frame, HStack, Text, VStack, ViewExt};
use choscorui::core::{FontWeight, Space, TextPreset};

pub fn label(_: &PageCx) -> AnyView {
    let rows = [
        ("label.email", false),
        ("label.disabled", true),
        ("label.vietnamese", false),
        ("label.cjk", false),
    ];
    let label = |(key, disabled): (&str, bool)| {
        Text::new(t!(key))
            .text_style(TextPreset::Small)
            .font_weight(FontWeight::MEDIUM)
            .opacity(if disabled { 0.5 } else { 1.0 })
    };
    column((
        label(rows[0]),
        label(rows[1]),
        label(rows[2]),
        label(rows[3]),
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
