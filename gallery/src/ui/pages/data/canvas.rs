//! The canvas page: 20,000 shapes with zoom, hit testing, rubber-band
//! selection, and culling, in a hosted canvas whose zoom persists.

use super::fill;
use crate::ui::page::PageCx;
use choscorui::compose::Property;
use choscorui::compose::view::{
    AnyView, Button, ControlView, Cx, HStack, HorizontalAlignment, Text, VStack, VerticalAlignment,
    View, ViewExt,
};
use choscorui::controls::{
    ButtonVariant, CanvasView, ControlContext, ControlEvent, ControlKind, ControlMessage,
    ControlValue, CustomControl, HostedCanvas, MAX_ZOOM, MIN_ZOOM, MeasureContext, PaintContext,
};
use choscorui::core::{
    Color, ColorRole, EventReply, ItemShape, ItemStyle, Measured, Point, Rect, Scene, SceneItem,
    SemanticAction, SemanticNode, SemanticRole, Size, Space, TextPreset,
};
use std::cell::RefCell;
use std::rc::Rc;

pub const CANVAS_ITEMS: usize = 20_000;
const CANVAS_COLUMNS: usize = 200;
const CANVAS_SPACING: f64 = 40.0;

/// The scene area that holds every item.
fn canvas_extent() -> Rect {
    Rect::new(
        Point::new(0.0, 0.0),
        Size::new(
            CANVAS_COLUMNS as f64 * CANVAS_SPACING,
            (CANVAS_ITEMS / CANVAS_COLUMNS) as f64 * CANVAS_SPACING,
        ),
    )
}

/// 28-unit squares and circles on a 40-unit grid, named "Node 0" on.
fn scene() -> Scene {
    let mut scene = Scene::new();
    let palette = [
        Color::rgba8(10, 132, 255, 255),
        Color::rgba8(48, 209, 88, 255),
        Color::rgba8(255, 159, 10, 255),
        Color::rgba8(191, 90, 242, 255),
    ];
    for i in 0..CANVAS_ITEMS {
        let origin = Point::new(
            (i % CANVAS_COLUMNS) as f64 * CANVAS_SPACING,
            (i / CANVAS_COLUMNS) as f64 * CANVAS_SPACING,
        );
        let rect = Rect::new(origin, Size::new(28.0, 28.0));
        let shape = if i % 3 == 0 {
            ItemShape::Ellipse(rect)
        } else {
            ItemShape::Rect(rect)
        };
        let _ = scene.add(
            SceneItem::new(shape, ItemStyle::filled(palette[i % palette.len()]))
                .named(format!("Node {i}"), SemanticRole::Button),
        );
    }
    scene
}

/// A hosted canvas whose zoom is a value: a `Number` value zooms about
/// the viewport center, and a zoom by wheel or key reports
/// `Changed(Number(zoom))`. Its semantic value names the focused item,
/// since the hosted canvas publishes no node per item.
struct ZoomCanvas {
    canvas: Rc<RefCell<HostedCanvas>>,
}

impl CustomControl for ZoomCanvas {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        self.canvas.borrow_mut().measure(cx)
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.canvas.borrow_mut().arrange(cx, bounds);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let mut canvas = self.canvas.borrow_mut();
        let before = canvas.view().zoom();
        let reply = canvas.event(cx, event);
        let after = canvas.view().zoom();
        if after != before {
            cx.emit(ControlMessage::Changed(ControlValue::Number(after)));
        }
        reply
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        self.canvas.borrow().paint(cx);
    }

    fn semantics(&self) -> SemanticNode {
        let mut canvas = self.canvas.borrow_mut();
        let mut node = canvas.semantics();
        let focus = canvas.view().focus();
        node.value = focus.and_then(|id| canvas.scene_mut().item(id).map(|item| item.name.clone()));
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        self.canvas.borrow_mut().perform(cx, action, value)
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.canvas.borrow_mut().set_enabled(enabled);
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        let mut canvas = self.canvas.borrow_mut();
        match value {
            ControlValue::Number(zoom) => {
                let center = canvas.view().viewport().center();
                canvas.view_mut().zoom_at(center, *zoom);
                true
            }
            value => canvas.set_value(value),
        }
    }
}

/// The canvas with zoom buttons and a readout. The zoom lives in the page
/// store, so it survives restarts.
struct CanvasPage {
    zoom: Property<f64>,
}

impl View for CanvasPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let canvas = Rc::new(RefCell::new(
            CanvasView::new("Canvas", Rect::default()).hosted(scene()),
        ));
        let selected = cx.state(0_usize);
        let zoom = self.zoom.clone();
        let step = |label: &'static str, factor: f64| {
            let zoom = zoom.clone();
            Button::new(label, move || {
                zoom.set((zoom.get() * factor).clamp(MIN_ZOOM, MAX_ZOOM));
            })
            .variant(ButtonVariant::Outline)
        };
        let (fitted, fit_zoom) = (canvas.clone(), zoom.clone());
        let fit = Button::new("Fit", move || {
            let mut canvas = fitted.borrow_mut();
            canvas.view_mut().fit(canvas_extent(), 8.0);
            fit_zoom.set(canvas.view().zoom());
        })
        .variant(ButtonVariant::Outline);
        let (shown, valued, reported) = (zoom.clone(), zoom.clone(), zoom.clone());
        let view = ControlView::new(ZoomCanvas { canvas })
            .kind(ControlKind::Canvas)
            .value(move || ControlValue::Number(valued.get()))
            .on_message(move |message| match message {
                ControlMessage::Changed(ControlValue::Number(zoom)) => reported.set(*zoom),
                ControlMessage::Changed(ControlValue::Indices(ids)) => selected.set(ids.len()),
                _ => {}
            });
        VStack::new((
            HStack::new((
                step("Zoom in", 1.25),
                step("Zoom out", 1.0 / 1.25),
                fit,
                Text::new(move || {
                    format!(
                        "Zoom {}% · {} selected",
                        (shown.get() * 100.0).round(),
                        selected.get()
                    )
                })
                .text_style(TextPreset::Small)
                .foreground(ColorRole::MutedForeground),
            ))
            .alignment(VerticalAlignment::Center)
            .spacing(Space::S3),
            view.frame(fill()),
        ))
        .alignment(HorizontalAlignment::Fill)
        .spacing(Space::S3)
        .frame(fill())
    }
}

pub fn canvas(cx: &PageCx) -> AnyView {
    CanvasPage {
        zoom: cx.store.canvas_zoom.clone(),
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::{NoDesktop, PageState};
    use crate::ui::page::PageStore;
    use crate::ui::pages::tests::Harness;
    use choscorui::compose::Runtime;
    use choscorui::compose::view::{Env, ViewHost};
    use choscorui::controls::{EstimatedText, ShortcutModifiers};
    use choscorui::core::{Catalog, DesignTokens, Locale, PointerButton, PointerEvent, WheelUnit};

    /// The most items that may have semantic nodes at once.
    const REALIZED_LIMIT: usize = 400;

    fn wheel(page: &mut Harness, at: Point, delta: Point, command: bool) {
        let modifiers = ShortcutModifiers {
            command,
            ..ShortcutModifiers::default()
        };
        page.host.pointer(
            PointerEvent::Wheel {
                position: at,
                delta,
                unit: WheelUnit::Points,
            },
            modifiers,
        );
        page.host.layout();
    }

    fn drag(page: &mut Harness, from: Point, to: Point) {
        let modifiers = ShortcutModifiers::default();
        let button = PointerButton::Primary;
        page.host.pointer(
            PointerEvent::Down {
                position: from,
                button,
            },
            modifiers,
        );
        page.host
            .pointer(PointerEvent::Move { position: to }, modifiers);
        page.host.pointer(
            PointerEvent::Up {
                position: to,
                button,
            },
            modifiers,
        );
        page.host.layout();
    }

    #[test]
    fn canvas_hit_tests_at_2x_zoom_and_rubber_band_selects() {
        let mut page = Harness::new("canvas");
        let node = page.node("Canvas");
        let bounds = page.host.tree().bounds(node).unwrap();
        let painted = page.host.paint().len();
        assert!(painted > 0 && painted < CANVAS_ITEMS, "{painted} commands");
        // Command and the wheel zoom about the pointer: e^(-0.01 dy).
        wheel(
            &mut page,
            bounds.origin,
            Point::new(0.0, -100.0 * std::f64::consts::LN_2),
            true,
        );
        assert!(page.host.find("Zoom 200% · 0 selected").is_some());
        // Pan so item 0 sits 100 points inside the canvas.
        let inside = Point::new(bounds.origin.x + 10.0, bounds.origin.y + 10.0);
        wheel(&mut page, inside, Point::new(-100.0, -100.0), false);
        let center = |index: usize| {
            let scene = |offset: usize| (offset as f64 * CANVAS_SPACING + 14.0 + 50.0) * 2.0;
            Point::new(
                bounds.origin.x + scene(index % CANVAS_COLUMNS),
                bounds.origin.y + scene(index / CANVAS_COLUMNS),
            )
        };
        let hit = center(201);
        drag(&mut page, hit, hit);
        assert_eq!(page.value("Canvas").as_deref(), Some("Node 201"));
        assert!(page.host.find("Zoom 200% · 1 selected").is_some());
        // Items are 28 units square on a 40-unit grid; start and end the
        // band 18 units outside them so the press lands on empty space.
        // The band spans items 0, 1, 200 and 201 and stops short of
        // column 2, which starts at 80 units.
        let margin = 18.0 * 2.0;
        let (from, to) = (center(0), center(201));
        drag(
            &mut page,
            Point::new(from.x - margin, from.y - margin),
            Point::new(to.x + margin, to.y + margin),
        );
        assert!(page.host.find("Zoom 200% · 4 selected").is_some());
        let realized = page.host.semantics().0.len();
        assert!(
            realized > 0 && realized <= REALIZED_LIMIT,
            "{realized} nodes"
        );
    }

    #[test]
    fn canvas_restores_the_stored_zoom_and_writes_changes_back() {
        let runtime = Runtime::new();
        let state = PageState {
            canvas_zoom: 2.0,
            dock: String::new(),
        };
        let cx = PageCx {
            desktop: Rc::new(NoDesktop),
            store: PageStore::new(&runtime, &state),
        };
        let env = Env::new(Rc::new(Catalog::new()), Locale::parse("en").unwrap());
        let mut host = ViewHost::new(
            Size::new(900.0, 700.0),
            DesignTokens::light(),
            env,
            runtime,
            Rc::new(EstimatedText),
        );
        host.mount(canvas(&cx));
        host.layout();
        let node = host.find("Canvas").unwrap();
        let description = host.semantic(node).unwrap().description;
        assert_eq!(description, "Zoom 200%");
        let zoom_in = host.find("Zoom in").unwrap();
        assert!(host.perform(zoom_in, SemanticAction::Activate, None));
        assert_eq!(cx.store.canvas_zoom.get(), 2.5);
        let description = host.semantic(node).unwrap().description;
        assert_eq!(description, "Zoom 250%");
    }

    #[test]
    fn canvas_zoom_buttons_and_fit_write_the_zoom() {
        let mut page = Harness::new("canvas");
        assert!(page.act("Zoom in", SemanticAction::Activate));
        assert!(page.host.find("Zoom 125% · 0 selected").is_some());
        let node = page.node("Canvas");
        let description = page.host.semantic(node).unwrap().description;
        assert_eq!(description, "Zoom 125%");
        assert!(page.act("Zoom out", SemanticAction::Activate));
        assert!(page.host.find("Zoom 100% · 0 selected").is_some());
        assert!(page.act("Fit", SemanticAction::Activate));
        let description = page.host.semantic(node).unwrap().description;
        assert!(description != "Zoom 100%", "{description}");
        assert!(
            page.host
                .find(&format!("{description} · 0 selected"))
                .is_some()
        );
    }
}
