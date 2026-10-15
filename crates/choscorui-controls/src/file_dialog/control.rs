//! Input handling of the file dialog and its place in a control host.

use super::browse::Row;
use super::confirm::{CANCEL, Confirm, REPLACE};
use super::geometry::{Geometry, NATURAL};
use super::{FileDialog, FileDialogOutcome, POLL_INTERVAL, Part, Stop};
use crate::custom::{ControlContext, ControlEvent, ControlValue, CustomControl, MeasureContext};
use crate::hosted;
use crate::style::PaintContext;
use crate::{EditCommand, ShortcutModifiers};
use choscorui_core::{
    EventPhase, EventReply, FileDialogKind, InputModality, Key, KeyEvent, Localizer, Measured,
    Point, PointerButton, PointerEvent, Rect, SemanticAction, SemanticNode, SemanticParts,
    TextInputInfo, WheelUnit,
};
use std::path::PathBuf;

/// Which rows the selection takes: directories in a directory dialog,
/// files in the other dialogs.
fn selectable(kind: FileDialogKind) -> fn(&Row) -> bool {
    if kind == FileDialogKind::Directory {
        |row| row.is_dir
    } else {
        |row| !row.is_dir
    }
}

impl FileDialog {
    /// The focus stops, in Tab order.
    fn stops(&self) -> Vec<Stop> {
        let mut stops = vec![Stop::Location, Stop::List];
        if self.kind == FileDialogKind::Save {
            stops.push(Stop::Name);
        }
        stops.extend([Stop::Filter, Stop::Hidden, Stop::Cancel, Stop::Accept]);
        stops
    }

    /// True when the accept button can run: a file dialog needs a row,
    /// and a save dialog needs a name.
    pub(super) fn accept_enabled(&self) -> bool {
        match self.kind {
            FileDialogKind::Open | FileDialogKind::OpenMany => {
                self.browse.current().is_some() || !self.browse.selected_rows().is_empty()
            }
            FileDialogKind::Directory => true,
            FileDialogKind::Save => !self.name.value().trim().is_empty(),
        }
    }

    fn changed(cx: &mut ControlContext<'_>) {
        cx.request_paint();
        cx.request_semantics();
    }

    /// Shows the directory `path`.
    fn open_directory(&mut self, cx: &mut ControlContext<'_>, path: PathBuf) {
        self.browse.navigate(path);
        self.location.replace_value(&self.browse.path_text());
        self.type_ahead = hosted::TypeAhead::default();
        self.hovered_row = None;
        self.next_poll = f64::NEG_INFINITY;
        self.sync_rows();
        cx.request_layout();
        Self::changed(cx);
    }

    fn go_parent(&mut self, cx: &mut ControlContext<'_>) {
        match self.browse.parent() {
            Some(parent) => self.open_directory(cx, parent),
            None => cx.beep(),
        }
    }

    /// Scrolls the list so that row `index` shows.
    fn scroll_to(&mut self, index: usize) {
        let g = self.geometry;
        let max = g.max_scroll(self.browse.rows().len());
        let top = index as f64 * g.row_height;
        let scroll = &mut self.browse.scroll;
        if top < *scroll {
            *scroll = top;
        } else if top + g.row_height > *scroll + g.rows.size.height {
            *scroll = top + g.row_height - g.rows.size.height;
        }
        *scroll = scroll.clamp(0.0, max);
    }

    /// Makes row `index` current. `extend` grows the selection of a
    /// dialog for several files from its anchor; otherwise the row is the
    /// whole selection. A save dialog copies the name of a file row.
    fn move_to(&mut self, index: usize, extend: bool) {
        let rule = selectable(self.kind);
        if extend && self.kind == FileDialogKind::OpenMany {
            self.browse.extend(index, rule);
        } else {
            self.browse.set_current(index);
            self.browse.select_only(index, rule);
        }
        if self.kind == FileDialogKind::Save
            && let Some(row) = self.browse.row(index).filter(|row| !row.is_dir)
        {
            let name = row.name.clone();
            self.name.replace_value(&name);
        }
        self.scroll_to(index);
    }

    /// Opens a directory row, or accepts a file row.
    fn activate_row(&mut self, cx: &mut ControlContext<'_>, index: usize) {
        let Some(row) = self.browse.row(index).cloned() else {
            return;
        };
        if row.is_dir {
            self.open_directory(cx, row.path);
        } else {
            if !self.browse.is_selected(index) {
                self.move_to(index, false);
            }
            self.accept(cx);
        }
    }

    /// Runs the accept button.
    fn accept(&mut self, cx: &mut ControlContext<'_>) {
        match self.kind {
            FileDialogKind::Open | FileDialogKind::OpenMany => {
                let paths: Vec<PathBuf> = self
                    .browse
                    .selected_rows()
                    .into_iter()
                    .map(|row| row.path.clone())
                    .collect();
                if !paths.is_empty() {
                    self.finish(cx, FileDialogOutcome::Selected(paths));
                } else if let Some(row) = self.browse.current_row().filter(|row| row.is_dir) {
                    let path = row.path.clone();
                    self.open_directory(cx, path);
                } else {
                    cx.beep();
                }
            }
            FileDialogKind::Directory => {
                let chosen = self
                    .browse
                    .selected_rows()
                    .first()
                    .map(|row| row.path.clone());
                let path = chosen.unwrap_or_else(|| self.browse.path().to_path_buf());
                self.finish(cx, FileDialogOutcome::Selected(vec![path]));
            }
            FileDialogKind::Save => self.save(cx),
        }
    }

    /// Saves to the typed name: opens a directory of that name, asks
    /// before it replaces a file, and else accepts the path.
    fn save(&mut self, cx: &mut ControlContext<'_>) {
        let name = self.name.value().trim().to_owned();
        if name.is_empty() {
            cx.beep();
            return;
        }
        let target = self.browse.path().join(&name);
        if target.is_dir() {
            self.name.replace_value("");
            self.open_directory(cx, target);
        } else if target.exists() {
            self.confirm = Some(Confirm::new(target, &self.labels));
            self.parts.clear();
            cx.request_layout();
            Self::changed(cx);
        } else {
            self.finish(cx, FileDialogOutcome::Selected(vec![target]));
        }
    }

    fn cancel(&mut self, cx: &mut ControlContext<'_>) {
        self.finish(cx, FileDialogOutcome::Cancelled);
    }

    /// Goes to the typed location: a directory opens, and an existing file
    /// of an open dialog is the outcome.
    fn go_to_location(&mut self, cx: &mut ControlContext<'_>) {
        let typed = PathBuf::from(self.location.value().trim());
        let path = if typed.is_relative() {
            self.browse.path().join(typed)
        } else {
            typed
        };
        let opens = matches!(self.kind, FileDialogKind::Open | FileDialogKind::OpenMany);
        if path.is_dir() {
            self.open_directory(cx, path);
        } else if opens && path.is_file() {
            self.finish(cx, FileDialogOutcome::Selected(vec![path]));
        } else {
            cx.beep();
        }
    }

    fn set_filter(&mut self, cx: &mut ControlContext<'_>, filter: usize) {
        if filter < self.filters.len() && filter != self.filter {
            self.filter = filter;
            self.sync_rows();
            self.browse.scroll = 0.0;
            Self::changed(cx);
        }
    }

    fn toggle_hidden(&mut self, cx: &mut ControlContext<'_>) {
        let show = !self.browse.show_hidden();
        self.browse.set_show_hidden(show);
        self.next_poll = f64::NEG_INFINITY;
        cx.request_layout();
        Self::changed(cx);
    }

    /// Runs the button-like part `part`.
    fn run(&mut self, cx: &mut ControlContext<'_>, part: Part) {
        match part {
            Part::Parent => self.go_parent(cx),
            Part::Filter => self.set_filter(cx, (self.filter + 1) % self.filters.len()),
            Part::Hidden => self.toggle_hidden(cx),
            Part::Cancel => self.cancel(cx),
            Part::Accept if self.accept_enabled() => self.accept(cx),
            Part::Accept => cx.beep(),
            Part::Confirm(button) => self.answer(cx, button),
        }
    }

    /// Closes the overwrite confirmation with `button`.
    fn answer(&mut self, cx: &mut ControlContext<'_>, button: usize) {
        let Some(confirm) = self.confirm.take() else {
            return;
        };
        self.parts.clear();
        cx.request_layout();
        Self::changed(cx);
        if button == REPLACE {
            self.finish(cx, FileDialogOutcome::Selected(vec![confirm.path]));
        }
    }

    fn part_at(&self, point: Point) -> Option<Part> {
        if let Some(confirm) = &self.confirm {
            return confirm.button_at(point).map(Part::Confirm);
        }
        let g = &self.geometry;
        [
            (g.parent, Part::Parent),
            (g.filter, Part::Filter),
            (g.hidden, Part::Hidden),
            (g.cancel, Part::Cancel),
            (g.accept, Part::Accept),
        ]
        .into_iter()
        .find(|(rect, _)| rect.contains(point))
        .map(|(_, part)| part)
    }

    /// The stop that a press on `part` focuses.
    fn stop_of(part: Part) -> Option<Stop> {
        match part {
            Part::Filter => Some(Stop::Filter),
            Part::Hidden => Some(Stop::Hidden),
            Part::Cancel => Some(Stop::Cancel),
            Part::Accept => Some(Stop::Accept),
            Part::Parent | Part::Confirm(_) => None,
        }
    }

    fn confirm_key(&mut self, cx: &mut ControlContext<'_>, key: Key, repeat: bool) {
        let Some(confirm) = &mut self.confirm else {
            return;
        };
        match key {
            Key::Tab => {
                confirm.focus = if confirm.focus == CANCEL {
                    REPLACE
                } else {
                    CANCEL
                };
                Self::changed(cx);
            }
            // Enter never runs Replace, the destructive button.
            Key::Enter if !repeat && confirm.focus == CANCEL => self.answer(cx, CANCEL),
            Key::Space if !repeat => {
                let focus = confirm.focus;
                self.answer(cx, focus);
            }
            Key::Escape => self.answer(cx, CANCEL),
            _ => {}
        }
    }

    /// The keys of the list.
    fn list_key(
        &mut self,
        cx: &mut ControlContext<'_>,
        key: Key,
        repeat: bool,
        modifiers: ShortcutModifiers,
    ) -> EventReply {
        let count = self.browse.rows().len();
        let current = self.browse.current();
        let last = count.saturating_sub(1);
        let page = self.geometry.page();
        let target = match key {
            Key::ArrowDown => Some(current.map_or(0, |index| (index + 1).min(last))),
            Key::ArrowUp => Some(current.map_or(last, |index| index.saturating_sub(1))),
            Key::Home => Some(0),
            Key::End => Some(last),
            Key::PageDown => Some(current.map_or(0, |index| (index + page).min(last))),
            Key::PageUp => Some(current.map_or(0, |index| index.saturating_sub(page))),
            _ => None,
        };
        if let Some(target) = target {
            if count > 0 {
                self.move_to(target, modifiers.shift);
                Self::changed(cx);
            }
            return EventReply::Stop;
        }
        match key {
            Key::Space if !repeat => {
                if let Some(index) = current {
                    let rule = selectable(self.kind);
                    if self.kind == FileDialogKind::OpenMany {
                        self.browse.toggle(index, rule);
                    } else {
                        self.browse.select_only(index, rule);
                    }
                    Self::changed(cx);
                }
                EventReply::Stop
            }
            Key::Enter if !repeat => {
                match current {
                    Some(index) => self.activate_row(cx, index),
                    None if self.accept_enabled() => self.accept(cx),
                    None => cx.beep(),
                }
                EventReply::Stop
            }
            Key::Backspace if !repeat => {
                self.go_parent(cx);
                EventReply::Stop
            }
            Key::Space | Key::Enter | Key::Backspace => EventReply::Stop,
            _ => EventReply::Continue,
        }
    }

    /// The keys of the stop that has the focus, after the keys of the
    /// whole dialog.
    fn key(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let modifiers = cx.modifiers();
        let (key, repeat) = match event {
            ControlEvent::Key(KeyEvent::Down { key, repeat }) => (*key, *repeat),
            ControlEvent::Key(KeyEvent::Up { key: Key::Space }) => return self.space_up(cx),
            _ => return self.editor_event(cx, event),
        };
        if self.confirm.is_some() {
            if !modifiers.is_chord() {
                self.confirm_key(cx, key, repeat);
            }
            return EventReply::Stop;
        }
        let plain = !modifiers.is_chord() && !modifiers.option;
        match key {
            Key::Escape if plain => {
                self.cancel(cx);
                return EventReply::Stop;
            }
            Key::ArrowUp if modifiers.option && !modifiers.is_chord() => {
                if !repeat {
                    self.go_parent(cx);
                }
                return EventReply::Stop;
            }
            Key::Tab if !modifiers.is_chord() => {
                let stops = self.stops();
                let at = stops
                    .iter()
                    .position(|stop| *stop == self.focus)
                    .unwrap_or(0);
                return match hosted::tab_stop(at, stops.len(), modifiers.shift) {
                    Some(next) => {
                        self.focus = stops[next];
                        self.space = None;
                        Self::changed(cx);
                        EventReply::Stop
                    }
                    None => EventReply::Continue,
                };
            }
            _ => {}
        }
        if modifiers.is_chord() && !matches!(self.focus, Stop::Location | Stop::Name) {
            return EventReply::Continue;
        }
        match self.focus {
            Stop::Location | Stop::Name => {
                let reply = self.editor_event(cx, event);
                if key == Key::Enter && !repeat && plain {
                    if self.focus == Stop::Location {
                        self.go_to_location(cx);
                    } else if self.accept_enabled() {
                        self.accept(cx);
                    } else {
                        cx.beep();
                    }
                    return EventReply::Stop;
                }
                Self::changed(cx);
                reply
            }
            Stop::List => self.list_key(cx, key, repeat, modifiers),
            Stop::Filter => match key {
                Key::ArrowDown | Key::ArrowUp => {
                    let last = self.filters.len() - 1;
                    let next = if key == Key::ArrowDown {
                        (self.filter + 1).min(last)
                    } else {
                        self.filter.saturating_sub(1)
                    };
                    self.set_filter(cx, next);
                    EventReply::Stop
                }
                _ => self.button_key(cx, key, repeat),
            },
            Stop::Hidden | Stop::Cancel | Stop::Accept => self.button_key(cx, key, repeat),
        }
    }

    /// Enter and Space on a button-like stop. Space shows the press and
    /// runs on its release. Enter runs Cancel or accept; on the filter
    /// and the checkbox, it runs the accept button as the default button.
    fn button_key(&mut self, cx: &mut ControlContext<'_>, key: Key, repeat: bool) -> EventReply {
        match key {
            Key::Space => {
                if !repeat {
                    self.space = Some(self.focus);
                    cx.request_paint();
                }
                EventReply::Stop
            }
            Key::Enter => {
                if !repeat {
                    let part = if self.focus == Stop::Cancel {
                        Part::Cancel
                    } else {
                        Part::Accept
                    };
                    self.run(cx, part);
                }
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }

    fn space_up(&mut self, cx: &mut ControlContext<'_>) -> EventReply {
        let Some(stop) = self.space.take() else {
            return EventReply::Continue;
        };
        cx.request_paint();
        let part = match stop {
            Stop::Filter => Part::Filter,
            Stop::Hidden => Part::Hidden,
            Stop::Cancel => Part::Cancel,
            Stop::Accept => Part::Accept,
            Stop::Location | Stop::List | Stop::Name => return EventReply::Stop,
        };
        if self.focus == stop {
            self.run(cx, part);
        }
        EventReply::Stop
    }

    /// Gives `event` to the field that has the focus.
    fn editor_event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        match self.focus {
            Stop::Location => {
                let rect = self.geometry.location_text;
                self.location.hosted_event(cx, event, rect)
            }
            Stop::Name => {
                let rect = self.geometry.name_text;
                self.name.hosted_event(cx, event, rect)
            }
            _ => EventReply::Continue,
        }
    }

    /// A press in the list: selects, extends, or toggles the row, and a
    /// double click opens or accepts it.
    fn press_row(&mut self, cx: &mut ControlContext<'_>, index: usize) {
        let modifiers = cx.modifiers();
        if cx.click_count() >= 2 {
            self.activate_row(cx, index);
            return;
        }
        let many = self.kind == FileDialogKind::OpenMany;
        if many && modifiers.is_chord() {
            self.browse.set_current(index);
            self.browse.toggle(index, selectable(self.kind));
        } else {
            self.move_to(index, many && modifiers.shift);
        }
        Self::changed(cx);
    }

    fn pointer(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let pressed = self.parts.pressed;
        let mut parts = self.parts;
        parts.track(cx, event, |point| self.part_at(point));
        self.parts = parts;
        let ControlEvent::Pointer(pointer) = event else {
            return EventReply::Continue;
        };
        match *pointer {
            PointerEvent::Down { position, button } => {
                if self.confirm.is_some() || button != PointerButton::Primary {
                    return EventReply::Stop;
                }
                let g = self.geometry;
                if let Some(stop) = self.parts.pressed.and_then(Self::stop_of) {
                    self.focus = stop;
                    cx.request_focus();
                } else if g.location.contains(position) {
                    self.focus = Stop::Location;
                    cx.request_focus();
                    self.location.hosted_event(cx, event, g.location_text);
                } else if g.name.contains(position) {
                    self.focus = Stop::Name;
                    cx.request_focus();
                    self.name.hosted_event(cx, event, g.name_text);
                } else if g.list.contains(position) {
                    self.focus = Stop::List;
                    cx.request_focus();
                    let count = self.browse.rows().len();
                    if let Some(index) = g.row_at(position, count, self.browse.scroll) {
                        self.press_row(cx, index);
                    }
                }
                Self::changed(cx);
                EventReply::Stop
            }
            PointerEvent::Move { position } => {
                let count = self.browse.rows().len();
                let row = self.geometry.row_at(position, count, self.browse.scroll);
                if row != self.hovered_row {
                    self.hovered_row = row;
                    cx.request_paint();
                }
                self.drag_editors(cx, event);
                EventReply::Continue
            }
            PointerEvent::Up { .. } => {
                self.drag_editors(cx, event);
                if let Some(part) = pressed.filter(|part| self.parts.hovered == Some(*part)) {
                    self.run(cx, part);
                }
                EventReply::Stop
            }
            PointerEvent::Wheel {
                position,
                delta,
                unit,
            } if self.geometry.list.contains(position) && self.confirm.is_none() => {
                let step = match unit {
                    WheelUnit::Points => delta.y,
                    WheelUnit::Lines => delta.y * self.geometry.row_height,
                };
                let max = self.geometry.max_scroll(self.browse.rows().len());
                self.browse.scroll = (self.browse.scroll + step).clamp(0.0, max);
                Self::changed(cx);
                EventReply::Stop
            }
            PointerEvent::Wheel { .. } => EventReply::Continue,
        }
    }

    /// Gives a pointer move or release to the fields, for a drag that
    /// selects text.
    fn drag_editors(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) {
        let g = self.geometry;
        self.location.hosted_event(cx, event, g.location_text);
        self.name.hosted_event(cx, event, g.name_text);
    }

    /// Typed text: the focused field takes it, and the list finds the
    /// first row that starts with it.
    fn text(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if self.confirm.is_some() {
            return EventReply::Stop;
        }
        if self.focus != Stop::List {
            let reply = self.editor_event(cx, event);
            Self::changed(cx);
            return reply;
        }
        let ControlEvent::Text(text) = event else {
            return EventReply::Continue;
        };
        let rows = self.browse.rows();
        let found = self.type_ahead.find(
            text,
            cx.time(),
            self.browse.current(),
            rows.len(),
            |index| rows[index].name.clone(),
        );
        if let Some(index) = found {
            self.move_to(index, false);
            Self::changed(cx);
        }
        EventReply::Stop
    }

    fn edit(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if self.focus == Stop::List {
            if event == &ControlEvent::Edit(EditCommand::SelectAll)
                && self.kind == FileDialogKind::OpenMany
            {
                self.browse.select_all(selectable(self.kind));
                Self::changed(cx);
                return EventReply::Stop;
            }
            return EventReply::Continue;
        }
        let reply = self.editor_event(cx, event);
        Self::changed(cx);
        reply
    }

    fn focus_changed(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent, focused: bool) {
        if focused && cx.input_modality() == InputModality::Keyboard {
            let stops = self.stops();
            self.focus = if cx.modifiers().shift {
                stops[stops.len() - 1]
            } else {
                stops[0]
            };
        }
        self.space = None;
        let g = self.geometry;
        self.location.hosted_event(cx, event, g.location_text);
        self.name.hosted_event(cx, event, g.name_text);
        let mut parts = self.parts;
        parts.track(cx, event, |_| None);
        self.parts = parts;
        Self::changed(cx);
    }
}

impl CustomControl for FileDialog {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        Measured::new(cx.constraints.constrain(NATURAL))
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.sync();
        let save = self.kind == FileDialogKind::Save;
        self.geometry = Geometry::new(cx, bounds, &self.labels, &self.filters, save);
        let max = self.geometry.max_scroll(self.browse.rows().len());
        self.browse.scroll = self.browse.scroll.clamp(0.0, max);
        if let Some(confirm) = &mut self.confirm {
            confirm.arrange(cx, bounds);
        }
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        if let ControlEvent::Timer = event {
            self.next_poll = cx.time() + POLL_INTERVAL;
            if self.poll() {
                cx.request_layout();
                Self::changed(cx);
            }
            return EventReply::Continue;
        }
        if !self.enabled {
            return EventReply::Continue;
        }
        self.sync();
        match event {
            ControlEvent::Pointer(_) | ControlEvent::Hover(_) => self.pointer(cx, event),
            ControlEvent::Key(_) if cx.is_focused() => self.key(cx, event),
            ControlEvent::Text(_) if cx.is_focused() => self.text(cx, event),
            ControlEvent::Compose(_) if cx.is_focused() => {
                let reply = self.editor_event(cx, event);
                Self::changed(cx);
                reply
            }
            ControlEvent::Edit(_) if cx.is_focused() => self.edit(cx, event),
            ControlEvent::FocusChanged(focused) => {
                self.focus_changed(cx, event, *focused);
                EventReply::Continue
            }
            ControlEvent::Dismiss => {
                self.space = None;
                self.drag_editors(cx, event);
                let mut parts = self.parts;
                parts.track(cx, event, |_| None);
                self.parts = parts;
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        self.paint_dialog(cx);
    }

    fn semantics(&self) -> SemanticNode {
        self.dialog_semantics()
    }

    fn semantic_parts(&self, strings: &Localizer, parts: &mut SemanticParts) {
        self.dialog_parts(strings, parts);
    }

    fn text_input(&self) -> Option<TextInputInfo> {
        match self.focus {
            Stop::Location if self.confirm.is_none() => self.location.hosted_text_input(),
            Stop::Name if self.confirm.is_none() => self.name.hosted_text_input(),
            _ => None,
        }
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        if !self.enabled {
            return false;
        }
        match action {
            SemanticAction::Dismiss => {
                match self.confirm {
                    Some(_) => self.answer(cx, CANCEL),
                    None => self.cancel(cx),
                }
                true
            }
            SemanticAction::Focus => {
                cx.request_focus();
                true
            }
            _ => false,
        }
    }

    fn focusable(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.location.set_enabled_state(enabled);
        self.name.set_enabled_state(enabled);
        if !enabled {
            self.parts.clear();
            self.space = None;
        }
    }

    /// Takes `Text` as the name of a save dialog.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        self.kind == FileDialogKind::Save && self.name.take_text(value)
    }

    /// The next poll while the listing loads.
    fn deadline(&self) -> Option<f64> {
        self.browse.is_loading().then_some(self.next_poll)
    }
}
