//! Experimental safe Rust facade for choscorui applications.
//!
//! The API is intentionally small while the macOS backend proves the shared
//! contracts. Native handles and backend FFI do not appear in this crate.

pub use choscorui_compose::{
    Binding, ComposeError, DeclarativeState, Element, Key as ElementKey, Property, ReactiveError,
    ReconcileReport, Reconciler, Runtime, StateAssignment, StateError, StateMachine, StateName,
    TransitionDescription,
};
pub use choscorui_controls::{
    Button, ButtonDecoration, ButtonInput, ButtonStyle, Checkbox, CheckboxInput, CheckboxStyle,
    CommandError, CommandId, CommandRegistry, CommandState, GroupBox, IntegerRange, Label,
    LabelStyle, ProgressBar, ProgressStyle, RadioGroup, RadioInput, RadioOption, RadioStyle,
    SemanticAction as ControlAction, Separator, SeparatorOrientation, SeparatorStyle, Shortcut,
    ShortcutModifiers, ShortcutScope, Slider, SliderInput, SliderOrientation, SliderStyle, SpinBox,
    SpinBoxInput, SpinBoxStyle, SplitOrientation, Splitter, SplitterInput, SplitterStyle,
    StackPage, StackedContainer, Tab, TabInput, TabSet, TabStyle, TextField, TextFieldChange,
    TextFieldInput, TextFieldStyle,
};
pub use choscorui_core::{
    AccessibilityPreferences, Axis, Color, CrossAlign, DesignTokens, Dirty, DisplayCommand,
    DisplayList, DisplayListError, EventPhase, EventReply, FormLayout, GridLayout, Insets, Key,
    KeyEvent, LayoutDirection, LayoutError, LinearLayout, NodeId, Point, PointerButton,
    PointerEvent, Rect, ScrollViewport, SemanticAction as AccessibilityAction, SemanticChange,
    SemanticEntry, SemanticNode, SemanticRole, SemanticState, SemanticText, SemanticTree, Size,
    TextBuffer, TextError, TextMotion, ThemeError, ThemeInvalidation, ThemeOverride, ThemeResolver,
    Tree, TreeError, WheelUnit,
};

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

#[cfg(target_os = "macos")]
pub use choscorui_macos::{
    AlertButton, AlertResponse, AlertSpec, AlertStyle, Application, Clipboard, DialogError,
    Error as PlatformError, FileDialogMode, FileDialogResponse, FileDialogSpec, ImeEvent, MainMenu,
    MenuBarModel, MenuCommandId, MenuEntry, MenuError, MenuItemState, MenuShortcut,
    MenuShortcutModifiers, MenuSpec, Modifiers, PostError, Poster, TextInputState, Timer, Window,
    WindowEvent, WindowOptions,
};
