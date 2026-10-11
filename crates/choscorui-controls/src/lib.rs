//! Reusable control behavior and simple visual recipes.

use choscorui_core::{Key, NodeId, TreeError};
use std::collections::HashMap;

mod checkbox;
pub use checkbox::{Checkbox, CheckboxInput, CheckboxSemantics};
mod text_field;
pub use text_field::{
    TextComposition, TextField, TextFieldChange, TextFieldInput, TextFieldSemanticAction,
    TextFieldSemantics, ValidationKind, ValidationState,
};
mod input_text;
pub use input_text::InputText;
mod range_controls;
#[expect(
    deprecated,
    reason = "re-exports the deprecated `ProgressBar` alias for one release"
)]
pub use range_controls::ProgressBar;
pub use range_controls::{
    INDETERMINATE_PERIOD, IntegerRange, Progress, RangeError, STEPPER_WIDTH, Slider, SliderInput,
    SliderOrientation, SliderSemanticAction, SpinBox, SpinBoxInput, SpinBoxSemanticAction,
};
mod selection_controls;
#[expect(
    deprecated,
    reason = "re-exports the deprecated `TabSet` alias for one release"
)]
pub use selection_controls::TabSet;
pub use selection_controls::{
    RadioGroup, RadioInput, RadioOption, SelectionError, Tab, TabInput, Tabs,
};
mod containers;
pub use containers::{
    ContainerError, GroupBox, Resizable, ResizableInput, SplitOrientation, StackPage,
    StackedContainer,
};
#[expect(
    deprecated,
    reason = "re-exports the deprecated `Splitter` and `SplitterInput` aliases for one release"
)]
pub use containers::{Splitter, SplitterInput};
mod input_rules;
pub use input_rules::{DateInput, InputMask, MaskError, MaskedText, NumberInput};
mod text_editor;
pub use text_editor::{TextEditor, TextEditorChange, TextEditorInput};
mod item_view;
pub use item_view::{
    ItemDelegate, ItemModifiers, ItemState, ItemView, ItemViewInput, ItemViewResponse,
    RealizedItem, TYPE_AHEAD_TIMEOUT, TextItemDelegate, ViewLayout,
};
mod model_widgets;
pub use model_widgets::{
    CellEdit, ColumnBrowser, DataMapper, DefaultEditorFactory, EditorFactory, EditorKind,
    HeaderColumn, HeaderInput, HeaderResponse, HeaderView, UndoHistoryRow, undo_history,
    undo_steps_to,
};
mod popup;
pub use popup::{
    Placement, PopupId, PopupPlacement, PopupPress, PopupStack, TOOLTIP_DELAY, TOOLTIP_GRACE,
    TooltipController, VisibleTooltip, place_popup,
};
mod menu;
#[expect(
    deprecated,
    reason = "re-exports the deprecated `PopupMenu` alias for one release"
)]
pub use menu::PopupMenu;
pub use menu::{DropdownMenu, MenuCheck, MenuItem, MenuResponse};
mod combo;
#[expect(
    deprecated,
    reason = "re-exports the deprecated `ComboBox` alias for one release"
)]
pub use combo::ComboBox;
pub use combo::{
    ComboChange, ComboInput, ComboItem, Combobox, CompletionMode, FontChooser, text_key,
};
mod widgets;
pub use widgets::{
    ButtonGroup, CommandLink, DecimalSpinBox, Dial, FocusFrame, KeySequenceChange, KeySequenceEdit,
    NumberBase, NumericDisplay, ScrollBar, ScrollBarInput, ScrollOrientation, SizeGrip, ToolBox,
    ToolButton, ToolButtonAction, ToolButtonMenu, shortcut_text,
};
mod datetime_controls;
pub use datetime_controls::{
    Calendar, CalendarCell, CalendarFocus, CalendarLayout, DateTimeEdit, DateTimeField,
    DateTimeKind,
};
mod dialogs;
#[expect(
    deprecated,
    reason = "re-exports the deprecated `MessageDialog` alias for one release"
)]
pub use dialogs::MessageDialog;
pub use dialogs::{
    AlertDialog, ColorPicker, DialogButton, DialogButtonRole, DialogId, DialogStack, FontDialog,
    InputDialog, InputKind, InputValue, Modality, dialog_key_button, order_dialog_buttons,
};
mod canvas;
pub use canvas::{CanvasInput, CanvasResponse, CanvasView, MAX_ZOOM, MIN_ZOOM};
mod custom;
pub use custom::{
    ControlContext, ControlEvent, ControlHost, ControlMessage, ControlValue, CustomControl,
    HostEnv, MeasureContext,
};
mod dock;
pub use dock::{
    DockArea, DockArrangement, DockError, DockFocus, DockLayout, DockPanel, DockRestoreReport,
    DockTarget, GroupFrame, PanelId, PanelLocation,
};
mod shell;
pub use shell::{
    MainWindowFrames, StatusBar, StatusBarLayout, StatusSection, Toolbar, ToolbarArea, ToolbarItem,
    ToolbarItemKind, ToolbarLayout, compose_main_window,
};
mod button;
pub use button::{Button, ButtonInput, ButtonSemantics};
mod style;
pub use style::{
    AddonAlign, AlertVariant, AttachmentState, BadgeVariant, BubbleVariant, ButtonVariant,
    CalendarDay, Content, ControlKind, ControlSize, Corners, Cursor, DefaultRecipe, Edges,
    EmptyMediaVariant, EstimatedText, FieldOrientation, ItemVariant, KbdVariant, MarkerVariant,
    MenuItemVariant, Orientation, PaintContext, Recipe, ResolvedStyle, Ring, ShapedText, SheetSide,
    SidebarVariant, Sides, Slot, StyleCacheStats, StyleQuery, Styles, TabsVariant, TextAlign,
    TextLine, TextMeasure, ToastVariant, ToggleVariant, Variant, VisualState, align_x,
    content_rect, draw_icon, draw_text, fill_shape, paint_box, paint_slot,
};
mod navigation;
pub use navigation::{
    DialogKeys, MnemonicHit, MnemonicTable, ScrollAction, ScrollArea, ScrollAreaInput,
    ScrollbarGeometry, parse_mnemonic,
};
mod static_controls;
pub use static_controls::{Label, Separator, SeparatorOrientation};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CommandId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandState {
    pub enabled: bool,
    pub checked: bool,
}

impl Default for CommandState {
    fn default() -> Self {
        Self {
            enabled: true,
            checked: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandError {
    DuplicateId,
    DuplicateShortcut,
    UnknownId,
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateId => f.write_str("a command with this id is already registered"),
            Self::DuplicateShortcut => {
                f.write_str("another command already uses this shortcut in the same scope")
            }
            Self::UnknownId => f.write_str("no command is registered with this id"),
        }
    }
}

impl std::error::Error for CommandError {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ShortcutModifiers {
    pub shift: bool,
    pub control: bool,
    pub option: bool,
    pub command: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Shortcut {
    pub key: Key,
    pub modifiers: ShortcutModifiers,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShortcutScope {
    Application,
    Window(u64),
    Focused { window: u64, node: NodeId },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ShortcutBinding {
    scope: ShortcutScope,
    shortcut: Shortcut,
    command: CommandId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandSyncError {
    Command(CommandError),
    Tree(TreeError),
}

impl std::fmt::Display for CommandSyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Command(error) => write!(f, "command error: {error}"),
            Self::Tree(error) => write!(f, "tree update failed: {error}"),
        }
    }
}

impl std::error::Error for CommandSyncError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Command(error) => Some(error),
            Self::Tree(error) => Some(error),
        }
    }
}

/// Application-owned command state shared by controls and future menu/toolbar views.
#[derive(Default)]
pub struct CommandRegistry {
    states: HashMap<CommandId, CommandState>,
    shortcuts: Vec<ShortcutBinding>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&mut self, id: CommandId, state: CommandState) -> Result<(), CommandError> {
        if self.states.contains_key(&id) {
            return Err(CommandError::DuplicateId);
        }
        self.states.insert(id, state);
        Ok(())
    }
    pub fn state(&self, id: CommandId) -> Result<CommandState, CommandError> {
        self.states.get(&id).copied().ok_or(CommandError::UnknownId)
    }
    pub fn set_enabled(&mut self, id: CommandId, enabled: bool) -> Result<(), CommandError> {
        self.states
            .get_mut(&id)
            .ok_or(CommandError::UnknownId)?
            .enabled = enabled;
        Ok(())
    }
    pub fn set_checked(&mut self, id: CommandId, checked: bool) -> Result<(), CommandError> {
        self.states
            .get_mut(&id)
            .ok_or(CommandError::UnknownId)?
            .checked = checked;
        Ok(())
    }
    pub fn activate(&self, id: CommandId) -> Result<Option<CommandId>, CommandError> {
        Ok(self.state(id)?.enabled.then_some(id))
    }

    /// Rejects a duplicate chord in the same scope. Different scopes may
    /// bind the same chord; focused, window, then application wins.
    pub fn bind_shortcut(
        &mut self,
        scope: ShortcutScope,
        shortcut: Shortcut,
        command: CommandId,
    ) -> Result<(), CommandError> {
        self.state(command)?;
        if self
            .shortcuts
            .iter()
            .any(|binding| binding.scope == scope && binding.shortcut == shortcut)
        {
            return Err(CommandError::DuplicateShortcut);
        }
        self.shortcuts.push(ShortcutBinding {
            scope,
            shortcut,
            command,
        });
        Ok(())
    }

    /// Removes bindings owned by a window or control when it is retired.
    pub fn unbind_scope(&mut self, scope: ShortcutScope) -> usize {
        let before = self.shortcuts.len();
        self.shortcuts.retain(|binding| binding.scope != scope);
        before - self.shortcuts.len()
    }

    pub fn unbind_window(&mut self, window: u64) -> usize {
        let before = self.shortcuts.len();
        self.shortcuts.retain(|binding| match binding.scope {
            ShortcutScope::Application => true,
            ShortcutScope::Window(id) | ShortcutScope::Focused { window: id, .. } => id != window,
        });
        before - self.shortcuts.len()
    }

    /// A disabled winner consumes the chord so a less-specific action cannot
    /// run unexpectedly. Unknown chords return None.
    pub fn resolve_shortcut(
        &self,
        shortcut: Shortcut,
        window: u64,
        focus: Option<NodeId>,
    ) -> Option<CommandId> {
        let focused_scope = focus.map(|node| ShortcutScope::Focused { window, node });
        for scope in [
            focused_scope,
            Some(ShortcutScope::Window(window)),
            Some(ShortcutScope::Application),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(binding) = self
                .shortcuts
                .iter()
                .find(|binding| binding.scope == scope && binding.shortcut == shortcut)
            {
                return self
                    .activate(binding.command)
                    .expect("bound command must exist");
            }
        }
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticAction {
    Activate,
    SetValue(bool),
}
