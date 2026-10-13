//! Safe Rust facade for choscorui applications.
//!
//! Depend on this crate alone. Native handles and backend FFI never appear
//! in its API, and it forbids `unsafe` code itself.
//!
//! - [`prelude`] holds the types most applications start with. Code that
//!   uses only the prelude compiles on every target.
//! - [`core`], [`controls`], and [`compose`] re-export the platform-neutral
//!   crates in full: geometry, the retained tree, layout, text, models,
//!   themes, accessibility semantics, controls, and declarative state.
//! - [`platform`] re-exports the backend for the current OS, as an escape
//!   hatch. It is empty on a target without a backend. Check
//!   [`capabilities`] before relying on optional features.
//!
//! The flat re-exports below predate the modules and remain for
//! compatibility; see docs/versioning.md for the stability policy.

#![forbid(unsafe_code)]

/// Platform-neutral foundation: geometry, tree, layout, text, models,
/// themes, semantics, settings, scene, and rich text.
pub mod core {
    pub use choscorui_core::*;
}

/// Controls, views, dialogs, and main-window composition.
pub mod controls {
    pub use choscorui_controls::*;
}

/// Declarative composition, reactive bindings, and state machines.
pub mod compose {
    pub use choscorui_compose::*;
}

mod app;
mod backend;
mod context;
pub use app::{App, AppError, Scene, WindowId};
pub use backend::platform;
pub use context::AppContext;

pub use choscorui_core::{BackendCapabilities, Support};

/// The examples in `docs/guide.md`, compiled and run as doctests so that
/// the guide stays correct.
#[cfg(doctest)]
#[doc = include_str!("../../../docs/guide.md")]
pub struct GuideExamples;

/// What the current backend provides. Without a backend (an unsupported
/// OS) every capability is `Unsupported`.
pub fn capabilities() -> BackendCapabilities {
    backend::capabilities()
}

/// The types most applications start with: the views, their modifiers
/// and state, the [`App`] entry point, and the desktop services. Every
/// item exists on every target.
pub mod prelude {
    pub use crate::capabilities;
    pub use crate::{App, AppContext, AppError, Scene, WindowId};
    pub use choscorui_compose::Runtime;
    pub use choscorui_compose::t;
    pub use choscorui_compose::view::*;
    pub use choscorui_controls::{
        ButtonVariant, ControlKind, ControlSize, FieldOrientation, IntegerRange, SheetSide,
        ShortcutModifiers, SplitOrientation, TabsVariant, Toast, ToastPosition, ToastVariant,
        Toasts,
    };
    pub use choscorui_core::{
        BaseColor, Catalog, Color, ColorRole, ColorScheme, DesignTokens, FileDialogKind,
        FileDialogRequest, FontWeight, Icon, LayoutDirection, Locale, NotificationPermission,
        Point, Rect, ServiceError, Services, Size, Space, StatusItemSpec, SystemNotification,
        TextPreset, WindowOptions,
    };
}

pub use choscorui_compose::{
    ComposeError, DeclarativeState, Element, Key as ElementKey, Property, PropertyBinding,
    ReactiveError, ReconcileReport, Reconciler, Runtime, StateAssignment, StateError, StateMachine,
    StateName, TransitionDescription,
};
pub use choscorui_controls::{
    Button, ButtonInput, ButtonVariant, Checkbox, CheckboxInput, CommandError, CommandId,
    CommandRegistry, CommandState, ControlContext, ControlEvent, ControlHost, ControlKind,
    ControlSize, CustomControl, GroupBox, IntegerRange, Label, PaintContext, Progress, RadioGroup,
    RadioInput, RadioOption, Recipe, Resizable, ResizableInput, ResolvedStyle,
    SemanticAction as ControlAction, Separator, SeparatorOrientation, Shortcut, ShortcutModifiers,
    ShortcutScope, Slider, SliderInput, SliderOrientation, SpinBox, SpinBoxInput, SplitOrientation,
    StackPage, StackedContainer, StyleQuery, Styles, Tab, TabInput, Tabs, TextField,
    TextFieldChange, TextFieldInput, VisualState,
};
pub use choscorui_core::{
    AccessibilityPreferences, Axis, Color, Constraints, CrossAlign, DesignTokens, Dirty,
    DisplayCommand, DisplayList, DisplayListError, EventPhase, EventReply, FormLayout, GridLayout,
    Insets, Key, KeyEvent, LayoutDirection, LayoutError, LinearLayout, Measured, NodeId, Point,
    PointerButton, PointerEvent, Rect, ScrollViewport, SemanticAction as AccessibilityAction,
    SemanticChange, SemanticEntry, SemanticNode, SemanticRole, SemanticState, SemanticText,
    SemanticTree, Size, TextBuffer, TextError, TextMotion, ThemeError, ThemeInvalidation,
    ThemeOverride, ThemeResolver, Tree, TreeError, WheelUnit,
};

/// The connection that [`Runtime::bind`] returns.
#[deprecated(since = "0.1.0", note = "renamed to `PropertyBinding`")]
pub type Binding = PropertyBinding;

/// The old name of [`Progress`].
#[deprecated(since = "0.1.0", note = "renamed to `Progress`")]
pub type ProgressBar = Progress;

/// The old name of [`Tabs`].
#[deprecated(since = "0.1.0", note = "renamed to `Tabs`")]
pub type TabSet = Tabs;

/// The old name of [`Resizable`].
#[deprecated(since = "0.1.0", note = "renamed to `Resizable`")]
pub type Splitter = Resizable;

/// The old name of [`ResizableInput`].
#[deprecated(since = "0.1.0", note = "renamed to `ResizableInput`")]
pub type SplitterInput = ResizableInput;

/// Stable retained identity of a UI node.
pub type Node = NodeId;

/// Resolved appearance values used by controls and styles.
pub type Theme = DesignTokens;

/// Platform-independent, owned input accepted by application dispatch code.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Pointer(PointerEvent),
    Key(KeyEvent),
    Text(String),
}

pub use choscorui_core::{
    MenuBarModel, MenuCommandId, MenuEntry, MenuError, MenuItemState, MenuShortcut,
    MenuShortcutModifiers, MenuSpec, WindowOptions,
};
