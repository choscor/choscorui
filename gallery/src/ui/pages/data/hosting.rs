//! A model that a data page shares with the views that show it.

use choscorui::core::{ItemKey, ItemModel, ModelChange, Role, SortFilterProxy, Value};
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
