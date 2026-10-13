//! Interactive view of a `Scene`: zoom and pan, hit testing and selection
//! in transformed coordinates, rubber-band selection, dragging, keyboard
//! nudging and focus, viewport culling, and accessible items.
//!
//! Several views may show one scene; each keeps its own zoom, pan, and
//! selection.

use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::hosted::Routed;
use crate::style::fill_area;
use crate::style::{ControlKind, PaintContext, Slot, StyleQuery};
use choscorui_core::{
    DisplayList, DisplayListError, EventReply, Key, KeyEvent, Localizer, Measured, Paint, Path,
    Point, PointerButton, PointerEvent, Rect, Scene, SceneItemId, SemanticAction,
    SemanticCollection, SemanticNode, SemanticRole, SemanticState, Size, StrokeStyle, Transform,
    WheelUnit,
};
use std::cell::RefCell;
use std::collections::BTreeSet;

/// Points that one wheel line pans in a control host.
const WHEEL_LINE: f64 = 20.0;

pub const MIN_ZOOM: f64 = 0.05;
pub const MAX_ZOOM: f64 = 32.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CanvasInput {
    /// Primary press in view coordinates. `extend` toggles membership
    /// (⌘/⇧-click).
    Press {
        at: Point,
        extend: bool,
    },
    Drag {
        at: Point,
    },
    Release {
        at: Point,
    },
    /// Scroll pans; with `zoom` (pinch or ⌘-scroll) it zooms about `at`.
    Wheel {
        dx: f64,
        dy: f64,
        at: Point,
        zoom: bool,
    },
    /// Magnification gesture factor about `at`.
    Magnify {
        factor: f64,
        at: Point,
    },
    Key {
        key: Key,
        shift: bool,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CanvasResponse {
    pub repaint: bool,
    pub selection_changed: bool,
    /// Items moved by drag or nudge (scene delta).
    pub moved: Option<(Vec<SceneItemId>, f64, f64)>,
    /// Delete/Backspace with a selection; the app removes the items.
    pub delete_requested: Vec<SceneItemId>,
    /// Return on the focused item.
    pub activated: Option<SceneItemId>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Gesture {
    None,
    /// Dragging selected items; last scene point.
    Move {
        last: Point,
        moved: bool,
    },
    /// Rubber band from a view-space anchor.
    Band {
        anchor: Point,
        current: Point,
        extend: bool,
    },
}

#[derive(Clone, Debug)]
pub struct CanvasView {
    pub name: String,
    viewport: Rect,
    zoom: f64,
    /// Scene point shown at the viewport's top-left.
    origin: Point,
    selection: BTreeSet<SceneItemId>,
    band_base: BTreeSet<SceneItemId>,
    focus: Option<SceneItemId>,
    gesture: Gesture,
}

impl CanvasView {
    pub fn new(name: impl Into<String>, viewport: Rect) -> Self {
        Self {
            name: name.into(),
            viewport,
            zoom: 1.0,
            origin: Point::new(0.0, 0.0),
            selection: BTreeSet::new(),
            band_base: BTreeSet::new(),
            focus: None,
            gesture: Gesture::None,
        }
    }

    pub fn viewport(&self) -> Rect {
        self.viewport
    }
    pub fn set_viewport(&mut self, viewport: Rect) {
        self.viewport = viewport;
    }
    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    /// Scene → view transform.
    pub fn transform(&self) -> Transform {
        Transform::translate(-self.origin.x, -self.origin.y)
            .then(Transform::scale(self.zoom, self.zoom))
            .then(Transform::translate(
                self.viewport.origin.x,
                self.viewport.origin.y,
            ))
    }
    pub fn view_to_scene(&self, point: Point) -> Point {
        Point::new(
            (point.x - self.viewport.origin.x) / self.zoom + self.origin.x,
            (point.y - self.viewport.origin.y) / self.zoom + self.origin.y,
        )
    }
    pub fn scene_to_view(&self, point: Point) -> Point {
        self.transform().apply(point)
    }
    /// The scene area currently visible.
    pub fn visible_scene_rect(&self) -> Rect {
        Rect::new(
            self.origin,
            Size::new(
                self.viewport.size.width / self.zoom,
                self.viewport.size.height / self.zoom,
            ),
        )
    }

    /// Zooms about a view point, keeping the scene point under it fixed.
    pub fn zoom_at(&mut self, at: Point, zoom: f64) -> bool {
        if !zoom.is_finite() {
            return false;
        }
        let zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        if zoom == self.zoom {
            return false;
        }
        let anchor = self.view_to_scene(at);
        self.zoom = zoom;
        self.origin = Point::new(
            anchor.x - (at.x - self.viewport.origin.x) / zoom,
            anchor.y - (at.y - self.viewport.origin.y) / zoom,
        );
        true
    }
    /// Pans by a view-space delta.
    pub fn pan(&mut self, dx: f64, dy: f64) -> bool {
        if !(dx.is_finite() && dy.is_finite()) || (dx == 0.0 && dy == 0.0) {
            return false;
        }
        self.origin = Point::new(
            self.origin.x + dx / self.zoom,
            self.origin.y + dy / self.zoom,
        );
        true
    }
    /// Fits a scene rect into the viewport (with a margin), centered.
    pub fn fit(&mut self, rect: Rect, margin: f64) {
        if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
            return;
        }
        let available = Size::new(
            (self.viewport.size.width - 2.0 * margin).max(1.0),
            (self.viewport.size.height - 2.0 * margin).max(1.0),
        );
        self.zoom = (available.width / rect.size.width)
            .min(available.height / rect.size.height)
            .clamp(MIN_ZOOM, MAX_ZOOM);
        let visible = self.visible_scene_rect();
        self.origin = Point::new(
            rect.center().x - visible.size.width / 2.0,
            rect.center().y - visible.size.height / 2.0,
        );
    }

    pub fn selection(&self) -> &BTreeSet<SceneItemId> {
        &self.selection
    }
    pub fn set_selection(&mut self, ids: impl IntoIterator<Item = SceneItemId>) {
        self.selection = ids.into_iter().collect();
    }
    pub fn focus(&self) -> Option<SceneItemId> {
        self.focus
    }

    /// Drops selection and focus for items no longer in the scene.
    pub fn prune(&mut self, scene: &Scene) -> bool {
        let before = self.selection.len();
        self.selection.retain(|id| scene.item(*id).is_some());
        if self.focus.is_some_and(|id| scene.item(id).is_none()) {
            self.focus = None;
        }
        before != self.selection.len()
    }

    /// The rubber band in view coordinates while dragging one.
    pub fn rubber_band(&self) -> Option<Rect> {
        match self.gesture {
            Gesture::Band {
                anchor, current, ..
            } => Some(rect_from(anchor, current)),
            _ => None,
        }
    }

    pub fn handle(&mut self, scene: &mut Scene, input: CanvasInput) -> CanvasResponse {
        let mut response = CanvasResponse::default();
        match input {
            CanvasInput::Press { at, extend } => {
                let point = self.view_to_scene(at);
                let hit = scene.hit_test(point);
                match hit {
                    Some(id) => {
                        let before = self.selection.clone();
                        if extend {
                            if !self.selection.remove(&id) {
                                self.selection.insert(id);
                            }
                        } else if !self.selection.contains(&id) {
                            self.selection = BTreeSet::from([id]);
                        }
                        self.focus = Some(id);
                        response.selection_changed = before != self.selection;
                        self.gesture = if self.selection.contains(&id) {
                            Gesture::Move {
                                last: point,
                                moved: false,
                            }
                        } else {
                            Gesture::None
                        };
                    }
                    None => {
                        if !extend && !self.selection.is_empty() {
                            self.selection.clear();
                            response.selection_changed = true;
                        }
                        self.band_base = self.selection.clone();
                        self.gesture = Gesture::Band {
                            anchor: at,
                            current: at,
                            extend,
                        };
                    }
                }
                response.repaint = true;
            }
            CanvasInput::Drag { at } => match self.gesture {
                Gesture::Move { last, .. } => {
                    let point = self.view_to_scene(at);
                    let (dx, dy) = (point.x - last.x, point.y - last.y);
                    let ids = self.movable(scene);
                    if (dx != 0.0 || dy != 0.0) && !ids.is_empty() {
                        scene.move_by(&ids, dx, dy);
                        response.moved = Some((ids, dx, dy));
                        response.repaint = true;
                    }
                    self.gesture = Gesture::Move {
                        last: point,
                        moved: true,
                    };
                }
                Gesture::Band { anchor, extend, .. } => {
                    self.gesture = Gesture::Band {
                        anchor,
                        current: at,
                        extend,
                    };
                    let band = rect_from(self.view_to_scene(anchor), self.view_to_scene(at));
                    let inside: BTreeSet<_> = scene.items_within(band).into_iter().collect();
                    let next: BTreeSet<_> = if extend {
                        self.band_base
                            .symmetric_difference(&inside)
                            .copied()
                            .collect()
                    } else {
                        inside
                    };
                    response.selection_changed = next != self.selection;
                    self.selection = next;
                    response.repaint = true;
                }
                Gesture::None => {}
            },
            CanvasInput::Release { .. } => {
                response.repaint = matches!(self.gesture, Gesture::Band { .. });
                self.gesture = Gesture::None;
            }
            CanvasInput::Wheel { dx, dy, at, zoom } => {
                response.repaint = if zoom {
                    self.zoom_at(at, self.zoom * (-dy * 0.01).exp())
                } else {
                    self.pan(dx, dy)
                };
            }
            CanvasInput::Magnify { factor, at } => {
                response.repaint = self.zoom_at(at, self.zoom * factor);
            }
            CanvasInput::Key { key, shift } => self.handle_key(scene, key, shift, &mut response),
        }
        response
    }

    fn movable(&self, scene: &Scene) -> Vec<SceneItemId> {
        self.selection
            .iter()
            .copied()
            .filter(|id| scene.item(*id).is_some_and(|item| item.movable))
            .collect()
    }

    fn handle_key(
        &mut self,
        scene: &mut Scene,
        key: Key,
        shift: bool,
        response: &mut CanvasResponse,
    ) {
        let step = if shift { 10.0 } else { 1.0 };
        let nudge = match key {
            Key::ArrowLeft => Some((-step, 0.0)),
            Key::ArrowRight => Some((step, 0.0)),
            Key::ArrowUp => Some((0.0, -step)),
            Key::ArrowDown => Some((0.0, step)),
            _ => None,
        };
        if let Some((dx, dy)) = nudge {
            let ids = self.movable(scene);
            if !ids.is_empty() {
                scene.move_by(&ids, dx, dy);
                response.moved = Some((ids, dx, dy));
                response.repaint = true;
            } else {
                // Nothing selected: arrows scroll the view.
                response.repaint = self.pan(dx * 20.0, dy * 20.0);
            }
            return;
        }
        let center = self.viewport.center();
        match key {
            Key::Tab => {
                // Focus cycles through selectable items in paint order.
                let order: Vec<_> = scene
                    .paint_order()
                    .to_vec()
                    .into_iter()
                    .filter(|id| {
                        scene
                            .item(*id)
                            .is_some_and(|item| item.selectable && item.visible)
                    })
                    .collect();
                if order.is_empty() {
                    return;
                }
                let position = self
                    .focus
                    .and_then(|focus| order.iter().position(|id| *id == focus));
                let next = match (position, shift) {
                    (None, false) => 0,
                    (None, true) => order.len() - 1,
                    (Some(p), false) => (p + 1) % order.len(),
                    (Some(p), true) => (p + order.len() - 1) % order.len(),
                };
                self.focus = Some(order[next]);
                self.selection = BTreeSet::from([order[next]]);
                self.ensure_visible(scene, order[next]);
                response.selection_changed = true;
                response.repaint = true;
            }
            Key::Space => {
                if let Some(focus) = self.focus {
                    if !self.selection.remove(&focus) {
                        self.selection.insert(focus);
                    }
                    response.selection_changed = true;
                    response.repaint = true;
                }
            }
            Key::Enter => response.activated = self.focus,
            Key::Escape => {
                if !self.selection.is_empty() {
                    self.selection.clear();
                    response.selection_changed = true;
                    response.repaint = true;
                }
            }
            Key::Delete | Key::Backspace => {
                response.delete_requested = self.selection.iter().copied().collect();
            }
            Key::Character('+') | Key::Character('=') => {
                response.repaint = self.zoom_at(center, self.zoom * 1.25);
            }
            Key::Character('-') => response.repaint = self.zoom_at(center, self.zoom / 1.25),
            Key::Character('0') => response.repaint = self.zoom_at(center, 1.0),
            _ => {}
        }
    }

    /// Pans the minimum amount so the item's bounds are visible.
    pub fn ensure_visible(&mut self, scene: &Scene, id: SceneItemId) {
        let (Some(bounds), visible) = (scene.world_bounds(id), self.visible_scene_rect()) else {
            return;
        };
        let shift = |min: f64, max: f64, vmin: f64, vmax: f64| {
            if max - min > vmax - vmin || min < vmin {
                min - vmin
            } else if max > vmax {
                max - vmax
            } else {
                0.0
            }
        };
        self.origin = Point::new(
            self.origin.x
                + shift(
                    bounds.min_x(),
                    bounds.max_x(),
                    visible.min_x(),
                    visible.max_x(),
                ),
            self.origin.y
                + shift(
                    bounds.min_y(),
                    bounds.max_y(),
                    visible.min_y(),
                    visible.max_y(),
                ),
        );
    }

    /// Paints visible items (culled through the spatial index), selection
    /// outlines, focus outline or ring, and rubber band. The outlines and the band
    /// come from the `Canvas` recipe. Returns items painted.
    pub fn paint(
        &self,
        cx: &mut PaintContext<'_>,
        scene: &mut Scene,
    ) -> Result<usize, DisplayListError> {
        let query = |slot| StyleQuery::new(ControlKind::Canvas, slot);
        let (selection, band_style) = (
            cx.style(query(Slot::Indicator)),
            cx.style(query(Slot::Range)),
        );
        let focused = cx.state.focused();
        let list = &mut *cx.list;
        list.push_clip(self.viewport);
        list.push_transform(self.transform())?;
        let painted = scene.paint(list, self.visible_scene_rect())?;
        list.pop_transform()?;
        let outline = |list: &mut DisplayList, rect: Rect, dashed: bool| {
            let mut style = StrokeStyle::new(selection.border_width);
            if dashed {
                style.dash = vec![4.0, 3.0];
            }
            list.stroke_path(
                Path::rect(rect.inset(-2.0)),
                Paint::Solid(selection.border),
                style,
            )
        };
        for id in &self.selection {
            if let Some(bounds) = scene.world_bounds(*id) {
                outline(list, self.transform().apply_rect(bounds), false)?;
            }
        }
        // The focused item shows a dashed outline while the canvas has
        // focus. With no focused item, keyboard focus rings the canvas.
        let item = self.focus.and_then(|focus| scene.world_bounds(focus));
        if let Some(bounds) = item.filter(|_| focused) {
            outline(list, self.transform().apply_rect(bounds).inset(-2.0), true)?;
        }
        if let Some(band) = self.rubber_band() {
            fill_area(list, band, false, &band_style.background);
            outline(list, band.inset(2.0), false)?;
        }
        list.pop_clip()?;
        if item.is_none() {
            cx.focus_ring(self.viewport, 0.0);
        }
        Ok(painted)
    }

    /// Accessible container for the canvas, with the zoom in the
    /// language of `strings`.
    pub fn semantics(
        &self,
        scene: &mut Scene,
        strings: &Localizer,
    ) -> (SemanticNode, SemanticCollection) {
        let count = scene.accessible_items().len();
        let percent = (self.zoom * 100.0).round().to_string();
        (
            SemanticNode {
                role: SemanticRole::List,
                name: self.name.clone(),
                description: strings.message("choscorui.zoom", &[("percent", &percent)]),
                value: None,
                state: SemanticState::default(),
                relations: Vec::new(),
                actions: vec![SemanticAction::Focus],
                text: None,
            },
            SemanticCollection {
                item_count: count,
                column_count: None,
            },
        )
    }

    /// Accessible items currently in view: (id, index among all
    /// accessible items, node, view-space bounds). Offscreen items are
    /// virtualized like list rows.
    pub fn item_semantics(
        &self,
        scene: &mut Scene,
    ) -> Vec<(SceneItemId, usize, SemanticNode, Rect)> {
        let visible: BTreeSet<_> = scene
            .items_in_rect(self.visible_scene_rect())
            .into_iter()
            .collect();
        let transform = self.transform();
        scene
            .accessible_items()
            .into_iter()
            .enumerate()
            .filter(|(_, id)| visible.contains(id))
            .filter_map(|(index, id)| {
                let item = scene.item(id)?;
                let bounds = transform.apply_rect(scene.world_bounds(id)?);
                let mut actions = vec![SemanticAction::Focus];
                if item.selectable {
                    actions.push(SemanticAction::Activate);
                }
                Some((
                    id,
                    index,
                    SemanticNode {
                        role: item.role.unwrap_or(SemanticRole::Image),
                        name: item.name.clone(),
                        description: String::new(),
                        value: None,
                        state: SemanticState {
                            selected: Some(self.selection.contains(&id)),
                            ..SemanticState::default()
                        },
                        relations: Vec::new(),
                        actions,
                        text: None,
                    },
                    bounds,
                ))
            })
            .collect()
    }

    /// Applies an assistive action to an item: Focus focuses and reveals
    /// it; Activate selects it.
    pub fn perform(&mut self, scene: &Scene, id: SceneItemId, action: SemanticAction) -> bool {
        if scene.item(id).is_none() {
            return false;
        }
        match action {
            SemanticAction::Focus => {
                self.focus = Some(id);
                self.ensure_visible(scene, id);
                true
            }
            SemanticAction::Activate => {
                self.focus = Some(id);
                self.selection = BTreeSet::from([id]);
                true
            }
            _ => false,
        }
    }
}

fn rect_from(a: Point, b: Point) -> Rect {
    Rect::new(
        Point::new(a.x.min(b.x), a.y.min(b.y)),
        Size::new((a.x - b.x).abs(), (a.y - b.y).abs()),
    )
}

impl CanvasView {
    /// The view for a control host, over its own `scene`. The hosted
    /// control lays the viewport out on its node's bounds.
    pub fn hosted(self, scene: Scene) -> HostedCanvas {
        HostedCanvas {
            view: self,
            scene: RefCell::new(scene),
            pressed: false,
        }
    }
}

/// A [`CanvasView`] mounted in a control host, with the scene it shows.
/// The scene sits in a `RefCell` because painting the scene updates its
/// spatial index.
///
/// - Pointer, wheel, and key input go to [`CanvasView::handle`]. Shift or
///   Command extends a press, and Command turns the wheel into zoom.
/// - A selection change reports `Changed(Indices(ids))` and Enter reports
///   `ItemActivated(id)`, with the `SceneItemId` numbers.
/// - Delete and Backspace remove the selected items from the scene.
pub struct HostedCanvas {
    view: CanvasView,
    scene: RefCell<Scene>,
    /// A primary press is down, so moves drag.
    pressed: bool,
}

impl HostedCanvas {
    pub fn view(&self) -> &CanvasView {
        &self.view
    }
    pub fn view_mut(&mut self) -> &mut CanvasView {
        &mut self.view
    }
    pub fn scene_mut(&mut self) -> &mut Scene {
        self.scene.get_mut()
    }

    fn selection_value(&self) -> ControlValue {
        ControlValue::Indices(
            self.view
                .selection()
                .iter()
                .map(|id| id.0 as usize)
                .collect(),
        )
    }

    /// Applies one input and reports what changed. Returns whether the
    /// input changed anything.
    fn apply(&mut self, cx: &mut ControlContext<'_>, input: CanvasInput) -> bool {
        let scene = self.scene.get_mut();
        let response = self.view.handle(scene, input);
        if !response.delete_requested.is_empty() {
            for id in &response.delete_requested {
                scene.remove(*id);
            }
            self.view.prune(scene);
        }
        let selection_changed = response.selection_changed || !response.delete_requested.is_empty();
        let changed = response.repaint
            || selection_changed
            || response.moved.is_some()
            || response.activated.is_some();
        if changed {
            cx.request_paint();
            cx.request_semantics();
        }
        if selection_changed {
            cx.emit(ControlMessage::Changed(self.selection_value()));
        }
        if let Some(id) = response.activated {
            cx.emit(ControlMessage::ItemActivated(id.0 as usize));
        }
        changed
    }
}

impl CustomControl for HostedCanvas {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        // A canvas takes the offered size, and its viewport's size when
        // unbounded.
        let max = cx.constraints.max;
        let viewport = self.view.viewport().size;
        let size = Size::new(
            if max.width.is_finite() {
                max.width
            } else {
                viewport.width
            },
            if max.height.is_finite() {
                max.height
            } else {
                viewport.height
            },
        );
        Measured::new(cx.constraints.constrain(size))
    }

    fn arrange(&mut self, _cx: &MeasureContext<'_>, bounds: Rect) {
        self.view.set_viewport(bounds);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let modifiers = cx.modifiers();
        let input = match crate::hosted::route(cx, event) {
            Some(Routed::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            })) => {
                self.pressed = true;
                cx.capture_pointer(true);
                CanvasInput::Press {
                    at: position,
                    extend: modifiers.shift || modifiers.command,
                }
            }
            Some(Routed::Pointer(PointerEvent::Move { position })) if self.pressed => {
                CanvasInput::Drag { at: position }
            }
            Some(Routed::Pointer(PointerEvent::Up { position, .. })) if self.pressed => {
                self.pressed = false;
                cx.capture_pointer(false);
                CanvasInput::Release { at: position }
            }
            Some(Routed::Pointer(PointerEvent::Wheel {
                position,
                delta,
                unit,
            })) => {
                let scale = match unit {
                    WheelUnit::Points => 1.0,
                    WheelUnit::Lines => WHEEL_LINE,
                };
                CanvasInput::Wheel {
                    dx: delta.x * scale,
                    dy: delta.y * scale,
                    at: position,
                    zoom: modifiers.command,
                }
            }
            Some(Routed::Key(KeyEvent::Down { key, .. })) => CanvasInput::Key {
                key,
                shift: modifiers.shift,
            },
            Some(Routed::Cancel) if self.pressed => {
                self.pressed = false;
                cx.capture_pointer(false);
                CanvasInput::Release {
                    at: self.view.viewport().origin,
                }
            }
            _ => {
                if let ControlEvent::FocusChanged(_) = event {
                    cx.request_paint();
                }
                return EventReply::Continue;
            }
        };
        let stop = matches!(
            input,
            CanvasInput::Press { .. } | CanvasInput::Drag { .. } | CanvasInput::Release { .. }
        );
        if self.apply(cx, input) || stop {
            EventReply::Stop
        } else {
            EventReply::Continue
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        // Paint has no error channel. The view fails only on an unbalanced
        // clip or transform stack, and it pops everything it pushes.
        let _ = self.view.paint(cx, &mut self.scene.borrow_mut());
    }

    fn semantics(&self) -> SemanticNode {
        self.semantics_in(Localizer::builtin())
    }

    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        self.view.semantics(&mut self.scene.borrow_mut(), strings).0
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        if action != SemanticAction::Focus {
            return false;
        }
        cx.request_focus();
        true
    }

    fn set_enabled(&mut self, enabled: bool) {
        if !enabled && self.pressed {
            self.pressed = false;
            let at = self.view.viewport().origin;
            self.view
                .handle(self.scene.get_mut(), CanvasInput::Release { at });
        }
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Indices(ids) = value else {
            return false;
        };
        let scene = self.scene.get_mut();
        let ids: Vec<SceneItemId> = ids.iter().map(|id| SceneItemId(*id as u64)).collect();
        if ids.iter().any(|id| scene.item(*id).is_none()) {
            return false;
        }
        self.view.set_selection(ids);
        true
    }
}
