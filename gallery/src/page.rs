//! What every gallery page implements, the platform-neutral inputs a page
//! is built from, and small layout, paint, and style helpers shared by the
//! category files.

use choscorui::controls::{
    Button, ButtonInput, ButtonVariant, CommandId, EstimatedText, HeaderStyle, HostEnv, InputText,
    PaintContext, ShortcutModifiers, Styles, TextItemDelegate, TextMeasure, ValidationState,
    VisualState,
};
use choscorui::core::{
    BackendCapabilities, Color, ColorRole, DesignTokens, DisplayList, Key, KeyEvent,
    LayoutDirection, Locale, NodeId, Point, PointerButton, PointerEvent, Rect, SemanticAction,
    SemanticCollection, SemanticNode, SemanticRole, SemanticState, Size, TextShaper, TextStyle,
    ThemeResolver, Tree, TypeSpec, grapheme_boundaries, lerp_color,
};
use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

/// The font every page draws with.
pub const FONT: &str = "Helvetica";

/// Sidebar groups, in display order.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum Category {
    Static,
    Inputs,
    Selection,
    Range,
    Text,
    DateTime,
    Data,
    Containers,
    Navigation,
    Popups,
    Dialogs,
    Shell,
    Recipes,
    System,
}

impl Category {
    #[cfg(test)]
    pub const ALL: [Category; 14] = [
        Self::Static,
        Self::Inputs,
        Self::Selection,
        Self::Range,
        Self::Text,
        Self::DateTime,
        Self::Data,
        Self::Containers,
        Self::Navigation,
        Self::Popups,
        Self::Dialogs,
        Self::Shell,
        Self::Recipes,
        Self::System,
    ];

    /// Stable id, also the translation key suffix (`category.<id>`).
    pub const fn id(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Inputs => "inputs",
            Self::Selection => "selection",
            Self::Range => "range",
            Self::Text => "text",
            Self::DateTime => "datetime",
            Self::Data => "data",
            Self::Containers => "containers",
            Self::Navigation => "navigation",
            Self::Popups => "popups",
            Self::Dialogs => "dialogs",
            Self::Shell => "shell",
            Self::Recipes => "recipes",
            Self::System => "system",
        }
    }
}

/// Builds a page's nodes under `parent` and returns the page.
pub type Build = fn(&mut Tree, NodeId, &Env<'_>) -> Box<dyn Page>;

/// One entry of the catalog.
pub struct PageInfo {
    /// Stable lowercase id used by the CLI, settings, and translations
    /// (`page.<id>`).
    pub id: &'static str,
    /// English title, also the fallback when a translation is missing.
    pub title: &'static str,
    pub category: Category,
    pub build: Build,
}

/// Page state that survives restarts.
#[derive(Clone, Debug, PartialEq)]
pub struct PageState {
    pub canvas_zoom: f64,
    /// `DockLayout::save` text; empty means the default arrangement.
    pub dock: String,
}

impl Default for PageState {
    fn default() -> Self {
        Self {
            canvas_zoom: 1.0,
            dock: String::new(),
        }
    }
}

/// Desktop integration supplied by the platform. Pages call it from
/// buttons; tests and other OSes use [`NoDesktop`].
pub trait Desktop {
    fn capabilities(&self) -> BackendCapabilities;
    /// Shows or removes a status item; returns whether it is now shown.
    fn toggle_status_item(&self, title: &str) -> Result<bool, String>;
    /// The application that opens `url`, if any.
    fn url_handler(&self, url: &str) -> Result<Option<String>, String>;
    fn notify(&self, title: &str, body: &str) -> Result<(), String>;
}

/// No desktop integration: every capability is unsupported.
#[cfg(test)]
pub struct NoDesktop;

#[cfg(test)]
impl Desktop for NoDesktop {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::none("headless")
    }
    fn toggle_status_item(&self, _title: &str) -> Result<bool, String> {
        Err("status items are unsupported".into())
    }
    fn url_handler(&self, _url: &str) -> Result<Option<String>, String> {
        Err("URL handlers are unsupported".into())
    }
    fn notify(&self, _title: &str, _body: &str) -> Result<(), String> {
        Err("notifications are unsupported".into())
    }
}

/// Everything a page reads while it is built, laid out, painted, and
/// driven. All of it is platform-neutral.
pub struct Env<'a> {
    pub tokens: &'a DesignTokens,
    pub locale: &'a Locale,
    pub direction: LayoutDirection,
    pub enabled: bool,
    pub shaper: &'a dyn TextShaper,
    pub state: &'a PageState,
    pub desktop: &'a dyn Desktop,
    /// Time since the window opened, for tooltips, progress, and messages.
    pub now: Duration,
    pub measure_cache: &'a MeasureCache,
}

/// Text, size and tracking bits, weight, and family of a measured line.
type SpecKey = (String, u64, u16, &'static str);

/// Remembers text widths and resolved styles so painting does not reshape
/// text or resolve recipes every frame.
#[derive(Default)]
pub struct MeasureCache {
    widths: RefCell<HashMap<(String, u64), f64>>,
    specs: RefCell<HashMap<SpecKey, f64>>,
    pub styles: Styles,
}

impl MeasureCache {
    pub fn clear(&self) {
        self.widths.borrow_mut().clear();
        self.specs.borrow_mut().clear();
    }
}

impl TextMeasure for Env<'_> {
    fn width(&self, text: &str, spec: &TypeSpec) -> f64 {
        if text.is_empty() {
            return 0.0;
        }
        let key = (
            text.to_owned(),
            spec.size.to_bits() ^ spec.tracking.to_bits(),
            spec.weight.0,
            spec.family.name(),
        );
        if let Some(width) = self.measure_cache.specs.borrow().get(&key) {
            return *width;
        }
        let width = self
            .shaper
            .shape(text, &spec.text_style(), None)
            .map_or_else(
                |_| EstimatedText.width(text, spec),
                |layout| layout.size().width,
            );
        let mut specs = self.measure_cache.specs.borrow_mut();
        if specs.len() > 4096 {
            specs.clear();
        }
        specs.insert(key, width);
        width
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Align {
    Leading,
    Center,
    Trailing,
}

impl Env<'_> {
    /// A paint context for `bounds` with this page's tokens, styles,
    /// direction, and time.
    pub fn cx<'b>(&'b self, list: &'b mut DisplayList, bounds: Rect) -> PaintContext<'b> {
        PaintContext::new(list, bounds, self.tokens, &self.measure_cache.styles, self)
            .rtl(self.rtl())
            .time(self.now.as_secs_f64())
    }

    /// The style registry pages measure and paint with.
    pub fn styles(&self) -> &Styles {
        &self.measure_cache.styles
    }

    /// The environment a `ControlHost` measures, arranges, and paints
    /// with: `themes`, this page's styles and text, direction, and time.
    pub fn host<'b>(&'b self, themes: &'b ThemeResolver) -> HostEnv<'b> {
        HostEnv::new(themes, &self.measure_cache.styles, self)
            .direction(self.direction)
            .time(self.now.as_secs_f64())
    }

    /// Text and glyph color on an accent fill. Every theme's accent is a
    /// saturated mid-tone, so white keeps contrast in light and dark modes.
    pub fn on_accent(&self) -> Color {
        Color::rgba8(255, 255, 255, 255)
    }

    pub fn rtl(&self) -> bool {
        self.direction == LayoutDirection::Rtl
    }

    /// Advance width of one line of `text` at `size`.
    pub fn measure(&self, text: &str, size: f64) -> f64 {
        if text.is_empty() {
            return 0.0;
        }
        let key = (text.to_owned(), size.to_bits());
        if let Some(width) = self.measure_cache.widths.borrow().get(&key) {
            return *width;
        }
        let width = self
            .shaper
            .shape(text, &TextStyle::new(FONT, size), None)
            .map_or(text.chars().count() as f64 * size * 0.55, |layout| {
                layout.size().width
            });
        let mut widths = self.measure_cache.widths.borrow_mut();
        if widths.len() > 4096 {
            widths.clear();
        }
        widths.insert(key, width);
        width
    }

    pub fn body(&self) -> f64 {
        (16.0_f64 * 0.8).round()
    }

    pub fn small(&self) -> f64 {
        (16.0_f64 * 0.7).round()
    }

    /// Draws one line of `text` vertically centered in `rect`, aligned in
    /// reading order (leading is the right edge in right-to-left layouts).
    pub fn text(
        &self,
        list: &mut DisplayList,
        rect: Rect,
        text: &str,
        size: f64,
        color: Color,
        align: Align,
    ) {
        if text.is_empty() {
            return;
        }
        let width = self.measure(text, size);
        let leading = match (align, self.rtl()) {
            (Align::Center, _) => None,
            (Align::Leading, false) | (Align::Trailing, true) => Some(true),
            _ => Some(false),
        };
        let x = match leading {
            None => rect.origin.x + (rect.size.width - width) / 2.0,
            Some(true) => rect.origin.x,
            Some(false) => rect.max_x() - width,
        };
        let baseline = rect.origin.y + rect.size.height / 2.0 + size * 0.35;
        let _ = list.draw_text(Point::new(x, baseline), text, FONT, size, color);
    }

    /// A body-sized label in the content color.
    pub fn label(&self, list: &mut DisplayList, rect: Rect, text: &str) {
        self.text(
            list,
            rect,
            text,
            self.body(),
            self.tokens.color(ColorRole::Foreground),
            Align::Leading,
        );
    }

    /// A small, dimmed caption.
    pub fn caption(&self, list: &mut DisplayList, rect: Rect, text: &str) {
        self.text(
            list,
            rect,
            text,
            self.small(),
            self.tokens.color(ColorRole::Foreground).with_alpha(0.65),
            Align::Leading,
        );
    }

    /// A rounded surface panel behind a group of controls.
    pub fn panel(&self, list: &mut DisplayList, rect: Rect) {
        let _ = list.fill_rounded_rect(
            rect,
            self.tokens.radius.min(8.0),
            self.tokens.color(ColorRole::Card),
        );
    }

    /// Surface color mixed toward content, for hover and pressed fills.
    pub fn mix(&self, amount: f64) -> Color {
        lerp_color(
            self.tokens.color(ColorRole::Card),
            self.tokens.color(ColorRole::Foreground),
            amount,
        )
    }

    /// A two-pixel accent ring around a focused rectangle.
    pub fn focus_ring(&self, list: &mut DisplayList, rect: Rect) {
        let _ = list.stroke_rounded_rect(
            Rect::new(
                Point::new(rect.origin.x - 2.0, rect.origin.y - 2.0),
                Size::new(rect.size.width + 4.0, rect.size.height + 4.0),
            ),
            self.tokens.radius.min(8.0),
            self.tokens.color(ColorRole::Primary),
            2.0,
        );
    }

    pub fn header_style(&self) -> HeaderStyle {
        HeaderStyle {
            background: self.mix(0.1),
            text: self.tokens.color(ColorRole::Foreground),
            divider: self.mix(0.3),
            focus: self.tokens.color(ColorRole::Primary),
            font_family: FONT.into(),
            font_size: self.small(),
        }
    }

    pub fn item_delegate(&self) -> TextItemDelegate {
        TextItemDelegate {
            font_family: FONT.into(),
            font_size: self.body(),
            text: self.tokens.color(ColorRole::Foreground),
            selected_text: self.on_accent(),
            selection: self.tokens.color(ColorRole::Primary),
            inactive_selection: self.mix(0.3),
            focus: self.tokens.color(ColorRole::Primary),
            ..TextItemDelegate::default()
        }
    }
}

/// A gallery page: one control (or a few related names) in useful
/// variants. Pages own their nodes under the parent they were built in.
pub trait Page: AsAny {
    /// Places the page's nodes inside `bounds`.
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>);
    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>);
    /// Semantic nodes for the page's tree nodes.
    fn semantics(&self, tree: &Tree, env: &Env<'_>) -> Vec<(NodeId, SemanticNode)>;
    /// Returns whether anything changed.
    fn pointer(
        &mut self,
        _tree: &mut Tree,
        _event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        false
    }
    /// Returns whether the key was consumed.
    fn key(
        &mut self,
        _tree: &mut Tree,
        _event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        false
    }
    fn text(&mut self, _tree: &mut Tree, _text: &str, _env: &Env<'_>) -> bool {
        false
    }
    /// An assistive-technology action on one of the page's nodes.
    fn action(
        &mut self,
        _tree: &mut Tree,
        _node: NodeId,
        _action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        false
    }
    /// Advances timers; returns whether to repaint.
    #[cfg(target_os = "macos")]
    fn tick(&mut self, _env: &Env<'_>) -> bool {
        false
    }
    /// True when the page's controls paint their own disabled state.
    /// Other pages are dimmed by the gallery while disabled.
    fn native_disabled(&self) -> bool {
        false
    }
    /// Writes state that should survive restarts.
    fn persist(&self, _state: &mut PageState) {}
    /// Virtualized collections among the page's nodes, published after
    /// [`semantics`](Self::semantics).
    fn collections(&self, _tree: &Tree) -> Vec<Collection> {
        Vec::new()
    }
}

/// A container that declares its full item count while only its realized
/// items are semantic nodes.
pub struct Collection {
    pub node: NodeId,
    pub info: SemanticCollection,
    /// Realized item nodes with their absolute indexes.
    pub items: Vec<(NodeId, usize)>,
}

/// Lets the autopilot reach a concrete page type through `dyn Page`.
pub trait AsAny {
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Any> AsAny for T {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// A semantic node with a role, name, and optional value.
pub fn sem(role: SemanticRole, name: impl Into<String>, value: Option<String>) -> SemanticNode {
    SemanticNode {
        role,
        name: name.into(),
        description: String::new(),
        value,
        state: SemanticState::default(),
        relations: Vec::new(),
        actions: Vec::new(),
        text: None,
    }
}

/// Attaches a node under `parent`, optionally focusable.
pub fn node(tree: &mut Tree, parent: NodeId, focusable: bool) -> NodeId {
    let id = tree
        .attach(parent, Rect::default())
        .expect("page parent is attached");
    if focusable {
        tree.set_focusable(id, true)
            .expect("node was just attached");
    }
    id
}

/// Lays rows out top to bottom inside `bounds`, mirroring horizontal
/// positions in right-to-left layouts.
pub struct Column {
    pub bounds: Rect,
    pub y: f64,
    pub gap: f64,
    rtl: bool,
}

impl Column {
    pub fn new(bounds: Rect, env: &Env<'_>) -> Self {
        Self {
            bounds,
            y: bounds.origin.y,
            gap: env.tokens.spacing * 1.5,
            rtl: env.rtl(),
        }
    }

    /// A full-width row.
    pub fn row(&mut self, height: f64) -> Rect {
        let rect = Rect::new(
            Point::new(self.bounds.origin.x, self.y),
            Size::new(self.bounds.size.width, height),
        );
        self.y += height + self.gap;
        rect
    }

    /// A row split into cells of the given widths, leading to trailing.
    /// A width of zero takes the remaining space.
    pub fn cells(&mut self, height: f64, widths: &[f64]) -> Vec<Rect> {
        let row = self.row(height);
        split(row, widths, self.gap, self.rtl)
    }

    /// Skips vertical space.
    pub fn space(&mut self, height: f64) {
        self.y += height;
    }

    /// Whatever height is left below the rows so far.
    pub fn rest(&mut self) -> Rect {
        let rect = Rect::new(
            Point::new(self.bounds.origin.x, self.y),
            Size::new(
                self.bounds.size.width,
                (self.bounds.max_y() - self.y).max(0.0),
            ),
        );
        self.y = self.bounds.max_y();
        rect
    }
}

/// Splits `row` into cells (leading to trailing). A zero width takes the
/// remaining space; cells that do not fit get zero width.
pub fn split(row: Rect, widths: &[f64], gap: f64, rtl: bool) -> Vec<Rect> {
    let fixed: f64 = widths.iter().sum::<f64>() + gap * widths.len().saturating_sub(1) as f64;
    let flexible = widths.iter().filter(|w| **w == 0.0).count().max(1) as f64;
    let spare = ((row.size.width - fixed) / flexible).max(0.0);
    let mut x = 0.0;
    widths
        .iter()
        .map(|width| {
            let width = if *width == 0.0 { spare } else { *width };
            let width = width.min((row.size.width - x).max(0.0));
            let left = if rtl {
                row.max_x() - x - width
            } else {
                row.origin.x + x
            };
            x += width + gap;
            Rect::new(
                Point::new(left, row.origin.y),
                Size::new(width, row.size.height),
            )
        })
        .collect()
}

/// A push button with a text label, the most common page building block.
pub struct Push {
    pub node: NodeId,
    pub button: Button,
    pub label: String,
}

impl Push {
    pub fn new(tree: &mut Tree, parent: NodeId, label: impl Into<String>, env: &Env<'_>) -> Self {
        let label = label.into();
        let node = node(tree, parent, true);
        let mut button =
            Button::new(label.clone(), CommandId(0)).with_variant(ButtonVariant::Outline);
        let _ = button.set_enabled(tree, node, env.enabled);
        Self {
            node,
            button,
            label,
        }
    }

    /// Feeds a pointer event; returns whether the button was activated.
    pub fn pointer(&mut self, tree: &mut Tree, event: PointerEvent) -> bool {
        matches!(
            self.button
                .handle(tree, self.node, ButtonInput::Pointer(event)),
            Ok(Some(_))
        )
    }

    /// Feeds a key event; returns whether the button was activated.
    pub fn key(&mut self, tree: &mut Tree, event: KeyEvent) -> bool {
        (tree.focus() == Some(self.node) || matches!(event, KeyEvent::Up { .. }))
            && matches!(
                self.button.handle(tree, self.node, ButtonInput::Key(event)),
                Ok(Some(_))
            )
    }

    /// Performs an assistive action aimed at this button.
    pub fn action(&mut self, tree: &mut Tree, node: NodeId, action: SemanticAction) -> bool {
        node == self.node
            && action == SemanticAction::Activate
            && matches!(
                self.button.handle(
                    tree,
                    self.node,
                    ButtonInput::Semantic(choscorui::controls::SemanticAction::Activate),
                ),
                Ok(Some(_))
            )
    }

    pub fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        let Some(bounds) = tree.bounds(self.node).filter(|b| b.size.width > 0.0) else {
            return;
        };
        self.button
            .paint(&mut env.cx(list, bounds), tree, self.node);
    }

    pub fn semantics(&self, tree: &Tree) -> (NodeId, SemanticNode) {
        (self.node, self.button.semantics(tree, self.node).shared())
    }
}

/// Turns primary press and release pairs on the same node into clicks,
/// for controls that only expose `handle_key`-style methods.
#[derive(Default)]
pub struct Clicks {
    pressed: Option<NodeId>,
}

impl Clicks {
    /// Returns the node and position of a completed click among `nodes`.
    pub fn feed(
        &mut self,
        tree: &Tree,
        event: PointerEvent,
        nodes: &[NodeId],
    ) -> Option<(NodeId, Point)> {
        let hit = |position: Point| {
            nodes
                .iter()
                .copied()
                .find(|id| tree.bounds(*id).is_some_and(|b| b.contains(position)))
        };
        match event {
            PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            } => {
                self.pressed = hit(position);
                None
            }
            PointerEvent::Up {
                position,
                button: PointerButton::Primary,
            } => {
                let pressed = self.pressed.take()?;
                (hit(position) == Some(pressed)).then_some((pressed, position))
            }
            _ => None,
        }
    }
}

/// The key of a plain key press, ignoring releases.
pub fn pressed(event: KeyEvent) -> Option<Key> {
    match event {
        KeyEvent::Down { key, .. } => Some(key),
        _ => None,
    }
}

/// Paints a single-line field: frame, text (or a dimmed placeholder), a
/// caret at the end while focused, and a validation message underneath
/// when `message_rect` is given.
// Each argument is one independent input of the painted field, and the
// callers pass them inline; a parameter struct would only rename them.
#[allow(clippy::too_many_arguments)]
pub fn paint_field(
    env: &Env<'_>,
    list: &mut DisplayList,
    rect: Rect,
    text: &str,
    placeholder: &str,
    focused: bool,
    validation: &ValidationState,
    message_rect: Option<Rect>,
) {
    let state = VisualState::NONE
        .set(VisualState::DISABLED, !env.enabled)
        .set(VisualState::FOCUSED, focused)
        .set(VisualState::FOCUS_VISIBLE, focused)
        .set(
            VisualState::INVALID,
            matches!(validation, ValidationState::Invalid(_)),
        );
    let caret = grapheme_boundaries(text).len() - 1;
    InputText::new(text)
        .placeholder(placeholder)
        .caret(caret)
        .paint(&mut env.cx(list, rect), state);
    if let Some(message_rect) = message_rect {
        let (message, color) = match validation {
            ValidationState::Valid => (
                "Valid".to_owned(),
                env.tokens.color(ColorRole::Foreground).with_alpha(0.6),
            ),
            ValidationState::Intermediate => {
                ("Incomplete".to_owned(), Color::rgba8(200, 140, 30, 255))
            }
            ValidationState::Invalid(message) => {
                (message.clone(), env.tokens.color(ColorRole::Destructive))
            }
        };
        env.text(
            list,
            message_rect,
            &message,
            env.small(),
            color,
            Align::Leading,
        );
    }
}

/// Applies typed text to `value`: printable characters append; backspace
/// and delete characters remove the last character. Returns whether the
/// value changed.
pub fn type_into(value: &mut String, text: &str) -> bool {
    let mut changed = false;
    for c in text.chars() {
        if c == '\u{8}' || c == '\u{7f}' {
            changed |= value.pop().is_some();
        } else if !c.is_control() {
            value.push(c);
            changed = true;
        }
    }
    changed
}

/// The semantic node of an editable text value with validation.
pub fn field_semantics(name: &str, value: &str, validation: &ValidationState) -> SemanticNode {
    let mut node = sem(SemanticRole::TextField, name, Some(value.to_owned()));
    node.state.invalid = matches!(validation, ValidationState::Invalid(_));
    if let ValidationState::Invalid(message) = validation {
        node.description = message.clone();
    }
    node.actions = vec![SemanticAction::Focus, SemanticAction::SetValue];
    node
}
