//! Dates and times: the month calendar and sectioned date/time editors.

use crate::page::{Column, Env, Page, Push, node, pressed};
use choscorui::controls::{
    Calendar, CalendarCell, CalendarFocus, CalendarLayout, ControlKind, DateTimeEdit, DateTimeKind,
    ShortcutModifiers, Slot, StyleQuery, TextAlign, ValidationState, VisualState,
};
use choscorui::core::{
    CivilDate, CivilDateTime, CivilTime, DisplayList, Key, KeyEvent, NodeId, Point, PointerButton,
    PointerEvent, Rect, SemanticAction, SemanticNode, Size, Tree,
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
    origin: Point,
}

pub fn calendar(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let previous = Push::new(tree, parent, "Previous month", env);
    let title = node(tree, parent, false);
    let next = Push::new(tree, parent, "Next month", env);
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
        origin: Point::default(),
    })
}

/// The day the demo treats as today.
const TODAY: (i32, u8, u8) = (2026, 10, 12);

impl CalendarPage {
    fn layout_of(&self, env: &Env<'_>) -> CalendarLayout {
        Calendar::layout(self.origin, env.styles(), env.tokens, env.rtl())
    }
}

impl Page for CalendarPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let size = Calendar::size(env.styles(), env.tokens);
        let area = column.cells(size.height, &[size.width])[0];
        self.origin = area.origin;
        let layout = self.layout_of(env);
        let _ = tree.set_bounds(self.previous.node, layout.previous);
        let _ = tree.set_bounds(self.title, layout.caption);
        let _ = tree.set_bounds(self.next.node, layout.next);
        let first = layout.days[0];
        let grid = Rect::new(
            Point::new(area.origin.x, first.origin.y),
            Size::new(area.size.width, first.size.height * 6.0),
        );
        let _ = tree.set_bounds(self.grid, grid);
        for (cell, rect) in self.cells.iter().zip(&layout.days) {
            let _ = tree.set_bounds(*cell, *rect);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        let layout = self.layout_of(env);
        let focus = if tree.focus_visible(self.grid) {
            CalendarFocus::Grid
        } else if tree.focus_visible(self.previous.node) {
            CalendarFocus::Previous
        } else if tree.focus_visible(self.next.node) {
            CalendarFocus::Next
        } else {
            CalendarFocus::None
        };
        let today = CivilDate::new(TODAY.0, TODAY.1, TODAY.2).ok();
        let grid = tree.bounds(self.grid).unwrap_or_default();
        self.calendar
            .paint(&mut env.cx(list, grid), &layout, today, focus);
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
            let message = Rect::new(
                Point::new(bounds.origin.x, bounds.max_y() + 2.0),
                Size::new(bounds.size.width + 240.0, 16.0),
            );
            let host = VisualState::NONE
                .set(VisualState::FOCUSED, tree.focus() == Some(*id))
                .set(VisualState::FOCUS_VISIBLE, tree.focus_visible(*id));
            edit.paint(&mut env.cx(list, bounds), host);
            if edit.validation() != ValidationState::Valid {
                let mut cx = env.cx(list, message);
                let style = cx.style(StyleQuery::new(ControlKind::Field, Slot::Error));
                cx.label(
                    message,
                    "Outside the allowed range; Return clamps it",
                    &style,
                    TextAlign::Start,
                );
            }
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
