//! Custom-drawn dialog contracts: platform button ordering, modal and
//! modeless dialog stacking with focus restoration, and the color, font,
//! single-value input, and message dialog models.
//!
//! Native dialogs (open/save/directory panels and alerts) live in the
//! macOS backend (`choscorui_macos::{FileDialogSpec, AlertSpec}`); the
//! dialogs here are drawn by the toolkit and use the same ordering and
//! keyboard rules so both kinds behave alike.

use crate::{ComboChange, ComboInput, FontChooser, NumberInput, ValidationState};
use choscorui_core::{
    Color, FontSlant, FontWeight, Key, Locale, NodeId, SemanticAction, SemanticNode, SemanticRole,
    SemanticState, TextStyle, Tree, TreeError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogButtonRole {
    /// Confirms (Return); the default button.
    Accept,
    /// Dismisses without changes (Escape; `DialogKeys::route` also maps ⌘.).
    Reject,
    /// Confirms a destructive action (for example "Delete").
    Destructive,
    /// Leaves without saving ("Don't Save").
    Discard,
    Help,
    /// Any other action ("Apply", "Reset").
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogButton {
    pub title: String,
    pub role: DialogButtonRole,
}

impl DialogButton {
    pub fn new(title: impl Into<String>, role: DialogButtonRole) -> Self {
        Self {
            title: title.into(),
            role,
        }
    }
}

/// Orders buttons left to right following the macOS Human Interface
/// Guidelines: Help at the far left, then Discard ("Don't Save"), a
/// flexible gap, other actions, Reject (Cancel), and the default button at
/// the far right. Right-to-left layouts mirror the whole row.
/// Returns indexes into `buttons` and the position of the gap.
pub fn order_dialog_buttons(buttons: &[DialogButton], rtl: bool) -> (Vec<usize>, usize) {
    let rank = |role: DialogButtonRole| match role {
        DialogButtonRole::Help => 0,
        DialogButtonRole::Discard => 1,
        DialogButtonRole::Other => 2,
        DialogButtonRole::Reject => 3,
        DialogButtonRole::Destructive | DialogButtonRole::Accept => 4,
    };
    let mut order: Vec<usize> = (0..buttons.len()).collect();
    order.sort_by_key(|index| rank(buttons[*index].role));
    let gap = order
        .iter()
        .position(|index| rank(buttons[*index].role) >= 2)
        .unwrap_or(order.len());
    if rtl {
        order.reverse();
        (order, buttons.len() - gap)
    } else {
        (order, gap)
    }
}

/// Which button the keyboard triggers.
pub fn dialog_key_button(buttons: &[DialogButton], key: Key) -> Option<usize> {
    let role = match key {
        // A destructive button is never triggered by Return.
        Key::Enter => DialogButtonRole::Accept,
        Key::Escape => DialogButtonRole::Reject,
        _ => return None,
    };
    buttons.iter().position(|button| button.role == role)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Modality {
    /// Blocks input to the rest of the window until closed.
    Modal,
    /// Floats while the rest of the window stays usable.
    Modeless,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DialogId(pub u64);

#[derive(Clone, Copy, Debug)]
struct OpenDialog {
    id: DialogId,
    root: NodeId,
    modality: Modality,
    restore_focus: Option<NodeId>,
}

/// Open dialogs in a window. Modal dialogs confine input routing to their
/// subtree; closing a dialog restores the focus it saved.
#[derive(Default)]
pub struct DialogStack {
    open: Vec<OpenDialog>,
    next: u64,
}

impl DialogStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a dialog whose content is the subtree at `root`, moving
    /// focus to `initial` (for example the first field or default button).
    pub fn open(
        &mut self,
        tree: &mut Tree,
        root: NodeId,
        modality: Modality,
        initial: Option<NodeId>,
    ) -> Result<DialogId, TreeError> {
        if !tree.contains(root) {
            return Err(TreeError::MissingNode);
        }
        let restore_focus = tree.focus();
        if let Some(initial) = initial {
            tree.set_focus(Some(initial))?;
        }
        self.next += 1;
        let id = DialogId(self.next);
        self.open.push(OpenDialog {
            id,
            root,
            modality,
            restore_focus,
        });
        Ok(id)
    }

    pub fn close(&mut self, tree: &mut Tree, id: DialogId) -> bool {
        let Some(index) = self.open.iter().position(|dialog| dialog.id == id) else {
            return false;
        };
        let dialog = self.open.remove(index);
        if let Some(focus) = dialog.restore_focus.filter(|node| tree.contains(*node)) {
            let _ = tree.set_focus(Some(focus));
        }
        true
    }

    /// The modal dialog that currently owns input, if any.
    pub fn active_modal(&self) -> Option<DialogId> {
        self.open
            .iter()
            .rev()
            .find(|dialog| dialog.modality == Modality::Modal)
            .map(|dialog| dialog.id)
    }

    fn is_descendant(tree: &Tree, node: NodeId, root: NodeId) -> bool {
        let mut current = Some(node);
        while let Some(id) = current {
            if id == root {
                return true;
            }
            current = tree.parent(id);
        }
        false
    }

    /// Whether pointer or keyboard input may reach `node`: anything when
    /// no modal is open, otherwise only the top modal's subtree.
    pub fn accepts_input(&self, tree: &Tree, node: NodeId) -> bool {
        match self
            .open
            .iter()
            .rev()
            .find(|dialog| dialog.modality == Modality::Modal)
        {
            Some(modal) => Self::is_descendant(tree, node, modal.root),
            None => true,
        }
    }

    /// Tab-cycles focus within the top modal dialog.
    pub fn focus_next(&self, tree: &mut Tree, reverse: bool) -> Option<NodeId> {
        let modal = self
            .open
            .iter()
            .rev()
            .find(|dialog| dialog.modality == Modality::Modal)?;
        let start = tree.focus();
        for _ in 0..1024 {
            let next = tree.focus_next(reverse)?;
            if Self::is_descendant(tree, next, modal.root) {
                return Some(next);
            }
            if Some(next) == start {
                break;
            }
        }
        None
    }

    pub fn semantics(&self, id: DialogId, title: &str) -> Option<SemanticNode> {
        let dialog = self.open.iter().find(|dialog| dialog.id == id)?;
        Some(SemanticNode {
            role: SemanticRole::Dialog,
            name: title.to_owned(),
            description: match dialog.modality {
                Modality::Modal => "Modal".into(),
                Modality::Modeless => String::new(),
            },
            value: None,
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: vec![SemanticAction::Dismiss],
            text: None,
        })
    }
}

/// Color chooser model: HSV editing with RGB and hex text, alpha, and
/// recent colors. All channels are kept in floating point so repeated
/// edits do not drift.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorPicker {
    hue: f64,
    saturation: f64,
    value: f64,
    alpha: f64,
    recent: Vec<Color>,
}

impl ColorPicker {
    pub fn new(color: Color) -> Self {
        let mut picker = Self {
            hue: 0.0,
            saturation: 0.0,
            value: 0.0,
            alpha: 1.0,
            recent: Vec::new(),
        };
        picker.set_color(color);
        picker
    }

    pub fn color(&self) -> Color {
        let (r, g, b) = hsv_to_rgb(self.hue, self.saturation, self.value);
        let byte = |channel: f64| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color::rgba8(byte(r), byte(g), byte(b), byte(self.alpha))
    }

    pub fn set_color(&mut self, color: Color) {
        let channel = |byte: u8| f64::from(byte) / 255.0;
        let (h, s, v) = rgb_to_hsv(
            channel(color.red),
            channel(color.green),
            channel(color.blue),
        );
        // Keep the previous hue for grays so dragging saturation back up
        // does not jump to red.
        if s > 0.0 {
            self.hue = h;
        }
        self.saturation = s;
        self.value = v;
        self.alpha = channel(color.alpha);
    }

    pub fn hsv(&self) -> (f64, f64, f64) {
        (self.hue, self.saturation, self.value)
    }
    pub fn set_hsv(&mut self, hue: f64, saturation: f64, value: f64) {
        self.hue = hue.rem_euclid(360.0);
        self.saturation = saturation.clamp(0.0, 1.0);
        self.value = value.clamp(0.0, 1.0);
    }
    pub fn set_alpha(&mut self, alpha: f64) {
        self.alpha = alpha.clamp(0.0, 1.0);
    }

    /// `#RRGGBB`, or `#RRGGBBAA` when not opaque.
    pub fn hex(&self) -> String {
        let color = self.color();
        if color.alpha == 255 {
            format!("#{:02X}{:02X}{:02X}", color.red, color.green, color.blue)
        } else {
            format!(
                "#{:02X}{:02X}{:02X}{:02X}",
                color.red, color.green, color.blue, color.alpha
            )
        }
    }

    /// Accepts `#RGB`, `#RRGGBB`, or `#RRGGBBAA` (the `#` is optional).
    pub fn set_hex(&mut self, text: &str) -> ValidationState {
        let digits = text.trim().trim_start_matches('#');
        let parse = |range: std::ops::Range<usize>| u8::from_str_radix(&digits[range], 16).ok();
        let color = match digits.len() {
            3 if digits.is_ascii() => {
                let expand = |index: usize| parse(index..index + 1).map(|value| value * 17);
                expand(0)
                    .zip(expand(1))
                    .zip(expand(2))
                    .map(|((r, g), b)| Color::rgba8(r, g, b, 255))
            }
            6 | 8 if digits.is_ascii() => {
                let alpha = if digits.len() == 8 {
                    parse(6..8)
                } else {
                    Some(255)
                };
                parse(0..2)
                    .zip(parse(2..4))
                    .zip(parse(4..6))
                    .zip(alpha)
                    .map(|(((r, g), b), a)| Color::rgba8(r, g, b, a))
            }
            0..=7 if digits.chars().all(|c| c.is_ascii_hexdigit()) => {
                return ValidationState::Intermediate;
            }
            _ => None,
        };
        match color {
            Some(color) => {
                self.set_color(color);
                ValidationState::Valid
            }
            None => ValidationState::Invalid("Enter a hex color such as #1E90FF".into()),
        }
    }

    /// Records the accepted color at the front of the recent list.
    pub fn accept(&mut self) -> Color {
        let color = self.color();
        self.recent.retain(|recent| *recent != color);
        self.recent.insert(0, color);
        self.recent.truncate(12);
        color
    }
    pub fn recent(&self) -> &[Color] {
        &self.recent
    }

    /// Accessible description such as "Red 30, Green 144, Blue 255".
    pub fn semantics(&self, name: &str) -> SemanticNode {
        let color = self.color();
        SemanticNode {
            role: SemanticRole::Group,
            name: name.to_owned(),
            description: format!(
                "Red {}, Green {}, Blue {}, Opacity {}%",
                color.red,
                color.green,
                color.blue,
                (self.alpha * 100.0).round()
            ),
            value: Some(self.hex()),
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: vec![SemanticAction::SetValue, SemanticAction::Focus],
            text: None,
        }
    }
}

fn hsv_to_rgb(hue: f64, saturation: f64, value: f64) -> (f64, f64, f64) {
    let chroma = value * saturation;
    let sector = (hue / 60.0).rem_euclid(6.0);
    let x = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (r, g, b) = match sector as u8 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = value - chroma;
    (r + m, g + m, b + m)
}

fn rgb_to_hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    (hue, saturation, max)
}

/// Font dialog model: family (with missing-font fallback), size, weight,
/// slant, and a preview style.
pub struct FontDialog {
    family: FontChooser,
    size: f64,
    weight: FontWeight,
    slant: FontSlant,
}

impl FontDialog {
    pub fn new(families: Vec<String>, current: &TextStyle) -> Self {
        Self {
            family: FontChooser::new(families, &current.family, "Helvetica"),
            size: current.size,
            weight: current.weight,
            slant: current.slant,
        }
    }
    pub fn family(&mut self, input: ComboInput) -> ComboChange {
        self.family.handle(input)
    }
    pub fn chooser(&self) -> &FontChooser {
        &self.family
    }
    pub fn set_size(&mut self, size: f64) -> ValidationState {
        if size.is_finite() && (1.0..=4096.0).contains(&size) {
            self.size = size;
            ValidationState::Valid
        } else {
            ValidationState::Invalid("Size must be between 1 and 4096 points".into())
        }
    }
    pub fn set_weight(&mut self, weight: FontWeight) {
        self.weight = FontWeight(weight.0.clamp(1, 1000));
    }
    pub fn set_slant(&mut self, slant: FontSlant) {
        self.slant = slant;
    }
    /// The chosen style (missing families resolve to the fallback).
    pub fn style(&self) -> TextStyle {
        TextStyle::new(self.family.resolved_family(), self.size)
            .with_weight(self.weight)
            .with_slant(self.slant)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum InputKind {
    Text {
        allow_empty: bool,
    },
    Integer {
        minimum: i64,
        maximum: i64,
    },
    Decimal {
        minimum: f64,
        maximum: f64,
        fraction_digits: usize,
    },
    Choice(Vec<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum InputValue {
    Text(String),
    Integer(i64),
    Decimal(f64),
    Choice(usize),
}

/// Single-value input dialog ("Rename", "Go to line"). Return accepts only
/// a valid value; the accept button's enabled state follows validation.
#[derive(Clone, Debug)]
pub struct InputDialog {
    pub title: String,
    pub prompt: String,
    kind: InputKind,
    locale: Locale,
    text: String,
}

impl InputDialog {
    pub fn new(
        title: impl Into<String>,
        prompt: impl Into<String>,
        kind: InputKind,
        locale: Locale,
        initial: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            prompt: prompt.into(),
            kind,
            locale,
            text: initial.into(),
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }
    pub fn validate(&self) -> (ValidationState, Option<InputValue>) {
        let trimmed = self.text.trim();
        match &self.kind {
            InputKind::Text { allow_empty } => {
                if trimmed.is_empty() && !allow_empty {
                    (ValidationState::Intermediate, None)
                } else {
                    (
                        ValidationState::Valid,
                        Some(InputValue::Text(self.text.clone())),
                    )
                }
            }
            InputKind::Integer { minimum, maximum } => match self.locale.parse_integer(trimmed) {
                Ok(value) if (*minimum..=*maximum).contains(&value) => {
                    (ValidationState::Valid, Some(InputValue::Integer(value)))
                }
                Ok(_) => (
                    ValidationState::Invalid(format!(
                        "Enter a number from {} to {}",
                        self.locale.format_integer(*minimum),
                        self.locale.format_integer(*maximum)
                    )),
                    None,
                ),
                Err(_) if trimmed.is_empty() || trimmed == "-" => {
                    (ValidationState::Intermediate, None)
                }
                Err(_) => (
                    ValidationState::Invalid("Enter a whole number".into()),
                    None,
                ),
            },
            InputKind::Decimal {
                minimum,
                maximum,
                fraction_digits,
            } => {
                let (state, value) = NumberInput::new(self.locale.clone(), *fraction_digits)
                    .with_range(Some(*minimum), Some(*maximum))
                    .validate(trimmed);
                (state, value.map(InputValue::Decimal))
            }
            InputKind::Choice(options) => match options.iter().position(|option| option == trimmed)
            {
                Some(index) => (ValidationState::Valid, Some(InputValue::Choice(index))),
                None => (
                    ValidationState::Invalid("Choose one of the options".into()),
                    None,
                ),
            },
        }
    }
    pub fn can_accept(&self) -> bool {
        self.validate().0 == ValidationState::Valid
    }
    /// Return: the value when valid; Escape: `Err(())` (cancelled).
    pub fn handle_key(&self, key: Key) -> Option<Result<InputValue, ()>> {
        match key {
            Key::Enter => self.validate().1.map(Ok),
            Key::Escape => Some(Err(())),
            _ => None,
        }
    }
}

/// Custom-drawn message dialog with ordered buttons and keyboard rules.
#[derive(Clone, Debug, PartialEq)]
pub struct MessageDialog {
    pub title: String,
    pub message: String,
    pub detail: Option<String>,
    pub buttons: Vec<DialogButton>,
}

impl MessageDialog {
    pub fn new(
        title: impl Into<String>,
        message: impl Into<String>,
        buttons: Vec<DialogButton>,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            detail: None,
            buttons,
        }
    }
    /// Buttons in display order with the gap index.
    pub fn layout(&self, rtl: bool) -> (Vec<&DialogButton>, usize) {
        let (order, gap) = order_dialog_buttons(&self.buttons, rtl);
        (
            order
                .into_iter()
                .map(|index| &self.buttons[index])
                .collect(),
            gap,
        )
    }
    pub fn handle_key(&self, key: Key) -> Option<usize> {
        dialog_key_button(&self.buttons, key)
    }
    pub fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Dialog,
            name: self.title.clone(),
            description: match &self.detail {
                Some(detail) => format!("{} {detail}", self.message),
                None => self.message.clone(),
            },
            value: None,
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: vec![SemanticAction::Dismiss],
            text: None,
        }
    }
}
