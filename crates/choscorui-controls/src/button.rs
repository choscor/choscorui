//! The button: momentary activation by pointer, Enter, Space, or an
//! assistive action, painted by the button recipe.

use crate::style::{
    ButtonVariant, Content, ControlKind, ControlSize, PaintContext, Slot, StyleQuery, Styles,
    TextAlign, TextMeasure, VisualState,
};
use crate::{CommandError, CommandId, CommandRegistry, CommandSyncError, SemanticAction};
use choscorui_core::{
    DesignTokens, Dirty, Icon, Key, KeyEvent, NodeId, PointerButton, PointerEvent, Size, Tree,
    TreeError,
};
use choscorui_core::{
    SemanticAction as CoreSemanticAction, SemanticNode, SemanticRole, SemanticState,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ButtonInput {
    Pointer(PointerEvent),
    Key(KeyEvent),
    Semantic(SemanticAction),
    Cancel,
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

/// Momentary button behavior. The caller owns command execution; the
/// button recipe owns the look.
pub struct Button {
    label: String,
    command: CommandId,
    enabled: bool,
    hovered: bool,
    pointer_armed: bool,
    pointer_inside: bool,
    space_armed: bool,
    variant: ButtonVariant,
    size: ControlSize,
    leading: Option<Icon>,
    trailing: Option<Icon>,
    expanded: bool,
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
            variant: ButtonVariant::Default,
            size: ControlSize::Default,
            leading: None,
            trailing: None,
            expanded: false,
        }
    }

    #[must_use]
    pub fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    #[must_use]
    pub fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// An icon before the label, or the only content of an icon size.
    #[must_use]
    pub fn with_icon(mut self, icon: Icon) -> Self {
        self.leading = Some(icon);
        self
    }

    /// An icon after the label.
    #[must_use]
    pub fn with_trailing_icon(mut self, icon: Icon) -> Self {
        self.trailing = Some(icon);
        self
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn command(&self) -> CommandId {
        self.command
    }

    pub fn variant(&self) -> ButtonVariant {
        self.variant
    }

    pub fn size(&self) -> ControlSize {
        self.size
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Marks that the button's popup is open; the button keeps its open
    /// look and does not move down while pressed.
    pub fn set_expanded(&mut self, expanded: bool) {
        self.expanded = expanded;
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
    ) -> Result<VisualState, CommandError> {
        let state = self.visual_state(tree, id);
        let enabled = commands.state(self.command)?.enabled;
        Ok(if enabled {
            state
        } else {
            state
                .with(VisualState::DISABLED)
                .without(VisualState::PRESSED)
        })
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
                tree.focus_by_pointer(id)?;
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

    /// True while a hosted button shows the pressed look: a press whose
    /// pointer is still inside, or Space held down.
    pub(crate) fn is_pressed(&self) -> bool {
        self.enabled && ((self.pointer_armed && self.pointer_inside) || self.space_armed)
    }

    /// The flags the button recipe reads.
    pub fn visual_state(&self, tree: &Tree, id: NodeId) -> VisualState {
        let pressed = self.enabled
            && ((self.pointer_armed && self.pointer_inside && tree.pointer_capture() == Some(id))
                || (self.space_armed && tree.focus() == Some(id)));
        VisualState::NONE
            .set(VisualState::DISABLED, !self.enabled)
            .set(VisualState::HOVERED, self.hovered)
            .set(VisualState::PRESSED, pressed)
            .set(VisualState::FOCUSED, tree.focus() == Some(id))
            .set(VisualState::FOCUS_VISIBLE, tree.focus_visible(id))
            .set(VisualState::OPEN, self.expanded)
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

    /// The style query of the button in `state`.
    pub fn query(&self, state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Button, Slot::Root)
            .variant(self.variant)
            .size(self.size)
            .state(state)
    }

    fn content(&self) -> Content<'_> {
        Content {
            leading: self.leading,
            label: if self.size.is_icon() { "" } else { &self.label },
            trailing: self.trailing,
        }
    }

    /// The natural size of the button: its label, icons, and padding, at
    /// least the recipe's height.
    pub fn measure(&self, styles: &Styles, tokens: &DesignTokens, text: &dyn TextMeasure) -> Size {
        let style = styles.resolve(tokens, self.query(VisualState::NONE));
        self.content().size(&style, text)
    }

    /// Paints the button in the context's bounds.
    pub fn paint(&self, cx: &mut PaintContext<'_>, tree: &Tree, id: NodeId) {
        let state = self.visual_state(tree, id);
        self.paint_state(cx, state);
    }

    /// Paints the button in an explicit visual state, for previews and
    /// tests.
    pub fn paint_state(&self, cx: &mut PaintContext<'_>, state: VisualState) {
        let style = cx.style(self.query(state));
        let bounds = cx.bounds;
        let content = self.content();
        cx.slot(bounds, &style, |cx, rect| {
            content.draw(cx, rect, &style, TextAlign::Center);
        });
    }
}

impl crate::CustomControl for Button {
    fn measure(&mut self, cx: &crate::MeasureContext<'_>) -> choscorui_core::Measured {
        let size = Button::measure(self, cx.styles, cx.tokens, cx.text);
        choscorui_core::Measured::new(cx.constraints.constrain(size))
    }

    fn event(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        event: &crate::ControlEvent,
    ) -> choscorui_core::EventReply {
        let Some(routed) = crate::hosted::route(cx, event) else {
            return choscorui_core::EventReply::Continue;
        };
        let input = match routed {
            crate::hosted::Routed::Pointer(event) => ButtonInput::Pointer(event),
            crate::hosted::Routed::Key(event) => ButtonInput::Key(event),
            crate::hosted::Routed::Cancel => ButtonInput::Cancel,
        };
        let node = cx.node();
        if let Ok(Some(_)) = self.handle(cx.tree_mut(), node, input) {
            cx.emit(crate::ControlMessage::Activated);
        }
        cx.request_paint();
        crate::hosted::routed_reply(routed)
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx
            .state
            .set(VisualState::PRESSED, self.is_pressed())
            .set(VisualState::OPEN, self.expanded);
        self.paint_state(cx, state);
    }

    fn semantics(&self) -> SemanticNode {
        ButtonSemantics {
            role: "button",
            name: self.label.clone(),
            enabled: self.enabled,
            focused: false,
            actions: &[],
        }
        .shared()
    }

    fn perform(
        &mut self,
        cx: &mut crate::ControlContext<'_>,
        action: CoreSemanticAction,
        _value: Option<&str>,
    ) -> bool {
        if action != CoreSemanticAction::Activate || !self.enabled {
            return false;
        }
        cx.emit(crate::ControlMessage::Activated);
        true
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.pointer_armed = false;
            self.pointer_inside = false;
            self.space_armed = false;
            self.hovered = false;
        }
    }
}
