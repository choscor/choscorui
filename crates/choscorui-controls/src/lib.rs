//! Reusable control behavior and simple visual recipes.

use choscorui_core::{
    Color, Dirty, DisplayList, Key, KeyEvent, NodeId, PointerButton, PointerEvent, Rect, Tree,
    TreeError,
};
use choscorui_core::{
    SemanticAction as CoreSemanticAction, SemanticNode, SemanticRole, SemanticState,
};
use std::collections::HashMap;

mod checkbox;
pub use checkbox::{
    Checkbox, CheckboxInput, CheckboxSemantics, CheckboxStyle, CheckboxVisualState,
};
mod text_field;
pub use text_field::{
    TextComposition, TextField, TextFieldChange, TextFieldInput, TextFieldSemanticAction,
    TextFieldSemantics, TextFieldStyle, TextFieldVisualState, ValidationKind, ValidationState,
};
mod range_controls;
pub use range_controls::{
    INDETERMINATE_PERIOD, IntegerRange, ProgressBar, ProgressStyle, ProgressVisualState,
    RangeError, Slider, SliderInput, SliderOrientation, SliderSemanticAction, SliderStyle,
    SliderVisualState, SpinBox, SpinBoxInput, SpinBoxSemanticAction, SpinBoxStyle,
    SpinBoxVisualState,
};
mod selection_controls;
pub use selection_controls::{
    RadioGroup, RadioInput, RadioOption, RadioStyle, RadioVisualState, SelectionError, Tab,
    TabInput, TabSet, TabStyle, TabVisualState,
};
mod containers;
pub use containers::{
    ContainerError, GroupBox, SplitOrientation, Splitter, SplitterInput, SplitterStyle,
    SplitterVisualState, StackPage, StackedContainer,
};
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
    HeaderColumn, HeaderInput, HeaderResponse, HeaderStyle, HeaderView, UndoHistoryRow,
    undo_history, undo_steps_to,
};
mod popup;
pub use popup::{
    MenuItem, MenuResponse, MenuStyle, Placement, PopupId, PopupMenu, PopupPlacement, PopupPress,
    PopupStack, TOOLTIP_DELAY, TOOLTIP_GRACE, TooltipController, VisibleTooltip, place_popup,
};
mod combo;
pub use combo::{
    ComboBox, ComboChange, ComboInput, ComboItem, CompletionMode, FontChooser, text_key,
};
mod widgets;
pub use widgets::{
    ButtonGroup, CommandLinkRecipe, DecimalSpinBox, Dial, FocusFrame, KeySequenceChange,
    KeySequenceEdit, NumberBase, NumericDisplay, ScrollBar, ScrollBarInput, ScrollOrientation,
    SizeGrip, ToolBox, ToolButton, ToolButtonAction, ToolButtonMenu, shortcut_text,
};
mod datetime_controls;
pub use datetime_controls::{Calendar, CalendarCell, DateTimeEdit, DateTimeField, DateTimeKind};
mod dialogs;
pub use dialogs::{
    ColorPicker, DialogButton, DialogButtonRole, DialogId, DialogStack, FontDialog, InputDialog,
    InputKind, InputValue, MessageDialog, Modality, dialog_key_button, order_dialog_buttons,
};
mod canvas;
pub use canvas::{CanvasInput, CanvasResponse, CanvasView, MAX_ZOOM, MIN_ZOOM};
mod custom;
pub use custom::{ControlContext, ControlEvent, ControlHost, CustomControl};
mod dock;
pub use dock::{
    DockArea, DockArrangement, DockError, DockLayout, DockPanel, DockRestoreReport, DockTarget,
    GroupFrame, PanelId, PanelLocation,
};
mod shell;
pub use shell::{
    MainWindowFrames, StatusBar, StatusBarLayout, StatusSection, Toolbar, ToolbarArea, ToolbarItem,
    ToolbarItemKind, ToolbarLayout, compose_main_window,
};
mod recipe;
pub use recipe::{
    ButtonRecipe, ContentAlignment, PillButtonRecipe, Slot, SlotContext, SlotLayout, paint_button,
};
mod navigation;
pub use navigation::{
    DialogKeys, MnemonicHit, MnemonicTable, ScrollAction, ScrollArea, ScrollAreaInput,
    ScrollAreaStyle, ScrollAreaVisualState, ScrollbarGeometry, parse_mnemonic,
};
mod static_controls;
pub use static_controls::{Label, LabelStyle, Separator, SeparatorOrientation, SeparatorStyle};

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ButtonInput {
    Pointer(PointerEvent),
    Key(KeyEvent),
    Semantic(SemanticAction),
    Cancel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ButtonVisualState {
    pub enabled: bool,
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ButtonSemantics {
    pub role: &'static str,
    pub name: String,
    pub enabled: bool,
    pub focused: bool,
    pub actions: &'static [SemanticAction],
}

impl ButtonSemantics {
    pub fn shared(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Button,
            name: self.name.clone(),
            description: String::new(),
            value: None,
            state: SemanticState {
                disabled: !self.enabled,
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions: self
                .enabled
                .then_some([CoreSemanticAction::Activate, CoreSemanticAction::Focus])
                .into_iter()
                .flatten()
                .collect(),
            text: None,
        }
    }
}

/// Momentary button behavior. The caller owns command execution and painting.
pub struct Button {
    label: String,
    command: CommandId,
    enabled: bool,
    hovered: bool,
    pointer_armed: bool,
    pointer_inside: bool,
    space_armed: bool,
}

impl Button {
    pub fn new(label: impl Into<String>, command: CommandId) -> Self {
        Self {
            label: label.into(),
            command,
            enabled: true,
            hovered: false,
            pointer_armed: false,
            pointer_inside: false,
            space_armed: false,
        }
    }

    /// Projects the application's command state into this control after a
    /// command change. Call before dispatching input or exporting semantics.
    pub fn sync_command(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        commands: &CommandRegistry,
    ) -> Result<(), CommandSyncError> {
        let state = commands
            .state(self.command)
            .map_err(CommandSyncError::Command)?;
        self.set_enabled(tree, id, state.enabled)
            .map_err(CommandSyncError::Tree)
    }

    /// Dispatches using the current application command state. A disabled
    /// command cancels any outstanding press before processing this input.
    pub fn handle_with_commands(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        input: ButtonInput,
        commands: &CommandRegistry,
    ) -> Result<Option<CommandId>, CommandSyncError> {
        self.sync_command(tree, id, commands)?;
        self.handle(tree, id, input).map_err(CommandSyncError::Tree)
    }

    pub fn visual_state_with_commands(
        &self,
        tree: &Tree,
        id: NodeId,
        commands: &CommandRegistry,
    ) -> Result<ButtonVisualState, CommandError> {
        let mut state = self.visual_state(tree, id);
        state.enabled = commands.state(self.command)?.enabled;
        state.pressed &= state.enabled;
        Ok(state)
    }

    pub fn semantics_with_commands(
        &self,
        tree: &Tree,
        id: NodeId,
        commands: &CommandRegistry,
    ) -> Result<ButtonSemantics, CommandError> {
        let mut semantics = self.semantics(tree, id);
        semantics.enabled = commands.state(self.command)?.enabled;
        semantics.actions = if semantics.enabled {
            &[SemanticAction::Activate]
        } else {
            &[]
        };
        Ok(semantics)
    }

    pub fn set_enabled(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        enabled: bool,
    ) -> Result<(), TreeError> {
        if !tree.contains(id) {
            return Err(TreeError::MissingNode);
        }
        let changed = self.enabled != enabled;
        self.enabled = enabled;
        if !enabled {
            self.pointer_armed = false;
            self.pointer_inside = false;
            self.space_armed = false;
            self.hovered = false;
            if tree.pointer_capture() == Some(id) {
                tree.set_pointer_capture(None)?;
            }
        }
        if changed {
            tree.invalidate(id, Dirty::PAINT)?;
            tree.invalidate(id, Dirty::SEMANTICS)?;
        }
        Ok(())
    }

    pub fn handle(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        input: ButtonInput,
    ) -> Result<Option<CommandId>, TreeError> {
        let bounds = tree.bounds(id).ok_or(TreeError::MissingNode)?;
        if !self.enabled {
            return Ok(None);
        }
        let previous_visual = self.visual_state(tree, id);
        let mut activated = false;
        match input {
            ButtonInput::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if tree.hit_test(position) == Some(id) => {
                tree.set_focus(Some(id))?;
                tree.set_pointer_capture(Some(id))?;
                self.pointer_armed = true;
                self.pointer_inside = true;
                self.hovered = true;
            }
            ButtonInput::Pointer(PointerEvent::Move { position }) => {
                self.hovered = bounds.contains(position)
                    && (tree.pointer_capture() == Some(id) || tree.hit_test(position) == Some(id));
                if self.pointer_armed {
                    self.pointer_inside = bounds.contains(position);
                }
            }
            ButtonInput::Pointer(PointerEvent::Up {
                position,
                button: PointerButton::Primary,
            }) if self.pointer_armed => {
                activated = tree.pointer_capture() == Some(id) && bounds.contains(position);
                if tree.pointer_capture() == Some(id) {
                    tree.set_pointer_capture(None)?;
                }
                self.pointer_armed = false;
                self.pointer_inside = false;
                self.hovered = bounds.contains(position);
            }
            ButtonInput::Key(KeyEvent::Down {
                key: Key::Enter,
                repeat: false,
            }) if tree.focus() == Some(id) => activated = true,
            ButtonInput::Key(KeyEvent::Down {
                key: Key::Space,
                repeat: false,
            }) if tree.focus() == Some(id) => self.space_armed = true,
            ButtonInput::Key(KeyEvent::Up { key: Key::Space }) => {
                activated = self.space_armed && tree.focus() == Some(id);
                self.space_armed = false;
            }
            ButtonInput::Semantic(SemanticAction::Activate) => activated = true,
            ButtonInput::Cancel => {
                if tree.pointer_capture() == Some(id) {
                    tree.set_pointer_capture(None)?;
                }
                self.pointer_armed = false;
                self.pointer_inside = false;
                self.space_armed = false;
            }
            _ => {}
        }
        if self.visual_state(tree, id) != previous_visual {
            tree.invalidate(id, Dirty::PAINT)?;
        }
        Ok(activated.then_some(self.command))
    }

    pub fn visual_state(&self, tree: &Tree, id: NodeId) -> ButtonVisualState {
        ButtonVisualState {
            enabled: self.enabled,
            hovered: self.hovered,
            pressed: self.enabled
                && ((self.pointer_armed
                    && self.pointer_inside
                    && tree.pointer_capture() == Some(id))
                    || (self.space_armed && tree.focus() == Some(id))),
            focused: tree.focus() == Some(id),
        }
    }

    pub fn semantics(&self, tree: &Tree, id: NodeId) -> ButtonSemantics {
        const ACTIVATE: &[SemanticAction] = &[SemanticAction::Activate];
        ButtonSemantics {
            role: "button",
            name: self.label.clone(),
            enabled: self.enabled,
            focused: tree.focus() == Some(id),
            actions: if self.enabled { ACTIVATE } else { &[] },
        }
    }
}

/// A swappable paint recipe; it does not own input, focus, or actions.
#[derive(Clone, Copy, Debug)]
pub enum ButtonDecoration {
    Plain,
    LeadingRail { width: f64, color: Color },
}

#[derive(Clone, Copy, Debug)]
pub struct ButtonStyle {
    pub normal: Color,
    pub hover: Color,
    pub pressed: Color,
    pub disabled: Color,
    pub focus: Color,
    pub decoration: ButtonDecoration,
}

impl ButtonStyle {
    pub fn paint(&self, list: &mut DisplayList, bounds: Rect, state: ButtonVisualState) {
        let color = if !state.enabled {
            self.disabled
        } else if state.pressed {
            self.pressed
        } else if state.hovered {
            self.hover
        } else {
            self.normal
        };
        list.fill_rect(bounds, color);
        if let ButtonDecoration::LeadingRail { width, color } = self.decoration
            && width.is_finite()
            && width > 0.0
        {
            list.fill_rect(
                Rect::new(
                    bounds.origin,
                    choscorui_core::Size::new(width.min(bounds.size.width), bounds.size.height),
                ),
                color,
            );
        }
        if state.focused {
            let t = 2.0;
            list.fill_rect(
                Rect::new(
                    bounds.origin,
                    choscorui_core::Size::new(bounds.size.width, t),
                ),
                self.focus,
            );
            list.fill_rect(
                Rect::new(
                    choscorui_core::Point::new(
                        bounds.origin.x,
                        bounds.origin.y + bounds.size.height - t,
                    ),
                    choscorui_core::Size::new(bounds.size.width, t),
                ),
                self.focus,
            );
        }
    }
}
