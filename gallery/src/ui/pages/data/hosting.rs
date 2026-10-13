//! Glue between the data pages and the hosted controls they mount: a
//! model that a page shares with the views that show it, and a control
//! that a page can still reach after mounting it.

use choscorui::controls::{
    ControlContext, ControlEvent, ControlValue, CustomControl, MeasureContext, PaintContext,
};
use choscorui::core::{
    EventPhase, EventReply, ItemKey, ItemModel, Key, KeyEvent, Localizer, Measured, ModelChange,
    Rect, Role, SemanticAction, SemanticNode, SortFilterProxy, Value,
};
use std::cell::RefCell;
use std::rc::Rc;

/// A model that a page edits and the hosted views read, through an
/// optional sort and filter proxy. Keep the proxy in sync with the model.
pub struct SharedModel<M> {
    pub model: Rc<RefCell<M>>,
    pub proxy: Option<Rc<RefCell<SortFilterProxy>>>,
}

impl<M> Clone for SharedModel<M> {
    fn clone(&self) -> Self {
        Self {
            model: self.model.clone(),
            proxy: self.proxy.clone(),
        }
    }
}

impl<M: ItemModel> SharedModel<M> {
    /// A model read directly.
    pub fn new(model: M) -> Self {
        Self {
            model: Rc::new(RefCell::new(model)),
            proxy: None,
        }
    }

    /// A model read through a proxy that keeps every row in source order.
    pub fn proxied(model: M) -> Self {
        let mut proxy = SortFilterProxy::new();
        proxy.sync(&model);
        Self {
            model: Rc::new(RefCell::new(model)),
            proxy: Some(Rc::new(RefCell::new(proxy))),
        }
    }

    /// Changes the proxy with `change`, then syncs it with the model.
    pub fn reproxy(&self, change: impl FnOnce(&mut SortFilterProxy)) {
        if let Some(proxy) = &self.proxy {
            let mut proxy = proxy.borrow_mut();
            change(&mut proxy);
            proxy.sync(&*self.model.borrow());
        }
    }

    fn read<R>(&self, read: impl FnOnce(&dyn ItemModel) -> R) -> R {
        let model = self.model.borrow();
        match &self.proxy {
            Some(proxy) => read(&proxy.borrow().view(&*model)),
            None => read(&*model),
        }
    }

    /// The positions of `keys` among the rows, ascending.
    pub fn positions(&self, keys: &[ItemKey]) -> Vec<usize> {
        let mut rows: Vec<usize> =
            self.read(|model| keys.iter().filter_map(|key| model.row(*key)).collect());
        rows.sort_unstable();
        rows
    }

    /// The keys of the rows at `rows`.
    pub fn keys(&self, rows: &[usize]) -> Vec<ItemKey> {
        self.read(|model| {
            rows.iter()
                .filter_map(|row| model.key(None, *row))
                .collect()
        })
    }

    /// Every column of the row `key`, for assistive technology.
    pub fn describe(&self, key: ItemKey) -> String {
        self.read(|model| {
            (0..model.column_count())
                .map(|column| {
                    model
                        .data(key, column, Role::Accessibility)
                        .to_display_string()
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
    }
}

impl<M: ItemModel> ItemModel for SharedModel<M> {
    fn row_count(&self, parent: Option<ItemKey>) -> usize {
        self.read(|model| model.row_count(parent))
    }
    fn column_count(&self) -> usize {
        self.read(|model| model.column_count())
    }
    fn key(&self, parent: Option<ItemKey>, row: usize) -> Option<ItemKey> {
        self.read(|model| model.key(parent, row))
    }
    fn parent(&self, key: ItemKey) -> Option<ItemKey> {
        self.read(|model| model.parent(key))
    }
    fn row(&self, key: ItemKey) -> Option<usize> {
        self.read(|model| model.row(key))
    }
    fn data(&self, key: ItemKey, column: usize, role: Role) -> Value {
        self.read(|model| model.data(key, column, role))
    }
    fn header(&self, column: usize, role: Role) -> Value {
        self.read(|model| model.header(column, role))
    }
    fn has_children(&self, key: ItemKey) -> bool {
        self.read(|model| model.has_children(key))
    }
    fn editable(&self, key: ItemKey, column: usize) -> bool {
        self.read(|model| model.editable(key, column))
    }
    fn revision(&self) -> u64 {
        self.read(|model| model.revision())
    }
    fn changes_since(&self, revision: u64) -> Vec<ModelChange> {
        self.read(|model| model.changes_since(revision))
    }
    fn can_fetch_more(&self, parent: Option<ItemKey>) -> bool {
        self.read(|model| model.can_fetch_more(parent))
    }
}

type KeyAction = Rc<dyn Fn(Key)>;

/// A mounted control that the page keeps a handle to, so message
/// handlers can read its state. With keys, the handle also takes those
/// key presses while the control has focus and gives them to an action
/// instead of the control.
pub struct Handle<C> {
    control: Rc<RefCell<C>>,
    keys: &'static [Key],
    action: Option<KeyAction>,
}

impl<C: CustomControl> Handle<C> {
    /// Mounts `control`.
    pub fn new(control: C) -> Self {
        Self::shared(Rc::new(RefCell::new(control)))
    }

    /// Mounts the control that `control` holds.
    pub fn shared(control: Rc<RefCell<C>>) -> Self {
        Self {
            control,
            keys: &[],
            action: None,
        }
    }

    /// Mounts `control`, with `keys` going to `action`.
    pub fn keyed(control: C, keys: &'static [Key], action: KeyAction) -> Self {
        Self {
            control: Rc::new(RefCell::new(control)),
            keys,
            action: Some(action),
        }
    }
}

impl<C: CustomControl> CustomControl for Handle<C> {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        self.control.borrow_mut().measure(cx)
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.control.borrow_mut().arrange(cx, bounds);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if let (Some(action), ControlEvent::Key(KeyEvent::Down { key, .. })) = (&self.action, event)
            && cx.phase() == EventPhase::Target
            && self.keys.contains(key)
        {
            action(*key);
            return EventReply::Stop;
        }
        self.control.borrow_mut().event(cx, event)
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        self.control.borrow().paint(cx);
    }

    fn overlay(&self) -> Option<Rect> {
        self.control.borrow().overlay()
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

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        self.control.borrow_mut().perform(cx, action, value)
    }

    fn focusable(&self) -> bool {
        self.control.borrow().focusable()
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.control.borrow_mut().set_enabled(enabled);
    }

    fn animating(&self, time: f64) -> bool {
        self.control.borrow().animating(time)
    }

    fn deadline(&self) -> Option<f64> {
        self.control.borrow().deadline()
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        self.control.borrow_mut().set_value(value)
    }

    fn set_open(&mut self, open: bool, time: f64) -> bool {
        self.control.borrow_mut().set_open(open, time)
    }
}
