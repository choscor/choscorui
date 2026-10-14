//! A reference to the control that a [`Configured`] view mounted, and the
//! shared control behind it.
//!
//! [`Configured`]: super::Configured

use choscorui_controls::{
    ControlContext, ControlEvent, ControlValue, CustomControl, MeasureContext, PaintContext,
    Shortcut,
};
use choscorui_core::{
    EventReply, Localizer, Measured, Point, Rect, SemanticAction, SemanticNode, SemanticParts,
    TextInputInfo,
};
use std::cell::RefCell;
use std::rc::Rc;

/// The control that a view mounted, which the app can read after the
/// mount, for example the scroll offset of a list. Give it to the view
/// with [`Configured::control_ref`]. It is empty until the view mounts,
/// and it follows each new build of the control.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_core::{ItemModel, StringListModel};
/// struct Fruits;
///
/// impl View for Fruits {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let selection = cx.state(Vec::new());
///         let model = || Box::new(StringListModel::from_strings(["Apple"])) as Box<dyn ItemModel>;
///         let list = ControlRef::new();
///         let offset = list.clone();
///         VStack::new((
///             List::new("Fruits", model, selection.binding()).control_ref(&list),
///             Button::new("Offset", move || {
///                 println!("{:?}", offset.with(|view| view.offset()));
///             }),
///         ))
///     }
/// }
/// ```
///
/// [`Configured::control_ref`]: super::Configured::control_ref
pub struct ControlRef<C>(Rc<RefCell<Option<Rc<RefCell<C>>>>>);

impl<C> Clone for ControlRef<C> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<C> Default for ControlRef<C> {
    fn default() -> Self {
        Self(Rc::default())
    }
}

impl<C> ControlRef<C> {
    /// An empty reference.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads the mounted control with `read`. Returns `None` before the
    /// view mounts, and while the control handles an event.
    pub fn with<R>(&self, read: impl FnOnce(&C) -> R) -> Option<R> {
        let control = self.0.borrow().clone()?;
        let control = control.try_borrow().ok()?;
        Some(read(&control))
    }

    /// The mounted control.
    pub(super) fn get(&self) -> Option<Rc<RefCell<C>>> {
        self.0.borrow().clone()
    }

    /// Points the reference at a new build of the control.
    pub(super) fn set(&self, control: Rc<RefCell<C>>) {
        *self.0.borrow_mut() = Some(control);
    }
}

/// A control that the host and a [`ControlRef`] share. It keeps a copy
/// of the window shortcuts of the control, which it cannot lend through
/// the cell.
pub(super) struct Shared<C> {
    control: Rc<RefCell<C>>,
    shortcuts: Vec<Shortcut>,
}

impl<C: CustomControl> Shared<C> {
    pub(super) fn new(control: Rc<RefCell<C>>) -> Self {
        let shortcuts = control.borrow().window_shortcuts().to_vec();
        Self { control, shortcuts }
    }

    /// Runs `change` on the control, then copies its window shortcuts.
    fn change<R>(&mut self, change: impl FnOnce(&mut C) -> R) -> R {
        let mut control = self.control.borrow_mut();
        let result = change(&mut control);
        if control.window_shortcuts() != self.shortcuts.as_slice() {
            self.shortcuts = control.window_shortcuts().to_vec();
        }
        result
    }
}

impl<C: CustomControl> CustomControl for Shared<C> {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        self.change(|control| control.measure(cx))
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.change(|control| control.arrange(cx, bounds));
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        self.change(|control| control.event(cx, event))
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        self.control.borrow().paint(cx);
    }

    fn overlay(&self) -> Option<Rect> {
        self.control.borrow().overlay()
    }

    fn overlay_takes_input(&self) -> bool {
        self.control.borrow().overlay_takes_input()
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        self.control.borrow().paint_overlay(cx);
    }

    fn modal(&self) -> bool {
        self.control.borrow().modal()
    }

    fn semantics(&self) -> SemanticNode {
        self.control.borrow().semantics()
    }

    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        self.control.borrow().semantics_in(strings)
    }

    fn semantic_parts(&self, strings: &Localizer, parts: &mut SemanticParts) {
        self.control.borrow().semantic_parts(strings, parts);
    }

    fn text_input(&self) -> Option<TextInputInfo> {
        self.control.borrow().text_input()
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        self.change(|control| control.perform(cx, action, value))
    }

    fn hit(&self, point: Point) -> bool {
        self.control.borrow().hit(point)
    }

    fn focusable(&self) -> bool {
        self.control.borrow().focusable()
    }

    fn window_shortcuts(&self) -> &[Shortcut] {
        &self.shortcuts
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.change(|control| control.set_enabled(enabled));
    }

    fn animating(&self, time: f64) -> bool {
        self.control.borrow().animating(time)
    }

    fn deadline(&self) -> Option<f64> {
        self.control.borrow().deadline()
    }

    fn captures_keys(&self) -> bool {
        self.control.borrow().captures_keys()
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        self.change(|control| control.set_value(value))
    }

    fn set_open(&mut self, open: bool, time: f64) -> bool {
        self.change(|control| control.set_open(open, time))
    }
}
