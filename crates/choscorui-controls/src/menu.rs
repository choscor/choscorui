//! A custom-drawn dropdown menu backed by commands, painted with the
//! `Menu` recipe.

use crate::custom::{ControlContext, ControlEvent, CustomControl, MeasureContext};
use crate::overlay::menu_popup::{CommandResult, MenuPopup};
use crate::style::{
    ControlKind, MenuItemVariant, PaintContext, Slot, StyleQuery, Styles, TextAlign, VisualState,
    content_rect, fill_shape,
};
use crate::{CommandId, CommandRegistry, CommandState};
use choscorui_core::{
    DesignTokens, EventPhase, EventReply, Icon, Key, KeyEvent, LayoutDirection, Localizer,
    Measured, Point, PointerEvent, Rect, SemanticAction, SemanticNode, SemanticParts, SemanticRole,
    SemanticState, Size, fold_for_search,
};

/// How an action shows its command's checked state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MenuCheck {
    /// No indicator.
    #[default]
    None,
    /// A check mark while the command is checked.
    Checkbox,
    /// A dot while the command is checked.
    Radio,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MenuItem {
    Action {
        label: String,
        command: CommandId,
        /// Display text such as "⌘C"; matching is done by the command
        /// registry.
        shortcut: Option<String>,
        variant: MenuItemVariant,
        check: MenuCheck,
    },
    Submenu {
        label: String,
        items: Vec<MenuItem>,
    },
    Separator,
}

impl MenuItem {
    pub fn action(label: impl Into<String>, command: CommandId) -> Self {
        Self::Action {
            label: label.into(),
            command,
            shortcut: None,
            variant: MenuItemVariant::Default,
            check: MenuCheck::None,
        }
    }

    pub fn submenu(label: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self::Submenu {
            label: label.into(),
            items,
        }
    }

    /// Sets the shortcut text of an action. Other items are unchanged.
    pub fn with_shortcut(mut self, text: impl Into<String>) -> Self {
        if let Self::Action { shortcut, .. } = &mut self {
            *shortcut = Some(text.into());
        }
        self
    }

    /// Paints an action with the destructive look.
    pub fn destructive(mut self) -> Self {
        if let Self::Action { variant, .. } = &mut self {
            *variant = MenuItemVariant::Destructive;
        }
        self
    }

    /// Shows the command's checked state with `kind`.
    pub fn with_check(mut self, kind: MenuCheck) -> Self {
        if let Self::Action { check, .. } = &mut self {
            *check = kind;
        }
        self
    }

    pub(crate) fn label(&self) -> Option<&str> {
        match self {
            Self::Action { label, .. } | Self::Submenu { label, .. } => Some(label),
            Self::Separator => None,
        }
    }

    pub(crate) fn check(&self) -> MenuCheck {
        match self {
            Self::Action { check, .. } => *check,
            _ => MenuCheck::None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuResponse {
    None,
    Activated(CommandId),
    /// The menu (or its last submenu level) closed without a choice.
    Closed,
}

/// Row heights and the panel inset, read from the `Menu` recipe.
#[derive(Clone, Copy, Debug, PartialEq)]
struct MenuMetrics {
    row: f64,
    separator: f64,
    inset: f64,
}

impl MenuMetrics {
    fn resolve(styles: &Styles, tokens: &DesignTokens) -> Self {
        let style = |slot| styles.resolve(tokens, StyleQuery::new(ControlKind::Menu, slot));
        let (item, rule, panel) = (style(Slot::Item), style(Slot::Separator), style(Slot::Root));
        Self {
            row: item
                .min_height
                .max(item.text.line_height + item.padding.vertical()),
            separator: rule.min_height + rule.padding.vertical(),
            inset: panel.padding.block_start,
        }
    }
}

/// Custom-drawn dropdown menu backed by commands: disabled commands are
/// skipped by the keyboard and cannot activate; checkable actions show
/// the command's checked state. Native context menus remain available
/// through the macOS backend.
#[derive(Clone, Debug)]
pub struct DropdownMenu {
    items: Vec<MenuItem>,
    /// Highlighted index per open level, root first.
    path: Vec<Option<usize>>,
    metrics: MenuMetrics,
    /// Whether submenus open toward the left.
    rtl: bool,
}

/// Deprecated name of [`DropdownMenu`].
#[deprecated(since = "0.1.0", note = "renamed to `DropdownMenu`")]
pub type PopupMenu = DropdownMenu;

impl DropdownMenu {
    /// A menu measured with the default recipe and the light theme. Call
    /// [`DropdownMenu::set_metrics`] when the theme or recipe differs.
    pub fn new(items: Vec<MenuItem>) -> Self {
        Self {
            items,
            path: vec![None],
            metrics: MenuMetrics::resolve(&Styles::new(), &DesignTokens::light()),
            rtl: false,
        }
    }

    /// Opens submenus toward the reading end: in a right-to-left layout,
    /// ArrowLeft opens a submenu and ArrowRight closes one.
    pub fn set_direction(&mut self, direction: LayoutDirection) {
        self.rtl = direction == LayoutDirection::Rtl;
    }

    /// Reads the row heights and the panel inset from `styles`.
    pub fn set_metrics(&mut self, styles: &Styles, tokens: &DesignTokens) {
        self.metrics = MenuMetrics::resolve(styles, tokens);
    }

    /// The items of every level, root first.
    pub(crate) fn items(&self) -> &[MenuItem] {
        &self.items
    }

    /// The highlighted index of each open level, root first.
    pub(crate) fn path(&self) -> &[Option<usize>] {
        &self.path
    }

    /// The panel padding above the first row.
    pub(crate) fn inset(&self) -> f64 {
        self.metrics.inset
    }

    /// The height of an item row.
    pub(crate) fn row_height(&self) -> f64 {
        self.metrics.row
    }

    /// Closes every submenu and clears the highlight.
    pub(crate) fn reset(&mut self) {
        self.path = vec![None];
    }

    pub fn depth(&self) -> usize {
        self.path.len()
    }

    pub(crate) fn level_items(&self, level: usize) -> &[MenuItem] {
        let mut items = &self.items[..];
        for index in self.path.iter().take(level) {
            match index.and_then(|index| items.get(index)) {
                Some(MenuItem::Submenu {
                    items: children, ..
                }) => items = children,
                _ => return &[],
            }
        }
        items
    }

    pub fn highlighted(&self) -> Option<usize> {
        *self.path.last().unwrap_or(&None)
    }

    fn enabled(item: &MenuItem, commands: &CommandRegistry) -> bool {
        match item {
            MenuItem::Action { command, .. } => commands
                .state(*command)
                .map(|state| state.enabled)
                .unwrap_or(false),
            MenuItem::Submenu { items, .. } => !items.is_empty(),
            MenuItem::Separator => false,
        }
    }

    fn step(&mut self, commands: &CommandRegistry, forward: bool) {
        let level = self.path.len() - 1;
        let items = self.level_items(level).to_vec();
        if items.is_empty() {
            return;
        }
        let count = items.len();
        let mut index = match (self.path[level], forward) {
            (Some(index), true) => (index + 1) % count,
            (Some(index), false) => (index + count - 1) % count,
            (None, true) => 0,
            (None, false) => count - 1,
        };
        for _ in 0..count {
            if Self::enabled(&items[index], commands) {
                self.path[level] = Some(index);
                return;
            }
            index = if forward {
                (index + 1) % count
            } else {
                (index + count - 1) % count
            };
        }
    }

    pub fn handle_key(&mut self, key: Key, commands: &CommandRegistry) -> MenuResponse {
        let level = self.path.len() - 1;
        match key.in_reading_order(self.rtl) {
            Key::ArrowDown => self.step(commands, true),
            Key::ArrowUp => self.step(commands, false),
            Key::Home => {
                self.path[level] = None;
                self.step(commands, true);
            }
            Key::End => {
                self.path[level] = None;
                self.step(commands, false);
            }
            Key::ArrowRight => self.open_submenu(commands),
            Key::ArrowLeft | Key::Escape => {
                if self.path.len() > 1 {
                    self.path.pop();
                } else {
                    return MenuResponse::Closed;
                }
            }
            Key::Enter | Key::Space => return self.activate_highlighted(commands),
            Key::Character(character) => {
                // Type-select: next enabled item starting with the letter.
                let items = self.level_items(level).to_vec();
                let needle = fold_for_search(&character.to_string());
                let start = self.path[level].map_or(0, |index| index + 1);
                for step in 0..items.len() {
                    let index = (start + step) % items.len();
                    if Self::enabled(&items[index], commands)
                        && items[index]
                            .label()
                            .is_some_and(|label| fold_for_search(label).starts_with(&needle))
                    {
                        self.path[level] = Some(index);
                        break;
                    }
                }
            }
            _ => {}
        }
        MenuResponse::None
    }

    fn open_submenu(&mut self, commands: &CommandRegistry) {
        let level = self.path.len() - 1;
        if let Some(index) = self.path[level]
            && let Some(MenuItem::Submenu { .. }) = self.level_items(level).get(index)
        {
            self.path.push(None);
            self.step(commands, true);
        }
    }

    fn activate_highlighted(&mut self, commands: &CommandRegistry) -> MenuResponse {
        let level = self.path.len() - 1;
        let Some(index) = self.path[level] else {
            return MenuResponse::None;
        };
        match self.level_items(level).get(index).cloned() {
            Some(MenuItem::Action { command, .. })
                if commands.state(command).is_ok_and(|s| s.enabled) =>
            {
                MenuResponse::Activated(command)
            }
            Some(MenuItem::Submenu { .. }) => {
                self.open_submenu(commands);
                MenuResponse::None
            }
            _ => MenuResponse::None,
        }
    }

    fn height(&self, item: &MenuItem) -> f64 {
        match item {
            MenuItem::Separator => self.metrics.separator,
            _ => self.metrics.row,
        }
    }

    /// Size of level `level` for placement.
    pub fn level_size(&self, level: usize, width: f64) -> Size {
        let rows = self
            .level_items(level)
            .iter()
            .map(|item| self.height(item))
            .sum::<f64>();
        Size::new(width, rows + 2.0 * self.metrics.inset)
    }

    /// The rect of item `index` of level `level` in a panel at `panel`.
    pub fn item_rect(&self, level: usize, panel: Rect, index: usize) -> Option<Rect> {
        let items = self.level_items(level);
        let item = items.get(index)?;
        let y = panel.origin.y
            + self.metrics.inset
            + items[..index]
                .iter()
                .map(|item| self.height(item))
                .sum::<f64>();
        let inset = self.metrics.inset;
        Some(Rect::from_xywh(
            panel.origin.x + inset,
            y,
            (panel.size.width - 2.0 * inset).max(0.0),
            self.height(item),
        ))
    }

    fn item_at(&self, level: usize, origin: Point, point: Point) -> Option<usize> {
        let mut y = origin.y + self.metrics.inset;
        for (index, item) in self.level_items(level).iter().enumerate() {
            let height = self.height(item);
            if point.y >= y && point.y < y + height {
                return Some(index);
            }
            y += height;
        }
        None
    }

    /// Pointer hover highlights; release over an enabled action activates.
    pub fn pointer(
        &mut self,
        level: usize,
        origin: Point,
        point: Point,
        released: bool,
        commands: &CommandRegistry,
    ) -> MenuResponse {
        if level >= self.path.len() {
            return MenuResponse::None;
        }
        let Some(index) = self.item_at(level, origin, point) else {
            return MenuResponse::None;
        };
        let item = self.level_items(level)[index].clone();
        if !Self::enabled(&item, commands) {
            return MenuResponse::None;
        }
        self.path.truncate(level + 1);
        self.path[level] = Some(index);
        if matches!(item, MenuItem::Submenu { .. }) {
            self.path.push(None);
            return MenuResponse::None;
        }
        if released {
            return self.activate_highlighted(commands);
        }
        MenuResponse::None
    }

    fn checked(item: &MenuItem, commands: &CommandRegistry) -> Option<bool> {
        match item {
            MenuItem::Action { command, check, .. } if *check != MenuCheck::None => {
                Some(commands.state(*command).is_ok_and(|state| state.checked))
            }
            _ => None,
        }
    }

    /// Adds the items of open level `level`, laid out from `rows`, as
    /// `MenuItem` parts under `parent`. Returns the position of the part
    /// of the highlighted item.
    pub fn semantic_parts(
        &self,
        level: usize,
        commands: &CommandRegistry,
        rows: Rect,
        parent: Option<usize>,
        parts: &mut SemanticParts,
    ) -> Option<usize> {
        let mut highlighted = None;
        for (index, item) in self.level_items(level).iter().enumerate() {
            let (Some(label), Some(bounds)) = (item.label(), self.item_rect(level, rows, index))
            else {
                continue;
            };
            let enabled = Self::enabled(item, commands);
            let current = self.path.get(level) == Some(&Some(index));
            let part = parts.push(parent, SemanticRole::MenuItem, label, bounds);
            if let MenuItem::Action {
                shortcut: Some(shortcut),
                ..
            } = item
            {
                part.node.description.clone_from(shortcut);
            }
            part.node.state = SemanticState {
                disabled: !enabled,
                checked: Self::checked(item, commands),
                selected: Some(current),
                expanded: matches!(item, MenuItem::Submenu { .. })
                    .then(|| current && self.path.len() > level + 1),
                ..SemanticState::default()
            };
            if enabled {
                part.node.actions.push(SemanticAction::Activate);
            }
            if current {
                highlighted = Some(parts.len() - 1);
            }
        }
        highlighted
    }

    /// The flags the item recipe reads for item `index` of `level`.
    fn item_state(&self, level: usize, index: usize, commands: &CommandRegistry) -> VisualState {
        let items = self.level_items(level);
        let highlighted = self.path.get(level) == Some(&Some(index));
        let open = highlighted && self.path.len() > level + 1;
        VisualState::NONE
            .set(VisualState::HIGHLIGHTED, highlighted)
            .set(VisualState::OPEN, open)
            .set(
                VisualState::DISABLED,
                !Self::enabled(&items[index], commands),
            )
            .set(
                VisualState::CHECKED,
                Self::checked(&items[index], commands) == Some(true),
            )
    }

    /// Paints level `level` as a panel in the context's bounds.
    pub fn paint(&self, cx: &mut PaintContext<'_>, level: usize, commands: &CommandRegistry) {
        let panel = cx.bounds;
        self.paint_scrolled(cx, level, commands, panel);
    }

    /// Paints level `level` as a panel in the context's bounds, with its
    /// rows laid out in `rows`: the panel of every row, which is taller
    /// than the bounds and moved up when the panel scrolls. Rows show
    /// only inside the panel padding.
    pub(crate) fn paint_scrolled(
        &self,
        cx: &mut PaintContext<'_>,
        level: usize,
        commands: &CommandRegistry,
        rows: Rect,
    ) {
        let panel = cx.bounds;
        let query = |slot| StyleQuery::new(ControlKind::Menu, slot);
        let root = cx.style(query(Slot::Root));
        cx.slot(panel, &root, |_, _| {});
        let inset = self.metrics.inset;
        cx.list.push_clip(Rect::from_xywh(
            panel.origin.x,
            panel.origin.y + inset,
            panel.size.width,
            (panel.size.height - 2.0 * inset).max(0.0),
        ));
        self.paint_rows(cx, level, commands, rows);
        let _ = cx.list.pop_clip();
    }

    fn paint_rows(
        &self,
        cx: &mut PaintContext<'_>,
        level: usize,
        commands: &CommandRegistry,
        panel: Rect,
    ) {
        let query = |slot| StyleQuery::new(ControlKind::Menu, slot);
        let items = self.level_items(level);
        let indent = items.iter().any(|item| item.check() != MenuCheck::None);
        for (index, item) in items.iter().enumerate() {
            let Some(row) = self.item_rect(level, panel, index) else {
                continue;
            };
            let state = self.item_state(level, index, commands);
            match item {
                MenuItem::Separator => {
                    let rule = cx.style(query(Slot::Separator));
                    let line = content_rect(row, &rule, cx.rtl);
                    let line = Rect::from_xywh(
                        line.origin.x,
                        line.origin.y,
                        line.size.width,
                        rule.min_height,
                    );
                    fill_shape(cx.list, line, &rule.radius, cx.rtl, &rule.background);
                }
                MenuItem::Action {
                    label,
                    shortcut,
                    variant,
                    check,
                    ..
                } => {
                    let style = cx.style(query(Slot::Item).variant(*variant).state(state));
                    let indicator = cx.style(query(Slot::ItemIndicator).state(state));
                    let shortcut_style = cx.style(query(Slot::Shortcut).state(state));
                    cx.slot(row, &style, |cx, inner| {
                        let mut text = inner;
                        if indent {
                            let (mark, rest) = split_start(inner, indicator.min_width, cx.rtl);
                            text = trim_start(rest, style.gap, cx.rtl);
                            if state.checked() {
                                paint_mark(cx, mark, *check, &indicator);
                            }
                        }
                        let keys = shortcut.as_deref().unwrap_or_default();
                        cx.label_and_trailing(text, label, &style, keys, &shortcut_style);
                    });
                }
                MenuItem::Submenu { label, .. } => {
                    let style = cx.style(query(Slot::Trigger).state(state));
                    cx.slot(row, &style, |cx, inner| {
                        let mut text = inner;
                        if indent {
                            let rest = split_start(inner, style.icon_size, cx.rtl).1;
                            text = trim_start(rest, style.gap, cx.rtl);
                        }
                        let chevron = if cx.rtl {
                            Icon::ChevronLeft
                        } else {
                            Icon::ChevronRight
                        };
                        let (end, rest) = split_end(text, style.icon_size, cx.rtl);
                        cx.icon(end, chevron, &style);
                        cx.label(rest, label, &style, TextAlign::Start);
                    });
                }
            }
        }
    }
}

fn paint_mark(
    cx: &mut PaintContext<'_>,
    rect: Rect,
    check: MenuCheck,
    style: &crate::style::ResolvedStyle,
) {
    match check {
        MenuCheck::None => {}
        MenuCheck::Checkbox => cx.icon(rect, Icon::Check, style),
        MenuCheck::Radio => {
            // A dot half the icon size, centered in the indicator.
            let size = style.icon_size / 2.0;
            let dot = Rect::from_xywh(
                rect.origin.x + (rect.size.width - size) / 2.0,
                rect.origin.y + (rect.size.height - size) / 2.0,
                size,
                size,
            );
            let _ = cx.list.fill_rounded_rect(dot, size / 2.0, style.foreground);
        }
    }
}

/// Splits `width` off the start of `rect` in reading order.
fn split_start(rect: Rect, width: f64, rtl: bool) -> (Rect, Rect) {
    let width = width.min(rect.size.width);
    let rest = rect.size.width - width;
    let (start_x, rest_x) = if rtl {
        (rect.origin.x + rest, rect.origin.x)
    } else {
        (rect.origin.x, rect.origin.x + width)
    };
    (
        Rect::from_xywh(start_x, rect.origin.y, width, rect.size.height),
        Rect::from_xywh(rest_x, rect.origin.y, rest, rect.size.height),
    )
}

/// Splits `width` off the end of `rect` in reading order.
fn split_end(rect: Rect, width: f64, rtl: bool) -> (Rect, Rect) {
    let (end, rest) = split_start(rect, rect.size.width - width.min(rect.size.width), rtl);
    (rest, end)
}

fn trim_start(rect: Rect, width: f64, rtl: bool) -> Rect {
    split_start(rect, width, rtl).1
}

impl DropdownMenu {
    /// The menu as a control for a [`crate::ControlHost`], named `name`,
    /// with its own command states. Every action starts enabled and
    /// unchecked.
    pub fn hosted(self, name: impl Into<String>) -> HostedDropdownMenu {
        let mut popup = MenuPopup::in_place(self.items);
        // Always shown: nothing animates.
        popup.open(Rect::default(), f64::NEG_INFINITY, false);
        HostedDropdownMenu {
            name: name.into(),
            popup,
            anchor: Rect::default(),
            keyboard: false,
        }
    }
}

/// A [`DropdownMenu`] shown in place in a [`crate::ControlHost`]: the root
/// level fills the control's bounds, and open submenus float beside it.
/// An activated action reports `Command`; Escape or ArrowLeft at the root
/// reports `Dismissed`. Make it with [`DropdownMenu::hosted`].
pub struct HostedDropdownMenu {
    name: String,
    popup: MenuPopup,
    /// The control's bounds, where the root level shows.
    anchor: Rect,
    /// Whether the keyboard opened the menu, so placing it highlights the
    /// first row.
    keyboard: bool,
}

impl HostedDropdownMenu {
    /// Marks a menu that the keyboard opened: when the arrange places it,
    /// the first row is highlighted, as in a keyboard-opened popup.
    #[must_use]
    pub fn opened_from_keyboard(mut self, keyboard: bool) -> Self {
        self.keyboard = keyboard;
        self
    }

    /// Sets the enabled and checked state of a menu command.
    pub fn set_command(&mut self, command: CommandId, state: CommandState) -> CommandResult {
        self.popup.set_command(command, state)
    }

    pub fn command(&self, command: CommandId) -> Option<CommandState> {
        self.popup.commands.state(command).ok()
    }

    pub fn menu(&self) -> &DropdownMenu {
        &self.popup.menu
    }

    /// Closes the submenus and clears the highlight.
    fn reset(&mut self, time: f64) {
        self.popup.close(time);
        self.popup.open(self.anchor, time, false);
    }

    fn respond(&mut self, cx: &mut ControlContext<'_>, response: MenuResponse) {
        if self.popup.finish(cx, response) {
            // The menu stays in place after a choice.
            self.reset(cx.time());
        }
        cx.request_paint();
        cx.request_semantics();
    }
}

impl CustomControl for HostedDropdownMenu {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let content = cx.style(StyleQuery::new(ControlKind::Menu, Slot::Content));
        self.popup.arrange(cx, content.min_width);
        let size = self
            .popup
            .panels()
            .first()
            .map_or(Size::new(0.0, 0.0), |panel| panel.size);
        Measured::new(cx.constraints.constrain(size))
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let content = cx.style(StyleQuery::new(ControlKind::Menu, Slot::Content));
        self.popup
            .arrange(cx, content.min_width.max(bounds.size.width));
        if bounds != self.anchor {
            self.anchor = bounds;
            self.popup.close(f64::NEG_INFINITY);
            self.popup.open(bounds, f64::NEG_INFINITY, self.keyboard);
        }
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        if let ControlEvent::Pointer(pointer) = event
            && self.popup.wheel(pointer)
        {
            cx.request_paint();
            return EventReply::Stop;
        }
        match event {
            ControlEvent::Pointer(PointerEvent::Down { position, .. }) => {
                match self.popup.pointer(*position, false) {
                    Some(_) => {
                        cx.request_paint();
                        cx.request_semantics();
                        EventReply::Stop
                    }
                    None => EventReply::Continue,
                }
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) => {
                if self.popup.pointer(*position, false).is_some() {
                    cx.request_paint();
                    cx.request_semantics();
                }
                EventReply::Continue
            }
            ControlEvent::Pointer(PointerEvent::Up { position, .. }) => {
                match self.popup.pointer(*position, true) {
                    Some(response) => {
                        self.respond(cx, response);
                        EventReply::Stop
                    }
                    None => EventReply::Continue,
                }
            }
            // Command and Control chords go on to the shortcuts.
            ControlEvent::Key(KeyEvent::Down { key, .. })
                if *key != Key::Tab && !cx.modifiers().is_chord() =>
            {
                let response = self.popup.key(*key);
                self.respond(cx, response);
                EventReply::Stop
            }
            ControlEvent::Dismiss | ControlEvent::FocusChanged(false) => {
                self.reset(cx.time());
                cx.request_paint();
                cx.request_semantics();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        if let Some(panel) = self.popup.panels().first() {
            self.popup
                .menu
                .paint(&mut cx.at(*panel), 0, &self.popup.commands);
        }
    }

    fn overlay(&self) -> Option<Rect> {
        self.popup
            .panels()
            .into_iter()
            .skip(1)
            .reduce(|a, b| a.union(b))
    }

    fn paint_overlay(&self, cx: &mut PaintContext<'_>) {
        for (level, panel) in self.popup.panels().into_iter().enumerate().skip(1) {
            let mut panel_cx = cx.at(panel);
            panel_cx.state = VisualState::OPEN;
            self.popup
                .menu
                .paint(&mut panel_cx, level, &self.popup.commands);
        }
    }

    fn semantics(&self) -> SemanticNode {
        let mut actions = vec![SemanticAction::Focus];
        if self.popup.menu.highlighted().is_some() {
            actions.insert(0, SemanticAction::Activate);
        }
        crate::hosted::node(SemanticRole::Menu, &self.name, actions)
    }

    fn semantic_parts(&self, _strings: &Localizer, parts: &mut SemanticParts) {
        self.popup.semantic_parts(None, parts);
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Activate => {
                let response = self.popup.key(Key::Enter);
                self.respond(cx, response);
                true
            }
            SemanticAction::Focus => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.reset(f64::NEG_INFINITY);
        }
    }
}
