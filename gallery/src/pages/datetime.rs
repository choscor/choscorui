//! Dates and times: the month calendar and sectioned date/time editors.

use crate::page::{Align, Column, Env, Page, Push, node, paint_field, pressed, split};
use choscorui::controls::{
    Calendar, CalendarCell, DateTimeEdit, DateTimeKind, ShortcutModifiers, ValidationState,
};
use choscorui::core::{
    CivilDate, CivilDateTime, CivilTime, ColorRole, DisplayList, Key, KeyEvent, NodeId, Point,
    PointerButton, PointerEvent, Rect, SemanticAction, SemanticNode, Size, Tree,
};

fn date(year: i32, month: u8, day: u8) -> CivilDate {
    CivilDate::new(year, month, day).expect("demo dates are valid")
}

fn date_time(day: CivilDate, hour: u8, minute: u8) -> CivilDateTime {
    CivilDateTime {
        date: day,
        time: CivilTime::new(hour, minute, 0).expect("demo times are valid"),
    }
}

/// A month calendar limited to 2026, with previous and next month buttons.
pub struct CalendarPage {
    grid: NodeId,
    title: NodeId,
    cells: Vec<NodeId>,
    previous: Push,
    next: Push,
    calendar: Calendar,
    chosen: Option<CivilDate>,
}

pub fn calendar(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let previous = Push::new(tree, parent, if env.rtl() { "›" } else { "‹" }, env);
    let title = node(tree, parent, false);
    let next = Push::new(tree, parent, if env.rtl() { "‹" } else { "›" }, env);
    let grid = node(tree, parent, true);
    let cells = (0..42).map(|_| node(tree, grid, false)).collect();
    let mut calendar = Calendar::new(env.locale.clone(), date(2026, 10, 12));
    calendar.set_range(Some(date(2026, 1, 1)), Some(date(2026, 12, 31)));
    Box::new(CalendarPage {
        grid,
        title,
        cells,
        previous,
        next,
        calendar,
        chosen: None,
    })
}

const CELL: f64 = 40.0;

impl Page for CalendarPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let header = column.cells(30.0, &[36.0, CELL * 7.0 - 72.0 - 2.0 * column.gap, 36.0]);
        let _ = tree.set_bounds(self.previous.node, header[0]);
        let _ = tree.set_bounds(self.title, header[1]);
        let _ = tree.set_bounds(self.next.node, header[2]);
        column.space(20.0);
        let grid = column.cells(CELL * 6.0, &[CELL * 7.0])[0];
        let _ = tree.set_bounds(self.grid, grid);
        for (index, cell) in self.cells.iter().enumerate() {
            let row = Rect::new(
                Point::new(grid.origin.x, grid.origin.y + (index / 7) as f64 * CELL),
                Size::new(grid.size.width, CELL),
            );
            let rect = split(row, &[CELL; 7], 0.0, env.rtl())[index % 7];
            let _ = tree.set_bounds(*cell, rect);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        self.previous.paint(tree, list, env);
        self.next.paint(tree, list, env);
        let title = tree.bounds(self.title).unwrap_or_default();
        env.text(
            list,
            title,
            &self.calendar.title(),
            env.body(),
            env.tokens.color(ColorRole::Foreground),
            Align::Center,
        );
        let grid = tree.bounds(self.grid).unwrap_or_default();
        let headers = Rect::new(
            Point::new(grid.origin.x, grid.origin.y - 22.0),
            Size::new(grid.size.width, 20.0),
        );
        for (name, rect) in
            self.calendar
                .weekday_headers()
                .iter()
                .zip(split(headers, &[CELL; 7], 0.0, env.rtl()))
        {
            env.text(
                list,
                rect,
                name,
                env.small(),
                env.tokens.color(ColorRole::Foreground).with_alpha(0.6),
                Align::Center,
            );
        }
        env.panel(list, grid);
        let focused = tree.focus() == Some(self.grid);
        for (cell, id) in self.calendar.cells().iter().zip(&self.cells) {
            let rect = tree.bounds(*id).unwrap_or_default().inset(3.0);
            let mut color = env.tokens.color(ColorRole::Foreground);
            if cell.selected {
                let _ = list.fill_rounded_rect(
                    rect,
                    rect.size.height / 2.0,
                    env.tokens.color(ColorRole::Primary),
                );
                color = env.on_accent();
                if focused {
                    env.focus_ring(list, rect);
                }
            } else if Some(cell.date) == self.chosen {
                let _ = list.stroke_rounded_rect(
                    rect,
                    rect.size.height / 2.0,
                    env.tokens.color(ColorRole::Primary),
                    1.5,
                );
            }
            if !cell.in_month || !cell.enabled || !env.enabled {
                color = color.with_alpha(0.35);
            }
            env.text(
                list,
                rect,
                &cell.date.day().to_string(),
                env.body(),
                color,
                Align::Center,
            );
        }
        if let Some(chosen) = self.chosen {
            let below = Rect::new(
                Point::new(grid.origin.x, grid.max_y() + 8.0),
                Size::new(grid.size.width + 200.0, 20.0),
            );
            env.caption(
                list,
                below,
                &format!("Chosen: {}", chosen.format_long(env.locale)),
            );
        }
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes = vec![
            self.previous.semantics(tree),
            self.next.semantics(tree),
            (self.grid, self.calendar.semantics("Calendar")),
        ];
        nodes.extend(
            self.calendar
                .cells()
                .iter()
                .zip(&self.cells)
                .map(|(cell, id)| (*id, self.calendar.cell_semantics(cell))),
        );
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        if self.previous.pointer(tree, event) {
            self.calendar.show_month(-1);
            return true;
        }
        if self.next.pointer(tree, event) {
            self.calendar.show_month(1);
            return true;
        }
        let PointerEvent::Down {
            position,
            button: PointerButton::Primary,
        } = event
        else {
            return false;
        };
        let cells: Vec<CalendarCell> = self.calendar.cells();
        let Some(cell) = cells.iter().zip(&self.cells).find_map(|(cell, id)| {
            tree.bounds(*id)
                .is_some_and(|b| b.contains(position))
                .then_some(*cell)
        }) else {
            return false;
        };
        let _ = tree.set_focus(Some(self.grid));
        if cell.enabled {
            self.calendar.select(cell.date);
            self.chosen = Some(cell.date);
        }
        true
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        if self.previous.key(tree, event) {
            self.calendar.show_month(-1);
            return true;
        }
        if self.next.key(tree, event) {
            self.calendar.show_month(1);
            return true;
        }
        if tree.focus() != Some(self.grid) {
            return false;
        }
        let Some(key) = pressed(event) else {
            return false;
        };
        // Left and right follow reading order: in right-to-left layouts the
        // left arrow moves to the next day.
        let key = match (key, env.rtl()) {
            (Key::ArrowLeft, true) => Key::ArrowRight,
            (Key::ArrowRight, true) => Key::ArrowLeft,
            (key, _) => key,
        };
        if matches!(key, Key::Enter | Key::Space) {
            self.chosen = Some(self.calendar.selected());
            return true;
        }
        self.calendar.handle_key(key)
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        if self.previous.action(tree, node, action) {
            self.calendar.show_month(-1);
            return true;
        }
        if self.next.action(tree, node, action) {
            self.calendar.show_month(1);
            return true;
        }
        let cells = self.calendar.cells();
        match self.cells.iter().position(|id| *id == node) {
            Some(index) if action == SemanticAction::Activate && cells[index].enabled => {
                self.calendar.select(cells[index].date);
                self.chosen = Some(cells[index].date);
                true
            }
            _ => false,
        }
    }
}

/// Date, time, and date-time editors; the last is limited to office hours
/// in 2026 and reports out-of-range values until Return clamps them.
pub struct DateTimeEditPage {
    edits: Vec<(NodeId, NodeId, DateTimeEdit, &'static str)>,
}

pub fn date_time_edit(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let today = date(2026, 10, 12);
    let mut edits = Vec::new();
    for (label, kind) in [
        ("Date", DateTimeKind::Date),
        ("Time", DateTimeKind::Time),
        (
            "Meeting (Oct 2026, 09:00 – 17:00 on the 12th)",
            DateTimeKind::DateTime,
        ),
    ] {
        let caption = node(tree, parent, false);
        let id = node(tree, parent, true);
        let mut edit = DateTimeEdit::new(label, env.locale.clone(), kind, date_time(today, 9, 30));
        if kind == DateTimeKind::DateTime {
            edit.set_range(Some(date_time(today, 9, 0)), Some(date_time(today, 17, 0)));
        }
        edits.push((caption, id, edit, label));
    }
    Box::new(DateTimeEditPage { edits })
}

impl DateTimeEditPage {
    /// The range-limited editor, for scripted checks.
    #[cfg(target_os = "macos")]
    pub fn limited_mut(&mut self) -> &mut DateTimeEdit {
        &mut self.edits[2].2
    }
}

impl Page for DateTimeEditPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for (caption, id, _, _) in &self.edits {
            let _ = tree.set_bounds(*caption, column.row(20.0));
            let _ = tree.set_bounds(*id, column.cells(32.0, &[260.0])[0]);
            column.space(18.0);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (caption, id, edit, label) in &self.edits {
            let note = format!("{label} — editing {:?}", edit.focused_field());
            env.caption(list, tree.bounds(*caption).unwrap_or_default(), &note);
            let bounds = tree.bounds(*id).unwrap_or_default();
            let validation = match edit.validation() {
                ValidationState::Intermediate => {
                    ValidationState::Invalid("Outside the allowed range; Return clamps it".into())
                }
                other => other,
            };
            let message = Rect::new(
                Point::new(bounds.origin.x, bounds.max_y() + 2.0),
                Size::new(bounds.size.width + 240.0, 16.0),
            );
            paint_field(
                env,
                list,
                bounds,
                &edit.text(),
                "",
                tree.focus() == Some(*id),
                &validation,
                Some(message),
            );
        }
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        self.edits
            .iter()
            .map(|(_, id, edit, _)| (*id, edit.semantics()))
            .collect()
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let PointerEvent::Down {
            position,
            button: PointerButton::Primary,
        } = event
        else {
            return false;
        };
        let hit = self
            .edits
            .iter()
            .find(|(_, id, _, _)| tree.bounds(*id).is_some_and(|b| b.contains(position)));
        match hit {
            Some((_, id, _, _)) => tree.set_focus(Some(*id)).is_ok(),
            None => false,
        }
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        env: &Env<'_>,
    ) -> bool {
        let Some(key) = pressed(event) else {
            return false;
        };
        let focus = tree.focus();
        let Some((_, _, edit, _)) = self
            .edits
            .iter_mut()
            .find(|(_, id, _, _)| Some(*id) == focus)
        else {
            return false;
        };
        let key = match (key, env.rtl()) {
            (Key::ArrowLeft, true) => Key::ArrowRight,
            (Key::ArrowRight, true) => Key::ArrowLeft,
            (key, _) => key,
        };
        if matches!(key, Key::Tab | Key::Escape) {
            return false;
        }
        edit.handle_key(key);
        true
    }

    fn text(&mut self, tree: &mut Tree, text: &str, _env: &Env<'_>) -> bool {
        let focus = tree.focus();
        let Some((_, _, edit, _)) = self
            .edits
            .iter_mut()
            .find(|(_, id, _, _)| Some(*id) == focus)
        else {
            return false;
        };
        let mut handled = false;
        for character in text.chars().filter(|c| !c.is_control()) {
            edit.handle_key(Key::Character(character));
            handled = true;
        }
        handled
    }

    fn action(
        &mut self,
        _tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        let key = match action {
            SemanticAction::Increment => Key::ArrowUp,
            SemanticAction::Decrement => Key::ArrowDown,
            _ => return false,
        };
        match self.edits.iter_mut().find(|(_, id, _, _)| *id == node) {
            Some((_, _, edit, _)) => edit.handle_key(key),
            None => false,
        }
    }
}
