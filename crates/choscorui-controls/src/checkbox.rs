use crate::SemanticAction;
use choscorui_core::{
    Color, Dirty, DisplayList, Key, KeyEvent, NodeId, Point, PointerButton, PointerEvent, Rect,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckboxVisualState {
    pub enabled: bool,
    pub checked: bool,
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckboxSemantics {
    pub role: &'static str,
    pub name: String,
    pub checked: bool,
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
            value: Some(self.checked.to_string()),
            state: SemanticState {
                disabled: !self.enabled,
                checked: Some(self.checked),
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

/// Swappable drawing recipe; checkbox behavior owns no colors or geometry.
#[derive(Clone, Copy, Debug)]
pub struct CheckboxStyle {
    pub box_color: Color,
    pub hover_color: Color,
    pub pressed_color: Color,
    pub check_color: Color,
    pub disabled_color: Color,
    pub focus_color: Color,
}

impl CheckboxStyle {
    pub fn paint(&self, list: &mut DisplayList, bounds: Rect, state: CheckboxVisualState) {
        let color = if !state.enabled {
            self.disabled_color
        } else if state.pressed {
            self.pressed_color
        } else if state.hovered {
            self.hover_color
        } else {
            self.box_color
        };
        list.fill_rect(bounds, color);
        if state.checked {
            list.fill_rect(
                Rect::new(
                    Point::new(bounds.origin.x + 8.0, bounds.origin.y + 8.0),
                    Size::new(
                        (bounds.size.width - 16.0).max(0.0),
                        (bounds.size.height - 16.0).max(0.0),
                    ),
                ),
                self.check_color,
            );
        }
        if state.focused {
            list.fill_rect(
                Rect::new(
                    Point::new(bounds.origin.x, bounds.origin.y + bounds.size.height - 2.0),
                    Size::new(bounds.size.width, 2.0),
                ),
                self.focus_color,
            );
        }
    }
}

/// Independent Boolean value control. Returns Some(value) only on a change.
pub struct Checkbox {
    label: String,
    checked: bool,
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
            enabled: true,
            hovered: false,
            pointer_armed: false,
            pointer_inside: false,
            space_armed: false,
        }
    }

    pub fn value(&self) -> bool {
        self.checked
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
                tree.set_focus(Some(id))?;
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
            && value != self.checked
        {
            self.checked = value;
            tree.invalidate(id, Dirty::PAINT)?;
            tree.invalidate(id, Dirty::SEMANTICS)?;
            return Ok(Some(value));
        }
        if self.visual_state(tree, id) != previous_visual {
            tree.invalidate(id, Dirty::PAINT)?;
        }
        Ok(None)
    }

    pub fn visual_state(&self, tree: &Tree, id: NodeId) -> CheckboxVisualState {
        CheckboxVisualState {
            enabled: self.enabled,
            checked: self.checked,
            hovered: self.hovered,
            pressed: self.enabled
                && ((self.pointer_armed
                    && self.pointer_inside
                    && tree.pointer_capture() == Some(id))
                    || (self.space_armed && tree.focus() == Some(id))),
            focused: tree.focus() == Some(id),
        }
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
            enabled: self.enabled,
            focused: tree.focus() == Some(id),
            actions: if self.enabled { ACTIONS } else { &[] },
        }
    }
}
