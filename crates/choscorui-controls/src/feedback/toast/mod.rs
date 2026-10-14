//! Toasts: short messages in a stack at a window corner, and the queue
//! that the app pushes them through.

mod toaster;

use crate::style::ToastVariant;
use std::cell::RefCell;
use std::rc::Rc;

pub use toaster::Toaster;

/// How long a toast shows before it closes, in seconds.
pub const TOAST_DURATION: f64 = 4.0;

/// Identifies a toast in its queue.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ToastId(pub u64);

/// One toast: a title, an optional description, a variant, and an
/// optional action button.
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub title: String,
    pub description: String,
    pub variant: ToastVariant,
    pub action: Option<String>,
    /// Seconds before the toast closes; `None` keeps it until it is
    /// dismissed.
    pub duration: Option<f64>,
}

impl Toast {
    /// A toast that closes after [`TOAST_DURATION`].
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: String::new(),
            variant: ToastVariant::Default,
            action: None,
            duration: Some(TOAST_DURATION),
        }
    }

    /// Adds a second line of text below the title.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Sets the variant. A `Loading` toast stays until it is updated or
    /// dismissed. Another variant after `Loading` closes after
    /// [`TOAST_DURATION`] again.
    #[must_use]
    pub fn with_variant(mut self, variant: ToastVariant) -> Self {
        if variant == ToastVariant::Loading {
            self.duration = None;
        } else if self.variant == ToastVariant::Loading && self.duration.is_none() {
            self.duration = Some(TOAST_DURATION);
        }
        self.variant = variant;
        self
    }

    /// Adds an action button with `label`.
    #[must_use]
    pub fn with_action(mut self, label: impl Into<String>) -> Self {
        self.action = Some(label.into());
        self
    }

    /// Closes the toast after `seconds`, or keeps it with `None`.
    #[must_use]
    pub fn with_duration(mut self, seconds: Option<f64>) -> Self {
        self.duration = seconds;
        self
    }
}

/// A queued change to the toasts.
#[derive(Clone, Debug, PartialEq)]
enum ToastOp {
    Push(ToastId, Toast),
    Update(ToastId, Toast),
    Dismiss(ToastId),
}

#[derive(Debug, Default)]
struct Queue {
    next: u64,
    ops: Vec<ToastOp>,
}

/// The handle that the app pushes toasts through. Clones share one
/// queue. A [`Toaster`] made with the handle shows the toasts at the next
/// [`ControlHost::advance`](crate::ControlHost::advance).
#[derive(Clone, Debug, Default)]
pub struct Toasts(Rc<RefCell<Queue>>);

impl Toasts {
    /// An empty toast queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues `toast` and returns its id.
    pub fn push(&self, toast: Toast) -> ToastId {
        let mut queue = self.0.borrow_mut();
        queue.next += 1;
        let id = ToastId(queue.next);
        queue.ops.push(ToastOp::Push(id, toast));
        id
    }

    /// Replaces the toast `id`, such as a loading toast that finished. Its
    /// timer starts again.
    pub fn update(&self, id: ToastId, toast: Toast) {
        self.0.borrow_mut().ops.push(ToastOp::Update(id, toast));
    }

    /// Closes the toast `id`.
    pub fn dismiss(&self, id: ToastId) {
        self.0.borrow_mut().ops.push(ToastOp::Dismiss(id));
    }

    fn pending(&self) -> bool {
        !self.0.borrow().ops.is_empty()
    }

    fn take(&self) -> Vec<ToastOp> {
        std::mem::take(&mut self.0.borrow_mut().ops)
    }
}

/// Where the toast stack sits in the window.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ToastPosition {
    TopStart,
    TopCenter,
    TopEnd,
    BottomStart,
    BottomCenter,
    #[default]
    BottomEnd,
}

impl ToastPosition {
    fn top(self) -> bool {
        matches!(self, Self::TopStart | Self::TopCenter | Self::TopEnd)
    }
}
