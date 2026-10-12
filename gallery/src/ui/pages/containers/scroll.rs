//! The scroll area page: a grid of tiles larger than its viewport.

use crate::ui::page::{PageCx, column};
use choscorui::compose::view::{AnyView, Configured, Frame, ViewExt};
use choscorui::controls::{
    ControlContext, ControlEvent, ControlKind, CustomControl, MeasureContext, PaintContext,
    ScrollArea, ScrollAreaInput, Slot, StyleQuery, TextAlign, paint_box,
};
use choscorui::core::{
    ColorRole, EventPhase, EventReply, Key, KeyEvent, Measured, NodeId, Point, PointerEvent,
    RadiusScale, Rect, ScrollViewport, SemanticAction, SemanticNode, Size, Tree,
};

const TILE: f64 = 90.0;
const TILES: usize = 14;

/// A 14 × 14 grid of tiles in a scroll area with overlay scroll bars. The
/// area scrolls a viewport and a content node of its own tree, which
/// covers the control's bounds; the control paints the visible tiles.
struct TileArea {
    tree: Tree,
    area: ScrollArea,
    /// The area's accessible name.
    name: String,
}

impl TileArea {
    fn new(name: String) -> Result<Self, String> {
        let mut tree = Tree::new(Rect::default());
        let viewport = tree
            .attach(tree.root(), Rect::default())
            .map_err(|error| error.to_string())?;
        tree.set_focusable(viewport, true)
            .map_err(|error| error.to_string())?;
        let content = tree
            .attach(viewport, Rect::default())
            .map_err(|error| error.to_string())?;
        let side = TILE * TILES as f64;
        let viewport = ScrollViewport::new(
            viewport,
            content,
            Size::new(side, side),
            Size::new(40.0, 40.0),
        )
        .map_err(|error| error.to_string())?;
        Ok(Self {
            tree,
            area: ScrollArea::new(viewport),
            name,
        })
    }

    fn viewport(&self) -> NodeId {
        self.area.node()
    }

    /// Handles `input` in the area's tree, and keeps the host's pointer
    /// capture with the area's. Returns whether the offset changed.
    fn handle(&mut self, cx: &mut ControlContext<'_>, input: ScrollAreaInput) -> bool {
        let captured = self.tree.pointer_capture().is_some();
        let scrolled = self.area.handle(&mut self.tree, input).unwrap_or(false);
        let capture = self.tree.pointer_capture().is_some();
        if capture != captured {
            cx.capture_pointer(capture);
        }
        if scrolled {
            cx.request_semantics();
        }
        cx.request_paint();
        scrolled
    }
}

impl CustomControl for TileArea {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        Measured::new(cx.constraints.max)
    }

    fn arrange(&mut self, _cx: &MeasureContext<'_>, bounds: Rect) {
        let root = self.tree.root();
        let _ = self.tree.set_bounds(root, bounds);
        let _ = self.area.viewport_mut().arrange(&mut self.tree, bounds);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        match event {
            ControlEvent::Pointer(event) => {
                let drag = self.tree.pointer_capture().is_some();
                let scrolled = self.handle(cx, ScrollAreaInput::Pointer(*event));
                let consumed = match event {
                    PointerEvent::Wheel { .. } => scrolled,
                    PointerEvent::Down { .. } => true,
                    PointerEvent::Move { .. } | PointerEvent::Up { .. } => drag,
                };
                if consumed {
                    EventReply::Stop
                } else {
                    EventReply::Continue
                }
            }
            ControlEvent::Key(event @ KeyEvent::Down { key, .. }) if *key != Key::Tab => {
                // The area takes keys while its viewport has focus; the
                // host only sends them while the control has it.
                let viewport = self.viewport();
                let _ = self.tree.set_focus(Some(viewport));
                let scrolled = self.handle(cx, ScrollAreaInput::Key(*event));
                let _ = self.tree.set_focus(None);
                if scrolled {
                    EventReply::Stop
                } else {
                    EventReply::Continue
                }
            }
            ControlEvent::FocusChanged(_) => {
                cx.request_paint();
                EventReply::Continue
            }
            ControlEvent::Dismiss => {
                self.handle(cx, ScrollAreaInput::Cancel);
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let viewport = cx.bounds;
        let panel = cx.style(StyleQuery::new(ControlKind::Card, Slot::Root));
        paint_box(cx.list, viewport, &panel, cx.rtl);
        let label = cx.style(StyleQuery::new(ControlKind::Label, Slot::Root));
        let content = self
            .tree
            .bounds(self.area.viewport().content())
            .unwrap_or_default();
        let (radius, fill) = (
            cx.tokens.radius(RadiusScale::Md),
            cx.tokens.color(ColorRole::Muted),
        );
        // Only the tiles that the viewport shows.
        let first = |offset: f64| ((offset / TILE).floor().max(0.0) as usize).min(TILES);
        let last =
            |offset: f64, extent: f64| (((offset + extent) / TILE).ceil() as usize).min(TILES);
        let offset = Point::new(
            viewport.origin.x - content.origin.x,
            viewport.origin.y - content.origin.y,
        );
        for row in first(offset.y)..last(offset.y, viewport.size.height) {
            for column in first(offset.x)..last(offset.x, viewport.size.width) {
                let tile = Rect::from_xywh(
                    content.origin.x + column as f64 * TILE,
                    content.origin.y + row as f64 * TILE,
                    TILE,
                    TILE,
                )
                .inset(6.0);
                let _ = cx.list.fill_rounded_rect(tile, radius, fill);
                let name = format!("{}{}", (b'A' + row as u8) as char, column + 1);
                cx.label(tile, &name, &label, TextAlign::Center);
            }
        }
        self.area.paint(cx, &self.tree);
        if cx.state.focus_visible() {
            let tokens = cx.tokens;
            let color = tokens.ring_color(tokens.color(ColorRole::Ring));
            let ring = viewport.inset(tokens.ring_width);
            let radius = tokens.radius(RadiusScale::Md);
            let _ = cx.list.draw_ring(ring, radius, tokens.ring_width, color);
        }
    }

    fn semantics(&self) -> SemanticNode {
        self.area.semantics(&self.tree, &self.name)
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Focus => {
                cx.request_focus();
                true
            }
            SemanticAction::Increment | SemanticAction::Decrement => {
                self.handle(cx, ScrollAreaInput::Semantic(action))
            }
            _ => false,
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        let _ = self.area.set_enabled(&mut self.tree, enabled);
    }
}

pub fn scroll_area(_: &PageCx) -> AnyView {
    column(
        Configured::try_custom(ControlKind::ScrollArea, |env| {
            TileArea::new(env.text("scroll-area.tiles", &[]))
        })
        .frame(Frame::size(560.0, 380.0)),
    )
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

        assert!(page.act("Tiles", SemanticAction::Focus));
        let home = KeyEvent::Down {
            key: Key::Home,
            repeat: false,
        };
        assert!(page.host.key(home, ShortcutModifiers::default()));
        assert_eq!(page.value("Tiles").as_deref(), Some("0%"));

        let tiles = page.node("Tiles");
        let center = page.host.tree().bounds(tiles).unwrap().center();
        let wheel = PointerEvent::Wheel {
            position: center,
            delta: Point::new(0.0, 3.0),
            unit: WheelUnit::Lines,
        };
        page.host.pointer(wheel, ShortcutModifiers::default());
        assert_ne!(page.value("Tiles").as_deref(), Some("0%"));
    }
}
