use crate::SemanticAction;
use crate::style::{
    ControlKind, PaintContext, Slot, StyleQuery, Styles, TextMeasure, VisualState, choice_size,
    paint_choice,
};
use choscorui_core::{
    DesignTokens, Dirty, Icon, Key, KeyEvent, NodeId, PointerButton, PointerEvent,
    SemanticAction as CoreSemanticAction, SemanticNode, SemanticRole, SemanticState, Size, Tree,
    TreeError,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CheckboxInput {
    Pointer(PointerEvent),
    Key(KeyEvent),
    Semantic(SemanticAction),
    Cancel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckboxSemantics {
    pub role: &'static str,
    pub name: String,
    pub checked: bool,
    /// Shown as mixed; `checked` still holds the value a toggle flips.
    pub mixed: bool,
    pub invalid: bool,
    pub enabled: bool,
    pub focused: bool,
    pub actions: &'static [SemanticAction],
}

impl CheckboxSemantics {
    pub fn shared(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Checkbox,
            name: self.name.clone(),
            description: String::new(),
            value: Some(if self.mixed {
                "mixed".to_owned()
            } else {
                self.checked.to_string()
            }),
            state: SemanticState {
                disabled: !self.enabled,
                checked: (!self.mixed).then_some(self.checked),
                invalid: self.invalid,
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions: if self.enabled {
                vec![
                    CoreSemanticAction::Activate,
                    CoreSemanticAction::SetValue,
                    CoreSemanticAction::Focus,
                ]
            } else {
                Vec::new()
            },
            text: None,
        }
    }
}

/// Independent Boolean value control. Returns Some(value) only on a change.
pub struct Checkbox {
    label: String,
    checked: bool,
    indeterminate: bool,
    invalid: bool,
    enabled: bool,
    hovered: bool,
    pointer_armed: bool,
    pointer_inside: bool,
    space_armed: bool,
}

impl Checkbox {
    pub fn new(label: impl Into<String>, checked: bool) -> Self {
        Self {
            label: label.into(),
            checked,
            indeterminate: false,
            invalid: false,
            enabled: true,
            hovered: false,
            pointer_armed: false,
            pointer_inside: false,
            space_armed: false,
        }
    }

    /// The checkbox with the invalid state from the start, for a host
    /// that has no tree to give [`Checkbox::set_invalid`].
    #[must_use]
    pub fn with_invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    pub fn value(&self) -> bool {
        self.checked
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn is_indeterminate(&self) -> bool {
        self.indeterminate
    }

    /// Shows the mixed state until the next change of value.
    pub fn set_indeterminate(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        indeterminate: bool,
    ) -> Result<(), TreeError> {
        if self.indeterminate != indeterminate {
            self.indeterminate = indeterminate;
            tree.invalidate(id, Dirty::PAINT)?;
            tree.invalidate(id, Dirty::SEMANTICS)?;
        }
        Ok(())
    }

    pub fn is_invalid(&self) -> bool {
        self.invalid
    }

    pub fn set_invalid(
        &mut self,
        tree: &mut Tree,
        id: NodeId,
        invalid: bool,
    ) -> Result<(), TreeError> {
        if self.invalid != invalid {
            self.invalid = invalid;
            tree.invalidate(id, Dirty::PAINT)?;
            tree.invalidate(id, Dirty::SEMANTICS)?;
        }
        Ok(())
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
        input: CheckboxInput,
    ) -> Result<Option<bool>, TreeError> {
        let bounds = tree.bounds(id).ok_or(TreeError::MissingNode)?;
        if !self.enabled {
            return Ok(None);
        }
        let previous_visual = self.visual_state(tree, id);
        let mut next = None;
        match input {
            CheckboxInput::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) if tree.hit_test(position) == Some(id) => {
                tree.focus_by_pointer(id)?;
                tree.set_pointer_capture(Some(id))?;
                self.pointer_armed = true;
                self.pointer_inside = true;
                self.hovered = true;
            }
            CheckboxInput::Pointer(PointerEvent::Move { position }) => {
                self.hovered = bounds.contains(position)
                    && (tree.pointer_capture() == Some(id) || tree.hit_test(position) == Some(id));
                if self.pointer_armed {
                    self.pointer_inside = bounds.contains(position);
                }
            }
            CheckboxInput::Pointer(PointerEvent::Up {
                position,
                button: PointerButton::Primary,
            }) if self.pointer_armed => {
                if tree.pointer_capture() == Some(id) && bounds.contains(position) {
                    next = Some(!self.checked);
                }
                if tree.pointer_capture() == Some(id) {
                    tree.set_pointer_capture(None)?;
                }
                self.pointer_armed = false;
                self.pointer_inside = false;
                self.hovered = bounds.contains(position);
            }
            CheckboxInput::Key(KeyEvent::Down {
                key: Key::Space,
                repeat: false,
            }) if tree.focus() == Some(id) => self.space_armed = true,
            CheckboxInput::Key(KeyEvent::Up { key: Key::Space }) => {
                if self.space_armed && tree.focus() == Some(id) {
                    next = Some(!self.checked);
                }
                self.space_armed = false;
            }
            CheckboxInput::Semantic(SemanticAction::Activate) => next = Some(!self.checked),
            CheckboxInput::Semantic(SemanticAction::SetValue(value)) => next = Some(value),
            CheckboxInput::Cancel => {
                if tree.pointer_capture() == Some(id) {
                    tree.set_pointer_capture(None)?;
                }
                self.pointer_armed = false;
                self.pointer_inside = false;
                self.space_armed = false;
            }
            _ => {}
        }
        if let Some(value) = next
            && (value != self.checked || self.indeterminate)
        {
            self.checked = value;
            self.indeterminate = false;
            tree.invalidate(id, Dirty::PAINT)?;
            tree.invalidate(id, Dirty::SEMANTICS)?;
            return Ok(Some(value));
        }
        if self.visual_state(tree, id) != previous_visual {
            tree.invalidate(id, Dirty::PAINT)?;
        }
        Ok(None)
    }

    /// The flags the checkbox recipe reads.
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
            .set(VisualState::INVALID, self.invalid)
            .set(VisualState::CHECKED, self.checked && !self.indeterminate)
            .set(VisualState::INDETERMINATE, self.indeterminate)
    }

    /// The natural size: the box, the gap, and the label.
    pub fn measure(&self, styles: &Styles, tokens: &DesignTokens, text: &dyn TextMeasure) -> Size {
        choice_size(ControlKind::Checkbox, &self.label, styles, tokens, text)
    }

    /// Paints the checkbox in the context's bounds.
    pub fn paint(&self, cx: &mut PaintContext<'_>, tree: &Tree, id: NodeId) {
        let state = self.visual_state(tree, id);
        self.paint_state(cx, state);
    }

    /// Paints the box at the start of the bounds and the label after it,
    /// in an explicit visual state.
    pub fn paint_state(&self, cx: &mut PaintContext<'_>, state: VisualState) {
        let indicator = cx.style(StyleQuery::new(ControlKind::Checkbox, Slot::Root).state(state));
        let mark = if state.indeterminate() {
            Some(Icon::Minus)
        } else if state.checked() {
            Some(Icon::Check)
        } else {
            None
        };
        paint_choice(cx, ControlKind::Checkbox, &self.label, state, |cx, rect| {
            if let Some(mark) = mark {
                cx.icon(rect, mark, &indicator);
            }
        });
    }

    pub fn semantics(&self, tree: &Tree, id: NodeId) -> CheckboxSemantics {
        const ACTIONS: &[SemanticAction] = &[
            SemanticAction::Activate,
            SemanticAction::SetValue(false),
            SemanticAction::SetValue(true),
        ];
        CheckboxSemantics {
            role: "checkbox",
            name: self.label.clone(),
            checked: self.checked,
            mixed: self.indeterminate,
            invalid: self.invalid,
            enabled: self.enabled,
            focused: tree.focus() == Some(id),
            actions: if self.enabled { ACTIONS } else { &[] },
        }
    }
}

impl Checkbox {
    /// The state flags that the checkbox keeps itself, without the host's
    /// hover and focus.
    fn own_state(&self) -> VisualState {
        let pressed =
            self.enabled && ((self.pointer_armed && self.pointer_inside) || self.space_armed);
        VisualState::NONE
            .set(VisualState::PRESSED, pressed)
            .set(VisualState::INVALID, self.invalid)
            .set(VisualState::CHECKED, self.checked && !self.indeterminate)
            .set(VisualState::INDETERMINATE, self.indeterminate)
    }
}

impl crate::CustomControl for Checkbox {
    fn measure(&mut self, cx: &crate::MeasureContext<'_>) -> choscorui_core::Measured {
        let size = Checkbox::measure(self, cx.styles, cx.tokens, cx.text);
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
            crate::hosted::Routed::Pointer(event) => CheckboxInput::Pointer(event),
            crate::hosted::Routed::Key(event) => CheckboxInput::Key(event),
            crate::hosted::Routed::Cancel => CheckboxInput::Cancel,
        };
        let node = cx.node();
        if let Ok(Some(value)) = self.handle(cx.tree_mut(), node, input) {
            cx.emit(crate::ControlMessage::Changed(crate::ControlValue::Bool(
                value,
            )));
        }
        cx.request_paint();
        crate::hosted::routed_reply(routed)
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx.state.with(self.own_state());
        self.paint_state(cx, state);
    }

    fn semantics(&self) -> SemanticNode {
        CheckboxSemantics {
            role: "checkbox",
            name: self.label.clone(),
            checked: self.checked,
            mixed: self.indeterminate,
            invalid: self.invalid,
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
        value: Option<&str>,
    ) -> bool {
        if !self.enabled {
            return false;
        }
        let next = match (action, value) {
            (CoreSemanticAction::Activate, _) => !self.checked,
            (CoreSemanticAction::SetValue, Some("true")) => true,
            (CoreSemanticAction::SetValue, Some("false")) => false,
            _ => return false,
        };
        if next != self.checked || self.indeterminate {
            self.checked = next;
            self.indeterminate = false;
            cx.request_paint();
            cx.request_semantics();
            cx.emit(crate::ControlMessage::Changed(crate::ControlValue::Bool(
                next,
            )));
        }
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

    fn set_value(&mut self, value: &crate::ControlValue) -> bool {
        match value {
            crate::ControlValue::Bool(checked) => {
                self.checked = *checked;
                self.indeterminate = false;
                true
            }
            crate::ControlValue::Mixed(state) => {
                self.checked = state.unwrap_or(false);
                self.indeterminate = state.is_none();
                true
            }
            _ => false,
        }
    }
}
