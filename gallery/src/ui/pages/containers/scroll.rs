//! The scroll area page: a grid of tiles taller than its viewport.

use super::panel;
use crate::ui::page::{PageCx, column, stateful};
use choscorui::compose::view::{
    AnyView, ForEach, Frame, Grid, GridItem, ScrollView, Text, View, ViewExt,
};
use choscorui::core::{ColorRole, RadiusScale, Space};

const TILE: f64 = 84.0;
const COLUMNS: usize = 6;
const ROWS: usize = 14;

/// One tile, named by its row letter and column number.
fn tile(index: usize) -> impl View {
    let (row, column) = (index / COLUMNS, index % COLUMNS);
    let name = format!("{}{}", char::from(b'A' + row as u8), column + 1);
    Text::new(name)
        .frame(Frame::size(TILE, TILE))
        .background(ColorRole::Muted)
        .corner_radius(RadiusScale::Md)
}

/// A 6 × 14 grid of tiles in a scroll view on the card surface. The
/// scroll bar takes the `ScrollArea` look. The scroll view takes focus,
/// so the arrow, page, Home, and End keys scroll it.
pub fn scroll_area(_: &PageCx) -> AnyView {
    let tiles: Vec<usize> = (0..COLUMNS * ROWS).collect();
    stateful(move |cx| {
        let name = cx.env().text("scroll-area.tiles", &[]);
        let grid = Grid::new(
            vec![GridItem::Fixed(TILE); COLUMNS],
            ForEach::new(tiles.clone(), |index| *index, |index: &usize| tile(*index)),
        )
        .spacing(6.0)
        .padding(Space::S2);
        column(panel(
            ScrollView::new(grid)
                .accessibility_label(name)
                .focusable(true)
                .frame(Frame::size(560.0, 380.0)),
        ))
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::controls::ShortcutModifiers;
    use choscorui::core::{Key, KeyEvent, Point, PointerEvent, SemanticAction, WheelUnit};

    #[test]
    fn the_tiles_scroll_from_actions_keys_and_the_wheel() {
        let mut page = Harness::new("scroll-area");
        assert_eq!(page.value("Tiles").as_deref(), Some("0%"));
        assert!(page.act("Tiles", SemanticAction::Increment));
        let paged = page.value("Tiles");
        assert_ne!(paged.as_deref(), Some("0%"));

        let tiles = page.node("Tiles");
        page.host.focus(Some(tiles));
        let home = KeyEvent::Down {
            key: Key::Home,
            repeat: false,
        };
        assert!(page.host.key(home, ShortcutModifiers::default()));
        page.host.layout();
        assert_eq!(page.value("Tiles").as_deref(), Some("0%"));

        let center = page.host.tree().bounds(tiles).unwrap().center();
        let wheel = PointerEvent::Wheel {
            position: center,
            delta: Point::new(0.0, 3.0),
            unit: WheelUnit::Lines,
        };
        page.host.pointer(wheel, ShortcutModifiers::default());
        page.host.layout();
        assert_ne!(page.value("Tiles").as_deref(), Some("0%"));
    }
}
